use super::manager::PromptManager;
use super::template::parse_template;
use kodegen_mcp_tool::{Tool, ToolExecutionContext};
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

    async fn execute(&self, args: Self::Args, _ctx: ToolExecutionContext) -> Result<Vec<Content>, McpError> {
        let start = std::time::Instant::now();

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

        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        let path = format!("~/.kodegen/prompts/{}.j2.md", args.name);
        let mut contents = Vec::new();

        // 1. TERMINAL SUMMARY - ANSI formatted with Nerd Font icons
        let summary = format!(
            "\x1b[32m Prompt Added: {}\x1b[0m\n\
              Template length: {} · Parameters: {}",
            args.name,
            template_length,
            param_count
        );
        contents.push(Content::text(summary));

        // 2. JSON METADATA
        let metadata = json!({
            "success": true,
            "name": args.name,
            "path": path,
            "elapsed_ms": elapsed_ms,
            "template_length": template_length,
            "parameter_count": param_count,
            "message": format!("Prompt '{}' created successfully", args.name)
        });
        let json_str = serde_json::to_string_pretty(&metadata)
            .unwrap_or_else(|_| "{}".to_string());
        contents.push(Content::text(json_str));

        Ok(contents)
    }

    fn prompt_arguments() -> Vec<PromptArgument> {
        vec![
            PromptArgument {
                name: "template_scope".to_string(),
                title: Some("Template Purpose".to_string()),
                description: Some(
                    "Type of template to focus examples on: 'code' (for code generation), \
                     'analysis' (for document analysis), 'workflow' (for multi-step processes), \
                     'documentation' (for doc generation), or 'custom' (general guidance)".to_string(),
                ),
                required: Some(false),
            },
            PromptArgument {
                name: "detail_level".to_string(),
                title: Some("Example Depth".to_string()),
                description: Some(
                    "How detailed the teaching should be: 'basic' (overview only), \
                     'intermediate' (practical examples), or 'advanced' (complex patterns and optimization)"
                        .to_string(),
                ),
                required: Some(false),
            },
        ]
    }

    async fn prompt(&self, args: Self::PromptArgs) -> Result<Vec<PromptMessage>, McpError> {
        // Extract arguments with defaults
        let template_scope = args.template_scope.as_deref().unwrap_or("custom");
        let detail_level = args.detail_level.as_deref().unwrap_or("intermediate");

        // Build conversation based on scope and detail level
        let (question, answer) = match (template_scope, detail_level) {
            // CODE SCOPE
            ("code", "basic") => (
                "How do I create prompt templates for code generation?",
                "Use prompt_add to create code generation templates with basic parameters:\n\n\
                 Example:\n\
                 ```\n\
                 prompt_add({\n\
                   \"name\": \"code_gen\",\n\
                   \"content\": \"---\\n\
                 title: \\\"Code Generator\\\"\\n\
                 description: \\\"Generate code in specified language\\\"\\n\
                 categories: [\\\"code\\\"]\\n\
                 author: \\\"your-name\\\"\\n\
                 parameters:\\n\
                   - name: \\\"language\\\"\\n\
                     description: \\\"Programming language\\\"\\n\
                     required: true\\n\
                   - name: \\\"style\\\"\\n\
                     description: \\\"Code style\\\"\\n\
                     required: false\\n\
                     default: \\\"standard\\\"\\n\
                 ---\\n\
                 \\n\
                 Generate {{ language }} code following {{ style }} style.\\n\
                 \\\"\n\
                 })\n\
                 ```\n\n\
                 Template features:\n\
                 - {{ variable }} - Variable substitution for parameters",
            ),
            ("code", "advanced") => (
                "How do I create advanced code generation templates?",
                "Create sophisticated code generation templates with conditionals, environment variables, and filters:\n\n\
                 Basic example:\n\
                 ```\n\
                 prompt_add({\n\
                   \"name\": \"simple_code\",\n\
                   \"content\": \"---\\n\
                 title: \\\"Simple Code Gen\\\"\\n\
                 parameters:\\n\
                   - name: \\\"language\\\"\\n\
                 ---\\n\
                 Generate {{ language }} code.\\n\
                 \\\"\n\
                 })\n\
                 ```\n\n\
                 Advanced example with all features:\n\
                 ```\n\
                 prompt_add({\n\
                   \"name\": \"advanced_code\",\n\
                   \"content\": \"---\\n\
                 title: \\\"Advanced Code Generator\\\"\\n\
                 description: \\\"Context-aware code generation\\\"\\n\
                 categories: [\\\"code\\\", \\\"generation\\\"]\\n\
                 author: \\\"your-name\\\"\\n\
                 parameters:\\n\
                   - name: \\\"language\\\"\\n\
                     description: \\\"Target language\\\"\\n\
                     required: true\\n\
                   - name: \\\"style\\\"\\n\
                     description: \\\"Code style (functional/oop)\\\"\\n\
                     default: \\\"functional\\\"\\n\
                   - name: \\\"framework\\\"\\n\
                     description: \\\"Framework to use\\\"\\n\
                     required: false\\n\
                   - name: \\\"features\\\"\\n\
                     description: \\\"List of features\\\"\\n\
                     required: false\\n\
                 ---\\n\
                 \\n\
                 # Code Generation Request\\n\
                 \\n\
                 Language: {{ language | upper }}\\n\
                 Style: {{ style }}\\n\
                 \\n\
                 {% if framework %}\\n\
                 Framework: {{ framework }}\\n\
                 Use {{ framework }}-specific patterns and conventions.\\n\
                 {% endif %}\\n\
                 \\n\
                 {% if features %}\\n\
                 Required features:\\n\
                 {% for feature in features %}\\n\
                 - {{ feature }}\\n\
                 {% endfor %}\\n\
                 {% endif %}\\n\
                 \\n\
                 Compiler: {{ env.COMPILER }}\\n\
                 \\n\
                 {% if style == 'functional' %}\\n\
                 Focus on pure functions, immutability, and composition.\\n\
                 {% else %}\\n\
                 Use object-oriented design with classes and inheritance.\\n\
                 {% endif %}\\n\
                 \\\"\n\
                 })\n\
                 ```\n\n\
                 Best practices:\n\
                 - **Parameter design**: Use required for essential params, defaults for optional\n\
                 - **Conditionals**: Adapt output based on context ({% if framework %}...{% endif %})\n\
                 - **Filters**: Transform variables ({{ language | upper }}, {{ style | lower }})\n\
                 - **Environment**: Access system info via {{ env.VAR }}\n\
                 - **Lists**: Iterate with {% for item in items %}...{% endfor %}\n\n\
                 Common patterns:\n\
                 - Language-specific conventions: Use conditionals to adapt style\n\
                 - Tool path configuration: {{ env.COMPILER }}, {{ env.LINTER }}\n\
                 - Feature toggles: {% if feature_name %} implementation {% endif %}\n\n\
                 Performance: Large templates with many conditionals parse efficiently. \
                 Template compilation is cached after first use.",
            ),
            ("code", _) => ( // intermediate or default
                "How do I create prompt templates for code generation?",
                "Use prompt_add to create code generation templates:\n\n\
                 Example:\n\
                 ```\n\
                 prompt_add({\n\
                   \"name\": \"code_generator\",\n\
                   \"content\": \"---\\n\
                 title: \\\"Code Generator\\\"\\n\
                 description: \\\"Generate code with specific parameters\\\"\\n\
                 categories: [\\\"code\\\"]\\n\
                 author: \\\"your-name\\\"\\n\
                 parameters:\\n\
                   - name: \\\"language\\\"\\n\
                     description: \\\"Programming language\\\"\\n\
                     required: true\\n\
                   - name: \\\"style\\\"\\n\
                     description: \\\"Code style (functional/oop)\\\"\\n\
                     required: false\\n\
                     default: \\\"functional\\\"\\n\
                   - name: \\\"framework\\\"\\n\
                     description: \\\"Framework to use\\\"\\n\
                     required: false\\n\
                 ---\\n\
                 \\n\
                 # Code Generation\\n\
                 \\n\
                 Generate {{ language }} code following {{ style }} style.\\n\
                 \\n\
                 {% if framework %}\\n\
                 Use {{ framework }} framework patterns.\\n\
                 {% endif %}\\n\
                 \\n\
                 Compiler path: {{ env.COMPILER }}\\n\
                 \\\"\n\
                 })\n\
                 ```\n\n\
                 Template features:\n\
                 - {{ variable }} - Variable substitution (language, style, framework)\n\
                 - {% if condition %} - Conditionals for optional features\n\
                 - {{ env.VAR }} - Environment variables for tool paths\n\
                 - {{ param | filter }} - Filters for text transformation\n\n\
                 The content is validated for syntax errors before saving.",
            ),

            // ANALYSIS SCOPE
            ("analysis", "basic") => (
                "How do I create templates for analyzing documents or code?",
                "Use prompt_add for analysis workflows:\n\n\
                 Example:\n\
                 ```\n\
                 prompt_add({\n\
                   \"name\": \"analyze\",\n\
                   \"content\": \"---\\n\
                 title: \\\"Analyzer\\\"\\n\
                 description: \\\"Analyze content\\\"\\n\
                 categories: [\\\"analysis\\\"]\\n\
                 author: \\\"your-name\\\"\\n\
                 parameters:\\n\
                   - name: \\\"depth\\\"\\n\
                     description: \\\"Analysis depth\\\"\\n\
                     default: \\\"standard\\\"\\n\
                 ---\\n\
                 \\n\
                 Perform {{ depth }} analysis.\\n\
                 \\\"\n\
                 })\n\
                 ```\n\n\
                 Template features:\n\
                 - {{ variable }} - Parameter substitution",
            ),
            ("analysis", "advanced") => (
                "How do I create advanced analysis templates?",
                "Create comprehensive analysis templates with depth control and focus areas:\n\n\
                 Basic example:\n\
                 ```\n\
                 prompt_add({\n\
                   \"name\": \"basic_analysis\",\n\
                   \"content\": \"---\\n\
                 title: \\\"Basic Analysis\\\"\\n\
                 parameters:\\n\
                   - name: \\\"depth\\\"\\n\
                 ---\\n\
                 Analyze with {{ depth }} depth.\\n\
                 \\\"\n\
                 })\n\
                 ```\n\n\
                 Advanced example:\n\
                 ```\n\
                 prompt_add({\n\
                   \"name\": \"advanced_analysis\",\n\
                   \"content\": \"---\\n\
                 title: \\\"Advanced Code Analysis\\\"\\n\
                 description: \\\"Multi-level code and document analysis\\\"\\n\
                 categories: [\\\"analysis\\\", \\\"code\\\"]\\n\
                 author: \\\"your-name\\\"\\n\
                 parameters:\\n\
                   - name: \\\"depth\\\"\\n\
                     description: \\\"Analysis depth (surface/standard/deep)\\\"\\n\
                     default: \\\"standard\\\"\\n\
                   - name: \\\"focus_areas\\\"\\n\
                     description: \\\"Specific areas to focus on\\\"\\n\
                     required: false\\n\
                   - name: \\\"output_format\\\"\\n\
                     description: \\\"Report format\\\"\\n\
                     default: \\\"markdown\\\"\\n\
                 ---\\n\
                 \\n\
                 # Analysis Request\\n\
                 \\n\
                 Analysis depth: {{ depth }}\\n\
                 \\n\
                 {% if depth == 'deep' %}\\n\
                 ## Deep Analysis Steps\\n\
                 1. Static code analysis\\n\
                 2. Dependency review\\n\
                 3. Security audit\\n\
                 4. Performance profiling\\n\
                 5. Best practices compliance\\n\
                 {% elif depth == 'standard' %}\\n\
                 ## Standard Analysis\\n\
                 - Code structure review\\n\
                 - Basic security checks\\n\
                 - Style compliance\\n\
                 {% else %}\\n\
                 ## Surface Analysis\\n\
                 - Quick overview\\n\
                 - Basic metrics\\n\
                 {% endif %}\\n\
                 \\n\
                 {% if focus_areas %}\\n\
                 Focus areas:\\n\
                 {% for area in focus_areas %}\\n\
                 - {{ area | capitalize }}\\n\
                 {% endfor %}\\n\
                 {% endif %}\\n\
                 \\n\
                 Output: {{ output_format | upper }}\\n\
                 Tool: {{ env.ANALYZER_PATH }}\\n\
                 \\\"\n\
                 })\n\
                 ```\n\n\
                 Best practices:\n\
                 - **Depth levels**: Use conditionals to vary analysis thoroughness\n\
                 - **Focus areas**: Allow targeting specific aspects via lists\n\
                 - **Output formats**: Support multiple report formats\n\
                 - **Tool integration**: Reference analysis tools via environment variables\n\n\
                 Patterns:\n\
                 - Conditional analysis steps based on depth parameter\n\
                 - Iteration over focus areas for targeted analysis\n\
                 - Filter usage for formatting (capitalize, upper)\n\
                 - Environment variables for tool paths",
            ),
            ("analysis", _) => ( // intermediate or default
                "How do I create templates for analyzing documents or code?",
                "Use prompt_add for analysis workflows:\n\n\
                 Example:\n\
                 ```\n\
                 prompt_add({\n\
                   \"name\": \"code_analysis\",\n\
                   \"content\": \"---\\n\
                 title: \\\"Code Analysis\\\"\\n\
                 description: \\\"Analyze code with configurable depth\\\"\\n\
                 categories: [\\\"analysis\\\"]\\n\
                 author: \\\"your-name\\\"\\n\
                 parameters:\\n\
                   - name: \\\"depth\\\"\\n\
                     description: \\\"Analysis depth (surface/standard/deep)\\\"\\n\
                     required: false\\n\
                     default: \\\"standard\\\"\\n\
                   - name: \\\"focus_areas\\\"\\n\
                     description: \\\"Areas to focus on\\\"\\n\
                     required: false\\n\
                 ---\\n\
                 \\n\
                 # Code Analysis\\n\
                 \\n\
                 Depth: {{ depth }}\\n\
                 \\n\
                 {% if depth == 'deep' %}\\n\
                 Perform comprehensive analysis including:\\n\
                 - Static analysis\\n\
                 - Security review\\n\
                 - Performance check\\n\
                 {% else %}\\n\
                 Perform {{ depth }} analysis.\\n\
                 {% endif %}\\n\
                 \\n\
                 {% if focus_areas %}\\n\
                 Focus on: {% for area in focus_areas %}{{ area }}{% if not loop.last %}, {% endif %}{% endfor %}\\n\
                 {% endif %}\\n\
                 \\\"\n\
                 })\n\
                 ```\n\n\
                 Template features:\n\
                 - {{ variable }} - Variable substitution (depth, focus_areas)\n\
                 - {% if condition %} - Conditional analysis steps\n\
                 - {% for item in items %} - Iterate over focus areas\n\
                 - {{ env.VAR }} - Environment variables for tool paths\n\n\
                 The content is validated for syntax errors before saving.",
            ),

            // WORKFLOW SCOPE
            ("workflow", "basic") => (
                "How do I create multi-step workflow templates?",
                "Use prompt_add for simple workflows:\n\n\
                 Example:\n\
                 ```\n\
                 prompt_add({\n\
                   \"name\": \"workflow\",\n\
                   \"content\": \"---\\n\
                 title: \\\"Simple Workflow\\\"\\n\
                 description: \\\"Execute steps\\\"\\n\
                 categories: [\\\"workflow\\\"]\\n\
                 author: \\\"your-name\\\"\\n\
                 parameters:\\n\
                   - name: \\\"steps\\\"\\n\
                     description: \\\"Steps to execute\\\"\\n\
                 ---\\n\
                 \\n\
                 Execute workflow steps.\\n\
                 \\\"\n\
                 })\n\
                 ```\n\n\
                 Template features:\n\
                 - {{ variable }} - Parameter substitution",
            ),
            ("workflow", "advanced") => (
                "How do I create complex multi-step workflow templates?",
                "Create sophisticated workflows with loops, conditionals, and step management:\n\n\
                 Basic example:\n\
                 ```\n\
                 prompt_add({\n\
                   \"name\": \"simple_workflow\",\n\
                   \"content\": \"---\\n\
                 title: \\\"Simple Workflow\\\"\\n\
                 parameters:\\n\
                   - name: \\\"steps\\\"\\n\
                 ---\\n\
                 Execute steps.\\n\
                 \\\"\n\
                 })\n\
                 ```\n\n\
                 Advanced example:\n\
                 ```\n\
                 prompt_add({\n\
                   \"name\": \"complex_workflow\",\n\
                   \"content\": \"---\\n\
                 title: \\\"Complex Workflow Engine\\\"\\n\
                 description: \\\"Multi-step workflow with conditionals and error handling\\\"\\n\
                 categories: [\\\"workflow\\\", \\\"automation\\\"]\\n\
                 author: \\\"your-name\\\"\\n\
                 parameters:\\n\
                   - name: \\\"steps\\\"\\n\
                     description: \\\"List of workflow steps\\\"\\n\
                     required: true\\n\
                   - name: \\\"parallel\\\"\\n\
                     description: \\\"Execute steps in parallel\\\"\\n\
                     default: false\\n\
                   - name: \\\"error_handling\\\"\\n\
                     description: \\\"Error handling strategy (abort/continue/retry)\\\"\\n\
                     default: \\\"abort\\\"\\n\
                   - name: \\\"conditions\\\"\\n\
                     description: \\\"Conditional step execution\\\"\\n\
                     required: false\\n\
                 ---\\n\
                 \\n\
                 # Workflow Execution Plan\\n\
                 \\n\
                 {% if parallel %}\\n\
                 Execution mode: PARALLEL\\n\
                 {% else %}\\n\
                 Execution mode: SEQUENTIAL\\n\
                 {% endif %}\\n\
                 \\n\
                 Error handling: {{ error_handling | upper }}\\n\
                 \\n\
                 ## Steps\\n\
                 {% for step in steps %}\\n\
                 ### Step {{ loop.index }}: {{ step }}\\n\
                 \\n\
                 {% if conditions and conditions[loop.index0] %}\\n\
                 Condition: {{ conditions[loop.index0] }}\\n\
                 {% endif %}\\n\
                 \\n\
                 {% if parallel %}\\n\
                 - Execute in parallel thread\\n\
                 {% else %}\\n\
                 - Wait for previous step completion\\n\
                 {% endif %}\\n\
                 \\n\
                 {% if error_handling == 'retry' %}\\n\
                 - Retry on failure (max 3 attempts)\\n\
                 {% elif error_handling == 'continue' %}\\n\
                 - Continue on failure\\n\
                 {% else %}\\n\
                 - Abort workflow on failure\\n\
                 {% endif %}\\n\
                 \\n\
                 {% endfor %}\\n\
                 \\n\
                 Workflow runner: {{ env.WORKFLOW_ENGINE }}\\n\
                 \\\"\n\
                 })\n\
                 ```\n\n\
                 Best practices:\n\
                 - **Step iteration**: Use {% for step in steps %} with loop.index for numbering\n\
                 - **Conditional execution**: Check conditions array for step-specific rules\n\
                 - **Branching logic**: Use {% if %} {% elif %} {% else %} for complex decisions\n\
                 - **Loop context**: Access loop.index (1-based) and loop.index0 (0-based)\n\n\
                 Common patterns:\n\
                 - Sequential vs parallel execution modes\n\
                 - Error handling strategies per step\n\
                 - Parameter passing between workflow steps\n\
                 - Conditional step execution based on previous results\n\n\
                 Template composition:\n\
                 - Break large workflows into reusable sub-templates\n\
                 - Use consistent parameter naming across workflow templates\n\
                 - Document step dependencies in template description",
            ),
            ("workflow", _) => ( // intermediate or default
                "How do I create multi-step workflow templates?",
                "Use prompt_add for multi-step workflows with loops and conditionals:\n\n\
                 Example:\n\
                 ```\n\
                 prompt_add({\n\
                   \"name\": \"workflow_template\",\n\
                   \"content\": \"---\\n\
                 title: \\\"Multi-Step Workflow\\\"\\n\
                 description: \\\"Execute workflow with conditional steps\\\"\\n\
                 categories: [\\\"workflow\\\"]\\n\
                 author: \\\"your-name\\\"\\n\
                 parameters:\\n\
                   - name: \\\"steps\\\"\\n\
                     description: \\\"List of steps to execute\\\"\\n\
                     required: true\\n\
                   - name: \\\"parallel\\\"\\n\
                     description: \\\"Run steps in parallel\\\"\\n\
                     required: false\\n\
                     default: false\\n\
                 ---\\n\
                 \\n\
                 # Workflow Execution\\n\
                 \\n\
                 {% if parallel %}\\n\
                 Mode: Parallel execution\\n\
                 {% else %}\\n\
                 Mode: Sequential execution\\n\
                 {% endif %}\\n\
                 \\n\
                 Steps:\\n\
                 {% for step in steps %}\\n\
                 {{ loop.index }}. {{ step }}\\n\
                 {% endfor %}\\n\
                 \\\"\n\
                 })\n\
                 ```\n\n\
                 Template features:\n\
                 - {{ variable }} - Variable substitution (steps, parallel)\n\
                 - {% if condition %} - Conditional branching for execution modes\n\
                 - {% for item in items %} - Loop over workflow steps\n\
                 - loop.index - Access step number in loops\n\
                 - {{ env.VAR }} - Environment variables\n\n\
                 The content is validated for syntax errors before saving.",
            ),

            // DOCUMENTATION SCOPE
            ("documentation", "basic") => (
                "How do I create documentation generation templates?",
                "Use prompt_add for simple documentation:\n\n\
                 Example:\n\
                 ```\n\
                 prompt_add({\n\
                   \"name\": \"doc_gen\",\n\
                   \"content\": \"---\\n\
                 title: \\\"Doc Generator\\\"\\n\
                 description: \\\"Generate docs\\\"\\n\
                 categories: [\\\"documentation\\\"]\\n\
                 author: \\\"your-name\\\"\\n\
                 parameters:\\n\
                   - name: \\\"title\\\"\\n\
                     description: \\\"Document title\\\"\\n\
                 ---\\n\
                 \\n\
                 # {{ title }}\\n\
                 \\\"\n\
                 })\n\
                 ```\n\n\
                 Template features:\n\
                 - {{ variable }} - Parameter substitution",
            ),
            ("documentation", "advanced") => (
                "How do I create advanced documentation templates?",
                "Create comprehensive documentation templates with structure and formatting:\n\n\
                 Basic example:\n\
                 ```\n\
                 prompt_add({\n\
                   \"name\": \"simple_doc\",\n\
                   \"content\": \"---\\n\
                 title: \\\"Simple Doc\\\"\\n\
                 parameters:\\n\
                   - name: \\\"title\\\"\\n\
                 ---\\n\
                 # {{ title }}\\n\
                 \\\"\n\
                 })\n\
                 ```\n\n\
                 Advanced example:\n\
                 ```\n\
                 prompt_add({\n\
                   \"name\": \"advanced_documentation\",\n\
                   \"content\": \"---\\n\
                 title: \\\"Advanced Documentation Generator\\\"\\n\
                 description: \\\"Generate structured documentation with optional sections\\\"\\n\
                 categories: [\\\"documentation\\\", \\\"markdown\\\"]\\n\
                 author: \\\"your-name\\\"\\n\
                 parameters:\\n\
                   - name: \\\"title\\\"\\n\
                     description: \\\"Document title\\\"\\n\
                     required: true\\n\
                   - name: \\\"language\\\"\\n\
                     description: \\\"Programming language\\\"\\n\
                     required: false\\n\
                   - name: \\\"audience\\\"\\n\
                     description: \\\"Target audience (beginner/intermediate/expert)\\\"\\n\
                     default: \\\"intermediate\\\"\\n\
                   - name: \\\"sections\\\"\\n\
                     description: \\\"Sections to include\\\"\\n\
                     required: false\\n\
                   - name: \\\"style\\\"\\n\
                     description: \\\"Documentation style\\\"\\n\
                     default: \\\"technical\\\"\\n\
                 ---\\n\
                 \\n\
                 # {{ title }}\\n\
                 \\n\
                 {% if language %}\\n\
                 **Language**: {{ language | upper }}\\n\
                 {% endif %}\\n\
                 \\n\
                 **Audience**: {{ audience | capitalize }}\\n\
                 \\n\
                 ---\\n\
                 \\n\
                 ## Overview\\n\
                 \\n\
                 {% if audience == 'beginner' %}\\n\
                 This documentation provides a gentle introduction with step-by-step examples.\\n\
                 {% elif audience == 'expert' %}\\n\
                 This documentation focuses on advanced usage and optimization techniques.\\n\
                 {% else %}\\n\
                 This documentation covers practical usage with real-world examples.\\n\
                 {% endif %}\\n\
                 \\n\
                 {% if sections %}\\n\
                 ## Table of Contents\\n\
                 {% for section in sections %}\\n\
                 {{ loop.index }}. {{ section | title }}\\n\
                 {% endfor %}\\n\
                 \\n\
                 {% for section in sections %}\\n\
                 ## {{ section | title }}\\n\
                 \\n\
                 {% if style == 'tutorial' %}\\n\
                 ### Step-by-step guide for {{ section }}\\n\
                 {% else %}\\n\
                 ### Technical reference for {{ section }}\\n\
                 {% endif %}\\n\
                 \\n\
                 {% endfor %}\\n\
                 {% endif %}\\n\
                 \\n\
                 ---\\n\
                 Generated by: {{ env.USER }}\\n\
                 \\\"\n\
                 })\n\
                 ```\n\n\
                 Best practices:\n\
                 - **Section organization**: Use loops to generate consistent section structure\n\
                 - **Audience targeting**: Adapt content depth via conditionals\n\
                 - **Text formatting**: Apply filters (title, capitalize, upper) for consistency\n\
                 - **Optional content**: Use conditionals for sections that may not always be needed\n\n\
                 Common patterns:\n\
                 - Table of contents generation from sections array\n\
                 - Audience-specific explanations (beginner vs expert)\n\
                 - Style variations (tutorial vs reference)\n\
                 - Metadata inclusion (author, date via env.USER)\n\n\
                 Documentation structure:\n\
                 - Start with overview and audience statement\n\
                 - Generate TOC from sections parameter\n\
                 - Iterate through sections with consistent formatting\n\
                 - Use filters for title casing and capitalization",
            ),
            ("documentation", _) => ( // intermediate or default
                "How do I create documentation generation templates?",
                "Use prompt_add for documentation generation with structured output:\n\n\
                 Example:\n\
                 ```\n\
                 prompt_add({\n\
                   \"name\": \"doc_template\",\n\
                   \"content\": \"---\\n\
                 title: \\\"Documentation Generator\\\"\\n\
                 description: \\\"Generate markdown documentation\\\"\\n\
                 categories: [\\\"documentation\\\"]\\n\
                 author: \\\"your-name\\\"\\n\
                 parameters:\\n\
                   - name: \\\"title\\\"\\n\
                     description: \\\"Document title\\\"\\n\
                     required: true\\n\
                   - name: \\\"language\\\"\\n\
                     description: \\\"Programming language\\\"\\n\
                     required: false\\n\
                   - name: \\\"audience\\\"\\n\
                     description: \\\"Target audience\\\"\\n\
                     default: \\\"general\\\"\\n\
                 ---\\n\
                 \\n\
                 # {{ title }}\\n\
                 \\n\
                 {% if language %}\\n\
                 Language: {{ language | upper }}\\n\
                 {% endif %}\\n\
                 \\n\
                 Audience: {{ audience | capitalize }}\\n\
                 \\n\
                 {% if audience == 'beginner' %}\\n\
                 This guide includes detailed explanations and examples.\\n\
                 {% else %}\\n\
                 This guide assumes familiarity with core concepts.\\n\
                 {% endif %}\\n\
                 \\\"\n\
                 })\n\
                 ```\n\n\
                 Template features:\n\
                 - {{ variable }} - Variable substitution (title, language, audience)\n\
                 - {% if condition %} - Conditionals for optional sections\n\
                 - {{ param | filter }} - Filters (upper, capitalize, title) for formatting\n\
                 - {{ env.VAR }} - Environment variables for metadata\n\n\
                 The content is validated for syntax errors before saving.",
            ),

            // CUSTOM/DEFAULT SCOPE
            _ => (
                "How do I create a custom prompt?",
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
        };

        Ok(vec![
            PromptMessage {
                role: PromptMessageRole::User,
                content: PromptMessageContent::text(question),
            },
            PromptMessage {
                role: PromptMessageRole::Assistant,
                content: PromptMessageContent::text(answer),
            },
        ])
    }
}
