//! Grep tool - Search files with ripgrep (SDK-aligned)

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::process::Stdio;
use tokio::process::Command;

use crate::tools::{parse_params, ToolContext, ToolResult};
use crate::tools::registry::Tool;

pub struct GrepTool;

#[derive(Deserialize)]
struct Params {
    pattern: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    glob: Option<String>,
    #[serde(default, rename = "type")]
    file_type: Option<String>,
    #[serde(default)]
    output_mode: Option<String>,
    #[serde(default, rename = "-i")]
    case_insensitive: Option<bool>,
    #[serde(default, rename = "-C")]
    context: Option<usize>,
    #[serde(default)]
    head_limit: Option<usize>,
}

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "Search file contents with regex (ripgrep). Supports output_mode: content/files_with_matches/count. Use glob or type params to filter files."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The regex pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "Directory or file to search in (default: current directory)"
                },
                "glob": {
                    "type": "string",
                    "description": "File glob pattern (e.g., '*.rs', '*.{ts,tsx}')"
                },
                "type": {
                    "type": "string",
                    "description": "File type to search (e.g., 'rust', 'python', 'js')"
                },
                "output_mode": {
                    "type": "string",
                    "enum": ["content", "files_with_matches", "count"],
                    "description": "Output mode: content (default), files_with_matches, or count"
                },
                "-i": {
                    "type": "boolean",
                    "description": "Case insensitive search"
                },
                "-C": {
                    "type": "number",
                    "description": "Lines of context to show before and after each match"
                },
                "head_limit": {
                    "type": "number",
                    "description": "Limit output to first N matches"
                }
            },
            "required": ["pattern"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> ToolResult {
        let params = match parse_params::<Params>(params) {
            Ok(p) => p,
            Err(e) => return e,
        };

        let output_mode = params.output_mode.as_deref().unwrap_or("content");
        let head_limit = params.head_limit.unwrap_or(100);

        let mut cmd = Command::new("rg");
        cmd.arg("--json");

        if params.case_insensitive.unwrap_or(false) {
            cmd.arg("-i");
        }

        if let Some(c) = params.context {
            cmd.arg("-C").arg(c.to_string());
        }

        if let Some(glob) = &params.glob {
            cmd.arg("--glob").arg(glob);
        }

        if let Some(file_type) = &params.file_type {
            cmd.arg("--type").arg(file_type);
        }

        cmd.arg(&params.pattern);

        // Resolve and validate search path within sandbox
        match &params.path {
            Some(path) => {
                let resolved = match ctx.sandboxed_resolve(path) {
                    Ok(p) => p,
                    Err(e) => return ToolResult::error(e),
                };
                cmd.arg(&resolved);
            }
            None => {
                cmd.current_dir(&ctx.working_dir);
            }
        }

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        match cmd.output().await {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                if !output.status.success() && output.status.code() != Some(1) {
                    return ToolResult::error(format!("Search failed: {}", stderr));
                }

                parse_rg_json(&stdout, output_mode, head_limit)
            }
            Err(e) => ToolResult::error(format!(
                "Failed to execute ripgrep: {}. Is 'rg' installed?",
                e
            )),
        }
    }
}

/// Parse ripgrep JSON output into SDK-compatible format
fn parse_rg_json(stdout: &str, output_mode: &str, head_limit: usize) -> ToolResult {
    let mut matches: Vec<Value> = Vec::new();
    let mut files_seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut file_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut total_matches = 0;

    for line in stdout.lines() {
        if line.is_empty() {
            continue;
        }

        let event: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let event_type = event.get("type").and_then(|t| t.as_str()).unwrap_or("");

        if event_type == "match" {
            total_matches += 1;

            let data = match event.get("data") {
                Some(d) => d,
                None => continue,
            };

            let file = data
                .get("path")
                .and_then(|p| p.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or("");

            files_seen.insert(file.to_string());
            *file_counts.entry(file.to_string()).or_insert(0) += 1;

            // For content mode, collect match details
            if output_mode == "content" && matches.len() < head_limit {
                let line_number = data.get("line_number").and_then(|n| n.as_u64());
                let line_text = data
                    .get("lines")
                    .and_then(|l| l.get("text"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .trim_end();

                let mut match_obj = json!({
                    "file": file,
                    "line": line_text
                });

                if let Some(ln) = line_number {
                    match_obj["line_number"] = json!(ln);
                }

                matches.push(match_obj);
            }
        }
    }

    // Build output based on mode
    let output = match output_mode {
        "files_with_matches" => {
            let files: Vec<String> = files_seen.into_iter().take(head_limit).collect();
            let count = files.len();
            json!({
                "files": files,
                "count": count
            })
        }
        "count" => {
            let counts: Vec<Value> = file_counts
                .into_iter()
                .take(head_limit)
                .map(|(file, count)| json!({"file": file, "count": count}))
                .collect();
            json!({
                "counts": counts,
                "total": total_matches
            })
        }
        _ => {
            // content mode (default)
            json!({
                "matches": matches,
                "total_matches": total_matches
            })
        }
    };

    ToolResult::success(output.to_string())
}
