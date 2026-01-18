//! Message rendering
//!
//! Renders the messages panel with all block types.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use regex::Regex;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, LazyLock};

/// Pattern for file references in user messages: [Image: filename] or [PDF: filename]
static FILE_REF_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[(Image|PDF): ([^\]]+)\]").unwrap());

use crate::tui::app::App;
use crate::tui::blocks::{ClipContext, StreamBlock};
use crate::tui::markdown::{apply_hyperlinks, apply_link_hover_style, RenderedMarkdown};
use crate::tui::state::SelectionArea;
use crate::tui::utils::wrap_line;

/// Simple content hash for cache keying
fn hash_content(s: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

/// Clear a rectangular area in the buffer before block rendering
/// This prevents character bleed from underlying Paragraph content
fn clear_area(buf: &mut ratatui::buffer::Buffer, area: Rect, bg_color: Color) {
    for y in area.y..area.y.saturating_add(area.height) {
        for x in area.x..area.x.saturating_add(area.width) {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_char(' ');
                cell.set_bg(bg_color);
                cell.set_fg(Color::Reset);
            }
        }
    }
}

impl App {
    /// Render the messages panel
    pub fn render_messages(&mut self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.theme.border_color));

        let inner = block.inner(area);
        f.render_widget(block, area);

        // Leave 4 chars padding for scrollbar on right side
        // IMPORTANT: Both wrap_width and content_width must use same padding to prevent overflow
        let wrap_width = inner.width.saturating_sub(4) as usize;
        let content_width = inner.width.saturating_sub(4); // Must match wrap_width to prevent block overflow

        // Get selection range if selecting in messages area
        let selection = if self.selection.area == SelectionArea::Messages {
            self.selection.normalized()
        } else {
            None
        };

        // Selection highlight colors from theme
        let sel_bg = self.theme.selection_bg_color;
        let sel_fg = self.theme.selection_fg_color;

        // Clear cache if width changed (resize invalidation)
        self.markdown_cache.check_width(wrap_width);

        // Pre-render all markdown content with link tracking
        // Uses Arc to avoid expensive clones on cache hits
        let mut rendered_markdown: Vec<Option<Arc<RenderedMarkdown>>> =
            Vec::with_capacity(self.messages.len());
        for (role, content) in &self.messages {
            if role == "assistant" {
                let content_hash = hash_content(content);
                let rendered = self.markdown_cache.get_or_render_with_links(
                    content,
                    content_hash,
                    wrap_width,
                    &self.theme,
                );
                rendered_markdown.push(Some(rendered));
            } else {
                rendered_markdown.push(None);
            }
        }

        // Track block positions: (line_start, height, block_index)
        let mut thinking_positions: Vec<(usize, u16, usize)> = Vec::new();
        let mut thinking_idx = 0;
        let mut bash_positions: Vec<(usize, u16, usize)> = Vec::new();
        let mut bash_idx = 0;
        let mut terminal_positions: Vec<(usize, u16, usize)> = Vec::new();
        let mut terminal_idx = 0;
        let mut tool_result_positions: Vec<(usize, u16, usize)> = Vec::new();
        let mut tool_result_idx = 0;
        let mut read_positions: Vec<(usize, u16, usize)> = Vec::new();
        let mut read_idx = 0;
        let mut edit_positions: Vec<(usize, u16, usize)> = Vec::new();
        let mut edit_idx = 0;
        let mut write_positions: Vec<(usize, u16, usize)> = Vec::new();
        let mut write_idx = 0;
        let mut web_search_positions: Vec<(usize, u16, usize)> = Vec::new();
        let mut web_search_idx = 0;
        let mut explore_positions: Vec<(usize, u16, usize)> = Vec::new();
        let mut explore_idx = 0;
        let mut build_positions: Vec<(usize, u16, usize)> = Vec::new();
        let mut build_idx = 0;
        let mut total_lines: usize = 0;

        // Store message heights from first pass to avoid recalculating in second pass
        let mut message_heights: Vec<usize> = Vec::with_capacity(self.messages.len());

        // First pass: calculate positions (using pre-rendered markdown)
        for (msg_idx, (role, content)) in self.messages.iter().enumerate() {
            if role == "thinking" {
                if let Some(tb) = self.blocks.thinking.get(thinking_idx) {
                    let height = tb.height(content_width, &self.theme);
                    thinking_positions.push((total_lines, height, thinking_idx));
                    total_lines += height as usize;
                    total_lines += 1; // blank after
                }
                thinking_idx += 1;
                message_heights.push(0); // Block - height tracked separately
                continue;
            }

            if role == "bash" {
                if let Some(bb) = self.blocks.bash.get(bash_idx) {
                    let height = bb.height(content_width, &self.theme);
                    bash_positions.push((total_lines, height, bash_idx));
                    total_lines += height as usize;
                    total_lines += 1; // blank after
                }
                bash_idx += 1;
                message_heights.push(0); // Block - height tracked separately
                continue;
            }

            if role == "terminal" {
                if self.blocks.pinned_terminal != Some(terminal_idx) {
                    if let Some(tp) = self.blocks.terminal.get(terminal_idx) {
                        let height = tp.height(content_width, &self.theme);
                        terminal_positions.push((total_lines, height, terminal_idx));
                        total_lines += height as usize;
                        total_lines += 1;
                    }
                }
                terminal_idx += 1;
                message_heights.push(0);
                continue;
            }

            if role == "tool_result" {
                if let Some(tr) = self.blocks.tool_result.get(tool_result_idx) {
                    let height = tr.height(content_width, &self.theme);
                    tool_result_positions.push((total_lines, height, tool_result_idx));
                    total_lines += height as usize;
                    total_lines += 1;
                }
                tool_result_idx += 1;
                message_heights.push(0);
                continue;
            }

            if role == "read" {
                if let Some(rb) = self.blocks.read.get(read_idx) {
                    let height = rb.height(content_width, &self.theme);
                    read_positions.push((total_lines, height, read_idx));
                    total_lines += height as usize;
                    total_lines += 1;
                }
                read_idx += 1;
                message_heights.push(0);
                continue;
            }

            if role == "edit" {
                if let Some(eb) = self.blocks.edit.get(edit_idx) {
                    let height = eb.height(content_width, &self.theme);
                    edit_positions.push((total_lines, height, edit_idx));
                    total_lines += height as usize;
                    total_lines += 1;
                }
                edit_idx += 1;
                message_heights.push(0);
                continue;
            }

            if role == "write" {
                if let Some(wb) = self.blocks.write.get(write_idx) {
                    let height = wb.height(content_width, &self.theme);
                    write_positions.push((total_lines, height, write_idx));
                    total_lines += height as usize;
                    total_lines += 1;
                }
                write_idx += 1;
                message_heights.push(0);
                continue;
            }

            if role == "web_search" {
                if let Some(ws) = self.blocks.web_search.get(web_search_idx) {
                    let height = ws.height(content_width, &self.theme);
                    web_search_positions.push((total_lines, height, web_search_idx));
                    total_lines += height as usize;
                    total_lines += 1;
                }
                web_search_idx += 1;
                message_heights.push(0);
                continue;
            }

            if role == "explore" {
                if let Some(eb) = self.blocks.explore.get(explore_idx) {
                    let height = eb.height(content_width, &self.theme);
                    explore_positions.push((total_lines, height, explore_idx));
                    total_lines += height as usize;
                    total_lines += 1;
                }
                explore_idx += 1;
                message_heights.push(0);
                continue;
            }

            if role == "build" {
                if let Some(bb) = self.blocks.build.get(build_idx) {
                    let height = bb.height(content_width, &self.theme);
                    build_positions.push((total_lines, height, build_idx));
                    total_lines += height as usize;
                    total_lines += 1;
                }
                build_idx += 1;
                message_heights.push(0);
                continue;
            }

            // Count content lines based on role and store height
            let msg_height = if role == "assistant" {
                // Use pre-rendered markdown lines
                rendered_markdown.get(msg_idx)
                    .and_then(|r| r.as_ref())
                    .map(|r| r.lines.len())
                    .unwrap_or(0)
            } else {
                // Plain text for user/system
                content.lines().map(|line| {
                    if line.is_empty() { 1 } else { wrap_line(line, wrap_width).len() }
                }).sum()
            };
            message_heights.push(msg_height);
            total_lines += msg_height;
            total_lines += 1; // blank line after message
        }

        // Second pass: build lines with placeholders for custom blocks
        // Also track message base line offsets for hyperlink positions
        // OPTIMIZATION: Only build styled content for visible messages
        let scroll_offset = self.scroll.offset;
        let viewport_height = inner.height as usize;
        let visible_start = scroll_offset.saturating_sub(viewport_height); // Buffer above
        let visible_end = scroll_offset + viewport_height * 2; // Buffer below

        let mut lines: Vec<Line> = Vec::with_capacity(total_lines.min(viewport_height * 3));
        let mut line_idx: usize = 0;
        let mut message_line_offsets: Vec<(usize, usize)> = Vec::new(); // (msg_idx, base_line)
        thinking_idx = 0;
        bash_idx = 0;
        terminal_idx = 0;
        tool_result_idx = 0;
        read_idx = 0;
        edit_idx = 0;
        write_idx = 0;
        web_search_idx = 0;
        explore_idx = 0;
        build_idx = 0;

        for (msg_idx, (role, content)) in self.messages.iter().enumerate() {
            if role == "thinking" {
                if let Some(&(_, height, _)) = thinking_positions.get(thinking_idx) {
                    for _ in 0..height {
                        lines.push(Line::from(""));
                        line_idx += 1;
                    }
                    lines.push(Line::from("")); // blank
                    line_idx += 1;
                }
                thinking_idx += 1;
                continue;
            }

            if role == "bash" {
                if let Some(&(_, height, _)) = bash_positions.get(bash_idx) {
                    for _ in 0..height {
                        lines.push(Line::from(""));
                        line_idx += 1;
                    }
                    lines.push(Line::from("")); // blank
                    line_idx += 1;
                }
                bash_idx += 1;
                continue;
            }

            if role == "terminal" {
                // Skip pinned terminal - it's rendered at top
                if self.blocks.pinned_terminal != Some(terminal_idx) {
                    // Find the position entry for this terminal_idx
                    if let Some(&(_, height, _)) = terminal_positions
                        .iter()
                        .find(|(_, _, idx)| *idx == terminal_idx)
                    {
                        for _ in 0..height {
                            lines.push(Line::from(""));
                            line_idx += 1;
                        }
                        lines.push(Line::from("")); // blank
                        line_idx += 1;
                    }
                }
                terminal_idx += 1;
                continue;
            }

            if role == "tool_result" {
                if let Some(&(_, height, _)) = tool_result_positions.get(tool_result_idx) {
                    for _ in 0..height {
                        lines.push(Line::from(""));
                        line_idx += 1;
                    }
                    lines.push(Line::from("")); // blank
                    line_idx += 1;
                }
                tool_result_idx += 1;
                continue;
            }

            if role == "read" {
                if let Some(&(_, height, _)) = read_positions.get(read_idx) {
                    for _ in 0..height {
                        lines.push(Line::from(""));
                        line_idx += 1;
                    }
                    lines.push(Line::from("")); // blank
                    line_idx += 1;
                }
                read_idx += 1;
                continue;
            }

            if role == "edit" {
                if let Some(&(_, height, _)) = edit_positions.get(edit_idx) {
                    for _ in 0..height {
                        lines.push(Line::from(""));
                        line_idx += 1;
                    }
                    lines.push(Line::from("")); // blank
                    line_idx += 1;
                }
                edit_idx += 1;
                continue;
            }

            if role == "write" {
                if let Some(&(_, height, _)) = write_positions.get(write_idx) {
                    for _ in 0..height {
                        lines.push(Line::from(""));
                        line_idx += 1;
                    }
                    lines.push(Line::from("")); // blank
                    line_idx += 1;
                }
                write_idx += 1;
                continue;
            }

            if role == "web_search" {
                if let Some(&(_, height, _)) = web_search_positions.get(web_search_idx) {
                    for _ in 0..height {
                        lines.push(Line::from(""));
                        line_idx += 1;
                    }
                    lines.push(Line::from("")); // blank
                    line_idx += 1;
                }
                web_search_idx += 1;
                continue;
            }

            if role == "explore" {
                if let Some(&(_, height, _)) = explore_positions.get(explore_idx) {
                    for _ in 0..height {
                        lines.push(Line::from(""));
                        line_idx += 1;
                    }
                    lines.push(Line::from("")); // blank
                    line_idx += 1;
                }
                explore_idx += 1;
                continue;
            }

            if role == "build" {
                if let Some(&(_, height, _)) = build_positions.get(build_idx) {
                    for _ in 0..height {
                        lines.push(Line::from(""));
                        line_idx += 1;
                    }
                    lines.push(Line::from("")); // blank
                    line_idx += 1;
                }
                build_idx += 1;
                continue;
            }

            // Get cached height for this message (avoids recalculating wrap_line)
            let msg_height = message_heights.get(msg_idx).copied().unwrap_or(0);
            let msg_end = line_idx + msg_height;

            // OPTIMIZATION: Check if this message is visible
            if msg_end < visible_start || line_idx > visible_end {
                // Off-screen: push empty placeholders (fast path)
                for _ in 0..msg_height {
                    lines.push(Line::from(""));
                    line_idx += 1;
                }
            } else if role == "assistant" {
                // On-screen assistant: render markdown
                if let Some(Some(rendered)) = rendered_markdown.get(msg_idx) {
                    message_line_offsets.push((msg_idx, line_idx));

                    if selection.is_some() {
                        for md_line in rendered.lines.iter() {
                            let highlighted = apply_selection_to_rendered_line(
                                md_line.clone(),
                                line_idx,
                                selection,
                                sel_bg,
                                sel_fg,
                            );
                            lines.push(highlighted);
                            line_idx += 1;
                        }
                    } else {
                        for md_line in rendered.lines.iter() {
                            lines.push(md_line.clone());
                            line_idx += 1;
                        }
                    }
                }
            } else {
                // On-screen user/system: render plain text
                let content_color = match role.as_str() {
                    "user" => self.theme.user_msg_color,
                    "system" => self.theme.system_msg_color,
                    _ => self.theme.text_color,
                };

                let hovered_file_ref = self.hover.message_file_ref.as_ref();

                for line in content.lines() {
                    if line.is_empty() {
                        lines.push(Line::from(""));
                        line_idx += 1;
                    } else {
                        for wrapped in wrap_line(line, wrap_width) {
                            let content_line = if role == "user" {
                                style_user_line_with_file_refs(
                                    &wrapped,
                                    line_idx,
                                    selection,
                                    Style::default().fg(content_color),
                                    self.theme.link_color,
                                    sel_bg,
                                    sel_fg,
                                    msg_idx,
                                    hovered_file_ref,
                                )
                            } else {
                                apply_selection_to_line(
                                    wrapped,
                                    line_idx,
                                    selection,
                                    Style::default().fg(content_color),
                                    sel_bg,
                                    sel_fg,
                                )
                            };
                            lines.push(content_line);
                            line_idx += 1;
                        }
                    }
                }
            }
            lines.push(Line::from("")); // Blank between messages
            line_idx += 1;
        }

        // Render text content
        // Clamp scroll offset to u16::MAX to prevent overflow (supports ~65k lines of paragraph text)
        let clamped_scroll = self.scroll.offset.min(u16::MAX as usize) as u16;
        f.render_widget(Paragraph::new(lines).scroll((clamped_scroll, 0)), inner);

        // Apply OSC 8 hyperlinks to the buffer after Paragraph rendering
        // This wraps each link cell's symbol with escape sequences
        for (msg_idx, base_line) in &message_line_offsets {
            if let Some(Some(rendered)) = rendered_markdown.get(*msg_idx) {
                if !rendered.links.is_empty() {
                    apply_hyperlinks(
                        f.buffer_mut(),
                        inner,
                        &rendered.links,
                        self.scroll.offset,
                        *base_line,
                    );

                    // Apply hover styling if this message contains the hovered link
                    if let Some(hovered) = &self.hover.message_link {
                        if hovered.msg_idx == *msg_idx {
                            apply_link_hover_style(
                                f.buffer_mut(),
                                inner,
                                &rendered.links,
                                Some(hovered),
                                self.scroll.offset,
                                *base_line,
                                self.theme.link_color,
                            );
                        }
                    }
                }
            }
        }

        // Overlay each block at its position
        // Use usize for all position math to avoid overflow, convert to u16 only for screen coords
        let scroll = self.scroll.offset;
        let inner_height = inner.height as usize;

        for (start_line, height, idx) in &thinking_positions {
            if let Some(tb) = self.blocks.thinking.get(*idx) {
                let block_y = *start_line;
                let block_height = *height as usize;

                // Check if visible (all math in usize)
                if block_y + block_height > scroll && block_y < scroll + inner_height {
                    let screen_y = if block_y >= scroll {
                        inner.y + (block_y - scroll).min(u16::MAX as usize) as u16
                    } else {
                        inner.y
                    };

                    let clip_top = scroll.saturating_sub(block_y) as u16;
                    let available_height = inner.height.saturating_sub(screen_y - inner.y);
                    let visible_height = height.saturating_sub(clip_top).min(available_height);
                    let clip_bottom = height.saturating_sub(clip_top + visible_height);

                    if visible_height > 0 {
                        let thinking_area = Rect {
                            x: inner.x,
                            y: screen_y,
                            width: content_width,
                            height: visible_height,
                        };

                        // Create clip context if partially visible
                        let clip = if clip_top > 0 || clip_bottom > 0 {
                            Some(ClipContext {
                                clip_top,
                                clip_bottom,
                            })
                        } else {
                            None
                        };

                        // Clear full inner.width to remove Paragraph bleed in scrollbar gap
                        let clear_rect = Rect {
                            x: inner.x,
                            y: screen_y,
                            width: inner.width,
                            height: visible_height,
                        };
                        clear_area(f.buffer_mut(), clear_rect, self.theme.bg_color);
                        tb.render(thinking_area, f.buffer_mut(), &self.theme, false, clip);
                    }
                }
            }
        }

        // Overlay each bash block at its position
        for (start_line, height, idx) in &bash_positions {
            if let Some(bb) = self.blocks.bash.get(*idx) {
                let block_y = *start_line;
                let block_height = *height as usize;

                if block_y + block_height > scroll && block_y < scroll + inner_height {
                    let screen_y = if block_y >= scroll {
                        inner.y + (block_y - scroll).min(u16::MAX as usize) as u16
                    } else {
                        inner.y
                    };

                    let clip_top = scroll.saturating_sub(block_y) as u16;
                    let available_height = inner.height.saturating_sub(screen_y - inner.y);
                    let visible_height = height.saturating_sub(clip_top).min(available_height);
                    let clip_bottom = height.saturating_sub(clip_top + visible_height);

                    if visible_height > 0 {
                        let bash_area = Rect {
                            x: inner.x,
                            y: screen_y,
                            width: content_width,
                            height: visible_height,
                        };

                        let clip = if clip_top > 0 || clip_bottom > 0 {
                            Some(ClipContext {
                                clip_top,
                                clip_bottom,
                            })
                        } else {
                            None
                        };

                        let clear_rect = Rect {
                            x: inner.x,
                            y: screen_y,
                            width: inner.width,
                            height: visible_height,
                        };
                        clear_area(f.buffer_mut(), clear_rect, self.theme.bg_color);
                        bb.render(bash_area, f.buffer_mut(), &self.theme, false, clip);
                    }
                }
            }
        }

        // Overlay each terminal pane at its position
        for (start_line, height, idx) in &terminal_positions {
            if let Some(tp) = self.blocks.terminal.get(*idx) {
                let block_y = *start_line;
                let block_height = *height as usize;

                if block_y + block_height > scroll && block_y < scroll + inner_height {
                    let screen_y = if block_y >= scroll {
                        inner.y + (block_y - scroll).min(u16::MAX as usize) as u16
                    } else {
                        inner.y
                    };

                    let clip_top = scroll.saturating_sub(block_y) as u16;
                    let available_height = inner.height.saturating_sub(screen_y - inner.y);
                    let visible_height = height.saturating_sub(clip_top).min(available_height);
                    let clip_bottom = height.saturating_sub(clip_top + visible_height);

                    if visible_height > 0 {
                        let terminal_area = Rect {
                            x: inner.x,
                            y: screen_y,
                            width: content_width,
                            height: visible_height,
                        };

                        let clip = if clip_top > 0 || clip_bottom > 0 {
                            Some(ClipContext {
                                clip_top,
                                clip_bottom,
                            })
                        } else {
                            None
                        };

                        let is_focused = self.blocks.focused_terminal == Some(*idx);
                        let clear_rect = Rect {
                            x: inner.x,
                            y: screen_y,
                            width: inner.width,
                            height: visible_height,
                        };
                        clear_area(f.buffer_mut(), clear_rect, self.theme.bg_color);
                        tp.render(terminal_area, f.buffer_mut(), &self.theme, is_focused, clip);
                    }
                }
            }
        }

        // Overlay each tool_result block at its position
        for (start_line, height, idx) in &tool_result_positions {
            if let Some(tr) = self.blocks.tool_result.get(*idx) {
                let block_y = *start_line;
                let block_height = *height as usize;

                if block_y + block_height > scroll && block_y < scroll + inner_height {
                    let screen_y = if block_y >= scroll {
                        inner.y + (block_y - scroll).min(u16::MAX as usize) as u16
                    } else {
                        inner.y
                    };

                    let clip_top = scroll.saturating_sub(block_y) as u16;
                    let available_height = inner.height.saturating_sub(screen_y - inner.y);
                    let visible_height = height.saturating_sub(clip_top).min(available_height);
                    let clip_bottom = height.saturating_sub(clip_top + visible_height);

                    if visible_height > 0 {
                        let tool_result_area = Rect {
                            x: inner.x,
                            y: screen_y,
                            width: content_width,
                            height: visible_height,
                        };

                        let clip = if clip_top > 0 || clip_bottom > 0 {
                            Some(ClipContext {
                                clip_top,
                                clip_bottom,
                            })
                        } else {
                            None
                        };

                        let clear_rect = Rect {
                            x: inner.x,
                            y: screen_y,
                            width: inner.width,
                            height: visible_height,
                        };
                        clear_area(f.buffer_mut(), clear_rect, self.theme.bg_color);
                        tr.render(tool_result_area, f.buffer_mut(), &self.theme, false, clip);
                    }
                }
            }
        }

        // Overlay each read block at its position
        for (start_line, height, idx) in &read_positions {
            if let Some(rb) = self.blocks.read.get(*idx) {
                let block_y = *start_line;
                let block_height = *height as usize;

                if block_y + block_height > scroll && block_y < scroll + inner_height {
                    let screen_y = if block_y >= scroll {
                        inner.y + (block_y - scroll).min(u16::MAX as usize) as u16
                    } else {
                        inner.y
                    };

                    let clip_top = scroll.saturating_sub(block_y) as u16;
                    let available_height = inner.height.saturating_sub(screen_y - inner.y);
                    let visible_height = height.saturating_sub(clip_top).min(available_height);
                    let clip_bottom = height.saturating_sub(clip_top + visible_height);

                    if visible_height > 0 {
                        let read_area = Rect {
                            x: inner.x,
                            y: screen_y,
                            width: content_width,
                            height: visible_height,
                        };

                        let clip = if clip_top > 0 || clip_bottom > 0 {
                            Some(ClipContext {
                                clip_top,
                                clip_bottom,
                            })
                        } else {
                            None
                        };

                        let clear_rect = Rect {
                            x: inner.x,
                            y: screen_y,
                            width: inner.width,
                            height: visible_height,
                        };
                        clear_area(f.buffer_mut(), clear_rect, self.theme.bg_color);
                        rb.render(read_area, f.buffer_mut(), &self.theme, false, clip);
                    }
                }
            }
        }

        // Overlay each edit block at its position
        for (start_line, height, idx) in &edit_positions {
            if let Some(eb) = self.blocks.edit.get(*idx) {
                let block_y = *start_line;
                let block_height = *height as usize;

                if block_y + block_height > scroll && block_y < scroll + inner_height {
                    let screen_y = if block_y >= scroll {
                        inner.y + (block_y - scroll).min(u16::MAX as usize) as u16
                    } else {
                        inner.y
                    };

                    let clip_top = scroll.saturating_sub(block_y) as u16;
                    let available_height = inner.height.saturating_sub(screen_y - inner.y);
                    let visible_height = height.saturating_sub(clip_top).min(available_height);
                    let clip_bottom = height.saturating_sub(clip_top + visible_height);

                    if visible_height > 0 {
                        let edit_area = Rect {
                            x: inner.x,
                            y: screen_y,
                            width: content_width,
                            height: visible_height,
                        };

                        let clip = if clip_top > 0 || clip_bottom > 0 {
                            Some(ClipContext {
                                clip_top,
                                clip_bottom,
                            })
                        } else {
                            None
                        };

                        let clear_rect = Rect {
                            x: inner.x,
                            y: screen_y,
                            width: inner.width,
                            height: visible_height,
                        };
                        clear_area(f.buffer_mut(), clear_rect, self.theme.bg_color);
                        eb.render(edit_area, f.buffer_mut(), &self.theme, false, clip);
                    }
                }
            }
        }

        // Overlay each write block at its position
        for (start_line, height, idx) in &write_positions {
            if let Some(wb) = self.blocks.write.get(*idx) {
                let block_y = *start_line;
                let block_height = *height as usize;

                if block_y + block_height > scroll && block_y < scroll + inner_height {
                    let screen_y = if block_y >= scroll {
                        inner.y + (block_y - scroll).min(u16::MAX as usize) as u16
                    } else {
                        inner.y
                    };

                    let clip_top = scroll.saturating_sub(block_y) as u16;
                    let available_height = inner.height.saturating_sub(screen_y - inner.y);
                    let visible_height = height.saturating_sub(clip_top).min(available_height);
                    let clip_bottom = height.saturating_sub(clip_top + visible_height);

                    if visible_height > 0 {
                        let write_area = Rect {
                            x: inner.x,
                            y: screen_y,
                            width: content_width,
                            height: visible_height,
                        };

                        let clip = if clip_top > 0 || clip_bottom > 0 {
                            Some(ClipContext {
                                clip_top,
                                clip_bottom,
                            })
                        } else {
                            None
                        };

                        let clear_rect = Rect {
                            x: inner.x,
                            y: screen_y,
                            width: inner.width,
                            height: visible_height,
                        };
                        clear_area(f.buffer_mut(), clear_rect, self.theme.bg_color);
                        wb.render(write_area, f.buffer_mut(), &self.theme, false, clip);
                    }
                }
            }
        }

        // Overlay each web_search block at its position
        for (start_line, height, idx) in &web_search_positions {
            if let Some(ws) = self.blocks.web_search.get(*idx) {
                let block_y = *start_line;
                let block_height = *height as usize;

                if block_y + block_height > scroll && block_y < scroll + inner_height {
                    let screen_y = if block_y >= scroll {
                        inner.y + (block_y - scroll).min(u16::MAX as usize) as u16
                    } else {
                        inner.y
                    };

                    let clip_top = scroll.saturating_sub(block_y) as u16;
                    let available_height = inner.height.saturating_sub(screen_y - inner.y);
                    let visible_height = height.saturating_sub(clip_top).min(available_height);
                    let clip_bottom = height.saturating_sub(clip_top + visible_height);

                    if visible_height > 0 {
                        let web_search_area = Rect {
                            x: inner.x,
                            y: screen_y,
                            width: content_width,
                            height: visible_height,
                        };

                        let clip = if clip_top > 0 || clip_bottom > 0 {
                            Some(ClipContext {
                                clip_top,
                                clip_bottom,
                            })
                        } else {
                            None
                        };

                        let clear_rect = Rect {
                            x: inner.x,
                            y: screen_y,
                            width: inner.width,
                            height: visible_height,
                        };
                        clear_area(f.buffer_mut(), clear_rect, self.theme.bg_color);
                        ws.render(web_search_area, f.buffer_mut(), &self.theme, false, clip);
                    }
                }
            }
        }

        // Overlay each explore block at its position
        for (start_line, height, idx) in &explore_positions {
            if let Some(eb) = self.blocks.explore.get(*idx) {
                let block_y = *start_line;
                let block_height = *height as usize;

                if block_y + block_height > scroll && block_y < scroll + inner_height {
                    let screen_y = if block_y >= scroll {
                        inner.y + (block_y - scroll).min(u16::MAX as usize) as u16
                    } else {
                        inner.y
                    };

                    let clip_top = scroll.saturating_sub(block_y) as u16;
                    let available_height = inner.height.saturating_sub(screen_y - inner.y);
                    let visible_height = height.saturating_sub(clip_top).min(available_height);
                    let clip_bottom = height.saturating_sub(clip_top + visible_height);

                    if visible_height > 0 {
                        let explore_area = Rect {
                            x: inner.x,
                            y: screen_y,
                            width: content_width,
                            height: visible_height,
                        };

                        let clip = if clip_top > 0 || clip_bottom > 0 {
                            Some(ClipContext {
                                clip_top,
                                clip_bottom,
                            })
                        } else {
                            None
                        };

                        let clear_rect = Rect {
                            x: inner.x,
                            y: screen_y,
                            width: inner.width,
                            height: visible_height,
                        };
                        clear_area(f.buffer_mut(), clear_rect, self.theme.bg_color);
                        eb.render(explore_area, f.buffer_mut(), &self.theme, false, clip);
                    }
                }
            }
        }

        // Overlay each build block at its position
        for (start_line, height, idx) in &build_positions {
            if let Some(bb) = self.blocks.build.get(*idx) {
                let block_y = *start_line;
                let block_height = *height as usize;

                if block_y + block_height > scroll && block_y < scroll + inner_height {
                    let screen_y = if block_y >= scroll {
                        inner.y + (block_y - scroll).min(u16::MAX as usize) as u16
                    } else {
                        inner.y
                    };

                    let clip_top = scroll.saturating_sub(block_y) as u16;
                    let available_height = inner.height.saturating_sub(screen_y - inner.y);
                    let visible_height = height.saturating_sub(clip_top).min(available_height);
                    let clip_bottom = height.saturating_sub(clip_top + visible_height);

                    if visible_height > 0 {
                        let build_area = Rect {
                            x: inner.x,
                            y: screen_y,
                            width: content_width,
                            height: visible_height,
                        };

                        let clip = if clip_top > 0 || clip_bottom > 0 {
                            Some(ClipContext {
                                clip_top,
                                clip_bottom,
                            })
                        } else {
                            None
                        };

                        let clear_rect = Rect {
                            x: inner.x,
                            y: screen_y,
                            width: inner.width,
                            height: visible_height,
                        };
                        clear_area(f.buffer_mut(), clear_rect, self.theme.bg_color);
                        bb.render(build_area, f.buffer_mut(), &self.theme, false, clip);
                    }
                }
            }
        }

        // Resize terminal PTYs to match render width (debounced)
        // Note: tick() is called in the event loop before render, not here
        for tp in &mut self.blocks.terminal {
            tp.resize_to_width(content_width);
        }
    }
}

/// Apply selection highlighting to a pre-rendered markdown line (preserves existing styles)
/// Uses CHARACTER indexing (not byte indexing) to handle UTF-8 safely
#[inline]
fn apply_selection_to_rendered_line(
    line: Line<'static>,
    line_idx: usize,
    selection: Option<((usize, usize), (usize, usize))>,
    sel_bg: Color,
    sel_fg: Color,
) -> Line<'static> {
    // Early return if no selection (most common case)
    let Some(((start_line, start_col), (end_line, end_col))) = selection else {
        return line;
    };

    // Early return if line outside selection range
    if line_idx < start_line || line_idx > end_line {
        return line;
    }

    // Calculate total CHARACTER count (not byte count!)
    let total_chars: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
    if total_chars == 0 {
        return line;
    }

    // Determine selection bounds for this line (in characters)
    let (sel_start, sel_end) = if line_idx == start_line && line_idx == end_line {
        (start_col.min(total_chars), end_col.min(total_chars))
    } else if line_idx == start_line {
        (start_col.min(total_chars), total_chars)
    } else if line_idx == end_line {
        (0, end_col.min(total_chars))
    } else {
        (0, total_chars)
    };

    if sel_start >= sel_end {
        return line;
    }

    // For full-line selection, just restyle all spans (faster path)
    if sel_start == 0 && sel_end >= total_chars {
        return Line::from(
            line.spans
                .into_iter()
                .map(|span| Span::styled(span.content, span.style.bg(sel_bg).fg(sel_fg)))
                .collect::<Vec<_>>(),
        );
    }

    // Partial selection - split spans at CHARACTER boundaries (UTF-8 safe)
    let mut new_spans = Vec::with_capacity(line.spans.len() + 2);
    let mut char_pos = 0;

    for span in line.spans {
        let span_char_len = span.content.chars().count();
        let span_char_end = char_pos + span_char_len;

        if span_char_end <= sel_start || char_pos >= sel_end {
            // Span is entirely outside selection
            new_spans.push(span);
        } else if char_pos >= sel_start && span_char_end <= sel_end {
            // Span is entirely inside selection
            new_spans.push(Span::styled(span.content, span.style.bg(sel_bg).fg(sel_fg)));
        } else {
            // Span partially overlaps - split at character boundaries
            let local_start = sel_start.saturating_sub(char_pos);
            let local_end = (sel_end - char_pos).min(span_char_len);
            let chars: Vec<char> = span.content.chars().collect();

            if local_start > 0 {
                let before: String = chars[..local_start].iter().collect();
                new_spans.push(Span::styled(before, span.style));
            }
            let selected: String = chars[local_start..local_end].iter().collect();
            new_spans.push(Span::styled(selected, span.style.bg(sel_bg).fg(sel_fg)));
            if local_end < span_char_len {
                let after: String = chars[local_end..].iter().collect();
                new_spans.push(Span::styled(after, span.style));
            }
        }

        char_pos = span_char_end;
    }

    Line::from(new_spans)
}

/// Apply selection highlighting to a line (takes ownership of text)
fn apply_selection_to_line(
    text: String,
    line_idx: usize,
    selection: Option<((usize, usize), (usize, usize))>,
    base_style: Style,
    sel_bg: Color,
    sel_fg: Color,
) -> Line<'static> {
    let Some(((start_line, start_col), (end_line, end_col))) = selection else {
        return Line::from(Span::styled(text, base_style));
    };

    // Check if this line is within selection
    if line_idx < start_line || line_idx > end_line {
        return Line::from(Span::styled(text, base_style));
    }

    let text_len = text.chars().count();
    let sel_style = Style::default()
        .bg(sel_bg)
        .fg(sel_fg)
        .add_modifier(Modifier::BOLD);

    // Determine selection bounds for this line
    let (sel_start, sel_end) = if line_idx == start_line && line_idx == end_line {
        // Selection starts and ends on this line
        (start_col.min(text_len), end_col.min(text_len))
    } else if line_idx == start_line {
        // Selection starts on this line, continues past
        (start_col.min(text_len), text_len)
    } else if line_idx == end_line {
        // Selection ends on this line
        (0, end_col.min(text_len))
    } else {
        // Entire line is selected
        (0, text_len)
    };

    if sel_start >= sel_end {
        return Line::from(Span::styled(text, base_style));
    }

    // Split text into three parts: before, selected, after
    let chars: Vec<char> = text.chars().collect();
    let before: String = chars[..sel_start].iter().collect();
    let selected: String = chars[sel_start..sel_end].iter().collect();
    let after: String = chars[sel_end..].iter().collect();

    let mut spans = Vec::new();
    if !before.is_empty() {
        spans.push(Span::styled(before, base_style));
    }
    if !selected.is_empty() {
        spans.push(Span::styled(selected, sel_style));
    }
    if !after.is_empty() {
        spans.push(Span::styled(after, base_style));
    }

    Line::from(spans)
}

/// Style a user message line with file references highlighted
/// File references like [filename.png] get link styling with hover effects
#[allow(clippy::too_many_arguments)]
fn style_user_line_with_file_refs(
    text: &str,
    line_idx: usize,
    selection: Option<((usize, usize), (usize, usize))>,
    base_style: Style,
    link_color: Color,
    sel_bg: Color,
    sel_fg: Color,
    msg_idx: usize,
    hovered_file_ref: Option<&(usize, String)>,
) -> Line<'static> {
    let link_style = Style::default()
        .fg(link_color)
        .add_modifier(Modifier::UNDERLINED);
    // Hover style: inverted colors for clear visibility
    let hover_style = Style::default()
        .fg(Color::Black)
        .bg(link_color)
        .add_modifier(Modifier::BOLD);
    let sel_style = Style::default()
        .bg(sel_bg)
        .fg(sel_fg)
        .add_modifier(Modifier::BOLD);

    // Find file reference matches with capture groups
    let captures: Vec<_> = FILE_REF_PATTERN.captures_iter(text).collect();

    if captures.is_empty() {
        // No file refs - use standard rendering
        return apply_selection_to_line(text.to_string(), line_idx, selection, base_style, sel_bg, sel_fg);
    }

    // Build spans with file refs styled
    let mut spans = Vec::new();
    let mut last_end = 0;

    for caps in captures {
        let full_match = caps.get(0).unwrap();

        // Add text before this match
        if full_match.start() > last_end {
            let before = &text[last_end..full_match.start()];
            spans.push(Span::styled(before.to_string(), base_style));
        }

        // Get the full file ref text (e.g., "[Image: screenshot.png]")
        let file_ref = full_match.as_str();
        // Extract display name from capture group 2 (the filename part)
        let display_name = caps.get(2).map(|m| m.as_str()).unwrap_or("");

        // Check if this file ref is hovered
        let is_hovered = hovered_file_ref
            .map(|(idx, name)| *idx == msg_idx && name == display_name)
            .unwrap_or(false);

        let style = if is_hovered { hover_style } else { link_style };
        spans.push(Span::styled(file_ref.to_string(), style));

        last_end = full_match.end();
    }

    // Add remaining text after last match
    if last_end < text.len() {
        let after = &text[last_end..];
        spans.push(Span::styled(after.to_string(), base_style));
    }

    // Apply selection if active
    if let Some(((start_line, start_col), (end_line, end_col))) = selection {
        if line_idx >= start_line && line_idx <= end_line {
            // Selection overlaps this line - need to apply selection styling
            let text_len = text.chars().count();
            let (sel_start, sel_end) = if line_idx == start_line && line_idx == end_line {
                (start_col.min(text_len), end_col.min(text_len))
            } else if line_idx == start_line {
                (start_col.min(text_len), text_len)
            } else if line_idx == end_line {
                (0, end_col.min(text_len))
            } else {
                (0, text_len)
            };

            if sel_start < sel_end {
                // Apply selection styling to spans
                return apply_selection_to_spans(spans, sel_start, sel_end, sel_style);
            }
        }
    }

    Line::from(spans)
}

/// Apply selection highlighting to a vector of spans
fn apply_selection_to_spans(
    spans: Vec<Span<'static>>,
    sel_start: usize,
    sel_end: usize,
    sel_style: Style,
) -> Line<'static> {
    let mut new_spans = Vec::with_capacity(spans.len() + 2);
    let mut char_pos = 0;

    for span in spans {
        let span_char_len = span.content.chars().count();
        let span_char_end = char_pos + span_char_len;

        if span_char_end <= sel_start || char_pos >= sel_end {
            // Span is entirely outside selection
            new_spans.push(span);
        } else if char_pos >= sel_start && span_char_end <= sel_end {
            // Span is entirely inside selection
            new_spans.push(Span::styled(span.content, sel_style));
        } else {
            // Span partially overlaps - split at character boundaries
            let local_start = sel_start.saturating_sub(char_pos);
            let local_end = (sel_end - char_pos).min(span_char_len);
            let chars: Vec<char> = span.content.chars().collect();

            if local_start > 0 {
                let before: String = chars[..local_start].iter().collect();
                new_spans.push(Span::styled(before, span.style));
            }
            let selected: String = chars[local_start..local_end].iter().collect();
            new_spans.push(Span::styled(selected, sel_style));
            if local_end < span_char_len {
                let after: String = chars[local_end..].iter().collect();
                new_spans.push(Span::styled(after, span.style));
            }
        }

        char_pos = span_char_end;
    }

    Line::from(new_spans)
}
