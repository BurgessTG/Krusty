//! Plan Mode tool - AI-controlled mode switching
//!
//! This tool is intercepted by the UI and handled specially.
//! It allows the AI to switch into Plan mode when the user requests planning.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tools::registry::{Tool, ToolContext, ToolResult};

pub struct EnterPlanModeTool;

#[async_trait]
impl Tool for EnterPlanModeTool {
    fn name(&self) -> &str {
        "enter_plan_mode"
    }

    fn description(&self) -> &str {
        "Switch to Plan mode to create a new implementation plan. Use this when the user asks you to plan something, design an approach, or when you need to create a structured plan before implementing. In Plan mode, you can explore the codebase but cannot make modifications. After creating a plan, the user will review and approve it before implementation begins."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "reason": {
                    "type": "string",
                    "description": "Brief explanation of why planning is needed (shown to user)"
                },
                "clear_existing": {
                    "type": "boolean",
                    "description": "If true, clears any existing active plan. Use when user wants to start fresh."
                }
            },
            "required": ["reason"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, _params: Value, _ctx: &ToolContext) -> ToolResult {
        // This tool is handled specially by the UI - this code shouldn't run
        ToolResult {
            output: json!({
                "note": "Plan mode switch handled by UI"
            }).to_string(),
            is_error: false,
        }
    }
}
