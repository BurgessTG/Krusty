# Krusty

A terminal-based AI coding assistant powered by Claude.

## Features

- **Multi-provider AI** - Anthropic Claude with OAuth or API key authentication
- **Zed Extension System** - 100+ language servers via WASM extensions
- **Tool Execution** - File operations, bash commands, grep, glob patterns
- **Session Management** - SQLite-backed conversation history
- **Theming** - Customizable color schemes
- **Skills** - Modular instructions for domain-specific tasks
- **Auto-updates** - Built-in update system

## Installation

### Homebrew (macOS/Linux)

```bash
brew install BurgessTG/tap/krusty
```

### Cargo (from crates.io)

```bash
cargo install krusty
```

### Shell Script (Linux/macOS)

```bash
curl -fsSL https://raw.githubusercontent.com/BurgessTG/Krusty/main/install.sh | sh
```

### From Source

```bash
git clone https://github.com/BurgessTG/Krusty.git
cd Krusty
cargo build --release
# Binary at target/release/krusty
```

### GitHub Releases

Download prebuilt binaries from [Releases](https://github.com/BurgessTG/Krusty/releases).

### Requirements

- An Anthropic API key or OAuth credentials

## Usage

```bash
# Start the TUI
krusty

# Or with a specific theme
krusty -t monokai

# List available themes
krusty themes

# Manage LSP extensions
krusty lsp list
krusty lsp install rust
krusty lsp status
```

## Authentication

On first run, use `/auth` in the TUI to authenticate:

- **OAuth** (recommended): Browser-based login with Anthropic
- **API Key**: Direct API key entry

Credentials are stored locally in `~/.krusty/tokens/`.

## Configuration

Krusty stores data in `~/.krusty/`:

```
~/.krusty/
├── extensions/     # Installed WASM extensions (Zed format)
├── logs/          # Application logs
├── tokens/        # OAuth/API credentials
└── bin/           # Auto-downloaded LSP binaries
```

## Slash Commands

In the TUI, use these commands:

- `/help` - Show available commands
- `/auth` - Manage authentication
- `/lsp` - Browse and install language server extensions
- `/sessions` - View conversation history
- `/clear` - Clear current conversation
- `/model` - Select AI model

## Architecture

```
crates/
├── krusty-core/   # Shared library (AI, tools, LSP, storage)
└── krusty-cli/    # Terminal UI application
```

Key modules in `krusty-core`:
- `ai/` - Anthropic Claude API client with streaming
- `tools/` - Tool execution framework
- `extensions/` - Zed WASM extension host
- `lsp/` - Language server protocol client
- `storage/` - SQLite persistence

## Development

```bash
cargo check           # Quick compilation check
cargo build           # Debug build
cargo build --release # Release build
cargo test            # Run tests
cargo clippy          # Lint check
```

## License

MIT License - see [LICENSE](LICENSE) for details.
