use super::manager::PromptManager;
use kodegen_mcp_schema::{McpError, Tool, ToolExecutionContext, ToolResponse};
use kodegen_mcp_schema::prompt::{DeletePromptArgs, PromptDeleteOutput, PromptDeletePrompts, PROMPT_DELETE};

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
    type Prompts = PromptDeletePrompts;

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

    async fn execute(&self, args: Self::Args, _ctx: ToolExecutionContext) -> Result<ToolResponse<<Self::Args as kodegen_mcp_schema::ToolArgs>::Output>, McpError> {
        if !args.confirm {
            return Err(McpError::InvalidArguments(
                "Must set confirm=true to delete a prompt".into(),
            ));
        }

        self.manager
            .delete_prompt(&args.name)
            .await
            .map_err(McpError::Other)?;

        // Terminal summary
        let summary = format!(
            "\x1b[31m󰜑 Prompt Deleted: {}\x1b[0m\n\
             \x1b[31m󰄳 Status: removed\x1b[0m",
            args.name
        );

        // Typed output
        let output = PromptDeleteOutput {
            success: true,
            name: args.name.clone(),
            message: format!("Prompt '{}' deleted successfully", args.name),
        };

        Ok(ToolResponse::new(summary, output))
    }
}
