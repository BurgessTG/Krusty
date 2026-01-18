//! Read tool - Read file contents

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::fs;

use crate::tools::{parse_params, ToolContext, ToolResult};
use crate::tools::registry::Tool;

pub struct ReadTool;

#[derive(Deserialize)]
struct Params {
    file_path: String,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
}

#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &str {
        "read"
    }

    fn description(&self) -> &str {
        "Read file contents. Supports line offset/limit for large files. Detects binary files."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The absolute path to the file to read"
                },
                "offset": {
                    "type": "number",
                    "description": "The line number to start reading from (1-indexed)"
                },
                "limit": {
                    "type": "number",
                    "description": "The number of lines to read"
                }
            },
            "required": ["file_path"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> ToolResult {
        let params = match parse_params::<Params>(params) {
            Ok(p) => p,
            Err(e) => return e,
        };

        // Use sandboxed resolve for multi-tenant path isolation
        let path = match ctx.sandboxed_resolve(&params.file_path) {
            Ok(p) => p,
            Err(e) => {
                // Fall back to regular resolve if file doesn't exist yet (for better error message)
                let fallback = ctx.resolve_path(&params.file_path);
                if !fallback.exists() {
                    return ToolResult::error(format!("File not found: {}", params.file_path));
                }
                return ToolResult::error(e);
            }
        };

        if !path.is_file() {
            return ToolResult::error(format!("Path is not a file: {}", path.display()));
        }

        let content = match fs::read(&path).await {
            Ok(bytes) => bytes,
            Err(e) => return ToolResult::error(format!("Failed to read file: {}", e)),
        };

        // Check for binary
        let check_len = content.len().min(8192);
        if content[..check_len].contains(&0) {
            let size = content.len();
            let size_str = match size {
                0..1024 => format!("{} bytes", size),
                1024..1_048_576 => format!("{:.1} KB", size as f64 / 1024.0),
                _ => format!("{:.1} MB", size as f64 / 1_048_576.0),
            };
            return ToolResult::success(
                json!({
                    "content": format!("Binary file: {} ({})", path.display(), size_str),
                    "total_lines": 0,
                    "lines_returned": 0
                })
                .to_string(),
            );
        }

        let content = match String::from_utf8(content) {
            Ok(s) => s,
            Err(e) => return ToolResult::error(format!("File is not valid UTF-8: {}", e)),
        };

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        let start = params.offset.unwrap_or(1).saturating_sub(1);
        let limit = params.limit.unwrap_or(2000);
        let end = (start + limit).min(total_lines);

        if start >= total_lines {
            return ToolResult::error(format!(
                "Start line {} is beyond file length ({})",
                start + 1,
                total_lines
            ));
        }

        let content = lines[start..end].join("\n");

        ToolResult::success(
            json!({
                "content": content,
                "total_lines": total_lines,
                "lines_returned": end - start,
                "start_line": start + 1
            })
            .to_string(),
        )
    }
}
