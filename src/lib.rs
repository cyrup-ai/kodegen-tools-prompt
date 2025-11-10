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

/// Start the prompt tools HTTP server programmatically.
///
/// This function is designed to be called from kodegend for embedded server mode.
/// It replicates the logic from main.rs but as a library function.
///
/// # Arguments
/// * `addr` - The socket address to bind to
/// * `tls_cert` - Optional path to TLS certificate file
/// * `tls_key` - Optional path to TLS private key file
///
/// # Returns
/// Returns `Ok(())` when the server shuts down gracefully, or an error if startup/shutdown fails.
pub async fn start_server(
    addr: std::net::SocketAddr,
    tls_cert: Option<std::path::PathBuf>,
    tls_key: Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    use kodegen_server_http::{Managers, RouterSet, register_tool};
    use kodegen_tools_config::ConfigManager;
    use rmcp::handler::server::router::{prompt::PromptRouter, tool::ToolRouter};
    use std::sync::Arc;

    let _ = env_logger::try_init();
    
    let config = ConfigManager::new();
    config.init().await?;
    
    let timestamp = chrono::Utc::now();
    let pid = std::process::id();
    let instance_id = format!("{}-{}", timestamp.format("%Y%m%d-%H%M%S-%9f"), pid);
    kodegen_mcp_tool::tool_history::init_global_history(instance_id.clone()).await;
    
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
    
    let router_set = RouterSet::new(tool_router, prompt_router, managers);
    
    let session_config = rmcp::transport::streamable_http_server::session::local::SessionConfig {
        channel_capacity: 16,
        keep_alive: Some(std::time::Duration::from_secs(3600)),
    };
    let session_manager = Arc::new(
        rmcp::transport::streamable_http_server::session::local::LocalSessionManager {
            sessions: Default::default(),
            session_config,
        }
    );
    
    let usage_tracker = kodegen_utils::usage_tracker::UsageTracker::new(
        format!("prompt-{}", instance_id)
    );
    
    let server = kodegen_server_http::HttpServer::new(
        router_set.tool_router,
        router_set.prompt_router,
        usage_tracker,
        config,
        router_set.managers,
        session_manager,
    );
    
    let shutdown_timeout = std::time::Duration::from_secs(30);
    let tls_config = tls_cert.zip(tls_key);
    let handle = server.serve_with_tls(addr, tls_config, shutdown_timeout).await?;
    
    handle.wait_for_completion(shutdown_timeout).await
        .map_err(|e| anyhow::anyhow!("Server shutdown error: {}", e))?;
    
    Ok(())
}
