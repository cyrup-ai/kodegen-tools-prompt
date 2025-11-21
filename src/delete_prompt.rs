use super::manager::PromptManager;
use kodegen_mcp_tool::{Tool, ToolExecutionContext};
use kodegen_mcp_tool::error::McpError;
use kodegen_mcp_schema::prompt::{DeletePromptArgs, DeletePromptPromptArgs, PROMPT_DELETE};
use rmcp::model::{Content, PromptArgument, PromptMessage, PromptMessageContent, PromptMessageRole};
use serde_json::json;

#[derive(Clone)]
pub struct DeletePromptTool {
    manager: PromptManager,
}

impl DeletePromptTool {
    /// Create with a pre-initialized PromptManager (for HTTP server)
    pub fn with_manager(manager: PromptManager) -> Self {
        Self { manager }
    }

    /// Create with default manager (for standalone use)
    pub async fn new() -> Result<Self, McpError> {
        let manager = PromptManager::new();
        manager.init().await?;
        Ok(Self { manager })
    }
}

impl Tool for DeletePromptTool {
    type Args = DeletePromptArgs;
    type PromptArgs = DeletePromptPromptArgs;

    fn name() -> &'static str {
        PROMPT_DELETE
    }

    fn description() -> &'static str {
        "Delete a prompt template. Requires confirm=true for safety. This action cannot be undone. \
         Default prompts can be deleted but will be recreated on next initialization."
    }

    fn read_only() -> bool {
        false
    }

    fn destructive() -> bool {
        true // Deletes file
    }

    fn idempotent() -> bool {
        false // Second deletion will fail (file gone)
    }

    async fn execute(&self, args: Self::Args, _ctx: ToolExecutionContext) -> Result<Vec<Content>, McpError> {
        if !args.confirm {
            return Err(McpError::InvalidArguments(
                "Must set confirm=true to delete a prompt".into(),
            ));
        }

        let start = std::time::Instant::now();
        
        self.manager
            .delete_prompt(&args.name)
            .await
            .map_err(McpError::Other)?;

        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        let mut contents = Vec::new();

        // 1. TERMINAL SUMMARY
        let summary = format!(
            "\x1b[31m󰜑 Prompt Deleted: {}\x1b[0m\n\
             \x1b[31m󰄳 Status: removed\x1b[0m",
            args.name
        );
        contents.push(Content::text(summary));

        // 2. JSON METADATA
        let metadata = json!({
            "success": true,
            "name": args.name,
            "elapsed_ms": elapsed_ms,
            "message": format!("Prompt '{}' deleted successfully", args.name)
        });
        let json_str = serde_json::to_string_pretty(&metadata)
            .unwrap_or_else(|_| "{}".to_string());
        contents.push(Content::text(json_str));

        Ok(contents)
    }

    fn prompt_arguments() -> Vec<PromptArgument> {
        vec![]
    }

    async fn prompt(&self, _args: Self::PromptArgs) -> Result<Vec<PromptMessage>, McpError> {
        Ok(vec![
            PromptMessage {
                role: PromptMessageRole::User,
                content: PromptMessageContent::text("How do I delete a prompt?"),
            },
            PromptMessage {
                role: PromptMessageRole::Assistant,
                content: PromptMessageContent::text(
                    "Use prompt_delete to remove a prompt template:\n\n\
                     Example:\n\
                     ```\n\
                     prompt_delete({\n\
                       \"name\": \"my_prompt\",\n\
                       \"confirm\": true\n\
                     })\n\
                     ```\n\n\
                     IMPORTANT:\n\
                     - You must set confirm=true for safety\n\
                     - This action is permanent and cannot be undone\n\
                     - Default prompts will be recreated on next initialization if deleted\n\n\
                     The deletion will fail if:\n\
                     - confirm is not true\n\
                     - The prompt does not exist",
                ),
            },
        ])
    }
}
