//! Task Complete tool - Mark plan tasks as complete
//!
//! This tool is intercepted by the UI and handled specially.
//! It updates the active plan's task status immediately.
//! Supports batch completion via task_ids array.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tools::registry::{Tool, ToolContext, ToolResult};

pub struct TaskCompleteTool;

#[async_trait]
impl Tool for TaskCompleteTool {
    fn name(&self) -> &str {
        "task_complete"
    }

    fn description(&self) -> &str {
        "Mark tasks as complete in the active plan. Supports single task (task_id) or batch (task_ids array). Silent - no announcement needed."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "Single task ID to mark complete (e.g., '1.1')"
                },
                "task_ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Multiple task IDs to mark complete (e.g., ['1.1', '1.2', '2.1'])"
                }
            },
            "additionalProperties": false
        })
    }

    async fn execute(&self, _params: Value, _ctx: &ToolContext) -> ToolResult {
        // This tool is handled specially by the UI - this code shouldn't run
        ToolResult {
            output: json!({ "note": "Task completion handled by UI" }).to_string(),
            is_error: false,
        }
    }
}
