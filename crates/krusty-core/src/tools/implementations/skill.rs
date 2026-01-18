//! Skill tool - Invoke skills to get specialized instructions

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::tools::{parse_params, ToolContext, ToolResult};
use crate::tools::registry::Tool;

pub struct SkillTool;

#[derive(Deserialize)]
struct Params {
    /// Name of the skill to invoke
    skill: String,
    /// Optional: specific file within the skill to read
    #[serde(default)]
    file: Option<String>,
}

#[async_trait]
impl Tool for SkillTool {
    fn name(&self) -> &str {
        "skill"
    }

    fn description(&self) -> &str {
        "Invoke a skill to get specialized instructions and guidance. Use this when a task matches an available skill's description."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "skill": {
                    "type": "string",
                    "description": "The name of the skill to invoke (e.g., 'git-commit', 'code-review')"
                },
                "file": {
                    "type": "string",
                    "description": "Optional: specific file within the skill to read (e.g., 'CHECKLIST.md')"
                }
            },
            "required": ["skill"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> ToolResult {
        let params = match parse_params::<Params>(params) {
            Ok(p) => p,
            Err(e) => return e,
        };

        let Some(skills_manager) = &ctx.skills_manager else {
            return ToolResult::error("Skills manager not available");
        };

        let mut manager = skills_manager.write().await;

        // If a specific file is requested, load that
        if let Some(ref file) = params.file {
            return match manager.load_file_from_skill(&params.skill, file) {
                Ok(content) => ToolResult::success(content),
                Err(e) => ToolResult::error(format!("Failed to load {}: {}", file, e)),
            };
        }

        // Load the main SKILL.md content
        match manager.load_skill_content(&params.skill) {
            Ok(content) => ToolResult::success(content),
            Err(e) => ToolResult::error(format!("Skill '{}' not found: {}", params.skill, e)),
        }
    }
}
