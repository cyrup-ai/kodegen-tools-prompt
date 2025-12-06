use super::manager::PromptManager;
use super::template::parse_template;
use kodegen_mcp_schema::{McpError, Tool, ToolExecutionContext, ToolArgs, ToolResponse};
use kodegen_mcp_schema::prompt::{EditPromptArgs, PromptEditOutput, PromptEditPrompts, PROMPT_EDIT};

#[derive(Clone)]
pub struct EditPromptTool {
    manager: PromptManager,
}

impl EditPromptTool {
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

impl Tool for EditPromptTool {
    type Args = EditPromptArgs;
    type Prompts = PromptEditPrompts;

    fn name() -> &'static str {
        PROMPT_EDIT
    }

    fn description() -> &'static str {
        "Edit an existing prompt template. Provide the prompt name and complete new content \
         (including YAML frontmatter). The content is validated before saving. Use get_prompt \
         to retrieve current content before editing."
    }

    fn read_only() -> bool {
        false
    }

    fn destructive() -> bool {
        true // Modifies existing file
    }

    fn idempotent() -> bool {
        true // Same content produces same result
    }

    async fn execute(&self, args: Self::Args, _ctx: ToolExecutionContext) -> Result<ToolResponse<<Self::Args as ToolArgs>::Output>, McpError> {
        // Edit prompt (validates syntax automatically, async)
        self.manager
            .edit_prompt(&args.name, &args.content)
            .await
            .map_err(McpError::Other)?;

        // Parse the updated template to extract metadata
        let filename = format!("{}.j2.md", args.name);
        let template = parse_template(&filename, &args.content)
            .map_err(McpError::Other)?;

        // Calculate metrics
        let template_length = args.content.len();
        let parameter_count = template.metadata.parameters.len();

        // Terminal summary
        let summary = format!(
            "\x1b[33m󰆐 Prompt Updated: {}\x1b[0m\n\
             󰢬 Template length: {} · Parameters: {}",
            args.name,
            template_length,
            parameter_count
        );

        let output = PromptEditOutput {
            success: true,
            name: args.name.clone(),
            message: format!("Prompt '{}' updated successfully ({} bytes, {} parameters)", args.name, template_length, parameter_count),
            path: None,
        };

        Ok(ToolResponse::new(summary, output))
    }
}
