use super::defaults;
use super::metadata::PromptTemplate;
use super::template::{parse_template, render_template};
use anyhow::{Context, Result};
use kodegen_mcp_tool::error::McpError;
use log::{info, warn};
use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::fs;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;

/// Cached template with file modification time for validation
struct CachedTemplate {
    template: PromptTemplate,
    file_mtime: SystemTime,
}

#[derive(Clone)]
pub struct PromptManager {
    prompts_dir: PathBuf,
    cache: Arc<RwLock<HashMap<String, CachedTemplate>>>,
}

impl Default for PromptManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptManager {
    /// Create new prompt manager (synchronous constructor)
    #[must_use]
    pub fn new() -> Self {
        let prompts_dir =
            get_prompts_directory().unwrap_or_else(|_| PathBuf::from(".kodegen/prompts"));
        Self {
            prompts_dir,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Initialize the prompt manager (async initialization)
    ///
    /// Call this after `new()` to perform async setup operations.
    pub async fn init(&self) -> Result<(), McpError> {
        // Ensure directory exists (async)
        fs::create_dir_all(&self.prompts_dir)
            .await
            .with_context(|| {
                format!(
                    "Failed to create prompts directory: {}",
                    self.prompts_dir.display()
                )
            })
            .map_err(McpError::Other)?;

        // Initialize default prompts if directory is empty (async)
        if let Err(e) = initialize_default_prompts(&self.prompts_dir).await {
            warn!("Failed to initialize default prompts: {e}");
            // Don't fail - user can add prompts manually
        }

        Ok(())
    }

    /// List all available prompts (async)
    pub async fn list_prompts(&self) -> Result<Vec<PromptTemplate>> {
        let mut prompts = Vec::new();

        let mut entries = fs::read_dir(&self.prompts_dir).await.with_context(|| {
            format!(
                "Failed to read prompts directory: {}",
                self.prompts_dir.display()
            )
        })?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            // Check if it's a .j2.md or .md file and try to load it
            if let Some(ext) = path.extension().and_then(|s| s.to_str())
                && ext == "md"
                && let Some(filename) = path.file_stem().and_then(|s| s.to_str())
            {
                // Try to load, but don't fail entire list if one is invalid
                match self.load_prompt(filename).await {
                    Ok(template) => prompts.push(template),
                    Err(e) => {
                        warn!("Failed to load prompt '{filename}': {e}");
                    }
                }
            }
        }

        Ok(prompts)
    }

    /// Load a specific prompt by filename (async)
    pub async fn load_prompt(&self, name: &str) -> Result<PromptTemplate> {
        // Validate name to prevent path traversal
        validate_prompt_name(name)?;

        let path = self.prompts_dir.join(format!("{name}.j2.md"));

        // Step 1: Check cache with read lock (allows concurrent reads)
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(name) {
                // Verify file hasn't been modified since caching
                if let Ok(current_meta) = fs::metadata(&path).await
                    && let Ok(current_mtime) = current_meta.modified()
                        && current_mtime == cached.file_mtime {
                            // Cache hit: file unchanged, return cached template
                            return Ok(cached.template.clone());
                        }
                // Cache stale: file modified, fall through to reload
            }
            // Cache miss: template not cached, fall through to load
        } // Read lock dropped here

        // Step 2: Cache miss or stale - load from disk
        let content = fs::read_to_string(&path)
            .await
            .with_context(|| format!("Failed to read prompt: {name}"))?;

        let metadata = fs::metadata(&path).await?;
        let file_mtime = metadata.modified()?;
        let template = parse_template(name, &content)?;

        // Step 3: Update cache with write lock
        {
            let mut cache = self.cache.write().await;
            cache.insert(
                name.to_string(),
                CachedTemplate {
                    template: template.clone(),
                    file_mtime,
                },
            );
        } // Write lock dropped here

        Ok(template)
    }

    /// Save a new prompt (async)
    pub async fn add_prompt(&self, name: &str, content: &str) -> Result<()> {
        // Validate name (prevent path traversal)
        validate_prompt_name(name)?;
        
        // Validate content syntax
        super::validation::validate_prompt_file(content)?;

        let path = self.prompts_dir.join(format!("{name}.j2.md"));

        // Atomic create-new operation - fails if file already exists
        match OpenOptions::new()
            .write(true)
            .create_new(true)  // Atomic: fails if file exists
            .open(&path)
            .await
        {
            Ok(mut file) => {
                // File created successfully, write content
                file.write_all(content.as_bytes())
                    .await
                    .with_context(|| format!("Failed to write prompt: {name}"))?;
                
                file.flush()
                    .await
                    .with_context(|| format!("Failed to flush prompt: {name}"))?;
                
                // Invalidate cache after successful write
                self.invalidate_cache(name).await;
                Ok(())
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                // File already exists - provide friendly error message
                anyhow::bail!("Prompt '{name}' already exists. Use edit_prompt to modify.")
            }
            Err(e) => {
                // Other IO error (permissions, disk full, etc.)
                Err(e).with_context(|| format!("Failed to create prompt: {name}"))?
            }
        }
    }

    /// Update an existing prompt (async)
    pub async fn edit_prompt(&self, name: &str, content: &str) -> Result<()> {
        // Validate name
        validate_prompt_name(name)?;

        // Validate content first
        super::validation::validate_prompt_file(content)?;

        let path = self.prompts_dir.join(format!("{name}.j2.md"));

        // Check exists (async)
        if !fs::try_exists(&path).await.unwrap_or(false) {
            anyhow::bail!("Prompt '{name}' not found. Use add_prompt to create.");
        }

        fs::write(&path, content)
            .await
            .with_context(|| format!("Failed to update prompt: {name}"))?;

        self.invalidate_cache(name).await;
        Ok(())
    }

    /// Delete a prompt (async)
    pub async fn delete_prompt(&self, name: &str) -> Result<()> {
        // Validate name
        validate_prompt_name(name)?;

        let path = self.prompts_dir.join(format!("{name}.j2.md"));

        // Check exists (async)
        if !fs::try_exists(&path).await.unwrap_or(false) {
            anyhow::bail!("Prompt '{name}' not found");
        }

        fs::remove_file(&path)
            .await
            .with_context(|| format!("Failed to delete prompt: {name}"))?;

        self.invalidate_cache(name).await;
        Ok(())
    }

    /// Render a prompt with parameters (async)
    pub async fn render_prompt(
        &self,
        name: &str,
        parameters: Option<HashMap<String, serde_json::Value>>,
    ) -> Result<String> {
        let template = self.load_prompt(name).await?;
        render_template(&template, parameters.as_ref())
    }

    /// Invalidate cached entry for a specific prompt
    async fn invalidate_cache(&self, name: &str) {
        let mut cache = self.cache.write().await;
        cache.remove(name);
    }
}

/// Get the prompts directory path
fn get_prompts_directory() -> Result<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;

    Ok(home.join(".kodegen").join("prompts"))
}

/// Validate prompt name to prevent path traversal
fn validate_prompt_name(name: &str) -> Result<()> {
    // Only alphanumeric, hyphen, underscore
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        anyhow::bail!(
            "Invalid prompt name: '{name}'. Only alphanumeric characters, hyphens, and underscores allowed."
        );
    }

    // No path traversal
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        anyhow::bail!("Invalid prompt name: '{name}'. Path separators and '..' not allowed.");
    }

    Ok(())
}

/// Initialize default prompts on first run (async)
async fn initialize_default_prompts(prompts_dir: &Path) -> Result<()> {
    // Check if directory has any .j2.md or .md files
    let mut entries = fs::read_dir(prompts_dir).await?;
    let mut has_prompts = false;

    while let Some(entry) = entries.next_entry().await? {
        if entry
            .path()
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|ext| ext == "md")
        {
            has_prompts = true;
            break;
        }
    }

    if !has_prompts {
        // Write default prompts from embedded data
        defaults::write_default_prompts(prompts_dir).await?;
        info!(
            "Initialized {} default prompts in {}",
            defaults::DEFAULT_PROMPTS.len(),
            prompts_dir.display()
        );
    }

    Ok(())
}
