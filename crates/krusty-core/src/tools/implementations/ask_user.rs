//! AskUserQuestion tool - Interactive user prompts
//!
//! This tool allows Claude to ask the user clarifying questions with
//! multiple choice options. It's intercepted in the UI layer (streaming.rs)
//! and never actually executes here - the UI handles showing the prompt
//! and sending the tool result back.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tools::{ToolContext, ToolResult};
use crate::tools::registry::Tool;

pub struct AskUserQuestionTool;

#[async_trait]
impl Tool for AskUserQuestionTool {
    fn name(&self) -> &str {
        "AskUserQuestion"
    }

    fn description(&self) -> &str {
        r#"Ask the user a question to clarify requirements or get their preference.

Use this tool when:
- Requirements are ambiguous and you need clarification
- Multiple valid approaches exist and user preference matters
- You need to confirm before making significant changes
- The user should choose between specific options

Guidelines:
- Use sparingly - only when you genuinely can't proceed without input
- Provide clear, distinct options (2-4 is ideal)
- Include a brief description for each option explaining trade-offs
- For yes/no questions, just ask directly in chat instead

The user can select an option by number, click, or type a custom response."#
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "questions": {
                    "type": "array",
                    "description": "Array of questions to ask (usually just one)",
                    "items": {
                        "type": "object",
                        "properties": {
                            "header": {
                                "type": "string",
                                "description": "Short label for the question (shown in title bar, max 20 chars)"
                            },
                            "question": {
                                "type": "string",
                                "description": "The full question text to display"
                            },
                            "options": {
                                "type": "array",
                                "description": "Available options for the user to choose from",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "label": {
                                            "type": "string",
                                            "description": "Option label (what user selects)"
                                        },
                                        "description": {
                                            "type": "string",
                                            "description": "Brief explanation of this option"
                                        }
                                    },
                                    "required": ["label"],
                                    "additionalProperties": false
                                }
                            },
                            "multiSelect": {
                                "type": "boolean",
                                "description": "Allow selecting multiple options (default: false)"
                            }
                        },
                        "required": ["header", "question", "options"],
                        "additionalProperties": false
                    }
                }
            },
            "required": ["questions"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, _params: Value, _ctx: &ToolContext) -> ToolResult {
        // This tool is intercepted in streaming.rs and handled by the UI.
        // If we get here, something went wrong with the interception.
        ToolResult::error("AskUserQuestion should be handled by UI layer, not executed directly")
    }
}
