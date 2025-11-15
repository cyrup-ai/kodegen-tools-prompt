use super::manager::PromptManager;
use kodegen_mcp_tool::Tool;
use kodegen_mcp_tool::error::McpError;
use kodegen_mcp_schema::prompt::{GetPromptArgs, GetPromptPromptArgs, GetPromptAction, PROMPT_GET};
use rmcp::model::{Content, PromptArgument, PromptMessage, PromptMessageContent, PromptMessageRole};
use serde_json::{Value, json};
use std::collections::HashMap;

#[derive(Clone)]
pub struct GetPromptTool {
    manager: PromptManager,
}

impl GetPromptTool {
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

impl Tool for GetPromptTool {
    type Args = GetPromptArgs;
    type PromptArgs = GetPromptPromptArgs;

    fn name() -> &'static str {
        PROMPT_GET
    }

    fn description() -> &'static str {
        "Browse and retrieve prompt templates. \n\n\
         Actions:\n\
         - list_categories: Show all prompt categories\n\
         - list_prompts: List all prompts (optionally filtered by category)\n\
         - get: Get prompt metadata and raw template content\n\
         - render: Render prompt with parameters\n\n\
         Examples:\n\
         - prompt_get({\"action\": \"list_categories\"})\n\
         - prompt_get({\"action\": \"list_prompts\", \"category\": \"onboarding\"})\n\
         - prompt_get({\"action\": \"get\", \"name\": \"getting_started\"})\n\
         - prompt_get({\"action\": \"render\", \"name\": \"analyze_project\", \"parameters\": {\"project_path\": \"/path\"}})"
    }

    fn read_only() -> bool {
        true
    }

    fn destructive() -> bool {
        false
    }

    fn idempotent() -> bool {
        true
    }

    async fn execute(&self, args: Self::Args) -> Result<Vec<Content>, McpError> {
        let start = std::time::Instant::now();
        let action_name = match args.action {
            GetPromptAction::ListCategories => "list_categories",
            GetPromptAction::ListPrompts => "list_prompts",
            GetPromptAction::Get => "get",
            GetPromptAction::Render => "render",
        };
        
        // Execute the action to get JSON result
        let result = match args.action {
            GetPromptAction::ListCategories => self.list_categories().await?,
            GetPromptAction::ListPrompts => self.list_prompts(args.category.as_deref()).await?,
            GetPromptAction::Get => {
                let name = args.name.ok_or_else(|| {
                    McpError::InvalidArguments("name required for get action".into())
                })?;
                self.get_prompt(&name).await?
            }
            GetPromptAction::Render => {
                let name = args.name.ok_or_else(|| {
                    McpError::InvalidArguments("name required for render action".into())
                })?;
                self.render_prompt(&name, args.parameters).await?
            }
        };
        
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        let mut contents = Vec::new();

        // 1. TERMINAL SUMMARY - varies by action
        let summary = match args.action {
            GetPromptAction::ListCategories => {
                let count = result["total"].as_u64().unwrap_or(0);
                format!(
                    "✓ Listed prompt categories\n\n\
                     Categories: {}\n\
                     Elapsed: {:.0}ms",
                    count, elapsed_ms
                )
            }
            GetPromptAction::ListPrompts => {
                let count = result["count"].as_u64().unwrap_or(0);
                let category_info = args.category
                    .as_ref()
                    .map(|c| format!(" in category '{}'", c))
                    .unwrap_or_default();
                format!(
                    "✓ Listed prompts{}\n\n\
                     Count: {}\n\
                     Elapsed: {:.0}ms",
                    category_info, count, elapsed_ms
                )
            }
            GetPromptAction::Get => {
                let name = result["name"].as_str().unwrap_or("unknown");
                format!(
                    "✓ Retrieved prompt '{}'\n\n\
                     Rendered: false\n\
                     Elapsed: {:.0}ms",
                    name, elapsed_ms
                )
            }
            GetPromptAction::Render => {
                let name = result["name"].as_str().unwrap_or("unknown");
                format!(
                    "✓ Rendered prompt '{}'\n\n\
                     Rendered: true\n\
                     Elapsed: {:.0}ms",
                    name, elapsed_ms
                )
            }
        };
        contents.push(Content::text(summary));

        // 2. JSON METADATA - complete result with timing
        let mut metadata = result;
        metadata["action"] = json!(action_name);
        metadata["elapsed_ms"] = json!(elapsed_ms);
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
                content: PromptMessageContent::text("How do I browse and use prompts?"),
            },
            PromptMessage {
                role: PromptMessageRole::Assistant,
                content: PromptMessageContent::text(
                    "Use prompt_get to browse and retrieve prompt templates:\n\n\
                     1. List all categories:\n\
                     ```\n\
                     get_prompt({\"action\": \"list_categories\"})\n\
                     ```\n\n\
                     2. List prompts (all or by category):\n\
                     ```\n\
                     get_prompt({\"action\": \"list_prompts\"})\n\
                     get_prompt({\"action\": \"list_prompts\", \"category\": \"onboarding\"})\n\
                     ```\n\n\
                     3. Get raw prompt content:\n\
                     ```\n\
                     get_prompt({\"action\": \"get\", \"name\": \"getting_started\"})\n\
                     ```\n\n\
                     4. Render prompt with parameters:\n\
                     ```\n\
                     get_prompt({\n\
                       \"action\": \"render\",\n\
                       \"name\": \"analyze_project\",\n\
                       \"parameters\": {\"project_path\": \"/my/project\"}\n\
                     })\n\
                     ```",
                ),
            },
        ])
    }
}

impl GetPromptTool {
    async fn list_categories(&self) -> Result<Value, McpError> {
        let prompts = self.manager.list_prompts().await.map_err(McpError::Other)?;

        // Group by category and count
        let mut category_map: HashMap<String, usize> = HashMap::new();
        for prompt in prompts {
            for cat in prompt.metadata.categories {
                *category_map.entry(cat).or_insert(0) += 1;
            }
        }

        let categories: Vec<_> = category_map
            .into_iter()
            .map(|(name, count)| json!({"name": name, "count": count}))
            .collect();

        Ok(json!({
            "categories": categories,
            "total": categories.len()
        }))
    }

    async fn list_prompts(&self, category: Option<&str>) -> Result<Value, McpError> {
        let mut prompts = self.manager.list_prompts().await.map_err(McpError::Other)?;

        // Filter by category if specified
        if let Some(cat) = category {
            prompts.retain(|p| p.metadata.categories.contains(&cat.to_string()));
        }

        let prompts_json: Vec<_> = prompts
            .iter()
            .map(|p| {
                json!({
                    "name": p.filename,
                    "title": p.metadata.title,
                    "description": p.metadata.description,
                    "categories": p.metadata.categories,
                    "author": p.metadata.author,
                    "verified": p.metadata.verified,
                    "parameters": p.metadata.parameters,
                })
            })
            .collect();

        Ok(json!({
            "prompts": prompts_json,
            "count": prompts_json.len(),
            "category": category
        }))
    }

    async fn get_prompt(&self, name: &str) -> Result<Value, McpError> {
        let template = self
            .manager
            .load_prompt(name)
            .await
            .map_err(McpError::Other)?;

        Ok(json!({
            "name": name,
            "metadata": template.metadata,
            "content": template.content,
            "rendered": false
        }))
    }

    async fn render_prompt(
        &self,
        name: &str,
        parameters: Option<HashMap<String, serde_json::Value>>,
    ) -> Result<Value, McpError> {
        let rendered = self
            .manager
            .render_prompt(name, parameters)
            .await
            .map_err(McpError::Other)?;

        Ok(json!({
            "name": name,
            "content": rendered,
            "rendered": true
        }))
    }
}
