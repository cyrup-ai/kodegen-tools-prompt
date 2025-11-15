use super::manager::PromptManager;
use kodegen_mcp_tool::Tool;
use kodegen_mcp_tool::error::McpError;
use kodegen_mcp_schema::prompt::{EditPromptArgs, EditPromptPromptArgs, PROMPT_EDIT};
use rmcp::model::{Content, PromptArgument, PromptMessage, PromptMessageContent, PromptMessageRole};
use serde_json::json;

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
    type PromptArgs = EditPromptPromptArgs;

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

    async fn execute(&self, args: Self::Args) -> Result<Vec<Content>, McpError> {
        let start = std::time::Instant::now();
        
        // Edit prompt (validates syntax automatically, async)
        self.manager
            .edit_prompt(&args.name, &args.content)
            .await
            .map_err(McpError::Other)?;

        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        let mut contents = Vec::new();

        // 1. TERMINAL SUMMARY
        let summary = format!(
            "✓ Prompt '{}' updated successfully\n\n\
             Path: ~/.kodegen/prompts/{}.j2.md\n\
             Elapsed: {:.0}ms",
            args.name, args.name, elapsed_ms
        );
        contents.push(Content::text(summary));

        // 2. JSON METADATA
        let metadata = json!({
            "success": true,
            "name": args.name,
            "elapsed_ms": elapsed_ms,
            "message": format!("Prompt '{}' updated successfully", args.name)
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
                content: PromptMessageContent::text("How do I edit an existing prompt?"),
            },
            PromptMessage {
                role: PromptMessageRole::Assistant,
                content: PromptMessageContent::text(
                    "Use prompt_edit to update an existing prompt template:\n\n\
                     1. First, get the current content:\n\
                     ```\n\
                     prompt_get({\"action\": \"get\", \"name\": \"my_prompt\"})\n\
                     ```\n\n\
                     2. Then edit it:\n\
                     ```\n\
                     prompt_edit({\n\
                       \"name\": \"my_prompt\",\n\
                       \"content\": \"---\\n\
                     title: \\\"Updated Title\\\"\\n\
                     description: \\\"Updated description\\\"\\n\
                     categories: [\\\"custom\\\"]\\n\
                     author: \\\"your-name\\\"\\n\
                     ---\\n\
                     \\n\
                     Updated template content here\\n\
                     \\\"\n\
                     })\n\
                     ```\n\n\
                     The new content completely replaces the old content. \
                     Template syntax is validated before saving.",
                ),
            },
        ])
    }
}
