//! LSP extension browser popup
//!
//! Browse and install Zed WASM extensions for language server support.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use serde::Deserialize;

use super::common::{
    center_content, center_rect, popup_block, popup_title, render_popup_background,
    scroll_indicator, PopupSize,
};
use crate::extensions::ExtensionManifest;
use crate::paths;
use crate::tui::themes::Theme;
use crate::tui::utils::truncate_ellipsis;

/// Zed API extension response
#[derive(Debug, Clone, Deserialize)]
pub struct ZedApiExtension {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub download_count: u64,
    #[serde(default)]
    pub provides: Vec<String>,
    // Note: API also returns authors, repository - not deserialized since unused
}

/// API response wrapper
#[derive(Debug, Deserialize)]
pub struct ZedApiResponse {
    pub data: Vec<ZedApiExtension>,
}

/// LSP extension info for display
#[derive(Debug, Clone)]
pub struct LspExtension {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub installed: bool,
    pub download_count: u64,
}

/// Loading state for async fetch
#[derive(Debug, Clone, PartialEq)]
pub enum LoadingState {
    NotLoaded,
    Loading,
    Loaded,
    Error(String),
}

/// LSP browser popup state
pub struct LspBrowserPopup {
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub extensions: Vec<LspExtension>,
    pub search_query: String,
    pub search_active: bool,
    pub installing: Option<String>,
    pub install_status: Option<String>,
    pub loading_state: LoadingState,
}

impl Default for LspBrowserPopup {
    fn default() -> Self {
        Self::new()
    }
}

impl LspBrowserPopup {
    const API_URL: &'static str = "https://api.zed.dev/extensions?max_schema_version=1";

    pub fn new() -> Self {
        Self {
            selected_index: 0,
            scroll_offset: 0,
            extensions: Vec::new(),
            search_query: String::new(),
            search_active: false,
            installing: None,
            install_status: None,
            loading_state: LoadingState::NotLoaded,
        }
    }

    /// Get download URL for an extension
    pub fn download_url(extension_id: &str) -> String {
        format!("https://api.zed.dev/extensions/{}/download", extension_id)
    }

    /// Start async fetch of extensions from API
    pub fn start_fetch(&mut self) {
        self.loading_state = LoadingState::Loading;
    }

    /// Called when API fetch completes successfully
    pub fn on_fetch_complete(&mut self, api_extensions: Vec<ZedApiExtension>) {
        let ext_dir = paths::extensions_dir();

        self.extensions = api_extensions
            .into_iter()
            .map(|api_ext| {
                // Check if installed locally
                let ext_path = ext_dir.join(&api_ext.id);
                let manifest_path = ext_path.join("extension.toml");
                let wasm_exists = ext_path.join("extension.wasm").exists()
                    || ext_path.join(format!("{}.wasm", api_ext.id)).exists();

                let (installed, local_version) = if manifest_path.exists() && wasm_exists {
                    if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                        if let Ok(manifest) = toml::from_str::<ExtensionManifest>(&content) {
                            (true, Some(manifest.version))
                        } else {
                            (true, None)
                        }
                    } else {
                        (true, None)
                    }
                } else {
                    (false, None)
                };

                LspExtension {
                    id: api_ext.id,
                    name: api_ext.name,
                    version: local_version.unwrap_or(api_ext.version),
                    description: api_ext.description.unwrap_or_default(),
                    installed,
                    download_count: api_ext.download_count,
                }
            })
            .collect();

        // Sort: installed first, then by download count
        self.extensions
            .sort_by(|a, b| match (a.installed, b.installed) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => b.download_count.cmp(&a.download_count),
            });

        self.loading_state = LoadingState::Loaded;
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    /// Called when API fetch fails
    pub fn on_fetch_error(&mut self, error: String) {
        self.loading_state = LoadingState::Error(error);
    }

    /// Check if needs to fetch from API
    pub fn needs_fetch(&self) -> bool {
        self.loading_state == LoadingState::NotLoaded
    }

    /// Check if currently loading
    pub fn is_loading(&self) -> bool {
        self.loading_state == LoadingState::Loading
    }

    /// Refresh the extension list (triggers re-fetch)
    pub fn refresh(&mut self) {
        self.loading_state = LoadingState::NotLoaded;
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    pub fn next(&mut self) {
        let filtered = self.filtered_extensions();
        if self.selected_index < filtered.len().saturating_sub(1) {
            self.selected_index += 1;
            self.ensure_visible();
        }
    }

    pub fn prev(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.ensure_visible();
        }
    }

    fn ensure_visible(&mut self) {
        self.ensure_visible_with_height(10); // Default fallback
    }

    fn ensure_visible_with_height(&mut self, visible_height: usize) {
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + visible_height {
            self.scroll_offset = self.selected_index - visible_height + 1;
        }
    }

    pub fn toggle_search(&mut self) {
        self.search_active = !self.search_active;
        if !self.search_active {
            self.search_query.clear();
            self.selected_index = 0;
            self.scroll_offset = 0;
        }
    }

    pub fn add_search_char(&mut self, c: char) {
        if self.search_active {
            self.search_query.push(c);
            self.selected_index = 0;
            self.scroll_offset = 0;
        }
    }

    pub fn backspace_search(&mut self) {
        if self.search_active {
            self.search_query.pop();
        }
    }

    fn filtered_extensions(&self) -> Vec<(usize, &LspExtension)> {
        if self.search_query.is_empty() {
            self.extensions.iter().enumerate().collect()
        } else {
            let query = self.search_query.to_lowercase();
            self.extensions
                .iter()
                .enumerate()
                .filter(|(_, ext)| {
                    ext.name.to_lowercase().contains(&query)
                        || ext.id.to_lowercase().contains(&query)
                        || ext.description.to_lowercase().contains(&query)
                })
                .collect()
        }
    }

    pub fn get_selected(&self) -> Option<&LspExtension> {
        let filtered = self.filtered_extensions();
        filtered.get(self.selected_index).map(|(_, ext)| *ext)
    }

    /// Check if currently installing
    pub fn is_installing(&self) -> bool {
        self.installing.is_some()
    }

    /// Mark extension as installing
    pub fn start_install(&mut self, ext_id: &str) {
        self.installing = Some(ext_id.to_string());
        self.install_status = Some(format!("Installing {}...", ext_id));
    }

    /// Called when install completes
    pub fn install_complete(&mut self, ext_id: &str, success: bool, message: Option<String>) {
        self.installing = None;

        if success {
            // Update installed status
            if let Some(ext) = self.extensions.iter_mut().find(|e| e.id == ext_id) {
                ext.installed = true;
            }
            self.install_status = Some(format!("✓ {} installed!", ext_id));
        } else {
            self.install_status =
                message.or_else(|| Some(format!("✗ Failed to install {}", ext_id)));
        }
    }

    /// Called when uninstall completes
    pub fn uninstall_complete(&mut self, ext_id: &str, success: bool) {
        if success {
            if let Some(ext) = self.extensions.iter_mut().find(|e| e.id == ext_id) {
                ext.installed = false;
            }
            self.install_status = Some(format!("✓ {} removed", ext_id));
        } else {
            self.install_status = Some(format!("✗ Failed to remove {}", ext_id));
        }
    }

    pub fn render(&self, f: &mut Frame, theme: &Theme) {
        let (w, h) = PopupSize::Large.dimensions();
        let area = center_rect(w, h, f.area());
        render_popup_background(f, area, theme);

        let block = popup_block(theme);
        let inner = block.inner(area);
        f.render_widget(block, area);

        // Calculate dynamic heights
        let search_height = if self.search_active { 2 } else { 0 };
        let status_height = if self.install_status.is_some() { 2 } else { 0 };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),             // Title
                Constraint::Length(search_height), // Search
                Constraint::Length(status_height), // Status
                Constraint::Min(5),                // Content
                Constraint::Length(2),             // Footer
            ])
            .split(inner);

        // Calculate dynamic visible height (reserve 2 for scroll indicators)
        // Each extension takes 2 lines (name + description)
        let visible_height = (chunks[3].height as usize).saturating_sub(2) / 2;

        // Title
        let filtered = self.filtered_extensions();
        let installed_count = self.extensions.iter().filter(|e| e.installed).count();
        let title_text = match &self.loading_state {
            LoadingState::Loading => "Zed Extensions - Loading...".to_string(),
            LoadingState::Error(_) => "Zed Extensions - Error".to_string(),
            _ if !self.search_query.is_empty() => {
                format!(
                    "Zed Extensions ({}/{}) - {} installed",
                    filtered.len(),
                    self.extensions.len(),
                    installed_count
                )
            }
            _ => format!(
                "Zed Extensions ({}) - {} installed",
                self.extensions.len(),
                installed_count
            ),
        };
        let title_lines = popup_title(&title_text, theme);
        let title = Paragraph::new(title_lines).alignment(Alignment::Center);
        f.render_widget(title, chunks[0]);

        // Search bar
        if self.search_active {
            let search = Paragraph::new(Line::from(vec![
                Span::styled("  Search: ", Style::default().fg(theme.accent_color)),
                Span::styled(
                    self.search_query.clone(),
                    Style::default().fg(theme.text_color),
                ),
                Span::styled(
                    "_",
                    Style::default()
                        .fg(theme.accent_color)
                        .add_modifier(Modifier::SLOW_BLINK),
                ),
            ]));
            f.render_widget(search, chunks[1]);
        }

        // Status message
        if let Some(ref status) = self.install_status {
            let color = if status.starts_with('✓') {
                theme.success_color
            } else if status.starts_with('✗') {
                theme.error_color
            } else {
                theme.warning_color
            };
            let status_widget = Paragraph::new(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(status.clone(), Style::default().fg(color)),
            ]));
            f.render_widget(status_widget, chunks[2]);
        }

        // Extension list - always in chunks[3], footer always in chunks[4]
        // (chunks[1] and [2] may have 0 height but still exist in the array)
        let mut lines: Vec<Line> = Vec::new();

        // Handle loading/error states
        match &self.loading_state {
            LoadingState::Loading => {
                lines.push(Line::from(vec![Span::styled(
                    "  Loading extensions from api.zed.dev...",
                    Style::default().fg(theme.dim_color),
                )]));
            }
            LoadingState::Error(err) => {
                lines.push(Line::from(vec![Span::styled(
                    format!("  Error: {}", err),
                    Style::default().fg(theme.error_color),
                )]));
                lines.push(Line::from(vec![Span::styled(
                    "  Press 'r' to retry",
                    Style::default().fg(theme.dim_color),
                )]));
            }
            LoadingState::NotLoaded => {
                lines.push(Line::from(vec![Span::styled(
                    "  Loading extensions...",
                    Style::default().fg(theme.dim_color),
                )]));
            }
            LoadingState::Loaded => {
                // Scroll up indicator
                if self.scroll_offset > 0 {
                    lines.push(scroll_indicator("up", self.scroll_offset, theme));
                }

                // Visible extensions
                let visible_end = (self.scroll_offset + visible_height).min(filtered.len());
                for (display_idx, (_, ext)) in filtered
                    .iter()
                    .enumerate()
                    .skip(self.scroll_offset)
                    .take(visible_height)
                {
                    let is_selected = display_idx == self.selected_index;
                    let is_installing = self
                        .installing
                        .as_ref()
                        .map(|id| id == &ext.id)
                        .unwrap_or(false);

                    // Status indicator
                    let status = if is_installing {
                        ("◐", theme.warning_color)
                    } else if ext.installed {
                        ("●", theme.success_color)
                    } else {
                        ("○", theme.dim_color)
                    };

                    let prefix = if is_selected { " › " } else { "   " };
                    let name_style = if is_selected {
                        Style::default()
                            .fg(theme.accent_color)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.text_color)
                    };

                    // Format download count
                    let downloads = if ext.download_count >= 1000 {
                        format!("{}k", ext.download_count / 1000)
                    } else {
                        format!("{}", ext.download_count)
                    };

                    lines.push(Line::from(vec![
                        Span::styled(prefix.to_string(), name_style),
                        Span::styled(status.0.to_string(), Style::default().fg(status.1)),
                        Span::raw(" "),
                        Span::styled(ext.name.clone(), name_style),
                        Span::styled(
                            format!(" v{}", ext.version),
                            Style::default().fg(theme.dim_color),
                        ),
                        Span::styled(
                            format!(" ({} downloads)", downloads),
                            Style::default().fg(theme.dim_color),
                        ),
                    ]));

                    // Description line (truncate safely)
                    let desc = truncate_ellipsis(&ext.description, 60).into_owned();
                    lines.push(Line::from(vec![
                        Span::raw("      "),
                        Span::styled(desc, Style::default().fg(theme.dim_color)),
                    ]));
                }

                // Scroll down indicator
                let remaining = filtered.len().saturating_sub(visible_end);
                if remaining > 0 {
                    lines.push(scroll_indicator("down", remaining, theme));
                }
            }
        }

        let content = Paragraph::new(lines).style(Style::default().bg(theme.bg_color));
        let content_area = center_content(chunks[3], 4);
        f.render_widget(content, content_area);

        // Footer
        let footer = if self.search_active {
            Paragraph::new(Line::from(vec![
                Span::styled("Type to search  ", Style::default().fg(theme.text_color)),
                Span::styled(
                    "Esc",
                    Style::default()
                        .fg(theme.accent_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(": close search", Style::default().fg(theme.text_color)),
            ]))
        } else if self.is_installing() {
            Paragraph::new(Line::from(vec![Span::styled(
                "Installing... please wait",
                Style::default().fg(theme.warning_color),
            )]))
        } else if self.is_loading() {
            Paragraph::new(Line::from(vec![Span::styled(
                "Loading from api.zed.dev...",
                Style::default().fg(theme.dim_color),
            )]))
        } else {
            Paragraph::new(Line::from(vec![
                Span::styled(
                    "/",
                    Style::default()
                        .fg(theme.accent_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(": search  ", Style::default().fg(theme.text_color)),
                Span::styled(
                    "↑↓",
                    Style::default()
                        .fg(theme.accent_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(": nav  ", Style::default().fg(theme.text_color)),
                Span::styled(
                    "Enter",
                    Style::default()
                        .fg(theme.accent_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(": install  ", Style::default().fg(theme.text_color)),
                Span::styled(
                    "r",
                    Style::default()
                        .fg(theme.accent_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(": refresh  ", Style::default().fg(theme.text_color)),
                Span::styled(
                    "Esc",
                    Style::default()
                        .fg(theme.accent_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(": close", Style::default().fg(theme.text_color)),
            ]))
        };
        f.render_widget(footer.alignment(Alignment::Center), chunks[4]);
    }
}

/// Categories to exclude from the LSP browser
const EXCLUDED_CATEGORIES: &[&str] = &[
    "themes",
    "icon-themes",
    "context-servers", // MCP servers
    "agent-servers",
    "snippets",
    "debug-adapters",
];

/// Categories that indicate language/LSP support
const LANGUAGE_CATEGORIES: &[&str] = &["languages", "grammars", "language-servers"];

/// Check if an extension should be included in the LSP browser
fn should_include_extension(ext: &ZedApiExtension) -> bool {
    let provides = &ext.provides;

    // If provides is empty, include it (catches extensions like Aiken with bad metadata)
    // These are likely language extensions that just didn't declare their provides
    if provides.is_empty() {
        return true;
    }

    // Exclude if it ONLY provides excluded categories
    let has_excluded = provides
        .iter()
        .any(|p| EXCLUDED_CATEGORIES.contains(&p.as_str()));
    let has_language = provides
        .iter()
        .any(|p| LANGUAGE_CATEGORIES.contains(&p.as_str()));

    // Include if it has language support, or if it doesn't have excluded-only categories
    has_language || !has_excluded
}

/// Fetch extensions from API (call this from async context)
/// Filters out themes, icon themes, MCP servers, agent servers, snippets, and debug adapters
pub async fn fetch_extensions_from_api() -> Result<Vec<ZedApiExtension>, String> {
    let client = reqwest::Client::new();

    let response = client
        .get(LspBrowserPopup::API_URL)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("API returned status {}", response.status()));
    }

    let api_response: ZedApiResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    // Filter extensions using our inclusion logic
    let filtered: Vec<ZedApiExtension> = api_response
        .data
        .into_iter()
        .filter(should_include_extension)
        .collect();

    Ok(filtered)
}
