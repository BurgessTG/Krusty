//! LSP extension installation handlers
//!
//! Handles downloading and managing LSP extensions from Zed marketplace

use std::path::PathBuf;
use std::sync::Arc;

use crate::extensions::WasmHost;
use crate::paths;
use crate::tui::app::App;

impl App {
    /// Start async LSP extension installation from Zed marketplace
    pub fn start_lsp_install(&mut self) {
        let ext = match self.popups.lsp.get_selected() {
            Some(e) => e.clone(),
            None => return,
        };

        if self.popups.lsp.is_installing() {
            return;
        }

        // If already installed, this is an uninstall
        if ext.installed {
            let ext_dir = paths::extensions_dir().join(&ext.id);
            if ext_dir.exists() {
                match std::fs::remove_dir_all(&ext_dir) {
                    Ok(_) => {
                        self.popups.lsp.uninstall_complete(&ext.id, true);
                        self.messages.push((
                            "system".to_string(),
                            format!("Removed extension: {}", ext.id),
                        ));
                    }
                    Err(e) => {
                        self.popups.lsp.install_status = Some(format!("Failed to remove: {}", e));
                    }
                }
            }
            return;
        }

        // Mark as installing
        self.popups.lsp.start_install(&ext.id);

        let (tx, rx) = tokio::sync::oneshot::channel();
        self.channels.lsp_install = Some(rx);

        let ext_id = ext.id.clone();
        let ext_dir = paths::extensions_dir();
        let wasm_host = self.wasm_host.clone();

        tokio::spawn(async move {
            let result = Self::download_and_install_extension(&ext_id, &ext_dir, wasm_host).await;
            let _ = tx.send(result);
        });
    }

    /// Download and install a Zed extension
    pub async fn download_and_install_extension(
        ext_id: &str,
        ext_dir: &std::path::Path,
        wasm_host: Option<Arc<WasmHost>>,
    ) -> Result<PathBuf, String> {
        let target_dir = ext_dir.join(ext_id);
        tokio::fs::create_dir_all(&target_dir)
            .await
            .map_err(|e| format!("Failed to create directory: {}", e))?;

        let download_url = crate::tui::popups::lsp_browser::LspBrowserPopup::download_url(ext_id);

        let client = reqwest::Client::new();
        let response = client
            .get(&download_url)
            .send()
            .await
            .map_err(|e| format!("Download failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!(
                "Extension '{}' not found in Zed marketplace (HTTP {})",
                ext_id,
                response.status()
            ));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        // Extract tar.gz
        use flate2::read::GzDecoder;
        use tar::Archive;

        let decoder = GzDecoder::new(&bytes[..]);
        let mut archive = Archive::new(decoder);
        archive
            .unpack(&target_dir)
            .map_err(|e| format!("Failed to extract: {}", e))?;

        // Try to load the extension to verify it works
        if let Some(host) = wasm_host {
            match host.load_extension_from_dir(&target_dir).await {
                Ok(ext) => {
                    tracing::info!(
                        "Loaded extension {} v{}",
                        ext.manifest.name,
                        ext.manifest.version
                    );
                }
                Err(e) => {
                    tracing::warn!("Extension downloaded but failed to load: {}", e);
                }
            }
        }

        Ok(target_dir)
    }

    /// Poll for LSP install completion
    pub fn poll_lsp_install(&mut self) {
        if let Some(rx) = &mut self.channels.lsp_install {
            match rx.try_recv() {
                Ok(result) => {
                    let ext_id = self.popups.lsp.installing.clone().unwrap_or_default();
                    match result {
                        Ok(path) => {
                            self.popups.lsp.install_complete(&ext_id, true, None);
                            self.messages.push((
                                "system".to_string(),
                                format!("Installed extension: {} at {:?}", ext_id, path),
                            ));
                            tracing::info!("Installed extension {} at {:?}", ext_id, path);
                            // Queue for LSP registration in main loop
                            self.pending_extension_paths.push(path);
                        }
                        Err(e) => {
                            self.popups
                                .lsp
                                .install_complete(&ext_id, false, Some(e.clone()));
                            self.messages
                                .push(("system".to_string(), format!("Install failed: {}", e)));
                        }
                    }
                    self.channels.lsp_install = None;
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    let ext_id = self.popups.lsp.installing.clone().unwrap_or_default();
                    self.popups.lsp.install_complete(
                        &ext_id,
                        false,
                        Some("Install task closed unexpectedly".to_string()),
                    );
                    self.channels.lsp_install = None;
                }
            }
        }
    }

    /// Start fetching extensions from Zed API
    pub fn start_extensions_fetch(&mut self) {
        if self.channels.lsp_fetch.is_some() {
            return;
        }

        self.popups.lsp.start_fetch();

        let (tx, rx) = tokio::sync::oneshot::channel();
        self.channels.lsp_fetch = Some(rx);

        tokio::spawn(async move {
            let result = crate::tui::popups::lsp_browser::fetch_extensions_from_api().await;
            let _ = tx.send(result);
        });
    }

    /// Install a built-in LSP server (from popup prompt)
    pub fn start_builtin_lsp_install(&mut self, builtin: &'static crate::lsp::builtin::BuiltinLsp) {
        let downloader = crate::lsp::LspDownloader::new();
        let lsp_manager = self.lsp_manager.clone();
        let builtin = builtin.clone();

        // Set up channel for result
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.channels.builtin_lsp_install = Some(rx);

        tokio::spawn(async move {
            match downloader.ensure_available(&builtin).await {
                Ok(bin_path) => {
                    if let Err(e) = lsp_manager.register_builtin_with_path(&builtin, &bin_path).await
                    {
                        let _ = tx.send(Err(format!("Failed to register: {}", e)));
                    } else {
                        let _ = tx.send(Ok(builtin.id.to_string()));
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(format!("Download failed: {}", e)));
                }
            }
        });
    }

    /// Install an extension LSP from Zed marketplace (from popup prompt)
    pub fn start_extension_lsp_install(&mut self, ext_name: String) {
        let ext_dir = paths::extensions_dir();
        let wasm_host = self.wasm_host.clone();

        // Set up channel for result
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.channels.extension_lsp_install = Some(rx);

        tokio::spawn(async move {
            match Self::download_and_install_extension(&ext_name, &ext_dir, wasm_host).await {
                Ok(_path) => {
                    let _ = tx.send(Ok(ext_name));
                }
                Err(e) => {
                    let _ = tx.send(Err(e));
                }
            }
        });
    }

    /// Poll for builtin LSP install completion
    pub fn poll_builtin_lsp_install(&mut self) {
        if let Some(rx) = &mut self.channels.builtin_lsp_install {
            match rx.try_recv() {
                Ok(Ok(name)) => {
                    self.popups.lsp_install.set_progress("Installed!");
                    self.messages.push(("system".to_string(), format!("Installed LSP: {}", name)));
                    self.popups.lsp_install.clear();
                    self.popup = crate::tui::app::Popup::None;
                    self.channels.builtin_lsp_install = None;
                }
                Ok(Err(e)) => {
                    self.popups.lsp_install.set_error(&format!("Failed: {}", e));
                    self.messages.push(("system".to_string(), format!("LSP install failed: {}", e)));
                    self.channels.builtin_lsp_install = None;
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    self.popups.lsp_install.clear();
                    self.popup = crate::tui::app::Popup::None;
                    self.channels.builtin_lsp_install = None;
                }
            }
        }
    }

    /// Poll for extension LSP install completion
    pub fn poll_extension_lsp_install(&mut self) {
        if let Some(rx) = &mut self.channels.extension_lsp_install {
            match rx.try_recv() {
                Ok(result) => {
                    match result {
                        Ok(name) => {
                            self.popups.lsp_install.set_progress("Installed!");
                            self.messages.push((
                                "system".to_string(),
                                format!("Installed extension: {}", name),
                            ));
                            self.popups.lsp_install.clear();
                            self.popup = crate::tui::app::Popup::None;
                        }
                        Err(e) => {
                            // Set error state so user can see message and dismiss with Esc
                            self.popups.lsp_install.set_error(&format!("Failed: {}", e));
                            self.messages
                                .push(("system".to_string(), format!("Extension install failed: {}", e)));
                        }
                    }
                    self.channels.extension_lsp_install = None;
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    self.popups.lsp_install.clear();
                    self.popup = crate::tui::app::Popup::None;
                    self.channels.extension_lsp_install = None;
                }
            }
        }
    }

    /// Poll for pending LSP install prompts (triggered by tools)
    ///
    /// IMPORTANT: This shows the popup IMMEDIATELY and pauses streaming.
    /// The popup interrupts the conversation to ask the user about LSP installation.
    pub fn poll_pending_lsp_install(&mut self) {
        if let Some(missing) = self.pending_lsp_install.take() {
            // Don't prompt if user said "always skip" for this extension
            if self.lsp_skip_list.contains(&missing.extension) {
                return;
            }

            // Don't prompt if popup already open (prevents stacking)
            if self.popup != crate::tui::app::Popup::None {
                // Put it back for later
                self.pending_lsp_install = Some(missing);
                return;
            }

            // Show the install popup IMMEDIATELY - this pauses streaming
            // (process_stream_events checks for this popup and skips processing)
            tracing::info!("LSP popup interrupting conversation for: {}", missing.extension);
            self.popups.lsp_install.set(missing);
            self.popup = crate::tui::app::Popup::LspInstall;
        }
    }

    /// Poll for extensions fetch completion
    pub fn poll_extensions_fetch(&mut self) {
        if let Some(rx) = &mut self.channels.lsp_fetch {
            match rx.try_recv() {
                Ok(result) => {
                    match result {
                        Ok(extensions) => {
                            self.popups.lsp.on_fetch_complete(extensions);
                            tracing::info!(
                                "Fetched {} extensions from API",
                                self.popups.lsp.extensions.len()
                            );
                        }
                        Err(e) => {
                            self.popups.lsp.on_fetch_error(e);
                        }
                    }
                    self.channels.lsp_fetch = None;
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    self.popups
                        .lsp
                        .on_fetch_error("Fetch task closed unexpectedly".to_string());
                    self.channels.lsp_fetch = None;
                }
            }
        }
    }

    /// Poll for missing LSP notifications from tools
    ///
    /// When write/edit tools detect a file without LSP support,
    /// they send a MissingLspInfo through the channel. We pick it up
    /// here and store it in pending_lsp_install for the popup.
    pub fn poll_missing_lsp(&mut self) {
        if let Some(rx) = &mut self.channels.missing_lsp {
            // Only take one at a time to avoid overwhelming the user
            match rx.try_recv() {
                Ok(missing) => {
                    // Don't prompt if already have a pending install or popup open
                    if self.pending_lsp_install.is_none() {
                        self.pending_lsp_install = Some(missing);
                    }
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {}
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    self.channels.missing_lsp = None;
                }
            }
        }
    }
}
