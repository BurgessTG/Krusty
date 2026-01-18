//! Theme builder with intelligent defaults

use super::Theme;
use ratatui::style::Color;

/// Builder pattern for creating themes with sensible defaults
pub struct ThemeBuilder {
    theme: Theme,
}

impl ThemeBuilder {
    /// Create a new theme builder with the given name and base colors
    pub fn new(name: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            theme: Theme {
                name: name.into(),
                display_name: display_name.into(),
                // Initialize with black - will be overridden
                bg_color: Color::Rgb(0, 0, 0),
                border_color: Color::Rgb(0, 0, 0),
                title_color: Color::Rgb(0, 0, 0),
                accent_color: Color::Rgb(0, 0, 0),
                text_color: Color::Rgb(0, 0, 0),
                success_color: Color::Rgb(0, 0, 0),
                dim_color: Color::Rgb(0, 0, 0),
                mode_view_color: Color::Rgb(0, 0, 0),
                mode_chat_color: Color::Rgb(0, 0, 0),
                mode_plan_color: Color::Rgb(0, 0, 0),
                mode_bash_color: Color::Rgb(0, 0, 0),
                mode_leader_color: Color::Rgb(0, 0, 0),
                warning_color: Color::Rgb(0, 0, 0),
                error_color: Color::Rgb(0, 0, 0),
                code_bg_color: Color::Rgb(0, 0, 0),
                cursor_color: Color::Rgb(0, 0, 0),
                selection_bg_color: Color::Rgb(0, 0, 0),
                selection_fg_color: Color::Rgb(0, 0, 0),
                user_msg_color: Color::Rgb(0, 0, 0),
                assistant_msg_color: Color::Rgb(0, 0, 0),
                system_msg_color: Color::Rgb(0, 0, 0),
                tool_msg_color: Color::Rgb(0, 0, 0),
                info_color: Color::Rgb(0, 0, 0),
                progress_color: Color::Rgb(0, 0, 0),
                input_bg_color: Color::Rgb(0, 0, 0),
                input_placeholder_color: Color::Rgb(0, 0, 0),
                input_border_color: Color::Rgb(0, 0, 0),
                user_msg_bg_color: Color::Rgb(0, 0, 0),
                assistant_msg_bg_color: Color::Rgb(0, 0, 0),
                system_msg_bg_color: Color::Rgb(0, 0, 0),
                tool_msg_bg_color: Color::Rgb(0, 0, 0),
                status_bar_bg_color: Color::Rgb(0, 0, 0),
                scrollbar_bg_color: Color::Rgb(0, 0, 0),
                scrollbar_fg_color: Color::Rgb(0, 0, 0),
                scrollbar_hover_color: Color::Rgb(0, 0, 0),
                logo_primary_color: Color::Rgb(0, 0, 0),
                logo_secondary_color: Color::Rgb(0, 0, 0),
                animation_color: Color::Rgb(0, 0, 0),
                processing_color: Color::Rgb(0, 0, 0),
                highlight_color: Color::Rgb(0, 0, 0),
                bubble_color: Color::Rgb(0, 0, 0),
                token_low_color: Color::Rgb(0, 0, 0),
                token_medium_color: Color::Rgb(0, 0, 0),
                token_high_color: Color::Rgb(0, 0, 0),
                token_critical_color: Color::Rgb(0, 0, 0),
                syntax_keyword_color: Color::Rgb(0, 0, 0),
                syntax_function_color: Color::Rgb(0, 0, 0),
                syntax_string_color: Color::Rgb(0, 0, 0),
                syntax_number_color: Color::Rgb(0, 0, 0),
                syntax_comment_color: Color::Rgb(0, 0, 0),
                syntax_type_color: Color::Rgb(0, 0, 0),
                syntax_variable_color: Color::Rgb(0, 0, 0),
                syntax_operator_color: Color::Rgb(0, 0, 0),
                syntax_punctuation_color: Color::Rgb(0, 0, 0),
                // Diff & code display colors
                diff_add_color: Color::Rgb(0, 0, 0),
                diff_add_bg_color: Color::Rgb(0, 0, 0),
                diff_remove_color: Color::Rgb(0, 0, 0),
                diff_remove_bg_color: Color::Rgb(0, 0, 0),
                diff_context_color: Color::Rgb(0, 0, 0),
                line_number_color: Color::Rgb(0, 0, 0),
                link_color: Color::Rgb(0, 0, 0),
                running_color: Color::Rgb(0, 0, 0),
            },
        }
    }

    /// Set core colors - these are required for every theme
    pub fn core_colors(
        mut self,
        bg: Color,
        border: Color,
        title: Color,
        accent: Color,
        text: Color,
        success: Color,
        dim: Color,
    ) -> Self {
        self.theme.bg_color = bg;
        self.theme.border_color = border;
        self.theme.title_color = title;
        self.theme.accent_color = accent;
        self.theme.text_color = text;
        self.theme.success_color = success;
        self.theme.dim_color = dim;
        self
    }

    /// Set mode colors (used as accent variants)
    pub fn mode_colors(
        mut self,
        view: Color,
        chat: Color,
        plan: Color,
        bash: Color,
        leader: Color,
    ) -> Self {
        self.theme.mode_view_color = view;
        self.theme.mode_chat_color = chat;
        self.theme.mode_plan_color = plan;
        self.theme.mode_bash_color = bash;
        self.theme.mode_leader_color = leader;
        self
    }

    /// Set special colors for warnings, errors, and code
    pub fn special_colors(mut self, warning: Color, error: Color, code_bg: Color) -> Self {
        self.theme.warning_color = warning;
        self.theme.error_color = error;
        self.theme.code_bg_color = code_bg;
        self
    }

    /// Set UI element colors
    pub fn ui_colors(mut self, cursor: Color, selection_bg: Color, selection_fg: Color) -> Self {
        self.theme.cursor_color = cursor;
        self.theme.selection_bg_color = selection_bg;
        self.theme.selection_fg_color = selection_fg;
        self
    }

    /// Set message role colors
    pub fn message_colors(
        mut self,
        user: Color,
        assistant: Color,
        system: Color,
        tool: Color,
    ) -> Self {
        self.theme.user_msg_color = user;
        self.theme.assistant_msg_color = assistant;
        self.theme.system_msg_color = system;
        self.theme.tool_msg_color = tool;
        self
    }

    /// Set status colors
    pub fn status_colors(mut self, info: Color, progress: Color) -> Self {
        self.theme.info_color = info;
        self.theme.progress_color = progress;
        self
    }

    /// Set all extended colors manually
    pub fn extended_colors(mut self, f: impl FnOnce(&mut Theme)) -> Self {
        f(&mut self.theme);
        self
    }

    /// Build the theme with intelligent defaults for any unset fields
    pub fn build(mut self) -> Theme {
        // Apply intelligent defaults for extended fields if not set
        if matches!(self.theme.input_bg_color, Color::Rgb(0, 0, 0)) {
            self.theme.input_bg_color = self.theme.code_bg_color;
        }
        if matches!(self.theme.input_placeholder_color, Color::Rgb(0, 0, 0)) {
            self.theme.input_placeholder_color = self.theme.dim_color;
        }
        if matches!(self.theme.input_border_color, Color::Rgb(0, 0, 0)) {
            self.theme.input_border_color = self.theme.border_color;
        }

        // Message backgrounds default to code_bg
        if matches!(self.theme.user_msg_bg_color, Color::Rgb(0, 0, 0)) {
            self.theme.user_msg_bg_color = self.theme.code_bg_color;
        }
        if matches!(self.theme.assistant_msg_bg_color, Color::Rgb(0, 0, 0)) {
            self.theme.assistant_msg_bg_color = self.theme.code_bg_color;
        }
        if matches!(self.theme.system_msg_bg_color, Color::Rgb(0, 0, 0)) {
            self.theme.system_msg_bg_color = self.theme.code_bg_color;
        }
        if matches!(self.theme.tool_msg_bg_color, Color::Rgb(0, 0, 0)) {
            self.theme.tool_msg_bg_color = self.theme.code_bg_color;
        }

        // Status bar and scrollbar
        if matches!(self.theme.status_bar_bg_color, Color::Rgb(0, 0, 0)) {
            self.theme.status_bar_bg_color = self.theme.code_bg_color;
        }
        if matches!(self.theme.scrollbar_bg_color, Color::Rgb(0, 0, 0)) {
            self.theme.scrollbar_bg_color = self.theme.code_bg_color;
        }
        if matches!(self.theme.scrollbar_fg_color, Color::Rgb(0, 0, 0)) {
            self.theme.scrollbar_fg_color = self.theme.border_color;
        }
        if matches!(self.theme.scrollbar_hover_color, Color::Rgb(0, 0, 0)) {
            self.theme.scrollbar_hover_color = self.theme.accent_color;
        }

        // Logo colors
        if matches!(self.theme.logo_primary_color, Color::Rgb(0, 0, 0)) {
            self.theme.logo_primary_color = self.theme.title_color;
        }
        if matches!(self.theme.logo_secondary_color, Color::Rgb(0, 0, 0)) {
            self.theme.logo_secondary_color = self.theme.accent_color;
        }

        // Animation colors
        if matches!(self.theme.animation_color, Color::Rgb(0, 0, 0)) {
            self.theme.animation_color = self.theme.title_color;
        }
        if matches!(self.theme.processing_color, Color::Rgb(0, 0, 0)) {
            self.theme.processing_color = self.theme.warning_color;
        }
        if matches!(self.theme.highlight_color, Color::Rgb(0, 0, 0)) {
            self.theme.highlight_color = self.theme.warning_color;
        }
        if matches!(self.theme.bubble_color, Color::Rgb(0, 0, 0)) {
            self.theme.bubble_color = self.theme.title_color;
        }

        // Token usage colors
        if matches!(self.theme.token_low_color, Color::Rgb(0, 0, 0)) {
            self.theme.token_low_color = self.theme.success_color;
        }
        if matches!(self.theme.token_medium_color, Color::Rgb(0, 0, 0)) {
            self.theme.token_medium_color = self.theme.warning_color;
        }
        if matches!(self.theme.token_high_color, Color::Rgb(0, 0, 0)) {
            self.theme.token_high_color = self.theme.mode_chat_color;
        }
        if matches!(self.theme.token_critical_color, Color::Rgb(0, 0, 0)) {
            self.theme.token_critical_color = self.theme.error_color;
        }

        // Syntax highlighting colors (with sensible defaults)
        if matches!(self.theme.syntax_keyword_color, Color::Rgb(0, 0, 0)) {
            self.theme.syntax_keyword_color = self.theme.mode_chat_color;
        }
        if matches!(self.theme.syntax_function_color, Color::Rgb(0, 0, 0)) {
            self.theme.syntax_function_color = self.theme.title_color;
        }
        if matches!(self.theme.syntax_string_color, Color::Rgb(0, 0, 0)) {
            self.theme.syntax_string_color = self.theme.success_color;
        }
        if matches!(self.theme.syntax_number_color, Color::Rgb(0, 0, 0)) {
            self.theme.syntax_number_color = self.theme.warning_color;
        }
        if matches!(self.theme.syntax_comment_color, Color::Rgb(0, 0, 0)) {
            self.theme.syntax_comment_color = self.theme.dim_color;
        }
        if matches!(self.theme.syntax_type_color, Color::Rgb(0, 0, 0)) {
            self.theme.syntax_type_color = self.theme.accent_color;
        }
        if matches!(self.theme.syntax_variable_color, Color::Rgb(0, 0, 0)) {
            self.theme.syntax_variable_color = self.theme.text_color;
        }
        if matches!(self.theme.syntax_operator_color, Color::Rgb(0, 0, 0)) {
            self.theme.syntax_operator_color = self.theme.text_color;
        }
        if matches!(self.theme.syntax_punctuation_color, Color::Rgb(0, 0, 0)) {
            self.theme.syntax_punctuation_color = self.theme.dim_color;
        }

        // Diff & code display colors (with sensible defaults)
        if matches!(self.theme.diff_add_color, Color::Rgb(0, 0, 0)) {
            self.theme.diff_add_color = self.theme.success_color;
        }
        if matches!(self.theme.diff_add_bg_color, Color::Rgb(0, 0, 0)) {
            // Derive a subtle background from success color
            if let Color::Rgb(r, g, b) = self.theme.success_color {
                self.theme.diff_add_bg_color = Color::Rgb(r / 6, g / 6, b / 6);
            } else {
                self.theme.diff_add_bg_color = Color::Rgb(20, 40, 20);
            }
        }
        if matches!(self.theme.diff_remove_color, Color::Rgb(0, 0, 0)) {
            self.theme.diff_remove_color = self.theme.error_color;
        }
        if matches!(self.theme.diff_remove_bg_color, Color::Rgb(0, 0, 0)) {
            // Derive a subtle background from error color
            if let Color::Rgb(r, g, b) = self.theme.error_color {
                self.theme.diff_remove_bg_color = Color::Rgb(r / 6, g / 6, b / 6);
            } else {
                self.theme.diff_remove_bg_color = Color::Rgb(40, 20, 20);
            }
        }
        if matches!(self.theme.diff_context_color, Color::Rgb(0, 0, 0)) {
            self.theme.diff_context_color = self.theme.dim_color;
        }
        if matches!(self.theme.line_number_color, Color::Rgb(0, 0, 0)) {
            self.theme.line_number_color = self.theme.dim_color;
        }
        if matches!(self.theme.link_color, Color::Rgb(0, 0, 0)) {
            self.theme.link_color = self.theme.accent_color;
        }
        if matches!(self.theme.running_color, Color::Rgb(0, 0, 0)) {
            self.theme.running_color = self.theme.accent_color;
        }

        self.theme
    }
}
