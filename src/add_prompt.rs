use super::manager::PromptManager;
use super::template::parse_template;
use kodegen_mcp_schema::{McpError, Tool, ToolExecutionContext, ToolResponse};
use kodegen_mcp_schema::prompt::{AddPromptArgs, PromptAddOutput, PromptAddPrompts, PROMPT_ADD};

#[derive(Clone)]
pub struct AddPromptTool {
    manager: PromptManager,
}

impl AddPromptTool {
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

impl Tool for AddPromptTool {
    type Args = AddPromptArgs;
    type Prompts = PromptAddPrompts;

    fn name() -> &'static str {
        PROMPT_ADD
    }

    fn description() -> &'static str {
        "Create a new prompt template. The content must include YAML frontmatter with metadata \
         (title, description, categories, author) followed by the template body. Template syntax \
         is validated before saving. Environment variables are accessible via {{ env.VAR }}. \
         Parameters can be defined in frontmatter and used via {{ param_name }}."
    }

    fn read_only() -> bool {
        false
    }

    fn destructive() -> bool {
        false // Creates new file, doesn't modify existing
    }

    fn idempotent() -> bool {
        false // Will fail if prompt already exists
    }

    async fn execute(&self, args: Self::Args, _ctx: ToolExecutionContext) -> Result<ToolResponse<<Self::Args as kodegen_mcp_schema::ToolArgs>::Output>, McpError> {
        // Parse template to extract metadata (for output formatting)
        let template = parse_template(&args.name, &args.content)
            .map_err(McpError::Other)?;

        // Extract statistics
        let param_count = template.metadata.parameters.len();
        let template_length = template.content.len();

        // Add prompt (validates syntax automatically, async)
        self.manager
            .add_prompt(&args.name, &args.content)
            .await
            .map_err(McpError::Other)?;

        let path = format!("~/.kodegen/prompts/{}.j2.md", args.name);

        // Terminal summary
        let summary = format!(
            "\x1b[32m Prompt Added: {}\x1b[0m\n\
              Template length: {} · Parameters: {}",
            args.name,
            template_length,
            param_count
        );

        // Typed output
        let output = PromptAddOutput {
            success: true,
            name: args.name.clone(),
            message: format!("Prompt '{}' created successfully", args.name),
            path: Some(path),
            template_length: Some(template_length),
            parameter_count: Some(param_count),
        };

        Ok(ToolResponse::new(summary, output))
    }
}
