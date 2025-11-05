use anyhow::{Context, Result};
use log::debug;
use std::io::ErrorKind;
use std::path::Path;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

/// Default prompts embedded at compile time
pub const DEFAULT_PROMPTS: &[(&str, &str)] = &[
    (
        "onb_001",
        include_str!("../data/default_prompts/onb_001.j2.md"),
    ),
    (
        "code_001",
        include_str!("../data/default_prompts/code_001.j2.md"),
    ),
    (
        "refactor_example",
        include_str!("../data/default_prompts/refactor_example.j2.md"),
    ),
    (
        "env_demo",
        include_str!("../data/default_prompts/env_demo.j2.md"),
    ),
];

/// Write default prompts to the prompts directory
///
/// Only writes prompts that don't already exist (preserves user modifications)
pub async fn write_default_prompts(prompts_dir: &Path) -> Result<()> {
    for (name, content) in DEFAULT_PROMPTS {
        let path = prompts_dir.join(format!("{name}.j2.md"));

        // Atomic create-new: only write if file doesn't exist
        match OpenOptions::new()
            .write(true)
            .create_new(true)  // Fails if file exists - preserves user changes
            .open(&path)
            .await
        {
            Ok(mut file) => {
                // File created successfully, write default content
                file.write_all(content.as_bytes())
                    .await
                    .with_context(|| format!("Failed to write default prompt: {name}"))?;
                
                file.flush()
                    .await
                    .with_context(|| format!("Failed to flush default prompt: {name}"))?;
                
                debug!("Wrote default prompt: {name}");
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                // File exists - skip silently (user has customized it)
                debug!("Skipped default prompt '{name}' (already exists)");
            }
            Err(e) if e.kind() == ErrorKind::PermissionDenied => {
                // Permission error - propagate with helpful context
                return Err(e).with_context(|| {
                    format!(
                        "Permission denied writing default prompt '{name}' to {}. \
                         Check directory permissions.",
                        path.display()
                    )
                });
            }
            Err(e) => {
                // Other IO error - propagate with context
                return Err(e).with_context(|| {
                    format!(
                        "Failed to create default prompt '{name}' at {}",
                        path.display()
                    )
                });
            }
        }
    }

    Ok(())
}
