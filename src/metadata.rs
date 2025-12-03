use kodegen_mcp_schema::prompt::{PromptParameterType, TemplateParamValue};
use serde::{Deserialize, Serialize};

/// Prompt metadata from YAML frontmatter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptMetadata {
    pub title: String,
    pub description: String,
    pub categories: Vec<String>,
    #[serde(default)]
    pub secondary_tag: Option<String>,
    pub author: String,
    #[serde(default)]
    pub verified: bool,
    #[serde(default)]
    pub votes: u32,
    #[serde(default)]
    pub parameters: Vec<ParameterDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterDefinition {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub param_type: PromptParameterType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<TemplateParamValue>,
}

/// Re-export ParameterType as alias for backwards source compat within this crate
pub type ParameterType = PromptParameterType;

/// Full prompt template (metadata + content)
#[derive(Debug, Clone)]
pub struct PromptTemplate {
    pub filename: String,
    pub metadata: PromptMetadata,
    pub content: String,
}
