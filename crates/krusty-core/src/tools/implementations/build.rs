//! Build tool - Spawn parallel Opus builder agents (The Kraken)
//!
//! This tool spawns a team of Opus agents that work together to build code.
//! Builders coordinate via SharedBuildContext to share types, modules, and file locks.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::agent::subagent::{SubAgentModel, SubAgentPool, SubAgentTask};
use crate::agent::{AgentCancellation, SharedBuildContext};
use crate::ai::anthropic::AnthropicClient;
use crate::tools::registry::{Tool, ToolContext, ToolResult};

/// Build tool for spawning parallel Opus builder agents
pub struct BuildTool {
    client: Arc<AnthropicClient>,
    cancellation: AgentCancellation,
}

impl BuildTool {
    pub fn new(client: Arc<AnthropicClient>, cancellation: AgentCancellation) -> Self {
        Self {
            client,
            cancellation,
        }
    }
}

#[derive(Deserialize)]
struct Params {
    /// The overall build goal/requirements
    prompt: String,

    /// Components to build in parallel (one agent per component)
    #[serde(default)]
    components: Option<Vec<String>>,

    /// Coding conventions all builders must follow
    #[serde(default)]
    conventions: Option<Vec<String>>,

    /// Maximum concurrent builders (default: 3 for Opus)
    #[serde(default = "default_concurrency")]
    max_concurrency: usize,

    /// Plan task IDs corresponding to each component (for auto-marking)
    /// Index i maps to components[i]
    #[serde(default)]
    task_ids: Option<Vec<String>>,
}

fn default_concurrency() -> usize {
    3 // Opus is expensive, keep concurrency lower
}

#[async_trait]
impl Tool for BuildTool {
    fn name(&self) -> &str {
        "build"
    }

    fn description(&self) -> &str {
        "Launch a team of parallel Opus builder agents to implement code together. \
         IMPORTANT: Builders share type signatures, module paths, and file locks to coordinate. \
         USE THIS TOOL ONLY when the user explicitly asks for: \
         'unleash the kraken', 'release the kraken', 'team of agents', 'squad of builders', \
         'agent swarm', 'parallel agents', 'builder swarm', or 'multiple agents working together'. \
         Do NOT use for normal coding tasks - only when user wants parallel agent coordination. \
         Pass 'components' array to assign work (e.g., ['auth module', 'api endpoints', 'database layer']). \
         Returns aggregated implementation results with line diff stats."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "Overall build goal and requirements for the builder team"
                },
                "components": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Components to build in parallel. Each gets its own Opus builder agent. Example: ['auth module', 'api endpoints', 'database models']"
                },
                "conventions": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Coding conventions all builders must follow. Example: ['Use anyhow for errors', 'Add tracing logs']"
                }
            },
            "required": ["prompt"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> ToolResult {
        info!(
            "Build tool (Kraken) execute called with params: {:?}",
            params
        );

        let params: Params = match serde_json::from_value(params) {
            Ok(p) => p,
            Err(e) => {
                warn!("Build tool: Invalid parameters: {}", e);
                return ToolResult {
                    output: json!({"error": format!("Invalid parameters: {}", e)}).to_string(),
                    is_error: true,
                };
            }
        };

        // Create shared build context
        let context = Arc::new(SharedBuildContext::new());

        // Set conventions if provided
        if let Some(conventions) = &params.conventions {
            context.set_conventions(conventions.clone());
        }

        // Build tasks - all use Opus for high-quality code generation
        let mut tasks: Vec<SubAgentTask> = Vec::new();

        if let Some(ref components) = params.components {
            let total = components.len();
            let other_components: Vec<_> = components.iter().map(|c| c.as_str()).collect();

            // One agent per component - each gets their own file for TRUE parallelism
            for (i, component) in components.iter().enumerate() {
                let name = component.split_whitespace().next().unwrap_or("builder");
                let others: Vec<_> = other_components
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(j, c)| format!("  - Builder {}: {}", j, c))
                    .collect();

                // Create detailed prompt emphasizing SEPARATE FILES
                let task_prompt = format!(
                    "You are Builder {} of {} in a parallel build team.\n\n\
                     YOUR COMPONENT: {}\n\n\
                     OVERALL GOAL:\n{}\n\n\
                     OTHER BUILDERS (working in parallel):\n{}\n\n\
                     PARALLEL BUILD STRATEGY:\n\
                     1. Create YOUR OWN file(s) for your component - don't wait for others\n\
                     2. Name files clearly: {}_something.ext (e.g., game_engine.py, snake_logic.py)\n\
                     3. If you need to import from another builder's module, assume it exists\n\
                     4. Export clear interfaces (functions, classes) others can import\n\
                     5. At the end, if a main.py/main.rs is needed, Builder 0 creates it and imports all modules\n\n\
                     COORDINATION:\n\
                     - Check [SHARED TYPES] for interfaces other builders registered\n\
                     - Register YOUR public functions/classes so others can import them\n\
                     - File locks are automatic - but you shouldn't need them if using separate files\n\n\
                     BUILD YOUR COMPONENT NOW. Create your file(s) and implement fully.",
                    i, total,
                    component,
                    params.prompt,
                    if others.is_empty() { "  (none - you're solo)".to_string() } else { others.join("\n") },
                    name.to_lowercase().replace(' ', "_")
                );

                let mut task = SubAgentTask::new(format!("builder-{}", i), task_prompt)
                    .with_name(name)
                    .with_model(SubAgentModel::Opus)
                    .with_working_dir(ctx.working_dir.clone());

                // Attach plan task ID if provided for auto-completion
                if let Some(ref task_ids) = params.task_ids {
                    if let Some(plan_task_id) = task_ids.get(i) {
                        task = task.with_plan_task_id(plan_task_id);
                    }
                }

                tasks.push(task);
            }
        } else {
            // Single builder for the whole task
            tasks.push(
                SubAgentTask::new("builder-main", params.prompt.clone())
                    .with_name("main")
                    .with_model(SubAgentModel::Opus)
                    .with_working_dir(ctx.working_dir.clone()),
            );
        }

        info!("Build tool: Created {} builder tasks", tasks.len());
        for (i, task) in tasks.iter().enumerate() {
            debug!(
                "Builder {}: id={}, name={}, model={:?}",
                i, task.id, task.name, task.model
            );
        }

        // Create pool and execute with build context
        let pool = SubAgentPool::new(self.client.clone(), self.cancellation.clone())
            .with_concurrency(params.max_concurrency)
            .with_override_model(ctx.current_model.clone());

        info!(
            "Build tool: Starting Kraken with max_concurrency={}",
            params.max_concurrency
        );

        // Execute builders with progress channel if available
        let results = if let Some(ref progress_tx) = ctx.build_progress_tx {
            pool.execute_builders(tasks, context.clone(), progress_tx.clone())
                .await
        } else {
            // Fallback: create a dummy channel and discard progress
            let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
            pool.execute_builders(tasks, context.clone(), tx).await
        };

        info!("Build tool: Kraken returned {} results", results.len());

        // Get final stats from context
        let stats = context.stats();

        // Format results
        let mut output = String::new();
        let mut all_files: Vec<String> = Vec::new();
        let mut total_turns = 0;
        let mut total_duration_ms = 0u64;
        let mut errors: Vec<String> = Vec::new();

        for result in &results {
            if result.success {
                output.push_str(&format!("\n## Builder: {}\n", result.task_id));
                output.push_str(&result.output);
                output.push('\n');
            } else if let Some(err) = &result.error {
                errors.push(format!("{}: {}", result.task_id, err));
            }

            all_files.extend(result.files_examined.clone());
            total_turns += result.turns_used;
            total_duration_ms += result.duration_ms;
        }

        // Add summary with build stats
        let summary = format!(
            "\n---\n**Build Complete**: {} builders, {} turns, {}ms\n\
             **Changes**: +{} -{} lines, {} files\n\
             **Locks**: {} contentions",
            results.len(),
            total_turns,
            total_duration_ms,
            stats.lines_added,
            stats.lines_removed,
            stats.files_modified,
            stats.lock_contentions,
        );
        output.push_str(&summary);

        if !errors.is_empty() {
            output.push_str("\n**Errors**: ");
            output.push_str(&errors.join(", "));
        }

        ToolResult {
            output,
            is_error: false,
        }
    }
}
