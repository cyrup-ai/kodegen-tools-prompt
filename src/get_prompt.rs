use super::manager::PromptManager;
use super::metadata::PromptTemplate;
use kodegen_mcp_schema::prompt::{
    CategoryInfo, GetPromptAction, GetPromptArgs, PromptCategoriesResult,
    PromptContentResult, PromptGetOutput, PromptGetPrompts, PromptListResult, PromptMetadataOutput,
    PromptParameterDef, PromptParameterType, PromptRenderedResult, PromptResult, PromptSummary,
    TemplateParamValue, PROMPT_GET,
};
use kodegen_mcp_schema::{McpError, Tool, ToolExecutionContext, ToolResponse};
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
    type Prompts = PromptGetPrompts;

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

    async fn execute(
        &self,
        args: Self::Args,
        _ctx: ToolExecutionContext,
    ) -> Result<ToolResponse<<Self::Args as kodegen_mcp_schema::ToolArgs>::Output>, McpError> {
        let start = std::time::Instant::now();
        let action = args.action.clone();

        // Execute the action to get typed result
        let result = match &args.action {
            GetPromptAction::ListCategories => {
                let mut res = self.list_categories().await?;
                res.elapsed_ms = Some(start.elapsed().as_secs_f64() * 1000.0);
                PromptResult::ListCategories(res)
            }
            GetPromptAction::ListPrompts => {
                let mut res = self.list_prompts(args.category.as_deref()).await?;
                res.elapsed_ms = Some(start.elapsed().as_secs_f64() * 1000.0);
                PromptResult::ListPrompts(res)
            }
            GetPromptAction::Get => {
                let name = args.name.as_ref().ok_or_else(|| {
                    McpError::InvalidArguments("name required for get action".into())
                })?;
                let mut res = self.get_prompt(name).await?;
                res.elapsed_ms = Some(start.elapsed().as_secs_f64() * 1000.0);
                PromptResult::Get(res)
            }
            GetPromptAction::Render => {
                let name = args.name.as_ref().ok_or_else(|| {
                    McpError::InvalidArguments("name required for render action".into())
                })?;
                let mut res = self.render_prompt(name, args.parameters).await?;
                res.elapsed_ms = Some(start.elapsed().as_secs_f64() * 1000.0);
                PromptResult::Render(res)
            }
        };

        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

        // Terminal summary - varies by action
        let summary = match &result {
            PromptResult::ListCategories(res) => {
                format!(
                    "\x1b[36m󰗚 Prompt: List Categories\x1b[0m\n󰈙 Categories: {} · Elapsed: {:.0}ms",
                    res.total, elapsed_ms
                )
            }
            PromptResult::ListPrompts(res) => {
                let category_suffix = res
                    .category
                    .as_ref()
                    .map(|c| format!(" ({})", c))
                    .unwrap_or_default();
                format!(
                    "\x1b[36m󰗚 Prompt: List Prompts{}\x1b[0m\n󰈙 Count: {} · Elapsed: {:.0}ms",
                    category_suffix, res.count, elapsed_ms
                )
            }
            PromptResult::Get(res) => {
                format!(
                    "\x1b[36m󰗚 Prompt: {}\x1b[0m\n󰈙 Template Length: {} chars · Parameters: {}",
                    res.name,
                    res.content.len(),
                    res.metadata.parameters.len()
                )
            }
            PromptResult::Render(res) => {
                format!(
                    "\x1b[36m󰗚 Prompt: {} (Rendered)\x1b[0m\n󰈙 Output Length: {} chars · Elapsed: {:.0}ms",
                    res.name,
                    res.content.len(),
                    elapsed_ms
                )
            }
        };

        // Typed output
        let output = PromptGetOutput {
            success: true,
            action,
            result,
        };

        Ok(ToolResponse::new(summary, output))
    }
}

impl GetPromptTool {
    async fn list_categories(&self) -> Result<PromptCategoriesResult, McpError> {
        let prompts = self.manager.list_prompts().await.map_err(McpError::Other)?;

        // Group by category and count
        let mut category_map: HashMap<String, usize> = HashMap::new();
        for prompt in prompts {
            for cat in prompt.metadata.categories {
                *category_map.entry(cat).or_insert(0) += 1;
            }
        }

        let categories: Vec<CategoryInfo> = category_map
            .into_iter()
            .map(|(name, count)| CategoryInfo { name, count })
            .collect();

        let total = categories.len();
        Ok(PromptCategoriesResult {
            categories,
            total,
            elapsed_ms: None,
        })
    }

    async fn list_prompts(&self, category: Option<&str>) -> Result<PromptListResult, McpError> {
        let mut prompts = self.manager.list_prompts().await.map_err(McpError::Other)?;

        // Filter by category if specified
        if let Some(cat) = category {
            prompts.retain(|p| p.metadata.categories.contains(&cat.to_string()));
        }

        let prompts_list: Vec<PromptSummary> = prompts
            .iter()
            .map(|p| PromptSummary {
                name: p.filename.clone(),
                title: p.metadata.title.clone(),
                description: p.metadata.description.clone(),
                categories: p.metadata.categories.clone(),
                author: p.metadata.author.clone(),
                verified: p.metadata.verified,
                parameters: p
                    .metadata
                    .parameters
                    .iter()
                    .map(|param| PromptParameterDef {
                        name: param.name.clone(),
                        description: param.description.clone(),
                        param_type: convert_param_type(&param.param_type),
                        required: param.required,
                        default: param.default.clone(),
                    })
                    .collect(),
            })
            .collect();

        let count = prompts_list.len();
        Ok(PromptListResult {
            prompts: prompts_list,
            count,
            category: category.map(String::from),
            elapsed_ms: None,
        })
    }

    async fn get_prompt(&self, name: &str) -> Result<PromptContentResult, McpError> {
        let template = self
            .manager
            .load_prompt(name)
            .await
            .map_err(McpError::Other)?;

        Ok(PromptContentResult {
            name: name.to_string(),
            metadata: convert_metadata(&template),
            content: template.content,
            rendered: false,
            elapsed_ms: None,
        })
    }

    async fn render_prompt(
        &self,
        name: &str,
        parameters: Option<HashMap<String, TemplateParamValue>>,
    ) -> Result<PromptRenderedResult, McpError> {
        let rendered = self
            .manager
            .render_prompt(name, parameters)
            .await
            .map_err(McpError::Other)?;

        Ok(PromptRenderedResult {
            name: name.to_string(),
            content: rendered,
            rendered: true,
            elapsed_ms: None,
        })
    }
}

/// Convert internal ParameterType to schema PromptParameterType
fn convert_param_type(pt: &super::metadata::ParameterType) -> PromptParameterType {
    match pt {
        super::metadata::ParameterType::String => PromptParameterType::String,
        super::metadata::ParameterType::Number => PromptParameterType::Number,
        super::metadata::ParameterType::Boolean => PromptParameterType::Boolean,
        super::metadata::ParameterType::Array => PromptParameterType::Array,
    }
}

/// Convert internal PromptTemplate metadata to schema PromptMetadataOutput
fn convert_metadata(template: &PromptTemplate) -> PromptMetadataOutput {
    PromptMetadataOutput {
        title: template.metadata.title.clone(),
        description: template.metadata.description.clone(),
        categories: template.metadata.categories.clone(),
        secondary_tag: template.metadata.secondary_tag.clone(),
        author: template.metadata.author.clone(),
        verified: template.metadata.verified,
        votes: template.metadata.votes,
        parameters: template
            .metadata
            .parameters
            .iter()
            .map(|param| PromptParameterDef {
                name: param.name.clone(),
                description: param.description.clone(),
                param_type: convert_param_type(&param.param_type),
                required: param.required,
                default: param.default.clone(),
            })
            .collect(),
    }
}
