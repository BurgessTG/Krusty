//! Extension and LSP server registration
//!
//! Handles loading WASM extensions and registering their language servers.

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;

use crate::extensions::types::WorktreeDelegate;
use crate::extensions::wasm_host::{WasmExtension, WasmHost};
use crate::lsp::LspManager;
use crate::paths;
use crate::tui::utils::{language_to_extensions, AppWorktreeDelegate};

/// Initialize language servers from all loaded WASM extensions
pub async fn initialize_extension_servers(
    wasm_host: Option<&Arc<WasmHost>>,
    lsp_manager: &Arc<LspManager>,
    working_dir: &Path,
) -> Result<()> {
    let Some(wasm_host) = wasm_host else {
        return Ok(());
    };

    let extensions_dir = paths::extensions_dir();
    if !extensions_dir.exists() {
        return Ok(());
    }

    let worktree = AppWorktreeDelegate::new(working_dir.to_path_buf());

    let entries = match std::fs::read_dir(&extensions_dir) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        if let Ok(extension) = wasm_host.load_extension_from_dir(&path).await {
            tracing::info!("Loaded extension: {}", extension.manifest.name);
            register_extension_lsp_servers(&extension, wasm_host, lsp_manager, worktree.clone())
                .await;
        }
    }

    Ok(())
}

/// Register language servers for a single extension
/// Used after installing extensions mid-session
pub async fn register_extension_servers(
    wasm_host: Option<&Arc<WasmHost>>,
    lsp_manager: &Arc<LspManager>,
    working_dir: &Path,
    extension_path: &Path,
) -> Result<()> {
    let Some(wasm_host) = wasm_host else {
        return Ok(());
    };

    let worktree = AppWorktreeDelegate::new(working_dir.to_path_buf());
    let extension = wasm_host.load_extension_from_dir(extension_path).await?;
    tracing::info!(
        "Registering servers for extension: {}",
        extension.manifest.name
    );
    register_extension_lsp_servers(&extension, wasm_host, lsp_manager, worktree).await;

    Ok(())
}

/// Helper to register all language servers from a loaded extension
async fn register_extension_lsp_servers(
    extension: &WasmExtension,
    wasm_host: &WasmHost,
    lsp_manager: &Arc<LspManager>,
    worktree: Arc<AppWorktreeDelegate>,
) {
    for (server_id, entry) in &extension.manifest.language_servers {
        match extension
            .language_server_command(
                server_id.clone().into(),
                worktree.clone() as Arc<dyn WorktreeDelegate>,
            )
            .await
        {
            Ok(mut command) => {
                // Resolve relative command path to absolute path
                let extension_work_dir = wasm_host.work_dir.join(&extension.manifest.id);
                let command_path = std::path::Path::new(&command.command);
                if command_path.is_relative() {
                    let absolute_path = extension_work_dir.join(command_path);
                    command.command = absolute_path.to_string_lossy().into_owned();
                }

                // Use languages list, falling back to singular language field
                let langs: Vec<&str> = if entry.languages.is_empty() {
                    entry.language.as_deref().into_iter().collect()
                } else {
                    entry.languages.iter().map(|s| s.as_str()).collect()
                };

                let file_extensions: Vec<String> = langs
                    .iter()
                    .flat_map(|lang| language_to_extensions(lang))
                    .collect();

                let full_server_id = format!("{}-{}", extension.manifest.id, server_id);

                tracing::info!(
                    "Registering LSP {} for languages {:?} (extensions: {:?})",
                    full_server_id,
                    langs,
                    file_extensions
                );

                if let Err(e) = lsp_manager
                    .register_from_extension(
                        &full_server_id,
                        command,
                        file_extensions,
                        50, // Extensions get lower priority than builtins
                    )
                    .await
                {
                    tracing::error!("Failed to register LSP {}: {}", full_server_id, e);
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to get language server command for {}/{}: {}",
                    extension.manifest.id,
                    server_id,
                    e
                );
            }
        }
    }
}
