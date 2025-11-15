use super::manager::PromptManager;
use kodegen_mcp_tool::Tool;
use kodegen_mcp_tool::error::McpError;
use kodegen_mcp_schema::prompt::{AddPromptArgs, AddPromptPromptArgs, PROMPT_ADD};
use rmcp::model::{Content, PromptArgument, PromptMessage, PromptMessageContent, PromptMessageRole};
use serde_json::json;

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
    type PromptArgs = AddPromptPromptArgs;

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

    async fn execute(&self, args: Self::Args) -> Result<Vec<Content>, McpError> {
        let start = std::time::Instant::now();
        
        // Add prompt (validates syntax automatically, async)
        self.manager
            .add_prompt(&args.name, &args.content)
            .await
            .map_err(McpError::Other)?;

        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        let path = format!("~/.kodegen/prompts/{}.j2.md", args.name);
        let mut contents = Vec::new();

        // 1. TERMINAL SUMMARY
        let summary = format!(
            "✓ Prompt '{}' created successfully\n\n\
             Path: {}\n\
             Elapsed: {:.0}ms",
            args.name, path, elapsed_ms
        );
        contents.push(Content::text(summary));

        // 2. JSON METADATA
        let metadata = json!({
            "success": true,
            "name": args.name,
            "path": path,
            "elapsed_ms": elapsed_ms,
            "message": format!("Prompt '{}' created successfully", args.name)
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
                content: PromptMessageContent::text("How do I create a custom prompt?"),
            },
            PromptMessage {
                role: PromptMessageRole::Assistant,
                content: PromptMessageContent::text(
                    "Use prompt_add to create custom prompt templates:\n\n\
                     Example:\n\
                     ```\n\
                     prompt_add({\n\
                       \"name\": \"my_workflow\",\n\
                       \"content\": \"---\\n\
                     title: \\\"My Custom Workflow\\\"\\n\
                     description: \\\"Description here\\\"\\n\
                     categories: [\\\"custom\\\"]\\n\
                     author: \\\"your-name\\\"\\n\
                     parameters:\\n\
                       - name: \\\"project_path\\\"\\n\
                         description: \\\"Project to analyze\\\"\\n\
                         required: false\\n\
                         default: \\\".\\\"\\n\
                     ---\\n\
                     \\n\
                     # My Workflow\\n\
                     \\n\
                     Project: {{ project_path }}\\n\
                     User: {{ env.USER }}\\n\
                     \\\"\n\
                     })\n\
                     ```\n\n\
                     Template features:\n\
                     - {{ variable }} - Variable substitution\n\
                     - {% if condition %} - Conditionals\n\
                     - {% for item in items %} - Loops\n\
                     - {{ env.VAR }} - Environment variables\n\
                     - {{ param | filter }} - Filters\n\n\
                     The content is validated for syntax errors before saving.",
                ),
            },
        ])
    }
}
