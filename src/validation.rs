use anyhow::Result;
use minijinja::Environment;
use lazy_static::lazy_static;
use regex::Regex;

/// Maximum template size in bytes (1MB)
const MAX_TEMPLATE_SIZE: usize = 1_000_000;

/// Validate `MiniJinja` template syntax
pub fn validate_template_syntax(content: &str) -> Result<()> {
    let mut env = Environment::new();

    // Try to add template - will fail if syntax invalid
    env.add_template("_validation", content)
        .map_err(|e| anyhow::anyhow!("Template syntax error: {e}"))?;

    Ok(())
}

/// Validate complete prompt file (metadata + content)
pub fn validate_prompt_file(content: &str) -> Result<()> {
    // Validate size first (security: prevent resource exhaustion)
    if content.len() > MAX_TEMPLATE_SIZE {
        anyhow::bail!(
            "Template too large ({} bytes). Maximum size is {} bytes (1MB).",
            content.len(),
            MAX_TEMPLATE_SIZE
        );
    }

    // Parse to ensure valid structure
    let template = super::template::parse_template("_validation", content)?;

    // Validate template syntax
    validate_template_syntax(&template.content)?;

    // Additional checks
    validate_no_dangerous_operations(&template.content)?;

    Ok(())
}

lazy_static! {
    /// Matches {% include with any whitespace control and spacing
    /// Pattern: {%[-+]?\s*include\s+
    /// - {%      = literal opening tag
    /// - [-+]?   = optional whitespace control (-, +)
    /// - \s*     = zero or more whitespace (spaces, tabs, newlines)
    /// - include = directive name
    /// - \s+     = required whitespace after directive
    static ref INCLUDE_PATTERN: Regex =
        Regex::new(r"\{%[-+]?\s*include\s+")
            .expect("Failed to compile include pattern");
    
    static ref EXTENDS_PATTERN: Regex =
        Regex::new(r"\{%[-+]?\s*extends\s+")
            .expect("Failed to compile extends pattern");
    
    static ref IMPORT_PATTERN: Regex =
        Regex::new(r"\{%[-+]?\s*import\s+")
            .expect("Failed to compile import pattern");
    
    /// Matches {% from for from-import statements
    static ref FROM_IMPORT_PATTERN: Regex =
        Regex::new(r"\{%[-+]?\s*from\s+")
            .expect("Failed to compile from import pattern");
}

/// Check for dangerous template operations
/// Based on security policy and runtime constraints (no loader configured)
fn validate_no_dangerous_operations(content: &str) -> Result<()> {
    // Block include directives (file access)
    if INCLUDE_PATTERN.is_match(content) {
        anyhow::bail!(
            "Template contains forbidden 'include' directive. \
             File inclusion is not allowed for security reasons."
        );
    }

    // Block extends directives (template inheritance)
    if EXTENDS_PATTERN.is_match(content) {
        anyhow::bail!(
            "Template contains forbidden 'extends' directive. \
             Template inheritance is not supported."
        );
    }

    // Block import directives (module loading)
    if IMPORT_PATTERN.is_match(content) || FROM_IMPORT_PATTERN.is_match(content) {
        anyhow::bail!(
            "Template contains forbidden 'import' directive. \
             Module imports are not allowed."
        );
    }

    Ok(())
}
