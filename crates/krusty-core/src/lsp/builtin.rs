//! Built-in LSP server definitions
//!
//! Zero-config language servers that work out of the box.
//! Supports auto-download from GitHub releases, npm/Bun install,
//! or toolchain-based installation.

use crate::extensions::wasm_host::Command;

/// How to install an LSP server
#[derive(Debug, Clone)]
pub enum LspInstallMethod {
    /// Download from GitHub releases
    GitHub {
        repo: &'static str,
        asset_pattern: &'static str,
    },
    /// Install via toolchain (go install, gem install, etc.)
    Toolchain {
        toolchain: &'static str,
        install_cmd: &'static [&'static str],
    },
    /// Install via npm/bun (typescript-language-server, etc.)
    Npm {
        package: &'static str,
    },
}

/// A built-in LSP server definition
#[derive(Debug, Clone)]
pub struct BuiltinLsp {
    pub id: &'static str,
    pub binary: &'static str,
    pub args: &'static [&'static str],
    pub extensions: &'static [&'static str],
    pub install: LspInstallMethod,
}

impl BuiltinLsp {
    pub fn to_command_with_path(&self, bin_path: &std::path::Path) -> Command {
        Command {
            command: bin_path.to_string_lossy().into_owned(),
            args: self.args.iter().map(|s| s.to_string()).collect(),
            env: Default::default(),
        }
    }

    pub fn file_extensions(&self) -> Vec<String> {
        self.extensions.iter().map(|s| s.to_string()).collect()
    }
}

pub static BUILTIN_LSPS: &[BuiltinLsp] = &[
    // Tier 2: GitHub releases download
    BuiltinLsp {
        id: "builtin-rust-analyzer",
        binary: "rust-analyzer",
        args: &[],
        extensions: &["rs"],
        install: LspInstallMethod::GitHub {
            repo: "rust-lang/rust-analyzer",
            asset_pattern: "rust-analyzer-{arch}-{platform}.gz",
        },
    },
    BuiltinLsp {
        id: "builtin-zls",
        binary: "zls",
        args: &[],
        extensions: &["zig"],
        install: LspInstallMethod::GitHub {
            repo: "zigtools/zls",
            asset_pattern: "zls-{arch}-{platform}.{ext}",
        },
    },
    BuiltinLsp {
        id: "builtin-clangd",
        binary: "clangd",
        args: &["--background-index", "--clang-tidy"],
        extensions: &["c", "cpp", "cc", "h", "hpp", "cxx"],
        install: LspInstallMethod::GitHub {
            repo: "clangd/clangd",
            asset_pattern: "clangd-{platform}-{version}.zip",
        },
    },
    BuiltinLsp {
        id: "builtin-lua",
        binary: "lua-language-server",
        args: &[],
        extensions: &["lua"],
        install: LspInstallMethod::GitHub {
            repo: "LuaLS/lua-language-server",
            asset_pattern: "lua-language-server-{version}-{platform}-{arch}.tar.gz",
        },
    },
    // Tier 4: Toolchain install
    BuiltinLsp {
        id: "builtin-gopls",
        binary: "gopls",
        args: &["serve"],
        extensions: &["go"],
        install: LspInstallMethod::Toolchain {
            toolchain: "go",
            install_cmd: &["go", "install", "golang.org/x/tools/gopls@latest"],
        },
    },
    // Tier 1: PATH only (user must install)
    BuiltinLsp {
        id: "builtin-pyright",
        binary: "pyright-langserver",
        args: &["--stdio"],
        extensions: &["py", "pyi"],
        install: LspInstallMethod::Npm { package: "pyright" },
    },
    BuiltinLsp {
        id: "builtin-typescript",
        binary: "typescript-language-server",
        args: &["--stdio"],
        extensions: &["ts", "tsx", "js", "jsx", "mjs", "cts", "mts"],
        install: LspInstallMethod::Npm { package: "typescript-language-server" },
    },
    BuiltinLsp {
        id: "builtin-bash",
        binary: "bash-language-server",
        args: &["start"],
        extensions: &["sh", "bash"],
        install: LspInstallMethod::Npm { package: "bash-language-server" },
    },
    BuiltinLsp {
        id: "builtin-yaml",
        binary: "yaml-language-server",
        args: &["--stdio"],
        extensions: &["yaml", "yml"],
        install: LspInstallMethod::Npm { package: "yaml-language-server" },
    },
    BuiltinLsp {
        id: "builtin-json",
        binary: "vscode-json-language-server",
        args: &["--stdio"],
        extensions: &["json"],
        install: LspInstallMethod::Npm { package: "vscode-langservers-extracted" },
    },
];
