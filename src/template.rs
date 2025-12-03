use super::metadata::{ParameterType, PromptMetadata, PromptTemplate};
use anyhow::{Context, Result};
use gray_matter::engine::YAML;
use gray_matter::{Matter, Pod};
use kodegen_mcp_schema::prompt::TemplateParamValue;
use minijinja::Environment;
use std::collections::HashMap;
use std::sync::{LazyLock, OnceLock};
use tokio::time::{timeout, Duration};

/// Static empty HashMap for use when no parameters are provided
static EMPTY_PARAMS: LazyLock<HashMap<String, TemplateParamValue>> = LazyLock::new(HashMap::new);

/// Get max parameter size (supports KODEGEN_MAX_PARAM_SIZE env var)
fn get_max_param_size() -> usize {
    static MAX_SIZE: OnceLock<usize> = OnceLock::new();
    *MAX_SIZE.get_or_init(|| {
        std::env::var("KODEGEN_MAX_PARAM_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1_000_000)
    })
}

/// Get max parameter count (supports KODEGEN_MAX_PARAM_COUNT env var)
fn get_max_param_count() -> usize {
    static MAX_COUNT: OnceLock<usize> = OnceLock::new();
    *MAX_COUNT.get_or_init(|| {
        std::env::var("KODEGEN_MAX_PARAM_COUNT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(100)
    })
}

/// Get max total parameters size (supports KODEGEN_MAX_TOTAL_PARAMS_SIZE env var)
fn get_max_total_params_size() -> usize {
    static MAX_TOTAL: OnceLock<usize> = OnceLock::new();
    *MAX_TOTAL.get_or_init(|| {
        std::env::var("KODEGEN_MAX_TOTAL_PARAMS_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10_000_000)
    })
}

/// Parse a .j2.md file into metadata and content
pub fn parse_template(filename: &str, file_content: &str) -> Result<PromptTemplate> {
    // Use gray_matter to split frontmatter and content
    let matter = Matter::<YAML>::new();
    let parsed: gray_matter::ParsedEntity<Pod> = matter
        .parse(file_content)
        .map_err(|e| anyhow::anyhow!("Failed to parse frontmatter: {e}"))?;

    // Extract and deserialize frontmatter
    let metadata: PromptMetadata = parsed
        .data
        .ok_or_else(|| anyhow::anyhow!("No frontmatter found in template"))?
        .deserialize()
        .context("Failed to parse YAML frontmatter")?;

    // Validate metadata
    validate_metadata(&metadata)?;

    // Get content (after frontmatter)
    let content = parsed.content;

    Ok(PromptTemplate {
        filename: filename.to_string(),
        metadata,
        content,
    })
}

/// Validate metadata fields
fn validate_metadata(metadata: &PromptMetadata) -> Result<()> {
    if metadata.title.is_empty() {
        anyhow::bail!("Title cannot be empty");
    }
    if metadata.description.is_empty() {
        anyhow::bail!("Description cannot be empty");
    }
    if metadata.categories.is_empty() {
        anyhow::bail!("At least one category is required");
    }
    if metadata.author.is_empty() {
        anyhow::bail!("Author cannot be empty");
    }
    
    // Validate parameter definitions
    for param in &metadata.parameters {
        validate_parameter_definition(param)
            .with_context(|| format!("Invalid parameter definition: '{}'", param.name))?;
    }
    
    Ok(())
}

/// Validate a parameter definition's default value and logical consistency
fn validate_parameter_definition(param: &super::metadata::ParameterDefinition) -> Result<()> {
    // Check 1: If default exists, validate it matches declared type
    if let Some(default) = &param.default {
        // Reuse existing validation logic!
        validate_parameter_type(param, default).with_context(|| {
            let actual_type = match default {
                TemplateParamValue::String(_) => "string",
                TemplateParamValue::Number(_) => "number",
                TemplateParamValue::Bool(_) => "boolean",
                TemplateParamValue::StringArray(_) => "array",
            };
            format!(
                "Parameter '{}' has default value type mismatch. \
                 Declared as {:?} but default value is {}. \
                 Default: {:?}\n\
                 \n\
                 Fix the template's YAML frontmatter to use the correct type for the default value.",
                param.name, param.param_type, actual_type, default
            )
        })?;
    }

    // Check 2: Validate logical consistency - required + default is contradictory
    if param.required && param.default.is_some() {
        anyhow::bail!(
            "Parameter '{}' is marked as required but has a default value. \
             This is contradictory - remove 'required: true' or remove the default.",
            param.name
        );
    }

    Ok(())
}

/// Render a template with parameters and environment variables
///
/// # Security Notes
/// - Template size is validated before parsing (max 1MB)
/// - Parameter sizes are validated before rendering (max 1MB per param, 10MB total)
/// - Parameter count is limited (max 100 parameters)
/// - `MiniJinja` has built-in recursion limits (default ~500 levels)
/// - **Timeout enforcement (5 seconds) prevents infinite loops and expensive operations**
/// - Rendering runs in `spawn_blocking` to prevent blocking async executor
/// - These protections prevent resource exhaustion from malicious templates and parameters
pub async fn render_template(
    template: &PromptTemplate,
    parameters: Option<&HashMap<String, TemplateParamValue>>,
) -> Result<String> {
    // Clone data for spawn_blocking (MiniJinja Environment is not Send)
    let template_content = template.content.clone();
    let template_filename = template.filename.clone();
    let ctx = build_context(template, parameters)?;
    
    // Run rendering in blocking task pool with timeout
    let render_task = tokio::task::spawn_blocking(move || {
        let mut env = Environment::new();
        env.set_auto_escape_callback(|_| minijinja::AutoEscape::None);
        env.add_template(&template_filename, &template_content)?;
        let tmpl = env.get_template(&template_filename)?;
        tmpl.render(ctx)
    });
    
    match timeout(Duration::from_secs(5), render_task).await {
        Ok(Ok(Ok(rendered))) => Ok(rendered),
        Ok(Ok(Err(e))) => Err(e.into()),
        Ok(Err(e)) => Err(anyhow::anyhow!("Render task panicked: {e}")),
        Err(_) => Err(anyhow::anyhow!(
            "Template rendering timed out after 5 seconds. \
             Template may contain infinite loops, deeply nested constructs, \
             or expensive operations. Simplify the template and try again."
        )),
    }
}

/// Build template context from parameters
fn build_context(
    template: &PromptTemplate,
    parameters: Option<&HashMap<String, TemplateParamValue>>,
) -> Result<minijinja::Value> {
    let params = parameters.unwrap_or(&EMPTY_PARAMS);

    // 🔒 SECURITY: Validate parameter sizes FIRST (before any processing)
    validate_parameter_sizes(params)?;

    // Validate parameters against definitions
    validate_parameters(template, params)?;

    // Apply defaults for missing optional parameters
    let mut params_with_defaults = apply_defaults(template, params);

    // Add environment variables
    add_env_vars(&mut params_with_defaults);

    Ok(minijinja::Value::from_serialize(&params_with_defaults))
}

/// Match environment variable name against a glob-style pattern
///
/// Patterns:
/// - "*" matches all
/// - "PREFIX*" matches names starting with PREFIX
/// - "*SUFFIX" matches names ending with SUFFIX  
/// - "*MIDDLE*" matches names containing MIDDLE
/// - "EXACT" matches exact name
fn matches_env_pattern(var_name: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    if pattern.starts_with('*') && pattern.ends_with('*') {
        // *MIDDLE* - contains
        if let Some(stripped) = pattern.strip_prefix('*').and_then(|s| s.strip_suffix('*')) {
            var_name.contains(stripped)
        } else {
            false
        }
    } else if let Some(suffix) = pattern.strip_prefix('*') {
        // *SUFFIX - ends with
        var_name.ends_with(suffix)
    } else if let Some(prefix) = pattern.strip_suffix('*') {
        // PREFIX* - starts with
        var_name.starts_with(prefix)
    } else {
        // EXACT - exact match
        var_name == pattern
    }
}

/// Load allowed environment variables from `KODEGEN_ALLOWED_ENV_VARS`
/// Format: Colon-separated on Unix/macOS, semicolon-separated on Windows
/// Default: Common safe variables (USER, HOME, SHELL, PWD, EDITOR, TERM, USERNAME, USERPROFILE)
fn load_allowed_env_vars_from_env() -> Vec<String> {
    let separator = if cfg!(windows) { ';' } else { ':' };

    match std::env::var("KODEGEN_ALLOWED_ENV_VARS") {
        Ok(custom) if !custom.is_empty() => {
            custom.split(separator)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        }
        _ => {
            // Default safe variables (Unix + Windows equivalents)
            vec![
                "USER", "HOME", "SHELL", "PWD", "EDITOR", "TERM",
                "USERNAME", "USERPROFILE", "HOMEDRIVE", "HOMEPATH"
            ]
            .into_iter()
            .map(String::from)
            .collect()
        }
    }
}

/// Load blocked environment variables from `KODEGEN_BLOCKED_ENV_VARS`
/// Format: Colon-separated on Unix/macOS, semicolon-separated on Windows
/// Default: Common sensitive patterns (*_SECRET, *_PASSWORD, *_TOKEN, *_KEY, etc.)
fn load_blocked_env_vars_from_env() -> Vec<String> {
    let separator = if cfg!(windows) { ';' } else { ':' };

    match std::env::var("KODEGEN_BLOCKED_ENV_VARS") {
        Ok(custom) => {
            // If explicitly set (even to empty), use it
            if custom.is_empty() {
                vec![] // User disabled blocking
            } else {
                custom.split(separator)
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            }
        }
        _ => {
            // Default: Block common sensitive patterns
            vec![
                "*_SECRET", "*SECRET*",
                "*_PASSWORD", "*PASSWORD*",
                "*_TOKEN", "*TOKEN*",
                "*_KEY", "*KEY*",
                "*_CREDENTIAL", "*CREDENTIAL*",
                "*_AUTH", "*AUTH*",
                "AWS_SECRET_ACCESS_KEY",
                "GITHUB_TOKEN",
                "DATABASE_PASSWORD",
            ]
            .into_iter()
            .map(String::from)
            .collect()
        }
    }
}

/// Add environment variables to template context
///
/// Security model:
/// 1. Blocklist is checked FIRST (takes precedence)
/// 2. Then allowlist is checked
/// 3. Supports glob patterns (*, PREFIX*, *SUFFIX, *MIDDLE*)
fn add_env_vars(params: &mut HashMap<String, TemplateParamValue>) {
    let allowed_patterns = load_allowed_env_vars_from_env();
    let blocked_patterns = load_blocked_env_vars_from_env();

    let safe_env_vars: Vec<String> = std::env::vars()
        .filter(|(key, _)| {
            // STEP 1: Check blocklist first (takes precedence)
            let is_blocked = blocked_patterns
                .iter()
                .any(|pattern| matches_env_pattern(key, pattern));

            if is_blocked {
                return false;
            }

            // STEP 2: Check allowlist
            allowed_patterns
                .iter()
                .any(|pattern| matches_env_pattern(key, pattern))
        })
        .map(|(k, v)| format!("{k}={v}"))
        .collect();

    // Add as env array in context (key=value format)
    params.insert("env".to_string(), TemplateParamValue::StringArray(safe_env_vars));
}

/// Get the byte size of a TemplateParamValue
fn param_value_size(value: &TemplateParamValue) -> usize {
    match value {
        TemplateParamValue::String(s) => s.len(),
        TemplateParamValue::Number(_) => 8, // f64
        TemplateParamValue::Bool(_) => 1,
        TemplateParamValue::StringArray(arr) => arr.iter().map(|s| s.len()).sum(),
    }
}

/// Validate parameter sizes to prevent resource exhaustion
fn validate_parameter_sizes(params: &HashMap<String, TemplateParamValue>) -> Result<()> {
    let max_param_size = get_max_param_size();
    let max_param_count = get_max_param_count();
    let max_total_size = get_max_total_params_size();

    // Check parameter count
    if params.len() > max_param_count {
        anyhow::bail!(
            "Too many parameters: {} (max {})\n\
             Consider: Reducing number of parameters or setting KODEGEN_MAX_PARAM_COUNT",
            params.len(),
            max_param_count
        );
    }

    // Check individual parameter sizes and total
    let mut total_size = 0;
    for (name, value) in params {
        let param_size = param_value_size(value);

        // Check individual parameter size
        if param_size > max_param_size {
            anyhow::bail!(
                "Parameter '{name}' is too large: {param_size} bytes (max {max_param_size} bytes / 1 MB)\n\
                 \n\
                 Consider:\n\
                 - Splitting data into smaller parameters\n\
                 - Using file references instead of inline data\n\
                 - Setting KODEGEN_MAX_PARAM_SIZE environment variable if this is legitimate"
            );
        }

        total_size += param_size;
    }

    // Check total size
    if total_size > max_total_size {
        anyhow::bail!(
            "Total parameter size too large: {total_size} bytes (max {max_total_size} bytes / 10 MB)\n\
             \n\
             Consider:\n\
             - Reducing parameter sizes\n\
             - Removing unnecessary parameters\n\
             - Setting KODEGEN_MAX_TOTAL_PARAMS_SIZE environment variable"
        );
    }

    Ok(())
}

/// Validate provided parameters match definitions
fn validate_parameters(
    template: &PromptTemplate,
    params: &HashMap<String, TemplateParamValue>,
) -> Result<()> {
    // Check required parameters are present
    for param_def in &template.metadata.parameters {
        if param_def.required && !params.contains_key(&param_def.name) {
            anyhow::bail!(
                "Required parameter '{}' not provided. Description: {}",
                param_def.name,
                param_def.description
            );
        }
    }

    // Validate types for provided parameters
    for param_def in &template.metadata.parameters {
        if let Some(value) = params.get(&param_def.name) {
            validate_parameter_type(param_def, value)?;
        }
    }

    Ok(())
}

/// Validate a parameter value matches its expected type
fn validate_parameter_type(
    param_def: &super::metadata::ParameterDefinition,
    value: &TemplateParamValue,
) -> Result<()> {
    let valid = match (&param_def.param_type, value) {
        (ParameterType::String, TemplateParamValue::String(_)) => true,
        (ParameterType::Number, TemplateParamValue::Number(_)) => true,
        (ParameterType::Boolean, TemplateParamValue::Bool(_)) => true,
        (ParameterType::Array, TemplateParamValue::StringArray(_)) => true,
        _ => false,
    };

    if !valid {
        let actual_type = match value {
            TemplateParamValue::String(_) => "string",
            TemplateParamValue::Number(_) => "number",
            TemplateParamValue::Bool(_) => "boolean",
            TemplateParamValue::StringArray(_) => "array",
        };
        anyhow::bail!(
            "Parameter '{}' has wrong type. Expected {:?}, got {}",
            param_def.name,
            param_def.param_type,
            actual_type
        );
    }

    Ok(())
}

/// Apply default values for missing optional parameters
fn apply_defaults(
    template: &PromptTemplate,
    params: &HashMap<String, TemplateParamValue>,
) -> HashMap<String, TemplateParamValue> {
    // Pre-allocate capacity for provided params + potential defaults
    let capacity = params.len() + template.metadata.parameters.len();
    let mut result = HashMap::with_capacity(capacity);

    // Copy all provided parameters
    for (key, value) in params {
        result.insert(key.clone(), value.clone());
    }

    // Add defaults for missing optional parameters
    for param_def in &template.metadata.parameters {
        if !result.contains_key(&param_def.name)
            && let Some(default) = param_def.default.as_ref()
        {
            result.insert(param_def.name.clone(), default.clone());
        }
    }

    result
}
