use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};

use super::theme::Theme;

/// Formats markdown-like text into Ratatui `Text` lines with proper styles.
pub struct MarkdownRenderer;

impl MarkdownRenderer {
    pub fn render(markdown: &str, theme: &Theme) -> Text<'static> {
        let mut lines = Vec::new();
        let mut in_code_block = false;
        let mut _code_lang = String::new();

        for raw_line in markdown.lines() {
            let trimmed = raw_line.trim_start();

            // Handle fenced code blocks
            if trimmed.starts_with("```") {
                if in_code_block {
                    in_code_block = false;
                    _code_lang.clear();
                    lines.push(Line::from(vec![
                        Span::styled("  └", Style::default().fg(Color::DarkGray)),
                        Span::styled("────────────────────────────────", Style::default().fg(Color::DarkGray)),
                    ]));
                } else {
                    in_code_block = true;
                    _code_lang = trimmed.trim_start_matches('`').trim().to_string();
                    let header = if _code_lang.is_empty() {
                        "─── code ───────────────────────".to_string()
                    } else {
                        format!("─── {} ───────────────────────", _code_lang)
                    };
                    lines.push(Line::from(vec![
                        Span::styled("  ┌─ ", Style::default().fg(Color::DarkGray)),
                        Span::styled(header, Style::default().fg(Color::DarkGray)),
                    ]));
                }
                continue;
            }

            if in_code_block {
                lines.push(Line::from(vec![
                    Span::styled("  │ ", Style::default().fg(Color::DarkGray)),
                    Span::styled(raw_line.to_string(), Style::default().fg(Color::Cyan)),
                ]));
                continue;
            }

            // Headers
            if let Some(rest) = trimmed.strip_prefix("### ") {
                lines.push(Line::from(vec![
                    Span::styled(rest.to_string(), theme.style_accent().add_modifier(Modifier::BOLD)),
                ]));
                continue;
            } else if let Some(rest) = trimmed.strip_prefix("## ") {
                lines.push(Line::from(vec![
                    Span::styled(rest.to_string(), theme.style_accent().add_modifier(Modifier::BOLD)),
                ]));
                continue;
            } else if let Some(rest) = trimmed.strip_prefix("# ") {
                lines.push(Line::from(vec![
                    Span::styled(rest.to_string(), theme.style_accent().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
                ]));
                continue;
            }

            // Unordered list items: "- " or "* "
            if let Some(rest) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
                let indent_len = raw_line.len() - trimmed.len();
                let indent = " ".repeat(indent_len + 2);
                let mut spans = vec![
                    Span::raw(indent),
                    Span::styled(format!("{} ", theme.symbols.bullet), theme.style_muted()),
                ];
                spans.extend(Self::parse_inline_spans(rest, theme));
                lines.push(Line::from(spans));
                continue;
            }

            // Numbered list items: "1. ", "2. ", etc.
            if let Some((num_str, rest)) = Self::parse_ordered_list(trimmed) {
                let indent_len = raw_line.len() - trimmed.len();
                let indent = " ".repeat(indent_len + 2);
                let mut spans = vec![
                    Span::raw(indent),
                    Span::styled(format!("{num_str}. "), theme.style_accent().add_modifier(Modifier::BOLD)),
                ];
                spans.extend(Self::parse_inline_spans(rest, theme));
                lines.push(Line::from(spans));
                continue;
            }

            // Regular paragraph line with inline styles
            let spans = Self::parse_inline_spans(raw_line, theme);
            lines.push(Line::from(spans));
        }

        Text::from(lines)
    }

    fn parse_ordered_list(line: &str) -> Option<(&str, &str)> {
        let (num_part, rest) = line.split_once(". ")?;
        if !num_part.is_empty() && num_part.chars().all(|c| c.is_ascii_digit()) {
            Some((num_part, rest))
        } else {
            None
        }
    }

    /// Parses inline bold `**text**`, inline code `` `text` ``, and citation references `[1]`.
    pub fn parse_inline_spans(text: &str, _theme: &Theme) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        let mut chars = text.chars().peekable();
        let mut current_buf = String::new();

        while let Some(c) = chars.next() {
            if c == '`' {
                // Inline code: collect until next '`'
                if !current_buf.is_empty() {
                    spans.push(Span::raw(std::mem::take(&mut current_buf)));
                }
                let mut code_buf = String::new();
                while let Some(&next_c) = chars.peek() {
                    chars.next();
                    if next_c == '`' {
                        break;
                    }
                    code_buf.push(next_c);
                }
                spans.push(Span::styled(
                    code_buf,
                    Style::default().fg(Color::Cyan),
                ));
            } else if c == '*' && chars.peek() == Some(&'*') {
                // Bold: **text**
                chars.next(); // consume second '*'
                if !current_buf.is_empty() {
                    spans.push(Span::raw(std::mem::take(&mut current_buf)));
                }
                let mut bold_buf = String::new();
                while let Some(next_c) = chars.next() {
                    if next_c == '*' && chars.peek() == Some(&'*') {
                        chars.next(); // consume second '*'
                        break;
                    }
                    bold_buf.push(next_c);
                }
                spans.push(Span::styled(
                    bold_buf,
                    Style::default().add_modifier(Modifier::BOLD),
                ));
            } else if c == '[' {
                // Potential citation [1] or link
                let mut bracket_buf = String::new();
                let mut is_citation = false;
                while let Some(&next_c) = chars.peek() {
                    if next_c == ']' {
                        chars.next();
                        is_citation = !bracket_buf.is_empty() && bracket_buf.chars().all(|ch| ch.is_ascii_digit());
                        break;
                    } else if next_c == '\n' || bracket_buf.len() > 10 {
                        break;
                    } else {
                        bracket_buf.push(chars.next().unwrap());
                    }
                }

                if is_citation {
                    if !current_buf.is_empty() {
                        spans.push(Span::raw(std::mem::take(&mut current_buf)));
                    }
                    spans.push(Span::styled(
                        format!("[{bracket_buf}]"),
                        Style::default().fg(Color::Cyan).add_modifier(Modifier::DIM),
                    ));
                } else {
                    current_buf.push('[');
                    current_buf.push_str(&bracket_buf);
                }
            } else {
                current_buf.push(c);
            }
        }

        if !current_buf.is_empty() {
            spans.push(Span::raw(current_buf));
        }

        spans
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_inline_code_and_bold() {
        let theme = Theme::detect();
        let spans = MarkdownRenderer::parse_inline_spans("Here is `code` and **bold text** with [1] citation", &theme);

        assert_eq!(spans.len(), 7);
        assert_eq!(spans[0].content, "Here is ");
        assert_eq!(spans[1].content, "code");
        assert_eq!(spans[2].content, " and ");
        assert_eq!(spans[3].content, "bold text");
        assert_eq!(spans[4].content, " with ");
        assert_eq!(spans[5].content, "[1]");
        assert_eq!(spans[6].content, " citation");
    }

    #[test]
    fn test_markdown_code_block() {
        let theme = Theme::detect();
        let md = "```rust\nlet a = 1;\n```";
        let text = MarkdownRenderer::render(md, &theme);
        assert_eq!(text.lines.len(), 3);
    }

    #[test]
    fn test_markdown_lists() {
        let theme = Theme::detect();
        let md = "- item 1\n- item 2\n1. first\n2. second";
        let text = MarkdownRenderer::render(md, &theme);
        assert_eq!(text.lines.len(), 4);
    }
}
