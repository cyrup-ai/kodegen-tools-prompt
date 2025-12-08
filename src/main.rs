// Category HTTP Server: Prompt Tools
//
// This binary serves prompt template management tools over HTTP/HTTPS transport.
// Managed by kodegend daemon, typically running on port kodegen_config::PORT_PROMPT (30449).

use anyhow::Result;
use kodegen_config::CATEGORY_PROMPT;
use kodegen_server_http::{ServerBuilder, Managers, RouterSet, register_tool};
use rmcp::handler::server::router::{prompt::PromptRouter, tool::ToolRouter};

#[tokio::main]
async fn main() -> Result<()> {
    ServerBuilder::new()
        .category(CATEGORY_PROMPT)
        .register_tools(|| async {
            let mut tool_router = ToolRouter::new();
            let mut prompt_router = PromptRouter::new();
            let managers = Managers::new();

            // Initialize PromptManager (clean async initialization)
            let manager = kodegen_tools_prompt::PromptManager::new();
            manager.init().await?;

            // Register all 4 prompt management tools with shared manager
            use kodegen_tools_prompt::*;

            (tool_router, prompt_router) = register_tool(
                tool_router,
                prompt_router,
                AddPromptTool::with_manager(manager.clone()),
            );
            (tool_router, prompt_router) = register_tool(
                tool_router,
                prompt_router,
                EditPromptTool::with_manager(manager.clone()),
            );
            (tool_router, prompt_router) = register_tool(
                tool_router,
                prompt_router,
                DeletePromptTool::with_manager(manager.clone()),
            );
            (tool_router, prompt_router) = register_tool(
                tool_router,
                prompt_router,
                GetPromptTool::with_manager(manager.clone()),
            );

            Ok(RouterSet::new(tool_router, prompt_router, managers))
        })
        .run()
        .await
}
