use super::manager::PromptManager;
use kodegen_mcp_tool::{Tool, ToolExecutionContext};
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

    async fn execute(&self, args: Self::Args, _ctx: ToolExecutionContext) -> Result<Vec<Content>, McpError> {
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
                    "\x1b[36m󰗚 Prompt: List Categories\x1b[0m\n󰈙 Categories: {} · Elapsed: {:.0}ms",
                    count, elapsed_ms
                )
            }
            GetPromptAction::ListPrompts => {
                let count = result["count"].as_u64().unwrap_or(0);
                let category_suffix = args.category
                    .as_ref()
                    .map(|c| format!(" ({})", c))
                    .unwrap_or_default();
                format!(
                    "\x1b[36m󰗚 Prompt: List Prompts{}\x1b[0m\n󰈙 Count: {} · Elapsed: {:.0}ms",
                    category_suffix, count, elapsed_ms
                )
            }
            GetPromptAction::Get => {
                let name = result["name"].as_str().unwrap_or("unknown");
                let content_length = result["content"]
                    .as_str()
                    .map(|s| s.len())
                    .unwrap_or(0);
                let param_count = result["metadata"]["parameters"]
                    .as_array()
                    .map(|arr| arr.len())
                    .unwrap_or(0);
                format!(
                    "\x1b[36m󰗚 Prompt: {}\x1b[0m\n󰈙 Template Length: {} chars · Parameters: {}",
                    name, content_length, param_count
                )
            }
            GetPromptAction::Render => {
                let name = result["name"].as_str().unwrap_or("unknown");
                let rendered_length = result["content"]
                    .as_str()
                    .map(|s| s.len())
                    .unwrap_or(0);
                format!(
                    "\x1b[36m󰗚 Prompt: {} (Rendered)\x1b[0m\n󰈙 Output Length: {} chars · Elapsed: {:.0}ms",
                    name, rendered_length, elapsed_ms
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
        vec![PromptArgument {
            name: "focus_area".to_string(),
            title: None,
            description: Some(
                "Which aspect to focus on: 'browsing' (discovering prompts), 'rendering' (using templates), \
                 'parameters' (template parameters and customization), or 'all' (comprehensive overview)"
                    .to_string(),
            ),
            required: Some(false),
        }]
    }

    async fn prompt(&self, _args: Self::PromptArgs) -> Result<Vec<PromptMessage>, McpError> {
        Ok(vec![
            PromptMessage {
                role: PromptMessageRole::User,
                content: PromptMessageContent::text(
                    "I need to work with prompt templates. What can the prompt_get tool do?"
                ),
            },
            PromptMessage {
                role: PromptMessageRole::Assistant,
                content: PromptMessageContent::text(
                    "The prompt_get tool helps you browse, discover, retrieve, and render prompt templates. It has 4 actions:\n\n\
                     **1. list_categories** - Discover all available prompt categories and their counts\n\
                     ```\n\
                     prompt_get({\"action\": \"list_categories\"})\n\
                     ```\n\
                     Returns categories like 'onboarding', 'analysis', 'documentation', etc. Use this to explore what prompts are available.\n\n\
                     **2. list_prompts** - List all prompts or filter by category\n\
                     ```\n\
                     prompt_get({\"action\": \"list_prompts\"})\n\
                     prompt_get({\"action\": \"list_prompts\", \"category\": \"onboarding\"})\n\
                     ```\n\
                     Each result includes: name, title, description, categories, author, verification status, and available parameters.\n\n\
                     **3. get** - Retrieve the raw template content and metadata\n\
                     ```\n\
                     prompt_get({\"action\": \"get\", \"name\": \"getting_started\"})\n\
                     ```\n\
                     Returns the Jinja2 template with YAML frontmatter containing metadata and parameter definitions.\n\n\
                     **4. render** - Render a template with actual parameter values\n\
                     ```\n\
                     prompt_get({\n\
                       \"action\": \"render\",\n\
                       \"name\": \"analyze_project\",\n\
                       \"parameters\": {\"project_path\": \"/my/project\", \"depth\": \"detailed\"}\n\
                     })\n\
                     ```\n\
                     This produces the final prompt text ready for use with an LLM."
                ),
            },
            PromptMessage {
                role: PromptMessageRole::User,
                content: PromptMessageContent::text(
                    "How do I understand template parameters and what makes a prompt renderable?"
                ),
            },
            PromptMessage {
                role: PromptMessageRole::Assistant,
                content: PromptMessageContent::text(
                    "Prompts are Jinja2 templates stored with YAML frontmatter. The metadata defines parameters:\n\n\
                     **Understanding Template Parameters:**\n\
                     - Parameters are defined in the YAML frontmatter section at the top of the template\n\
                     - Each parameter has a name, description, type, and sometimes default values\n\
                     - Use list_prompts to see available parameters for each prompt\n\
                     - Use get to inspect the actual template and see how parameters are used\n\n\
                     **Rendering Workflow:**\n\
                     1. List prompts to find a template matching your need\n\
                     2. Use get to understand what parameters it requires\n\
                     3. Use render with parameter values matching the template's requirements\n\n\
                     **Example:**\n\
                     ```\n\
                     // First, explore\n\
                     prompt_get({\"action\": \"list_prompts\", \"category\": \"analysis\"})\n\
                     \n\
                     // Get template details\n\
                     prompt_get({\"action\": \"get\", \"name\": \"code_review_checklist\"})\n\
                     \n\
                     // Render with your data\n\
                     prompt_get({\n\
                       \"action\": \"render\",\n\
                       \"name\": \"code_review_checklist\",\n\
                       \"parameters\": {\n\
                         \"code_snippet\": \"...\",\n\
                         \"language\": \"rust\",\n\
                         \"context\": \"memory-safe networking code\"\n\
                       }\n\
                     })\n\
                     ```\n\n\
                     **Best Practice:** Always use get to inspect parameters before rendering to ensure you're providing the right values."
                ),
            },
            PromptMessage {
                role: PromptMessageRole::User,
                content: PromptMessageContent::text(
                    "When should I use each action? Any tips for working with prompts efficiently?"
                ),
            },
            PromptMessage {
                role: PromptMessageRole::Assistant,
                content: PromptMessageContent::text(
                    "**Decision Tree for Choosing Actions:**\n\n\
                     - **list_categories** → When you need to explore what domains of prompts exist\n\
                     - **list_prompts** → When you know the category but not the exact prompt name\n\
                     - **list_prompts** (no category) → Quick scan of ALL available prompts\n\
                     - **get** → When you need to see the template structure or understand parameters\n\
                     - **render** → When you're ready to generate final prompt text for an LLM\n\n\
                     **Efficiency Tips:**\n\
                     1. Cache category knowledge - don't query list_categories repeatedly\n\
                     2. Use category filters when searching large prompt libraries\n\
                     3. Get familiar with commonly-used prompts to reduce lookup overhead\n\
                     4. Store rendered prompts for reuse with the same parameters\n\
                     5. Validate parameter types before rendering - check the template's parameter definitions\n\n\
                     **Common Workflows:**\n\n\
                     *Workflow A: Discover and use a prompt*\n\
                     - list_prompts → get (on interesting prompt) → render (with your data)\n\n\
                     *Workflow B: Quick rendering of known template*\n\
                     - render directly (if you already know the prompt name and parameters)\n\n\
                     *Workflow C: Categorized search*\n\
                     - list_categories → list_prompts (with category) → get → render\n\n\
                     **Important Notes:**\n\
                     - The tool is read-only (doesn't modify prompts or templates)\n\
                     - All operations are idempotent - safe to retry\n\
                     - Use verified prompts (check the \"verified\" flag in list output) for production workflows"
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
