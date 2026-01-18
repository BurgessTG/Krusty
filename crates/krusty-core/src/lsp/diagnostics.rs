//! Diagnostics cache - stores LSP diagnostics by file
//!
//! Formats diagnostics for injection into tool output.

use lsp_types::{Diagnostic, DiagnosticSeverity, Uri};
use std::collections::HashMap;
use std::path::Path;
use std::sync::RwLock;

/// Maximum errors to show per file
const MAX_ERRORS_PER_FILE: usize = 20;

/// Cache for LSP diagnostics
pub struct DiagnosticsCache {
    cache: RwLock<HashMap<Uri, Vec<Diagnostic>>>,
}

impl Default for DiagnosticsCache {
    fn default() -> Self {
        Self::new()
    }
}

impl DiagnosticsCache {
    pub fn new() -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
        }
    }

    /// Update diagnostics for a file
    pub fn update(&self, uri: Uri, diagnostics: Vec<Diagnostic>) {
        let mut cache = self.cache.write().unwrap_or_else(|e| e.into_inner());
        if diagnostics.is_empty() {
            cache.remove(&uri);
        } else {
            cache.insert(uri, diagnostics);
        }
    }

    /// Get total error count across all files
    pub fn error_count(&self) -> usize {
        let cache = self.cache.read().unwrap_or_else(|e| e.into_inner());
        cache
            .values()
            .flat_map(|diags| diags.iter())
            .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
            .count()
    }

    /// Get total warning count across all files
    pub fn warning_count(&self) -> usize {
        let cache = self.cache.read().unwrap_or_else(|e| e.into_inner());
        cache
            .values()
            .flat_map(|diags| diags.iter())
            .filter(|d| d.severity == Some(DiagnosticSeverity::WARNING))
            .count()
    }

    /// Pretty format a single diagnostic
    ///
    /// Format: `ERROR [12:5] expected `,` but found `}`
    pub fn pretty_format(diag: &Diagnostic) -> String {
        let severity = match diag.severity {
            Some(DiagnosticSeverity::ERROR) => "ERROR",
            Some(DiagnosticSeverity::WARNING) => "WARN",
            Some(DiagnosticSeverity::INFORMATION) => "INFO",
            Some(DiagnosticSeverity::HINT) => "HINT",
            None | Some(_) => "INFO",
        };
        let line = diag.range.start.line + 1;
        let col = diag.range.start.character + 1;
        format!("{} [{}:{}] {}", severity, line, col, diag.message)
    }

    /// Format diagnostics for a specific file for tool output injection
    ///
    /// Returns XML-formatted diagnostics block
    pub fn format_for_file(&self, path: &Path) -> Option<String> {
        let uri = path_to_uri(path)?;
        let cache = self.cache.read().unwrap_or_else(|e| e.into_inner());
        let diagnostics = cache.get(&uri)?;

        // Filter to errors only (or all if no errors)
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
            .take(MAX_ERRORS_PER_FILE)
            .collect();

        if errors.is_empty() {
            return None;
        }

        let mut output = String::new();
        output.push_str("<file_diagnostics>\n");
        for diag in &errors {
            output.push_str(&format!("{}\n", Self::pretty_format(diag)));
        }
        output.push_str("</file_diagnostics>\n");

        Some(output)
    }

    /// Format diagnostics for display (all files)
    pub fn format_for_display(&self) -> String {
        let cache = self.cache.read().unwrap_or_else(|e| e.into_inner());
        let mut output = String::new();

        for (uri, diagnostics) in cache.iter() {
            let path = uri.path();
            for diag in diagnostics {
                let severity = match diag.severity {
                    Some(DiagnosticSeverity::ERROR) => "ERROR",
                    Some(DiagnosticSeverity::WARNING) => "WARN",
                    Some(DiagnosticSeverity::INFORMATION) => "INFO",
                    Some(DiagnosticSeverity::HINT) => "HINT",
                    None | Some(_) => "???",
                };

                let line = diag.range.start.line + 1;
                output.push_str(&format!(
                    "[{}] {}:{} - {}\n",
                    severity, path, line, diag.message
                ));
            }
        }

        output
    }
}

/// Convert a Path to an lsp_types::Uri
fn path_to_uri(path: &Path) -> Option<Uri> {
    let url = url::Url::from_file_path(path).ok()?;
    std::str::FromStr::from_str(url.as_str()).ok()
}
