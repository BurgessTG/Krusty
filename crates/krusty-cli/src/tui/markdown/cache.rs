//! Markdown Cache
//!
//! Caches rendered markdown lines to avoid re-rendering on every frame.

use std::collections::HashMap;
use std::sync::Arc;

use ratatui::text::Line;

use super::links::RenderedMarkdown;
use crate::tui::themes::Theme;

/// Cache key: (content_hash, wrap_width)
type CacheKey = (u64, usize);

/// Cached markdown with link tracking
pub struct MarkdownCache {
    /// The cache: (message_content_hash, width) -> rendered markdown with links
    cache: HashMap<CacheKey, Arc<RenderedMarkdown>>,
    /// Legacy cache for backward compatibility (no link tracking)
    legacy_cache: HashMap<CacheKey, Arc<Vec<Line<'static>>>>,
    /// Last render width to invalidate on resize
    last_width: usize,
}

impl Default for MarkdownCache {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownCache {
    /// Create a new empty cache
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            legacy_cache: HashMap::new(),
            last_width: 0,
        }
    }

    /// Check if width changed and invalidate if needed
    /// Returns true if cache was invalidated
    pub fn check_width(&mut self, width: usize) -> bool {
        if self.last_width != width {
            self.cache.clear();
            self.legacy_cache.clear();
            self.last_width = width;
            true
        } else {
            false
        }
    }

    /// Get cached lines for content hash (legacy, no link tracking)
    pub fn get(&self, content_hash: u64, width: usize) -> Option<Arc<Vec<Line<'static>>>> {
        self.legacy_cache.get(&(content_hash, width)).cloned()
    }

    /// Get or render markdown with link tracking, caching the result
    pub fn get_or_render_with_links(
        &mut self,
        content: &str,
        content_hash: u64,
        width: usize,
        theme: &Theme,
    ) -> Arc<RenderedMarkdown> {
        let key = (content_hash, width);

        if let Some(cached) = self.cache.get(&key) {
            Arc::clone(cached)
        } else {
            let rendered = super::render_with_links(content, width, theme);
            let arc = Arc::new(rendered);
            self.cache.insert(key, Arc::clone(&arc));
            arc
        }
    }

    /// Get cached rendered markdown (from the links cache)
    pub fn get_rendered(&self, content_hash: u64, width: usize) -> Option<Arc<RenderedMarkdown>> {
        self.cache.get(&(content_hash, width)).cloned()
    }
}
