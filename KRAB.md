# krusty-public

## Tech Stack

- Rust

## Architecture

Main modules in **krusty-core**:
- **acp**: Agent Client Protocol server - enables Krusty to act as an ACP-compatible agent for editors (Zed, Neovim, JetBrains)
- **agent**: Core agent system with event bus, state tracking, hooks, dual-mind architecture, sub-agent pool, and summarization
- **ai**: Multi-provider AI layer supporting Anthropic, OpenAI, Google, OpenRouter, OpenCodeZen formats with streaming and retry logic
- **auth**: Authentication system with OAuth, device flow, PKCE, and credential storage
- **extensions**: WASM extension support (Zed-compatible) for adding LSP capabilities
- **index**: Smart codebase memory system with tree-sitter parsing, local embeddings (fastembed), semantic retrieval, and insight accumulation
- **mcp**: Model Context Protocol client supporting local (stdio) and remote (URL) MCP servers
- **plan**: Task planning with checkboxes, plan mode restrictions, and database persistence
- **process**: Background process tracking with spawn/kill/suspend/resume capabilities and multi-tenant user isolation
- **skills**: Filesystem-based skill system for extending Claude capabilities with domain-specific expertise
- **storage**: SQLite persistence for sessions, plans, preferences, file activity tracking, and API credentials
- **tools**: Tool registry and implementations (glob, grep, read, write, bash, etc.) with image and MCP tool support
- **lsp**: LSP integration utilities

**krusty-cli** crates add TUI with ratatui, gamepad support, syntax highlighting, auth flows, and terminal/PTY handling.

**Key Design Patterns**:

1. **Builder Pattern**: `DualMindBuilder`, `SubAgentPool::new()` with chained configuration methods
2. **Event Bus Pattern**: `AgentEventBus` for centralized event dispatching
3. **Manager/Registry Pattern**: `McpManager`, `ToolRegistry`, `ProcessRegistry`, `PlanManager` for lifecycle and discovery
4. **Strategy Pattern**: Format handlers for different AI providers (Anthropic vs OpenAI vs Google) routed via `uses_openai_format()`, `uses_google_format()`, etc.
5. **State Pattern**: `AgentState`, `SessionState`, `ProcessStatus` for state machine behavior
6. **Repository Pattern**: `InsightStore`, `PlanStore`, `CredentialStore` for data access abstraction
7. **Dependency Injection**: `Arc<AiClient>`, `Arc<ToolRegistry>` passed through constructors
8. **Dual-Mind Architecture**: Big Claw (executor) + Little Claw (analyst) with pre/post-review dialogue
9. **Sub-Agent Pattern**: `SubAgentPool` for concurrent lightweight agent execution with semaphore-controlled concurrency and staggered spawning
10. **Protocol Wrapper Pattern**: `AcpServer`, `McpManager` implementing standardized JSON-RPC/protobuf protocols
11. **Pinch Context**: Session transition pattern for preserving context across restarts
12. **Hook Pattern**: `SafetyHook`, `LoggingHook`, `UserHook` for extensible agent behavior

## Key Files

- `crates/krusty-core/src/lib.rs` - Main library entry point re-exporting AI clients, indexers, storage, tools, and MCP support
- `crates/krusty-core/src/agent/mod.rs` - Core agent system with event handling, state tracking, hooks, and dual-mind reasoning (Big Claw/Little Claw)
- `crates/krusty-core/src/index/mod.rs` - Semantic codebase indexing using tree-sitter AST parsing and local embeddings via fastembed
- `crates/krusty-core/src/plan/mod.rs` - SQLite-backed multi-phase task planning system with session linkage and legacy file migration
- `crates/krusty-core/src/ai/mod.rs` - AI provider layer supporting Anthropic, OpenRouter, OpenCodeZen with format detection and retry logic
- `crates/krusty-core/src/tools/registry.rs` - Tool registry with pre/post execution hooks, timeout handling, sandboxed path resolution for security
- `crates/krusty-core/src/storage/mod.rs` - SQLite persistence for sessions, plans, preferences, credentials, and file activity tracking
- `crates/krusty-core/src/acp/mod.rs` - Agent Client Protocol server enabling editor integration (Zed, Neovim, JetBrains) via JSON-RPC 2.0
- `crates/krusty-cli/src/tui/mod.rs` - Terminal UI framework with app lifecycle, handlers, rendering, streaming, plugins, and theming
- `crates/krusty-core/src/mcp/manager.rs` - Model Context Protocol manager for extending AI capabilities with external tools

## Conventions

### Error Handling
- **Framework**: `anyhow::Result` + `thiserror::Error`
- Pattern: Use `anyhow::Result` for application-level error handling, `thiserror` for specific error enums
- Example: `AcpError` enum derives `Error`, implements `From<anyhow::Error>`
- Error messages use `#[error("...")]` attribute format

### Logging
- **Framework**: `tracing`
- Levels used: `tracing::info!`, `tracing::debug!`, `tracing::warn!`, `tracing::error!`
- Structured logging: `tracing::info!(key = %value, "message")` style

### Async
- **Runtime**: `tokio` (version 1.40, features = ["full"])
- Pattern: `async fn` returning `Result<T>`
- `async-trait = "0.1"` for trait method async support

### Testing
- **Framework**: Built-in Rust `#[test]` with `#[cfg(test)]`
- **Location**: Inline within source files (`crates/*/src/*_tests.rs` and inline `#[cfg(test)] mod tests`)
- **Async tests**: `#[tokio::test]` decorator
- **Test helpers**: `tempfile` for temporary directories

### Naming Conventions
- **Types**: `PascalCase` (structs, enums, traits)
- **Functions/variables**: `snake_case`
- **Constants**: `SCREAMING_SNAKE_CASE` (e.g., `INDEX_VERSION`, `EMBEDDING_DIM`)
- **Builder methods**: `with_*` prefix (e.g., `with_embeddings`)
- **Getters**: `get_*` prefix (e.g., `get_symbol`, `get_by_id`, `get_stats`)
- **Setters**: `set_*` prefix (e.g., `set_mode`, `set_dual_mind_enabled`)
- **Event handlers**: `handle_*` prefix (e.g., `handle_event`)
- **Lifecycle hooks**: `on_*` prefix (e.g., `on_activate`, `on_deactivate`)
- **Public API**: Explicit `pub` visibility on types and functions

### Key Files Examined
- `/home/burgess/Work/Krusty-Dev/krusty-public/Cargo.toml`
- `/home/burgess/Work/Krusty-Dev/krusty-public/crates/krusty-core/Cargo.toml`
- `/home/burgess/Work/Krusty-Dev/krusty-public/crates/krusty-core/src/acp/error.rs`
- `/home/burgess/Work/Krusty-Dev/krusty-public/crates/krusty-core/src/storage/database_tests.rs`
- Multiple source files for grep patterns on naming/patterns

## Build & Run

```bash
cargo build                              # Debug build
cargo build --release                    # Release build
cargo check                              # Type check without building
cargo test                               # Run tests
cargo clippy                             # Run lints
cargo clippy -- -D warnings              # Lints as errors
cargo fmt                                # Format code
cargo tree -p krusty -i                  # Show dependency tree
cargo update -p <package>                # Update specific dependency
```


**Runtime & Async**
- tokio
- async-trait
- futures
- tokio-stream

**Error Handling**
- anyhow
- thiserror

**Serialization**
- serde (+ derive)
- serde_json
- serde_yaml
- toml

**Logging**
- tracing
- tracing-subscriber

**HTTP**
- reqwest

**Terminal UI**
- ratatui
- ratatui-image
- crossterm
- unicode-width
- textwrap
- arboard

**Database**
- rusqlite

**WASM Extensions**
- wasmtime
- wasmtime-wasi
- wasmparser

**Code Parsing**
- tree-sitter

**LSP**
- lsp-types

**CLI**
- clap

**Version Control**
- git2

**Local Embeddings**
- fastembed

**Image Processing**
- image

**Utilities**
- chrono
- uuid
- regex
- which
- walkdir
- glob
- shell-words
- dirs
- url

## Notes for AI

<!-- Add project-specific instructions here -->

