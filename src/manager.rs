use super::defaults;
use super::metadata::PromptTemplate;
use super::template::{parse_template, render_template};
use anyhow::{Context, Result};
use kodegen_config::KodegenConfig;
use kodegen_mcp_tool::error::McpError;
use log::{debug, info, warn};
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

            // CHANGE 1: Check file type first (reject symlinks and directories)
            let file_type = match entry.file_type().await {
                Ok(ft) => ft,
                Err(e) => {
                    warn!("Failed to get file type for {}: {e}", path.display());
                    continue;
                }
            };

            // CHANGE 2: Skip non-regular files (directories, symlinks, etc.)
            if !file_type.is_file() {
                debug!("Skipping non-file entry: {}", path.display());
                continue;
            }

            // CHANGE 3: Check for .j2.md extension (not just .md)
            let filename_str = match path.file_name().and_then(|s| s.to_str()) {
                Some(name) if name.ends_with(".j2.md") => name,
                _ => continue, // Skip files that don't match pattern
            };

            // CHANGE 4: Extract stem by removing ".j2.md" suffix (6 chars)
            let stem = &filename_str[..filename_str.len() - 6];

            // Validate prompt name before attempting load (reuses existing validation)
            if !is_valid_prompt_name(stem) {
                warn!("Invalid prompt filename (skipping): {stem}");
                continue;
            }

            // Load prompt (now guaranteed to be safe, regular file)
            match self.load_prompt(stem).await {
                Ok(template) => prompts.push(template),
                Err(e) => {
                    warn!("Failed to load prompt '{stem}': {e}");
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
                
                // Sync to disk for durability (survive power loss)
                file.sync_all()
                    .await
                    .with_context(|| format!("Failed to sync prompt to disk: {name}"))?;
                
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
        validate_prompt_name(name)?;
        super::validation::validate_prompt_file(content)?;

        let path = self.prompts_dir.join(format!("{name}.j2.md"));

        // Atomic update-only operation - fails if file doesn't exist
        match OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(false)  // CRITICAL: Fail if file doesn't exist (edit-only semantics)
            .open(&path)
            .await
        {
            Ok(mut file) => {
                // Write new content to existing file
                file.write_all(content.as_bytes())
                    .await
                    .with_context(|| format!("Failed to write prompt: {name}"))?;
                
                // Ensure data is flushed to disk
                file.flush()
                    .await
                    .with_context(|| format!("Failed to flush prompt: {name}"))?;
                
                // Sync to disk for durability (survive power loss)
                file.sync_all()
                    .await
                    .with_context(|| format!("Failed to sync prompt to disk: {name}"))?;
                
                // Invalidate cache after successful write
                self.invalidate_cache(name).await;
                Ok(())
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {
                // File doesn't exist - provide helpful error message
                anyhow::bail!("Prompt '{name}' not found. Use add_prompt to create.")
            }
            Err(e) => {
                // Other IO error (permissions, disk full, etc.)
                Err(e).with_context(|| format!("Failed to update prompt: {name}"))?
            }
        }
    }

    /// Delete a prompt (async)
    pub async fn delete_prompt(&self, name: &str) -> Result<()> {
        validate_prompt_name(name)?;

        let path = self.prompts_dir.join(format!("{name}.j2.md"));

        // Attempt delete directly, handle errors appropriately
        match fs::remove_file(&path).await {
            Ok(()) => {
                self.invalidate_cache(name).await;
                Ok(())
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {
                anyhow::bail!("Prompt '{name}' not found")
            }
            Err(e) if e.kind() == ErrorKind::IsADirectory => {
                anyhow::bail!("'{name}' is a directory, not a prompt file")
            }
            Err(e) if e.kind() == ErrorKind::PermissionDenied => {
                anyhow::bail!("Permission denied to delete prompt '{name}'")
            }
            Err(e) => Err(e).with_context(|| format!("Failed to delete prompt: {name}"))?
        }
    }

    /// Render a prompt with parameters (async)
    pub async fn render_prompt(
        &self,
        name: &str,
        parameters: Option<HashMap<String, serde_json::Value>>,
    ) -> Result<String> {
        let template = self.load_prompt(name).await?;
        render_template(&template, parameters.as_ref()).await
    }

    /// Invalidate cached entry for a specific prompt
    async fn invalidate_cache(&self, name: &str) {
        let mut cache = self.cache.write().await;
        cache.remove(name);
    }
}

/// Get the prompts directory path
/// Supports both local (.kodegen/prompts/) and user-global config with precedence
fn get_prompts_directory() -> Result<PathBuf> {
    KodegenConfig::local_config_dir()
        .ok()
        .map(|dir| dir.join("prompts"))
        .or_else(|| {
            KodegenConfig::user_config_dir()
                .ok()
                .map(|dir| dir.join("prompts"))
        })
        .ok_or_else(|| anyhow::anyhow!("Cannot determine prompts directory"))
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

/// Quick validation check for prompt names (inline version for list_prompts)
/// Mirrors the logic in validate_prompt_name() for early filtering
fn is_valid_prompt_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        && !name.contains("..")
}

/// Initialize default prompts on first run (async)
async fn initialize_default_prompts(prompts_dir: &Path) -> Result<()> {
    // Fast check: does the first default prompt exist?
    // If it exists, assume initialization already happened
    let first_default = defaults::DEFAULT_PROMPTS[0].0;
    let path = prompts_dir.join(format!("{first_default}.j2.md"));
    
    // Check existence, propagating errors instead of masking them
    match fs::try_exists(&path).await {
        Ok(true) => {
            // Initialization already complete - return immediately
            return Ok(());
        }
        Ok(false) => {
            // First run: write all default prompts
            defaults::write_default_prompts(prompts_dir).await?;
            
            info!(
                "Initialized {} default prompts in {}",
                defaults::DEFAULT_PROMPTS.len(),
                prompts_dir.display()
            );
        }
        Err(e) => {
            // Permission or IO error checking existence - propagate with context
            return Err(e).with_context(|| {
                format!(
                    "Failed to check if default prompts exist in {}. \
                     Check directory permissions.",
                    prompts_dir.display()
                )
            });
        }
    }
    
    Ok(())
}
