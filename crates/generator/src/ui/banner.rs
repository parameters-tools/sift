use std::path::Path;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use super::theme::Theme;
use super::welcome::format_count;

pub const LOGO_WIDTH: usize = 28;

pub const LOGO: [&str; 6] = [
    "███████╗██╗███████╗████████╗",
    "██╔════╝██║██╔════╝╚══██╔══╝",
    "███████╗██║█████╗     ██║   ",
    "╚════██║██║██╔══╝     ██║   ",
    "███████║██║██║        ██║   ",
    "╚══════╝╚═╝╚═╝        ╚═╝   ",
];

pub fn compute_banner_box_width(terminal_width: usize) -> usize {
    (terminal_width.saturating_sub(2)).min(80)
}

pub fn use_compact_fallback(terminal_width: usize, theme: &Theme) -> bool {
    terminal_width < 60
        || std::env::var_os("NO_COLOR").is_some()
        || !theme.use_color
        || !theme.use_unicode
        || !theme.is_tty
}

pub fn render_banner_lines(
    theme: &Theme,
    cwd: &Path,
    sources_count: i64,
    chunks_count: i64,
    terminal_width: usize,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let cwd_str = cwd.to_string_lossy();

    if use_compact_fallback(terminal_width, theme) {
        let sparkle_str = if theme.use_unicode { "✻" } else { "*" };
        let sparkle = Span::styled(format!("{sparkle_str} "), theme.style_accent());
        let title = Span::styled("Sift", theme.style_bold());
        let sep = Span::styled("  ·  ", theme.style_muted());
        let subtitle = Span::raw("Ask your documents. Get cited answers.");
        lines.push(Line::from(vec![sparkle, title, sep, subtitle]));

        lines.push(Line::from(vec![
            Span::styled("cwd: ", theme.style_muted()),
            Span::raw(cwd_str.to_string()),
        ]));
    } else {
        let box_width = compute_banner_box_width(terminal_width);
        let inner_width = box_width.saturating_sub(2);

        // Top border
        let h = theme.borders.horizontal.to_string().repeat(box_width.saturating_sub(2));
        let top = format!("{}{}{}", theme.borders.top_left, h, theme.borders.top_right);
        lines.push(Line::from(vec![Span::styled(top, theme.style_accent())]));

        // Blank line inside box
        lines.push(make_banner_box_line(Vec::new(), 0, inner_width, theme));

        // 6 Logo lines
        for (i, logo_row) in LOGO.iter().enumerate() {
            let mut spans = Vec::new();
            spans.push(Span::raw("   "));
            spans.push(Span::styled(*logo_row, theme.style_accent()));

            let mut content_w = 3 + LOGO_WIDTH;

            let (text, style) = match i {
                1 => ("Ask your documents.", theme.style_bold()),
                2 => ("Get cited answers.", theme.style_bold()),
                4 => ("/help for help, /status for setup", theme.style_muted()),
                _ => ("", theme.style_muted()),
            };

            if !text.is_empty() && inner_width > content_w + 4 {
                let avail = inner_width.saturating_sub(content_w + 4);
                let (display_text, actual_w) = if UnicodeWidthStr::width(text) > avail {
                    truncate_to_width(text, avail)
                } else {
                    (text.to_string(), UnicodeWidthStr::width(text))
                };
                spans.push(Span::raw("    "));
                spans.push(Span::styled(display_text, style));
                content_w += 4 + actual_w;
            }

            lines.push(make_banner_box_line(spans, content_w, inner_width, theme));
        }

        // Blank line inside box
        lines.push(make_banner_box_line(Vec::new(), 0, inner_width, theme));

        // cwd line: "cwd: <path>" in dim text
        let cwd_prefix = "   cwd: ";
        let cwd_prefix_w = UnicodeWidthStr::width(cwd_prefix);
        let max_path_w = inner_width.saturating_sub(cwd_prefix_w);
        let (display_path, path_w) = if UnicodeWidthStr::width(cwd_str.as_ref()) > max_path_w {
            truncate_to_width(&cwd_str, max_path_w)
        } else {
            (cwd_str.to_string(), UnicodeWidthStr::width(cwd_str.as_ref()))
        };
        let cwd_w = cwd_prefix_w + path_w;
        let cwd_spans = vec![
            Span::styled(cwd_prefix, theme.style_muted()),
            Span::styled(display_path, theme.style_muted()),
        ];
        lines.push(make_banner_box_line(cwd_spans, cwd_w, inner_width, theme));

        // Bottom border
        let bottom = format!("{}{}{}", theme.borders.bottom_left, h, theme.borders.bottom_right);
        lines.push(Line::from(vec![Span::styled(bottom, theme.style_accent())]));
    }

    lines.push(Line::default());

    // Below the box: dim text (unchanged)
    if chunks_count > 0 {
        let chunks_str = format_count(chunks_count);
        let sources_str = format_count(sources_count);
        let chunk_word = if chunks_count == 1 { "chunk" } else { "chunks" };
        let source_word = if sources_count == 1 { "source" } else { "sources" };
        let msg = format!(" {chunks_str} {chunk_word} from {sources_str} {source_word} ready");
        lines.push(Line::from(vec![Span::styled(msg, theme.style_muted())]));
        lines.push(Line::default());
    } else {
        lines.push(Line::from(vec![Span::styled(" Tips for getting started:", theme.style_muted())]));
        lines.push(Line::default());
        lines.push(Line::from(vec![Span::styled(" 1. /add <path|url> to index your documents", theme.style_muted())]));
        lines.push(Line::from(vec![Span::styled(" 2. Ask a question directly once sources are indexed", theme.style_muted())]));
        lines.push(Line::from(vec![Span::styled(" 3. /model to switch chat or embedding models", theme.style_muted())]));
        lines.push(Line::default());
    }

    lines
}

fn make_banner_box_line(
    mut inner_spans: Vec<Span<'static>>,
    content_width: usize,
    inner_width: usize,
    theme: &Theme,
) -> Line<'static> {
    let padding = inner_width.saturating_sub(content_width);
    let mut spans = Vec::with_capacity(inner_spans.len() + 3);
    spans.push(Span::styled(theme.borders.vertical.to_string(), theme.style_accent()));
    spans.append(&mut inner_spans);
    if padding > 0 {
        spans.push(Span::raw(" ".repeat(padding)));
    }
    spans.push(Span::styled(theme.borders.vertical.to_string(), theme.style_accent()));
    Line::from(spans)
}

pub fn render_banner_box(
    theme: &Theme,
    cwd: &Path,
    sources_count: i64,
    chunks_count: i64,
    terminal_width: usize,
) -> String {
    let mut out = Vec::new();
    let cwd_str = cwd.to_string_lossy();

    if use_compact_fallback(terminal_width, theme) {
        let sparkle = theme.paint_accent(theme.symbols.sparkle);
        let title = theme.paint_bold("Sift");
        let sep = theme.paint_muted("·");
        out.push(format!("{sparkle} {title}  {sep}  Ask your documents. Get cited answers."));
        out.push(format!("{} {cwd_str}", theme.paint_muted("cwd:")));
    } else {
        let box_width = compute_banner_box_width(terminal_width);
        let inner_width = box_width.saturating_sub(2);

        // Top border
        let h = theme.borders.horizontal.to_string().repeat(box_width.saturating_sub(2));
        let top = format!("{}{}{}", theme.borders.top_left, h, theme.borders.top_right);
        out.push(theme.paint_accent(&top));

        // Blank line inside box
        out.push(format_banner_box_line("", "", inner_width, theme));

        // 6 Logo lines
        for (i, logo_row) in LOGO.iter().enumerate() {
            let logo_formatted = theme.paint_accent(logo_row);
            let raw_content_w = 3 + LOGO_WIDTH;

            let (raw_text, _formatted_text) = match i {
                1 => ("Ask your documents.", theme.paint_bold("Ask your documents.")),
                2 => ("Get cited answers.", theme.paint_bold("Get cited answers.")),
                4 => ("/help for help, /status for setup", theme.paint_muted("/help for help, /status for setup")),
                _ => ("", String::new()),
            };

            let (raw_right, formatted_right) = if !raw_text.is_empty() && inner_width > raw_content_w + 4 {
                let avail = inner_width.saturating_sub(raw_content_w + 4);
                let (display_text, _) = if UnicodeWidthStr::width(raw_text) > avail {
                    truncate_to_width(raw_text, avail)
                } else {
                    (raw_text.to_string(), UnicodeWidthStr::width(raw_text))
                };
                let formatted = match i {
                    1 => theme.paint_bold(&display_text),
                    2 => theme.paint_bold(&display_text),
                    _ => theme.paint_muted(&display_text),
                };
                (format!("    {display_text}"), format!("    {formatted}"))
            } else {
                (String::new(), String::new())
            };

            let raw = format!("   {}{raw_right}", LOGO[i]);
            let formatted = format!("   {logo_formatted}{formatted_right}");
            out.push(format_banner_box_line(&raw, &formatted, inner_width, theme));
        }

        // Blank line inside box
        out.push(format_banner_box_line("", "", inner_width, theme));

        // cwd line
        let cwd_prefix = "   cwd: ";
        let cwd_prefix_w = UnicodeWidthStr::width(cwd_prefix);
        let max_path_w = inner_width.saturating_sub(cwd_prefix_w);
        let (display_path, _) = if UnicodeWidthStr::width(cwd_str.as_ref()) > max_path_w {
            truncate_to_width(&cwd_str, max_path_w)
        } else {
            (cwd_str.to_string(), UnicodeWidthStr::width(cwd_str.as_ref()))
        };
        let cwd_raw = format!("{cwd_prefix}{display_path}");
        let cwd_formatted = format!("{}{display_path}", theme.paint_muted(cwd_prefix));
        out.push(format_banner_box_line(&cwd_raw, &cwd_formatted, inner_width, theme));

        // Bottom border
        let bottom = format!("{}{}{}", theme.borders.bottom_left, h, theme.borders.bottom_right);
        out.push(theme.paint_accent(&bottom));
    }

    out.push(String::new());

    // Below the box: dim text
    if chunks_count > 0 {
        let chunks_str = format_count(chunks_count);
        let sources_str = format_count(sources_count);
        let chunk_word = if chunks_count == 1 { "chunk" } else { "chunks" };
        let source_word = if sources_count == 1 { "source" } else { "sources" };
        let msg = format!(" {chunks_str} {chunk_word} from {sources_str} {source_word} ready");
        out.push(theme.paint_muted(&msg));
        out.push(String::new());
    } else {
        out.push(theme.paint_muted(" Tips for getting started:"));
        out.push(String::new());
        out.push(theme.paint_muted(" 1. /add <path|url> to index your documents"));
        out.push(theme.paint_muted(" 2. Ask a question directly once sources are indexed"));
        out.push(theme.paint_muted(" 3. /model to switch chat or embedding models"));
        out.push(String::new());
    }

    out.join("\n")
}

fn format_banner_box_line(
    content_raw: &str,
    content_formatted: &str,
    inner_width: usize,
    theme: &Theme,
) -> String {
    let raw_width = UnicodeWidthStr::width(content_raw);
    let padding = inner_width.saturating_sub(raw_width);
    let spaces = " ".repeat(padding);

    let left = theme.paint_accent(&theme.borders.vertical.to_string());
    let right = theme.paint_accent(&theme.borders.vertical.to_string());

    format!("{left}{content_formatted}{spaces}{right}")
}

fn truncate_to_width(s: &str, max_width: usize) -> (String, usize) {
    if max_width == 0 {
        return (String::new(), 0);
    }
    let ellipsis = "…";
    let ellipsis_width = UnicodeWidthStr::width(ellipsis);
    let target = max_width.saturating_sub(ellipsis_width);

    let mut width = 0;
    let mut out = String::new();

    for c in s.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if width + cw > target {
            out.push_str(ellipsis);
            width += ellipsis_width;
            return (out, width);
        }
        out.push(c);
        width += cw;
    }

    (out, width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut in_escape = false;
        for c in s.chars() {
            if c == '\x1b' {
                in_escape = true;
            } else if in_escape {
                if c == 'm' {
                    in_escape = false;
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    fn make_test_theme() -> Theme {
        let mut theme = Theme::detect();
        theme.use_color = true;
        theme.use_unicode = true;
        theme.is_tty = true;
        theme.borders = crate::ui::theme::BorderChars::UNICODE_ROUNDED;
        theme.symbols = crate::ui::theme::Symbols::UNICODE;
        theme
    }

    #[test]
    fn test_logo_rows_are_all_the_same_width() {
        assert_eq!(LOGO.len(), 6);
        for (i, row) in LOGO.iter().enumerate() {
            let w = UnicodeWidthStr::width(*row);
            assert_eq!(
                w, LOGO_WIDTH,
                "Logo row {i} has width {w}, expected {LOGO_WIDTH}"
            );
        }
    }

    #[test]
    fn test_every_banner_line_has_equal_display_width_at_60_80_100_120_160() {
        let theme = make_test_theme();
        let cwd = PathBuf::from("/home/kevin/advanced-rag");

        for term_width in [60, 80, 100, 120, 160] {
            let expected_width = compute_banner_box_width(term_width);
            let expected_calc = (term_width - 2).min(80);
            assert_eq!(expected_width, expected_calc);

            // 1. Test Ratatui Lines
            let lines = render_banner_lines(&theme, &cwd, 0, 0, term_width);
            assert!(lines.len() >= 11);
            let box_lines = &lines[0..11];
            for (idx, line) in box_lines.iter().enumerate() {
                let display_w = line.width();
                assert_eq!(
                    display_w, expected_width,
                    "Line {idx} at terminal width {term_width} has display width {display_w}, expected {expected_width}"
                );
            }

            // 2. Test rendered String box
            let rendered_str = render_banner_box(&theme, &cwd, 0, 0, term_width);
            let str_lines: Vec<&str> = rendered_str.lines().collect();
            assert!(str_lines.len() >= 11);
            for (idx, line) in str_lines[0..11].iter().enumerate() {
                let plain = strip_ansi(line);
                let display_w = UnicodeWidthStr::width(plain.as_str());
                assert_eq!(
                    display_w, expected_width,
                    "String line {idx} at terminal width {term_width} has display width {display_w}, expected {expected_width}"
                );
            }
        }
    }

    #[test]
    fn test_compact_fallback_used_below_60_and_no_color() {
        let theme = make_test_theme();
        let cwd = PathBuf::from("/home/kevin/advanced-rag");

        // 1. Terminal width below 60 (e.g. 59, 50)
        for w in [59, 50] {
            let lines = render_banner_lines(&theme, &cwd, 0, 0, w);
            let first_line = lines[0].spans.iter().map(|s| s.content.as_ref()).collect::<String>();
            assert!(
                first_line.contains("✻ Sift  ·  Ask your documents. Get cited answers.")
                    || first_line.contains("* Sift  ·  Ask your documents. Get cited answers."),
                "Width {w} should use compact fallback, got: '{first_line}'"
            );
            let second_line = lines[1].spans.iter().map(|s| s.content.as_ref()).collect::<String>();
            assert!(second_line.starts_with("cwd: /home/kevin/advanced-rag"));
            // Should not contain the big logo
            assert!(!first_line.contains("███████╗"));
        }

        // 2. NO_COLOR is set / no-color theme at width >= 60
        let no_color_theme = Theme::no_color();
        let lines_nc = render_banner_lines(&no_color_theme, &cwd, 0, 0, 80);
        let nc_first = lines_nc[0].spans.iter().map(|s| s.content.as_ref()).collect::<String>();
        assert!(
            nc_first.contains("✻ Sift  ·  Ask your documents. Get cited answers.")
                || nc_first.contains("* Sift  ·  Ask your documents. Get cited answers."),
            "NO_COLOR theme should use compact fallback, got: '{nc_first}'"
        );
        let nc_second = lines_nc[1].spans.iter().map(|s| s.content.as_ref()).collect::<String>();
        assert!(nc_second.starts_with("cwd: /home/kevin/advanced-rag"));
    }
}
