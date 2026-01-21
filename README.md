# Krusty

A terminal-based AI coding assistant. Multi-provider, multi-model, with 100+ language servers via Zed extensions.

## Installation

### Quick Install (Linux/macOS)

```bash
curl -fsSL https://raw.githubusercontent.com/BurgessTG/Krusty/main/install.sh | sh
```

### Homebrew (macOS/Linux)

```bash
brew tap BurgessTG/tap
brew install krusty
```

### From Source

```bash
git clone https://github.com/BurgessTG/Krusty.git
cd Krusty
cargo build --release
./target/release/krusty
```

### GitHub Releases

Download prebuilt binaries from [Releases](https://github.com/BurgessTG/Krusty/releases):
- Linux (x86_64, ARM64)
- macOS (Intel, Apple Silicon)
- Windows (x86_64)

## Supported Providers

Krusty supports multiple AI providers. Add API keys via `/auth` in the TUI.

| Provider | Models | Notes |
|----------|--------|-------|
| **Anthropic** | Claude Opus 4.5, Sonnet 4.5, Haiku 4.5 | Extended thinking, web search |
| **OpenRouter** | 100+ models (GPT, Gemini, Llama, Claude, etc.) | Pay-per-use aggregator |
| **OpenCode Zen** | Claude, GPT-5, Gemini, Qwen | Curated for coding |
| **Z.ai** | GLM 4.7, 4.6, 4.5 | Budget-friendly |
| **MiniMax** | M2.1 | Fast, interleaved thinking |
| **Kimi** | K2 | 256K context |

Switch providers and models anytime with `/model` or `Ctrl+M`.

## Controls

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Enter` | Send message (or newline with Shift) |
| `Ctrl+C` | Cancel current generation |
| `Ctrl+L` | Clear screen |
| `Ctrl+M` | Open model selector |
| `Ctrl+N` | New session |
| `Ctrl+P` | Open session picker |
| `Ctrl+K` | Open command palette |
| `Esc` | Close popup / Cancel |
| `Tab` | Autocomplete slash commands |
| `↑/↓` | Scroll / Navigate history |
| `PgUp/PgDn` | Scroll messages |

### Slash Commands

| Command | Description |
|---------|-------------|
| `/help` | Show all commands |
| `/auth` | Manage API keys for providers |
| `/model` | Select AI model and provider |
| `/sessions` | Browse conversation history |
| `/clear` | Clear current conversation |
| `/lsp` | Browse and install language servers |
| `/mcp` | Manage MCP servers |
| `/hooks` | Configure pre/post tool hooks |
| `/skills` | Browse available skills |
| `/theme` | Change color theme |
| `/processes` | View background processes |
| `/init` | Initialize project context file |
| `/compact` | Summarize and compact conversation |

### Mouse

- Click to select text
- Scroll wheel to navigate
- Click links to open in browser
- Click code blocks to copy

## Features

### Multi-Provider AI
Configure multiple providers and switch between them seamlessly. Your conversation continues even when switching models.

### Language Server Protocol (LSP)
Install language servers from Zed's extension marketplace for 100+ languages:

```bash
krusty lsp install rust
krusty lsp install python
krusty lsp install typescript
```

Or use `/lsp` in the TUI to browse and install interactively.

### Tool Execution
Krusty can execute tools on your behalf:
- **Read/Write/Edit** - File operations with syntax highlighting
- **Bash** - Run shell commands with safety prompts
- **Glob/Grep** - Search files and content
- **Web Search** - Search the web (Anthropic models)

### Skills
Modular instruction sets for domain-specific tasks. Add custom skills in `~/.krusty/skills/` or project `.krusty/skills/`.

### Sessions
All conversations are saved locally in SQLite. Resume any session with `/sessions`.

### Themes
30+ built-in themes. Switch with `/theme` or:

```bash
krusty -t dracula
krusty -t tokyo_night
krusty -t gruvbox_dark
krusty themes  # List all
```

### Auto-Updates
Krusty checks for updates and can self-update.

## Configuration

Data stored in `~/.krusty/`:

```
~/.krusty/
├── credentials.json  # API keys (encrypted)
├── preferences.json  # Settings
├── extensions/       # Zed LSP extensions
├── skills/          # Custom skills
├── logs/            # Application logs
└── bin/             # LSP binaries
```

### Project Configuration

Add a `KRUSTY.md` or `CLAUDE.md` file to your project root for project-specific instructions that are automatically included in context.

## Development

```bash
cargo check           # Type check
cargo build           # Debug build
cargo build --release # Release build
cargo test            # Run tests
cargo clippy          # Lint
cargo fmt             # Format code
```

### Git Hooks

Install the pre-commit hook to automatically check formatting before commits:

```bash
cp .githooks/pre-commit .git/hooks/pre-commit
```

Or configure git to use the `.githooks` directory:

```bash
git config core.hooksPath .githooks
```

## License

MIT License - see [LICENSE](LICENSE) for details.
