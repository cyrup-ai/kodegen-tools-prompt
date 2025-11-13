# kodegen-tools-prompt

[![License](https://img.shields.io/badge/license-Apache%202.0%20OR%20MIT-blue.svg)](LICENSE.md)
[![Rust](https://img.shields.io/badge/rust-nightly-orange.svg)](rust-toolchain.toml)

Memory-efficient, blazing-fast MCP tools for prompt template management in code generation agents.

## Overview

**kodegen-tools-prompt** is a Rust-based [Model Context Protocol (MCP)](https://modelcontextprotocol.io/) server that provides AI agents with powerful prompt template management capabilities. It enables creating, editing, retrieving, and rendering Jinja2-templated prompts with YAML frontmatter metadata.

Part of the [KODEGEN.ai](https://kodegen.ai) ecosystem, this server runs as an HTTP service managed by the kodegend daemon.

## Features

- 🚀 **Fast**: Blazing-fast template operations with async I/O
- 🔒 **Secure**: Built-in validation, size limits, and path traversal prevention
- 📝 **Jinja2 Templates**: Full Jinja2 syntax support with parameters and environment variables
- 🏷️ **Rich Metadata**: YAML frontmatter with categories, descriptions, and parameter definitions
- 🔄 **CRUD Operations**: Complete create, read, update, delete workflow
- 🎯 **MCP Native**: First-class MCP protocol support via HTTP/SSE transport
- 📦 **Default Prompts**: Ships with curated templates for common workflows

## Installation

### Prerequisites

- Rust nightly toolchain (automatically installed via rust-toolchain.toml)
- Cargo

### Build from Source

```bash
git clone https://github.com/cyrup-ai/kodegen-tools-prompt.git
cd kodegen-tools-prompt
cargo build --release
```

The binary will be available at `target/release/kodegen-prompt`.

## Usage

### Running the Server

```bash
cargo run --bin kodegen-prompt
```

The server typically runs on port 30438 when managed by kodegend.

### MCP Tools

The server provides four MCP tools:

#### 1. `prompt_add` - Create New Prompt

```json
{
  "name": "my_workflow",
  "content": "---\ntitle: \"My Workflow\"\ndescription: \"Custom workflow\"\ncategories: [\"custom\"]\nauthor: \"your-name\"\nparameters:\n  - name: \"project_path\"\n    description: \"Project directory\"\n    required: false\n    default: \".\"\n---\n\n# My Workflow\n\nProject: {{ project_path }}\nUser: {{ env.USER }}"
}
```

#### 2. `prompt_get` - Retrieve and Render Prompts

List all categories:
```json
{
  "action": "list_categories"
}
```

List prompts by category:
```json
{
  "action": "list_prompts",
  "category": "onboarding"
}
```

Get prompt metadata and content:
```json
{
  "action": "get",
  "name": "getting_started"
}
```

Render prompt with parameters:
```json
{
  "action": "render",
  "name": "analyze_project",
  "parameters": {
    "project_path": "/path/to/project"
  }
}
```

#### 3. `prompt_edit` - Update Existing Prompt

```json
{
  "name": "my_workflow",
  "content": "---\ntitle: \"Updated Workflow\"\n..."
}
```

#### 4. `prompt_delete` - Remove Prompt

```json
{
  "name": "my_workflow"
}
```

## Prompt Template Format

Prompts are stored as `.j2.md` files with YAML frontmatter:

```markdown
---
title: "Prompt Title"
description: "What this prompt does"
categories: ["category1", "category2"]
author: "your-name"
verified: true
parameters:
  - name: "param_name"
    description: "Parameter description"
    param_type: "string"  # string | number | boolean | array
    required: false
    default: "default_value"
---

# Template Content

Use {{ param_name }} for parameters.
Access environment variables: {{ env.USER }}, {{ env.HOME }}

## Jinja2 Syntax Support

{% if condition %}
Conditional content
{% endif %}

{% for item in items %}
- {{ item }}
{% endfor %}

Apply filters: {{ value | upper }}
```

### Template Features

- **Variables**: `{{ variable_name }}`
- **Conditionals**: `{% if condition %}...{% endif %}`
- **Loops**: `{% for item in items %}...{% endfor %}`
- **Filters**: `{{ value | filter_name }}`
- **Environment Variables**: `{{ env.USER }}`, `{{ env.HOME }}`, `{{ env.SHELL }}`, etc.

### Storage Location

Prompts are stored in: `~/.kodegen/prompts/`

## Development

### Build and Test

```bash
# Build the project
cargo build

# Run tests
cargo test

# Run with logging
RUST_LOG=info cargo run --bin kodegen-prompt

# Format code
cargo fmt

# Lint
cargo clippy -- -D warnings
```

### Run Examples


The repository includes integration test examples:

```bash
# Run the prompt demo (tests all CRUD operations)
cargo run --example prompt_demo
```

This example:
1. Connects to the local HTTP server
2. Creates a test prompt
3. Retrieves the prompt
4. Edits the prompt
5. Deletes the prompt

### Project Structure

```
src/
├── lib.rs              # Public API exports
├── main.rs             # HTTP server binary
├── manager.rs          # PromptManager core logic
├── template.rs         # Jinja2 parsing/rendering
├── metadata.rs         # Data structures
├── validation.rs       # Security validation
├── add_prompt.rs       # AddPromptTool
├── edit_prompt.rs      # EditPromptTool
├── delete_prompt.rs    # DeletePromptTool
├── get_prompt.rs       # GetPromptTool
└── defaults.rs         # Embedded default prompts

data/default_prompts/   # Default templates
examples/               # Integration examples
```

## Architecture

### Core Components

- **PromptManager**: Orchestrates all prompt operations with async file I/O
- **Template Engine**: Parses YAML frontmatter and renders Jinja2 templates
- **MCP Tools**: Four tools implementing the MCP Tool trait
- **Validation System**: Security-focused validation with size limits and forbidden directives
- **Default Prompts**: Compile-time embedded templates for common workflows

### Security Features

1. **Template Size Limit**: Maximum 1MB per template
2. **Forbidden Directives**: Blocks `{% include %}`, `{% extends %}`, `{% import %}`
3. **Path Traversal Prevention**: Name validation prevents directory traversal
4. **Environment Variable Whitelist**: Only safe variables exposed (USER, HOME, SHELL, PWD, EDITOR, TERM)
5. **Recursion Limits**: MiniJinja built-in protection (~500 levels)
6. **Timeout Enforcement**: 5-second rendering timeout

See [CLAUDE.md](CLAUDE.md) for detailed architecture documentation.

## Dependencies

### Core Dependencies

- **[rmcp](https://github.com/modelcontextprotocol/rust-sdk)** (0.8) - MCP SDK for server/client/transport
- **[minijinja](https://github.com/mitsuhiko/minijinja)** (2) - Jinja2 template engine
- **[gray_matter](https://github.com/kytta/gray_matter-rs)** (0.3) - YAML frontmatter parsing
- **[tokio](https://tokio.rs)** (1) - Async runtime
- **[serde](https://serde.rs)** / **serde_json** (1) - Serialization
- **[anyhow](https://github.com/dtolnay/anyhow)** (1) - Error handling
- **[dirs](https://github.com/dirs-dev/dirs-rs)** (6) - Cross-platform paths

### KODEGEN Dependencies

- **kodegen_mcp_tool** (0.1) - Tool trait definitions
- **kodegen_mcp_schema** (0.1) - Args schema definitions
- **kodegen_server_http** (0.1) - HTTP server framework

## Default Prompts

The server ships with curated default prompts:

- **getting_started** - Introduction to Kodegen MCP basics
- **code_generation** - Code generation workflow examples
- **env_demo** - Environment variable usage demonstration
- **refactor_example** - Refactoring workflow template

Default prompts are automatically installed on first run if the prompts directory is empty.

## Contributing

Contributions are welcome! Please ensure:

1. Code is formatted with `cargo fmt`
2. All tests pass with `cargo test`
3. Clippy produces no warnings: `cargo clippy -- -D warnings`
4. New features include appropriate tests
5. Security considerations are documented

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE.md) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT License ([LICENSE-MIT](LICENSE.md) or http://opensource.org/licenses/MIT)

at your option.

## Related Projects

- [KODEGEN.ai](https://kodegen.ai) - Main project website
- [MCP Protocol](https://modelcontextprotocol.io/) - Model Context Protocol specification
- [MiniJinja](https://github.com/mitsuhiko/minijinja) - Jinja2 template engine for Rust

---

**Built with ❤️ by the KODEGEN.ai team**
