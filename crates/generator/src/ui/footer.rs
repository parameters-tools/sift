use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use super::theme::Theme;
use super::welcome::format_count;

#[derive(Debug, Clone)]
pub struct FooterState {
    pub is_busy: bool,
    pub ctrl_c_primed: bool,
    pub shortcuts_open: bool,
    pub sources_count: i64,
    pub chunks_count: i64,
    pub api_reachable: bool,
}

impl FooterState {
    pub fn render_ratatui_line(&self, width: usize, theme: &Theme) -> Line<'static> {
        if self.shortcuts_open {
            let shortcuts = "Shortcuts: Enter Submit · Shift/Alt+Enter Newline · Up/Down History · / Commands · Ctrl+C Exit";
            return Line::from(vec![
                Span::styled(format!("  {shortcuts}"), theme.style_muted()),
            ]);
        }

        // Left hint
        let left_raw = if self.ctrl_c_primed {
            "Press Ctrl-C again to exit".to_string()
        } else if self.is_busy {
            "esc to interrupt".to_string()
        } else {
            "? for shortcuts".to_string()
        };

        let left_span = if self.ctrl_c_primed {
            Span::styled(format!("  {left_raw}"), theme.style_warning())
        } else {
            Span::styled(format!("  {left_raw}"), theme.style_muted())
        };

        // Right status
        let (right_raw, right_style) = if !self.api_reachable {
            ("API unreachable".to_string(), theme.style_error())
        } else if self.chunks_count == 0 {
            ("index empty".to_string(), theme.style_warning())
        } else {
            let s = format_count(self.sources_count);
            let c = format_count(self.chunks_count);
            (format!("{s} sources · {c} chunks"), theme.style_muted())
        };

        let left_w = UnicodeWidthStr::width(left_raw.as_str()) + 2;
        let right_w = UnicodeWidthStr::width(right_raw.as_str());

        if left_w + right_w >= width {
            return Line::from(vec![left_span]);
        }

        let padding_spaces = width.saturating_sub(left_w + right_w);
        let padding = " ".repeat(padding_spaces);

        Line::from(vec![
            left_span,
            Span::raw(padding),
            Span::styled(right_raw, right_style),
        ])
    }

    pub fn render_line(&self, width: usize, theme: &Theme) -> String {
        if self.shortcuts_open {
            let shortcuts = "Shortcuts: Enter Submit · Shift/Alt+Enter Newline · Up/Down History · / Commands · Ctrl+C Exit";
            return theme.paint_muted(&format!("  {shortcuts}"));
        }

        // Left hint
        let left_raw = if self.ctrl_c_primed {
            "Press Ctrl-C again to exit".to_string()
        } else if self.is_busy {
            "esc to interrupt".to_string()
        } else {
            "? for shortcuts".to_string()
        };

        let left_formatted = if self.ctrl_c_primed {
            theme.paint_warning(&format!("  {left_raw}"))
        } else {
            theme.paint_muted(&format!("  {left_raw}"))
        };

        // Right status
        let (right_raw, right_formatted) = if !self.api_reachable {
            (
                "API unreachable".to_string(),
                theme.paint_error("API unreachable"),
            )
        } else if self.chunks_count == 0 {
            (
                "index empty".to_string(),
                theme.paint_warning("index empty"),
            )
        } else {
            let s = format_count(self.sources_count);
            let c = format_count(self.chunks_count);
            let raw = format!("{s} sources · {c} chunks");
            let formatted = theme.paint_muted(&raw);
            (raw, formatted)
        };

        let left_w = UnicodeWidthStr::width(left_raw.as_str()) + 2;
        let right_w = UnicodeWidthStr::width(right_raw.as_str());

        if left_w + right_w >= width {
            return left_formatted;
        }

        let padding_spaces = width.saturating_sub(left_w + right_w);
        let padding = " ".repeat(padding_spaces);

        format!("{left_formatted}{padding}{right_formatted}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_footer_empty_index_warning() {
        let theme = Theme::detect();
        let footer = FooterState {
            is_busy: false,
            ctrl_c_primed: false,
            shortcuts_open: false,
            sources_count: 0,
            chunks_count: 0,
            api_reachable: true,
        };

        let rendered = footer.render_line(80, &theme);
        assert!(rendered.contains("? for shortcuts"));
        assert!(rendered.contains("index empty"));
    }

    #[test]
    fn test_footer_busy_esc_hint() {
        let theme = Theme::detect();
        let footer = FooterState {
            is_busy: true,
            ctrl_c_primed: false,
            shortcuts_open: false,
            sources_count: 5,
            chunks_count: 200,
            api_reachable: true,
        };

        let rendered = footer.render_line(80, &theme);
        assert!(rendered.contains("esc to interrupt"));
        assert!(rendered.contains("5 sources · 200 chunks"));
    }

    #[test]
    fn test_footer_shortcuts_open() {
        let theme = Theme::detect();
        let footer = FooterState {
            is_busy: false,
            ctrl_c_primed: false,
            shortcuts_open: true,
            sources_count: 5,
            chunks_count: 200,
            api_reachable: true,
        };

        let rendered = footer.render_line(80, &theme);
        assert!(rendered.contains("Shortcuts:"));
    }

    #[test]
    fn test_footer_right_aligns_to_box_edge() {
        let theme = Theme::detect();
        let footer = FooterState {
            is_busy: false,
            ctrl_c_primed: false,
            shortcuts_open: false,
            sources_count: 3,
            chunks_count: 76,
            api_reachable: true,
        };

        for width in [58, 78, 98, 118, 158] {
            let line = footer.render_ratatui_line(width, &theme);
            let display_w = line.width();
            assert_eq!(display_w, width);
            let last_span = line.spans.last().unwrap();
            assert_eq!(last_span.content, "3 sources · 76 chunks");
        }
    }
}
