//! Popup keyboard event handlers
//!
//! Handles keyboard input for all popup dialogs.

use crossterm::event::{KeyCode, KeyModifiers};

use crate::tui::app::{App, Popup, View};
use crate::tui::popups::auth::AuthState;
use crate::tui::utils::McpStatusUpdate;
use krusty_core::mcp::tool::register_mcp_tools;

impl App {
    /// Handle keyboard events when a popup is open
    pub fn handle_popup_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        match &self.popup {
            Popup::Help => match code {
                KeyCode::Esc => self.popup = Popup::None,
                KeyCode::Tab => self.popups.help.next_tab(),
                _ => {}
            },
            Popup::ThemeSelect => {
                match code {
                    KeyCode::Esc => {
                        // Restore original theme on cancel
                        self.restore_original_theme();
                        self.popup = Popup::None;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.popups.theme.prev();
                        // Live preview on navigation
                        if let Some(name) = self.popups.theme.get_selected_theme_name() {
                            self.preview_theme(&name);
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.popups.theme.next();
                        // Live preview on navigation
                        if let Some(name) = self.popups.theme.get_selected_theme_name() {
                            self.preview_theme(&name);
                        }
                    }
                    KeyCode::Enter => {
                        if let Some(name) = self.popups.theme.get_selected_theme_name() {
                            self.set_theme(&name); // Apply AND save
                            self.popup = Popup::None;
                        }
                    }
                    _ => {}
                }
            }
            Popup::ModelSelect => {
                if self.popups.model.search_active {
                    match code {
                        KeyCode::Esc => self.popups.model.toggle_search(), // Clear filter and exit search
                        KeyCode::Enter => self.popups.model.close_search(), // Keep filter, exit search
                        KeyCode::Backspace => self.popups.model.backspace_search(),
                        KeyCode::Char(c) => self.popups.model.add_search_char(c),
                        _ => {}
                    }
                } else {
                    match code {
                        KeyCode::Esc => self.popup = Popup::None,
                        KeyCode::Up | KeyCode::Char('k') => self.popups.model.prev(),
                        KeyCode::Down | KeyCode::Char('j') => self.popups.model.next(),
                        KeyCode::Char('i') | KeyCode::Char('/') => {
                            self.popups.model.toggle_search()
                        }
                        KeyCode::Enter => {
                            // Get selected model metadata first to check context window
                            let metadata = self.popups.model.get_selected_metadata().cloned();

                            if let Some(metadata) = metadata {
                                // Check if current context exceeds new model's limit
                                if self.context_tokens_used > metadata.context_window {
                                    // Block the switch - context is too large for this model
                                    let used_k = self.context_tokens_used as f64 / 1000.0;
                                    let max_k = metadata.context_window as f64 / 1000.0;
                                    self.popups.model.set_error(format!(
                                        "Context too large ({:.0}k) for {} ({:.0}k max). Clear conversation or choose a larger model.",
                                        used_k, metadata.display_name, max_k
                                    ));
                                } else {
                                    let provider_id = metadata.provider;
                                    let model_id = metadata.id;

                                    // Switch provider if selecting model from different provider
                                    if provider_id != self.active_provider {
                                        self.switch_provider(provider_id);
                                        // Try loading auth if switched and not authenticated
                                        if !self.is_authenticated() {
                                            let _ =
                                                futures::executor::block_on(self.try_load_auth());
                                        }
                                    }
                                    self.current_model = model_id.clone();

                                    // Mark model as recently used
                                    let registry = self.services.model_registry.clone();
                                    futures::executor::block_on(registry.mark_recent(&model_id));

                                    // Save to preferences (current model + recent list)
                                    if let Some(ref prefs) = self.services.preferences {
                                        if let Err(e) = prefs.set_current_model(&model_id) {
                                            tracing::warn!("Failed to save current model: {}", e);
                                        }
                                        if let Err(e) = prefs.add_recent_model(&model_id) {
                                            tracing::warn!("Failed to save recent model: {}", e);
                                        }
                                    }

                                    self.popup = Popup::None;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Popup::SessionList => {
                match code {
                    KeyCode::Esc => self.popup = Popup::None,
                    KeyCode::Up | KeyCode::Char('k') => self.popups.session.prev(),
                    KeyCode::Down | KeyCode::Char('j') => self.popups.session.next(),
                    KeyCode::Char('d') | KeyCode::Delete => {
                        if let Some(session) = self.popups.session.delete_selected() {
                            self.delete_session(&session.id);
                        }
                    }
                    KeyCode::Enter => {
                        if let Some(session) = self.popups.session.get_selected_session() {
                            let session_id = session.id.clone();
                            // Save current session's block UI states before switching
                            self.save_block_ui_states();
                            if let Err(e) = self.load_session(&session_id) {
                                self.messages.push((
                                    "system".to_string(),
                                    format!("Failed to load session: {}", e),
                                ));
                            } else {
                                // Defer view change to avoid popup collision artifacts
                                self.pending_view_change = Some(View::Chat);
                            }
                            self.popup = Popup::None;
                        }
                    }
                    _ => {}
                }
            }
            Popup::Auth => {
                self.handle_auth_popup_key(code, modifiers);
            }
            Popup::LspBrowser => {
                self.handle_lsp_popup_key(code);
            }
            Popup::ProcessList => {
                self.handle_process_popup_key(code);
            }
            Popup::Pinch => {
                self.handle_pinch_popup_key(code, modifiers);
            }
            Popup::LspInstall => {
                self.handle_lsp_install_popup_key(code);
            }
            Popup::FilePreview => {
                self.handle_file_preview_popup_key(code);
            }
            Popup::SkillsBrowser => {
                self.handle_skills_popup_key(code);
            }
            Popup::McpBrowser => {
                self.handle_mcp_popup_key(code);
            }
            Popup::Hooks => {
                self.handle_hooks_popup_key(code);
            }
            Popup::None => {}
        }
    }

    /// Handle MCP browser popup keyboard events
    pub fn handle_mcp_popup_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => {
                self.popup = Popup::None;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.popups.mcp.prev();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.popups.mcp.next();
            }
            KeyCode::Enter => {
                self.popups.mcp.toggle_expand();
            }
            KeyCode::Char('c') => {
                // Connect (or reconnect if already connected)
                if let Some(server) = self.popups.mcp.get_selected() {
                    if server.server_type == "remote" {
                        self.popups
                            .mcp
                            .set_status("Remote servers handled by API".to_string());
                        return;
                    }

                    let name = server.name.clone();
                    let mcp = self.services.mcp_manager.clone();
                    let registry = self.services.tool_registry.clone();
                    let status_tx = self.services.mcp_status_tx.clone();

                    self.popups
                        .mcp
                        .set_status(format!("Connecting to {}...", name));

                    tokio::spawn(async move {
                        // Disconnect first if already connected (makes this a reconnect)
                        mcp.disconnect(&name).await;
                        match mcp.connect(&name).await {
                            Ok(()) => {
                                register_mcp_tools(mcp.clone(), &registry).await;
                                let tool_count = if let Some(client) = mcp.get_client(&name).await {
                                    client.get_tools().await.len()
                                } else {
                                    0
                                };
                                let _ = status_tx.send(McpStatusUpdate {
                                    success: true,
                                    message: format!("{} connected ({} tools)", name, tool_count),
                                });
                            }
                            Err(e) => {
                                let _ = status_tx.send(McpStatusUpdate {
                                    success: false,
                                    message: format!("{}: {}", name, e),
                                });
                            }
                        }
                    });
                }
            }
            KeyCode::Char('d') => {
                // Disconnect
                if let Some(server) = self.popups.mcp.get_selected() {
                    if server.server_type == "remote" {
                        self.popups
                            .mcp
                            .set_status("Remote servers handled by API".to_string());
                        return;
                    }

                    let name = server.name.clone();
                    let mcp = self.services.mcp_manager.clone();
                    let status_tx = self.services.mcp_status_tx.clone();

                    tokio::spawn(async move {
                        mcp.disconnect(&name).await;
                        let _ = status_tx.send(McpStatusUpdate {
                            success: true,
                            message: format!("{} disconnected", name),
                        });
                    });
                }
            }
            _ => {}
        }
    }

    /// Handle pinch popup keyboard events
    pub fn handle_pinch_popup_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        use crate::tui::popups::pinch::PinchStage;

        match &self.popups.pinch.stage {
            PinchStage::PreservationInput { .. } => {
                match code {
                    KeyCode::Esc => {
                        self.popups.pinch.reset();
                        self.popup = Popup::None;
                    }
                    KeyCode::Enter => {
                        // Start summarization
                        self.start_pinch_summarization();
                    }
                    KeyCode::Backspace => self.popups.pinch.backspace(),
                    KeyCode::Char(c) if !modifiers.contains(KeyModifiers::CONTROL) => {
                        self.popups.pinch.add_char(c);
                    }
                    _ => {}
                }
            }
            PinchStage::Summarizing { .. } => {
                // Allow cancel during summarization
                if code == KeyCode::Esc {
                    self.cancellation.cancel();
                    self.popups.pinch.reset();
                    self.popup = Popup::None;
                }
            }
            PinchStage::DirectionInput { .. } => {
                match code {
                    KeyCode::Esc => {
                        self.popups.pinch.reset();
                        self.popup = Popup::None;
                    }
                    KeyCode::Up => self.popups.pinch.scroll_up(),
                    KeyCode::Down => self.popups.pinch.scroll_down(),
                    KeyCode::Enter => {
                        // Create the linked session
                        self.complete_pinch();
                    }
                    KeyCode::Backspace => self.popups.pinch.backspace(),
                    KeyCode::Char(c) if !modifiers.contains(KeyModifiers::CONTROL) => {
                        self.popups.pinch.add_char(c);
                    }
                    _ => {}
                }
            }
            PinchStage::Creating => {
                // No input during creation
            }
            PinchStage::Complete {
                new_session_id,
                auto_continue,
                ..
            } => {
                match code {
                    KeyCode::Enter => {
                        // Switch to new session
                        let id = new_session_id.clone();
                        let should_continue = *auto_continue;
                        self.save_block_ui_states();
                        if let Err(e) = self.load_session(&id) {
                            self.messages.push((
                                "system".to_string(),
                                format!("Failed to load session: {}", e),
                            ));
                        } else if should_continue {
                            // Direction was provided - auto-start AI response
                            // The user message is already in the conversation from load_session
                            self.send_to_ai();
                        }
                        self.popups.pinch.reset();
                        self.popup = Popup::None;
                    }
                    KeyCode::Esc => {
                        self.popups.pinch.reset();
                        self.popup = Popup::None;
                    }
                    _ => {}
                }
            }
            PinchStage::Error { .. } => {
                if code == KeyCode::Esc {
                    self.popups.pinch.reset();
                    self.popup = Popup::None;
                }
            }
        }
    }

    /// Handle auth popup keyboard events
    pub fn handle_auth_popup_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        match &self.popups.auth.state {
            AuthState::ProviderSelection { .. } => match code {
                KeyCode::Esc => self.popup = Popup::None,
                KeyCode::Up => self.popups.auth.prev_provider(),
                KeyCode::Down => self.popups.auth.next_provider(),
                KeyCode::Enter => {
                    // Always proceed to auth flow - allows re-authentication
                    // Provider switch happens when auth completes, not here
                    self.popups.auth.confirm_provider();
                }
                _ => {}
            },
            AuthState::ApiKeyInput { provider, .. } => {
                let provider = *provider;
                match code {
                    KeyCode::Esc => self.popups.auth.go_back(),
                    KeyCode::Backspace
                        if self.popups.auth.get_api_key().is_none_or(str::is_empty) =>
                    {
                        self.popups.auth.go_back();
                    }
                    KeyCode::Backspace => self.popups.auth.backspace_api_key(),
                    // Ctrl+V to paste API key
                    KeyCode::Char('v') if modifiers.contains(KeyModifiers::CONTROL) => {
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            if let Ok(text) = clipboard.get_text() {
                                // Trim whitespace and add each character
                                for c in text.trim().chars() {
                                    self.popups.auth.add_api_key_char(c);
                                }
                            }
                        }
                    }
                    KeyCode::Char(c) if !modifiers.contains(KeyModifiers::CONTROL) => {
                        self.popups.auth.add_api_key_char(c);
                    }
                    KeyCode::Enter => {
                        // Clone key first to avoid borrow issues
                        let key = self.popups.auth.get_api_key().map(|k| k.to_string());
                        if let Some(key) = key {
                            if !key.is_empty() {
                                // Switch to this provider before saving (set_api_key uses active_provider)
                                if self.active_provider != provider {
                                    self.switch_provider(provider);
                                }
                                self.set_api_key(key);
                                self.messages.push((
                                    "system".to_string(),
                                    format!("{} API key saved!", provider),
                                ));
                                self.popups.auth.set_api_key_complete();

                                // If OpenRouter was just authenticated, fetch models immediately
                                if provider == crate::ai::providers::ProviderId::OpenRouter {
                                    self.start_openrouter_fetch();
                                }

                                // If OpenCode Zen was just authenticated, fetch models immediately
                                if provider == crate::ai::providers::ProviderId::OpenCodeZen {
                                    self.start_opencodezen_fetch();
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            AuthState::AuthMethodSelection { .. } => match code {
                KeyCode::Esc => self.popups.auth.go_back(),
                KeyCode::Up => self.popups.auth.prev_auth_method(),
                KeyCode::Down => self.popups.auth.next_auth_method(),
                KeyCode::Enter => {
                    // confirm_auth_method returns the method if OAuth was selected
                    // For API key, it transitions internally
                    if let Some((provider, method)) = self.popups.auth.confirm_auth_method() {
                        // OAuth flow selected - start the async flow
                        self.start_oauth_flow(provider, method);
                    }
                }
                _ => {}
            },
            AuthState::OAuthBrowserWaiting { .. } | AuthState::OAuthDeviceCode { .. } => {
                // During OAuth flow, only allow cancellation
                if code == KeyCode::Esc {
                    self.popups.auth.go_back();
                }
            }
            AuthState::Complete { .. } => {
                if code == KeyCode::Esc || code == KeyCode::Enter {
                    self.popups.auth.reset();
                    self.popup = Popup::None;
                }
            }
        }
    }

    /// Start OAuth flow for a provider
    fn start_oauth_flow(
        &mut self,
        provider: crate::ai::providers::ProviderId,
        method: krusty_core::auth::AuthMethod,
    ) {
        use crate::tui::utils::{DeviceCodeInfo, OAuthStatusUpdate};
        use krusty_core::auth::{
            openai_oauth_config, AuthMethod, BrowserOAuthFlow, DeviceCodeFlow,
        };

        let status_tx = self.services.oauth_status_tx.clone();

        match method {
            AuthMethod::OAuthBrowser => {
                self.popups
                    .auth
                    .set_oauth_browser_status("Opening browser...");

                tokio::spawn(async move {
                    // Get OAuth config for the provider
                    let config = match provider {
                        crate::ai::providers::ProviderId::OpenAI => openai_oauth_config(),
                        _ => {
                            let _ = status_tx.send(OAuthStatusUpdate {
                                provider,
                                success: false,
                                message: format!("{} does not support OAuth", provider),
                                device_code: None,
                                token: None,
                            });
                            return;
                        }
                    };

                    let flow = BrowserOAuthFlow::new(config);
                    match flow.run().await {
                        Ok(token) => {
                            let _ = status_tx.send(OAuthStatusUpdate {
                                provider,
                                success: true,
                                message: "Authentication successful".to_string(),
                                device_code: None,
                                token: Some(token),
                            });
                        }
                        Err(e) => {
                            let _ = status_tx.send(OAuthStatusUpdate {
                                provider,
                                success: false,
                                message: format!("OAuth failed: {}", e),
                                device_code: None,
                                token: None,
                            });
                        }
                    }
                });
            }
            AuthMethod::OAuthDevice => {
                tokio::spawn(async move {
                    // Get OAuth config for the provider
                    let config = match provider {
                        crate::ai::providers::ProviderId::OpenAI => openai_oauth_config(),
                        _ => {
                            let _ = status_tx.send(OAuthStatusUpdate {
                                provider,
                                success: false,
                                message: format!("{} does not support OAuth", provider),
                                device_code: None,
                                token: None,
                            });
                            return;
                        }
                    };

                    let flow = DeviceCodeFlow::new(config);

                    // Request device code
                    match flow.request_code().await {
                        Ok(code_response) => {
                            // Send device code info to UI
                            let _ = status_tx.send(OAuthStatusUpdate {
                                provider,
                                success: true, // Partial success - code received
                                message: "Enter the code in your browser".to_string(),
                                device_code: Some(DeviceCodeInfo {
                                    user_code: code_response.user_code.clone(),
                                    verification_uri: code_response.verification_uri.clone(),
                                }),
                                token: None,
                            });

                            // Poll for token
                            match flow
                                .poll_for_token(&code_response.device_code, code_response.interval)
                                .await
                            {
                                Ok(token) => {
                                    let _ = status_tx.send(OAuthStatusUpdate {
                                        provider,
                                        success: true,
                                        message: "Authentication successful".to_string(),
                                        device_code: None,
                                        token: Some(token),
                                    });
                                }
                                Err(e) => {
                                    let _ = status_tx.send(OAuthStatusUpdate {
                                        provider,
                                        success: false,
                                        message: format!("Device auth failed: {}", e),
                                        device_code: None,
                                        token: None,
                                    });
                                }
                            }
                        }
                        Err(e) => {
                            let _ = status_tx.send(OAuthStatusUpdate {
                                provider,
                                success: false,
                                message: format!("Failed to get device code: {}", e),
                                device_code: None,
                                token: None,
                            });
                        }
                    }
                });
            }
            AuthMethod::ApiKey => {
                // This shouldn't happen as API key is handled internally
            }
        }
    }

    /// Handle process list popup keyboard events
    pub fn handle_process_popup_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => self.popup = Popup::None,
            KeyCode::Up | KeyCode::Char('k') => self.popups.process.prev(),
            KeyCode::Down | KeyCode::Char('j') => self.popups.process.next(),
            KeyCode::Char('s') => {
                if let Some(proc) = self.popups.process.get_selected() {
                    let id = proc.id.clone();
                    let registry = self.process_registry.clone();

                    if proc.is_running() {
                        // Running -> Suspend
                        tokio::spawn(async move {
                            if let Err(e) = registry.suspend(&id).await {
                                tracing::error!("Failed to suspend process: {}", e);
                            }
                        });
                    } else if proc.is_suspended() {
                        // Suspended -> Resume
                        tokio::spawn(async move {
                            if let Err(e) = registry.resume(&id).await {
                                tracing::error!("Failed to resume process: {}", e);
                            }
                        });
                    }
                }
            }
            KeyCode::Char('d') | KeyCode::Delete => {
                if let Some(proc) = self.popups.process.get_selected() {
                    if proc.is_running() {
                        let id = proc.id.clone();

                        // Check if this is a terminal pane and close it
                        if let Some(idx) = self
                            .blocks
                            .terminal
                            .iter()
                            .position(|t| t.get_process_id() == Some(&id))
                        {
                            self.close_terminal(idx);
                        } else {
                            // Not a terminal - just kill via registry
                            let registry = self.process_registry.clone();
                            tokio::spawn(async move {
                                if let Err(e) = registry.kill(&id).await {
                                    tracing::error!("Failed to kill process: {}", e);
                                }
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Handle LSP browser popup keyboard events
    pub fn handle_lsp_popup_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => {
                if self.popups.lsp.search_active {
                    self.popups.lsp.toggle_search();
                } else {
                    self.popup = Popup::None;
                }
            }
            KeyCode::Up => self.popups.lsp.prev(),
            KeyCode::Down => self.popups.lsp.next(),
            KeyCode::Char('k') if !self.popups.lsp.search_active => {
                self.popups.lsp.prev();
            }
            KeyCode::Char('j') if !self.popups.lsp.search_active => {
                self.popups.lsp.next();
            }
            KeyCode::Char('/') if !self.popups.lsp.search_active => {
                self.popups.lsp.toggle_search();
            }
            KeyCode::Char('r') if !self.popups.lsp.search_active => {
                self.popups.lsp.refresh();
                self.start_extensions_fetch();
            }
            KeyCode::Enter => {
                if self.popups.lsp.needs_fetch() {
                    self.start_extensions_fetch();
                } else if !self.popups.lsp.is_loading() {
                    self.start_lsp_install();
                }
            }
            KeyCode::Backspace if self.popups.lsp.search_active => {
                self.popups.lsp.backspace_search();
            }
            KeyCode::Char(c) if self.popups.lsp.search_active => {
                self.popups.lsp.add_search_char(c);
            }
            _ => {}
        }
    }

    /// Handle file preview popup keyboard events
    pub fn handle_file_preview_popup_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => {
                self.popups.file_preview.reset();
                self.popup = Popup::None;
            }
            KeyCode::Char('o') | KeyCode::Char('O') => {
                self.popups.file_preview.open_external();
            }
            KeyCode::Left => {
                self.popups.file_preview.prev_page();
            }
            KeyCode::Right => {
                self.popups.file_preview.next_page();
            }
            _ => {}
        }
    }

    /// Handle LSP install popup keyboard events
    pub fn handle_lsp_install_popup_key(&mut self, code: KeyCode) {
        use crate::lsp::manager::LspSuggestion;

        // Don't handle keys while installing
        if self.popups.lsp_install.installing {
            return;
        }

        match code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                // Extract info before mutating
                let suggestion = self
                    .popups
                    .lsp_install
                    .get_info()
                    .map(|i| i.suggested.clone());

                match suggestion {
                    Some(LspSuggestion::Builtin(builtin)) => {
                        self.popups.lsp_install.start_install();
                        self.start_builtin_lsp_install(builtin);
                    }
                    Some(LspSuggestion::Extension(name)) => {
                        self.popups.lsp_install.start_install();
                        self.start_extension_lsp_install(name);
                    }
                    Some(LspSuggestion::None) | None => {
                        // Nothing to install
                        self.popups.lsp_install.clear();
                        self.popup = Popup::None;
                    }
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                // Skip for this session - add to skip list to prevent re-prompting
                if let Some(ext) = self
                    .popups
                    .lsp_install
                    .get_info()
                    .map(|i| i.extension.clone())
                {
                    self.services.lsp_skip_list.insert(ext);
                }
                self.popups.lsp_install.clear();
                self.popup = Popup::None;
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                // Always skip - add to skip list
                if let Some(ext) = self
                    .popups
                    .lsp_install
                    .get_info()
                    .map(|i| i.extension.clone())
                {
                    self.services.lsp_skip_list.insert(ext);
                }
                self.popups.lsp_install.clear();
                self.popup = Popup::None;
            }
            _ => {}
        }
    }

    /// Handle skills browser popup keyboard events
    pub fn handle_skills_popup_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => {
                if self.popups.skills.search_active {
                    self.popups.skills.toggle_search();
                } else {
                    self.popup = Popup::None;
                }
            }
            KeyCode::Up => self.popups.skills.prev(),
            KeyCode::Down => self.popups.skills.next(),
            KeyCode::Char('k') if !self.popups.skills.search_active => {
                self.popups.skills.prev();
            }
            KeyCode::Char('j') if !self.popups.skills.search_active => {
                self.popups.skills.next();
            }
            KeyCode::Char('/') if !self.popups.skills.search_active => {
                self.popups.skills.toggle_search();
            }
            KeyCode::Char('r') if !self.popups.skills.search_active => {
                self.refresh_skills_browser();
            }
            KeyCode::Backspace if self.popups.skills.search_active => {
                self.popups.skills.backspace_search();
            }
            KeyCode::Char(c) if self.popups.skills.search_active => {
                self.popups.skills.add_search_char(c);
            }
            _ => {}
        }
    }

    /// Handle hooks popup keyboard events
    fn handle_hooks_popup_key(&mut self, code: KeyCode) {
        use crate::paths;
        use crate::storage::Database;
        use crate::tui::popups::hooks::HooksStage;

        match &self.popups.hooks.stage {
            HooksStage::List => match code {
                KeyCode::Esc => self.popup = Popup::None,
                KeyCode::Up | KeyCode::Char('k') => self.popups.hooks.prev(),
                KeyCode::Down | KeyCode::Char('j') => self.popups.hooks.next(),
                KeyCode::Enter => {
                    if self.popups.hooks.is_add_new_selected() {
                        self.popups.hooks.start_add();
                    }
                }
                KeyCode::Char(' ') => {
                    if let Some(id) = self.popups.hooks.get_selected_hook_id() {
                        if let Ok(db) = Database::new(&paths::config_dir().join("krusty.db")) {
                            let id = id.to_string();
                            futures::executor::block_on(async {
                                let _ = self
                                    .services
                                    .user_hook_manager
                                    .write()
                                    .await
                                    .toggle(&db, &id);
                            });
                            self.refresh_hooks_popup();
                        }
                    }
                }
                KeyCode::Char('d') => {
                    if let Some(id) = self.popups.hooks.get_selected_hook_id() {
                        if let Ok(db) = Database::new(&paths::config_dir().join("krusty.db")) {
                            let id = id.to_string();
                            futures::executor::block_on(async {
                                let _ = self
                                    .services
                                    .user_hook_manager
                                    .write()
                                    .await
                                    .delete(&db, &id);
                            });
                            self.refresh_hooks_popup();
                        }
                    }
                }
                _ => {}
            },
            HooksStage::SelectType { .. } => match code {
                KeyCode::Esc => self.popups.hooks.go_back(),
                KeyCode::Up | KeyCode::Char('k') => self.popups.hooks.prev(),
                KeyCode::Down | KeyCode::Char('j') => self.popups.hooks.next(),
                KeyCode::Enter => self.popups.hooks.confirm_type(),
                _ => {}
            },
            HooksStage::EnterMatcher { .. } => match code {
                KeyCode::Esc => self.popups.hooks.go_back(),
                KeyCode::Enter => self.popups.hooks.confirm_matcher(),
                KeyCode::Backspace => self.popups.hooks.backspace(),
                KeyCode::Char(c) => self.popups.hooks.add_char(c),
                _ => {}
            },
            HooksStage::EnterCommand { .. } => match code {
                KeyCode::Esc => self.popups.hooks.go_back(),
                KeyCode::Enter => self.popups.hooks.confirm_command(),
                KeyCode::Backspace => self.popups.hooks.backspace(),
                KeyCode::Char(c) => self.popups.hooks.add_char(c),
                _ => {}
            },
            HooksStage::Confirm { .. } => match code {
                KeyCode::Esc => self.popups.hooks.go_back(),
                KeyCode::Enter => {
                    if let Some(hook) = self.popups.hooks.get_pending_hook() {
                        if let Ok(db) = Database::new(&paths::config_dir().join("krusty.db")) {
                            futures::executor::block_on(async {
                                let _ = self
                                    .services
                                    .user_hook_manager
                                    .write()
                                    .await
                                    .save(&db, hook);
                            });
                            self.refresh_hooks_popup();
                            self.popups.hooks.reset();
                        }
                    }
                }
                _ => {}
            },
        }
    }

    /// Refresh hooks popup with current hooks from database
    fn refresh_hooks_popup(&mut self) {
        let hooks = futures::executor::block_on(async {
            self.services
                .user_hook_manager
                .read()
                .await
                .hooks()
                .to_vec()
        });
        self.popups.hooks.set_hooks(hooks);
    }
}
