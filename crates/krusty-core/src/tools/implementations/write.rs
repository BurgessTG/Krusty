//! Write tool - Create or overwrite files

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::fs;
use tracing::info;

use crate::tools::{parse_params, ToolContext, ToolResult};
use crate::tools::registry::Tool;

pub struct WriteTool;

#[derive(Deserialize)]
struct Params {
    file_path: String,
    content: String,
}

#[async_trait]
impl Tool for WriteTool {
    fn name(&self) -> &str {
        "write"
    }

    fn description(&self) -> &str {
        "Create or overwrite files. Creates parent directories if needed. Reports LSP errors after write."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to write"
                },
                "content": {
                    "type": "string",
                    "description": "The content to write to the file"
                }
            },
            "required": ["file_path", "content"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> ToolResult {
        let params = match parse_params::<Params>(params) {
            Ok(p) => p,
            Err(e) => return e,
        };

        // First resolve the path normally
        let path = ctx.resolve_path(&params.file_path);
        info!("Write tool: resolved path = {:?}, working_dir = {:?}", path, ctx.working_dir);

        // Validate sandbox if configured (must check before creating directories)
        if let Some(ref sandbox) = ctx.sandbox_root {
            // For new files, validate the parent directory is within sandbox
            let check_path = if path.exists() {
                path.clone()
            } else {
                path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| path.clone())
            };

            if let Ok(canonical) = check_path.canonicalize() {
                if !canonical.starts_with(sandbox) {
                    return ToolResult::error(format!(
                        "Access denied: path '{}' is outside workspace",
                        params.file_path
                    ));
                }
            } else if !check_path.starts_with(sandbox) {
                // Parent doesn't exist yet - check if it would be within sandbox
                return ToolResult::error(format!(
                    "Access denied: path '{}' is outside workspace",
                    params.file_path
                ));
            }
        }

        // Create parent directories if needed
        if let Some(parent) = path.parent().filter(|p| !p.exists()) {
            info!("Write tool: creating parent directory {:?}", parent);
            if let Err(e) = fs::create_dir_all(parent).await {
                return ToolResult::error(format!("Failed to create directory: {}", e));
            }
        }

        match fs::write(&path, &params.content).await {
            Ok(_) => {
                ctx.touch_file(&path, true).await;

                let mut output = json!({
                    "message": format!("Successfully wrote {} lines", params.content.lines().count()),
                    "bytes_written": params.content.len(),
                    "file_path": path.display().to_string()
                })
                .to_string();

                if let Some(diagnostics) = ctx.get_file_diagnostics(&path) {
                    output.push_str("\n\n");
                    output.push_str(&diagnostics);
                    output.push_str("This file has errors. Please fix them.");
                }

                ToolResult::success(output)
            }
            Err(e) => ToolResult::error(format!("Failed to write file: {}", e)),
        }
    }
}
