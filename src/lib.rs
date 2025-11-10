mod defaults;
pub mod manager;
pub mod metadata;
pub mod template;
pub mod validation;

pub mod add_prompt;
pub use add_prompt::*;

pub mod edit_prompt;
pub use edit_prompt::*;

pub mod delete_prompt;
pub use delete_prompt::*;

pub mod get_prompt;
pub use get_prompt::*;

// Re-export commonly used types
pub use manager::PromptManager;
pub use metadata::{ParameterDefinition, ParameterType, PromptMetadata, PromptTemplate};

/// Start the prompt tools HTTP server programmatically
///
/// Returns a ServerHandle for graceful shutdown control.
/// This function is non-blocking - the server runs in background tasks.
///
/// # Arguments
/// * `addr` - Socket address to bind to (e.g., "127.0.0.1:30446")
/// * `tls_cert` - Optional path to TLS certificate file
/// * `tls_key` - Optional path to TLS private key file
///
/// # Returns
/// ServerHandle for graceful shutdown, or error if startup fails
pub async fn start_server(
    addr: std::net::SocketAddr,
    tls_cert: Option<std::path::PathBuf>,
    tls_key: Option<std::path::PathBuf>,
) -> anyhow::Result<kodegen_server_http::ServerHandle> {
    use kodegen_server_http::{create_http_server, Managers, RouterSet, register_tool};
    use rmcp::handler::server::router::{prompt::PromptRouter, tool::ToolRouter};
    use std::time::Duration;

    let tls_config = match (tls_cert, tls_key) {
        (Some(cert), Some(key)) => Some((cert, key)),
        _ => None,
    };

    let shutdown_timeout = Duration::from_secs(30);

    create_http_server("prompt", addr, tls_config, shutdown_timeout, |_config, _tracker| {
        Box::pin(async move {
            let mut tool_router = ToolRouter::new();
            let mut prompt_router = PromptRouter::new();
            let managers = Managers::new();

            // Initialize PromptManager (clean async initialization)
            let manager = crate::PromptManager::new();
            manager.init().await?;

            // Register all 4 prompt management tools with shared manager
            (tool_router, prompt_router) = register_tool(
                tool_router,
                prompt_router,
                crate::AddPromptTool::with_manager(manager.clone()),
            );
            (tool_router, prompt_router) = register_tool(
                tool_router,
                prompt_router,
                crate::EditPromptTool::with_manager(manager.clone()),
            );
            (tool_router, prompt_router) = register_tool(
                tool_router,
                prompt_router,
                crate::DeletePromptTool::with_manager(manager.clone()),
            );
            (tool_router, prompt_router) = register_tool(
                tool_router,
                prompt_router,
                crate::GetPromptTool::with_manager(manager.clone()),
            );

            Ok(RouterSet::new(tool_router, prompt_router, managers))
        })
    }).await
}
