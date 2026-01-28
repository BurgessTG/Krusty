# krusty-public

## Tech Stack

- Rust

## Architecture

## Main Modules/Crates

### krusty-core (Core Library)
- **acp**: Agent Client Protocol server - JSON-RPC 2.0 interface for editor integration (Zed, Neovim, JetBrains)
- **agent**: Agent system with event bus, hooks, subagents, and Dual-Mind architecture (Big Claw/Little Claw)
- **ai**: Multi-provider AI layer supporting Anthropic, OpenRouter, OpenCodeZen, Google with format detection
- **auth**: OAuth/PKCE/device flow authentication for API providers
- **extensions**: Zed-compatible WASM extension system with runtime, manifest, and GitHub integration
- **index**: Smart codebase memory with tree-sitter parsing, local embeddings (fastembed), and semantic retrieval
- **lsp**: Language Server Protocol manager - spawns/manages multiple LSP servers per project
- **mcp**: Model Context Protocol client for local (stdio) and remote (URL) MCP servers
- **plan**: Plan management with SQLite-backed storage, phase/task tracking
- **process**: Process registry for tracking and managing spawned processes
- **skills**: Skills manager for global and project-specific skill loading (SKILL.md format)
- **storage**: SQLite persistence for sessions, plans, preferences, credentials, file activity
- **tools**: Tool registry with trait-based implementations, hooks, and sandbox enforcement

### krusty-cli (TUI Application)
- **tui**: Terminal UI built with ratatui - blocks (read, write, bash, explore), components, handlers, plugins
- **plugins**: Retro game plugins (brick_breaker, pew_pew, libretro, gamepad) - easter eggs for the CLI

## Key Design Patterns

1. **Dual-Mind Architecture (Big Claw/Little Claw)**: Two independent agent instances collaborate - Big Claw executes, Little Claw reviews/validates, dialogue appears in thinking blocks

2. **Trait-based Plugin System**: `Tool` trait with async_trait for extensibility; `PreToolHook`/`PostToolHook` hooks for logging, safety, validation

3. **Registry Pattern**: ToolRegistry, LspManager, SkillsManager, McpManager provide centralized lookup/management

4. **Event Bus Architecture**: AgentEventBus for centralized event dispatching across the agent system

5. **Strategy Pattern**: Multiple AI provider implementations (Anthropic, OpenRouter, OpenCodeZen) via AiClientConfig

6. **Repository Pattern**: PlanStore, SessionManager, CredentialStore abstract SQLite operations

7. **Builder Swarm (Octopod)**: SharedBuildContext coordinates multiple builder agents with type registry and file locks

8. **ACP Bridge**: JSON-RPC 2.0 over stdio for editor integration; KrustyAgent handles session/prompt/update requests

9. **WASM Extension Host**: Zed-compatible extensions loaded via wasmtime with command/manifest support

10. **Builder Pattern**: DualMindBuilder, AppBuilder for fluent configuration

## Key Files

- `crates/krusty-core/src/lib.rs` - Core library exports for AI, storage, tools, LSP/MCP, and agent systems
- `crates/krusty-cli/src/tui/app.rs` - Main TUI application with event loop, state management, and terminal rendering
- `crates/krusty-core/src/agent/mod.rs` - Agent system with event bus, hooks, sub-agents, and dual-mind quality control
- `crates/krusty-core/src/ai/mod.rs` - AI provider layer supporting Anthropic, OpenRouter, OpenCodeZen, and other providers
- `crates/krusty-core/src/index/indexer.rs` - Codebase indexer with Rust symbol parsing and semantic embeddings
- `crates/krusty-core/src/tools/registry.rs` - Tool registry managing file, bash, explore, and build tools with sandbox support
- `crates/krusty-core/src/storage/database.rs` - SQLite database wrapper with versioned migrations for sessions, plans, and codebases
- `crates/krusty-core/src/plan/manager.rs` - Plan manager with SQLite-backed 1:1 session-plan linkage
- `crates/krusty-core/src/mcp/manager.rs` - MCP server manager for local stdio connections and remote server support
- `Cargo.toml` - Workspace configuration defining crates, release profile, and lint rules

## Conventions

## Findings

### Error Handling
- **anyhow** = "1.0" - Primary error handling crate
- **thiserror** = "2.0" - For defining custom error types
- Pattern: `anyhow::Result<T>`, `anyhow::bail!()`, `anyhow::anyhow!()`, `.context()`
- Custom errors via thiserror derive (e.g., `AcpError`)

### Logging
- **tracing** = "0.1" - Structured logging framework
- **tracing-subscriber** - With env-filter for filtering
- Macros: `info!`, `debug!`, `warn!`, `error!`
- Structured fields: `info!(key = %value, "message")`

### Async
- **tokio** = "1.40" with "full" feature - Async runtime
- **async-trait** = "0.1" - For async trait methods
- Tokiosync primitives: `Mutex`, `RwLock`, `mpsc`, `oneshot`
- `tokio::select!`, `tokio::time::timeout`, `tokio::spawn`
- `#[tokio::main]` entry point

### Testing
- Location: Inline `#[cfg(test)]` modules with `mod tests { ... }`
- Framework: Standard Rust `#[test]` + `#[tokio::test]`
- No external test framework (using std lib)
- Dev dependency: `tempfile` for tests

### Naming Conventions
- **Constants**: `SCREAMING_SNAKE_CASE`, organized in dedicated `constants.rs` files
- **Constant modules**: `token_limits::SMALL`, `timeouts::TOOL_EXECUTION`, `models::OPUS_4_5`
- **Structs**: `PascalCase`, `pub struct` with public fields
- **Functions**: `snake_case`
- **Private fields**: Prefixed with underscore `_field`
- **Modules**: `snake_case`
- **Enums**: `PascalCase`
- **Tests**: `mod tests` with `#[test]`/`#[tokio::test]` functions

### Files Examined
- `/home/burgess/Work/Krusty-Dev/krusty-public/Cargo.toml`
- `/home/burgess/Work/Krusty-Dev/krusty-public/crates/krusty-core/Cargo.toml`
- `/home/burgess/Work/Krusty-Dev/krusty-public/crates/krusty-cli/Cargo.toml`
- `/home/burgess/Work/Krusty-Dev/krusty-public/crates/krusty-core/src/lib.rs`
- `/home/burgess/Work/Krusty-Dev/krusty-public/crates/krusty-core/src/agent/mod.rs`
- `/home/burgess/Work/Krusty-Dev/krusty-public/crates/krusty-core/src/agent/constants.rs`
- Multiple source files examined via grep for patterns

## Build & Run

## Build Commands

```bash
cargo build                   # Debug build
cargo build --release         # Release build
cargo check                   # Type check
cargo test                    # Run tests
cargo clippy                  # Lint
cargo clippy --workspace -- -D warnings  # Lint all with strict mode
cargo fmt --all               # Format code
cargo build --workspace       # Build all workspace crates
cargo test --workspace        # Test all workspace crates
krusty acp                    # Editor integration via ACP
krusty lsp install <lang>     # Install language server
```

## Key Dependencies

**Runtime/Async**
- tokio

**Error Handling**
- anyhow, thiserror

**Serialization**
- serde, serde_json, toml, serde_yaml

**Logging**
- tracing, tracing-subscriber

**HTTP**
- reqwest

**TUI**
- ratatui, ratatui-image, crossterm, unicode-width, textwrap, arboard, image, palette

**PTY/Terminal**
- vt100, portable-pty

**Database**
- rusqlite

**LSP**
- lsp-types

**WASM Extensions**
- wasmtime, wasmtime-wasi, wasmparser, semver

**Auth**
- sha2, hmac, base64, rand

**URL/Web**
- url, webbrowser, tiny_http, httpdate

**Web Content**
- html2md, scraper

**Utilities**
- which, chrono, uuid, regex, glob, walkdir, similar, shell-words

**Archives**
- flate2, tar, zip, xz2

**Concurrency**
- dashmap, parking_lot, moka

**System**
- libc, libloading

**Code Parsing**
- tree-sitter, tree-sitter-rust

**Embeddings**
- fastembed

**ACP Protocol**
- agent-client-protocol

**Version Control**
- git2

**Syntax Highlighting**
- syntect

**File System**
- ignore

## Notes for AI

<!-- Add project-specific instructions here -->

