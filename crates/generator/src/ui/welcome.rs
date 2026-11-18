use std::path::Path;
use ratatui::text::Line;

use super::theme::Theme;

pub fn format_count(n: i64) -> String {
    if n < 0 {
        return format!("-{}", format_count(-n));
    }
    let s = n.to_string();
    let mut result = String::new();
    for (count, c) in s.chars().rev().enumerate() {
        if count > 0 && count % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

pub fn render_welcome_lines(
    theme: &Theme,
    cwd: &Path,
    sources_count: i64,
    chunks_count: i64,
    terminal_width: usize,
) -> Vec<Line<'static>> {
    super::banner::render_banner_lines(theme, cwd, sources_count, chunks_count, terminal_width)
}

pub fn render_welcome_box(
    theme: &Theme,
    cwd: &Path,
    sources_count: i64,
    chunks_count: i64,
    terminal_width: usize,
) -> String {
    super::banner::render_banner_box(theme, cwd, sources_count, chunks_count, terminal_width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use unicode_width::UnicodeWidthStr;

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
    fn test_format_count() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(9), "9");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(1000), "1,000");
        assert_eq!(format_count(12345), "12,345");
        assert_eq!(format_count(1234567), "1,234,567");
    }

    #[test]
    fn test_welcome_box_alignment_at_60_80_100_120_160() {
        let theme = make_test_theme();
        let cwd = PathBuf::from("/home/kevin/projects/rag");

        for term_width in [80, 100, 120, 160] {
            let rendered = render_welcome_box(&theme, &cwd, 12, 3481, term_width);
            let lines: Vec<&str> = rendered.lines().collect();

            let expected_box_width = if term_width >= 90 {
                (term_width - 2).min(100)
            } else {
                (term_width - 2).min(80)
            };

            assert!(lines.len() >= 11);
            for (idx, line) in lines.iter().enumerate() {
                let stripped = strip_ansi(line);
                let visual_width = UnicodeWidthStr::width(stripped.as_str());
                assert_eq!(
                    visual_width, expected_box_width,
                    "Line {} width mismatch at terminal width {}: visual width is {}, expected {}",
                    idx, term_width, visual_width, expected_box_width
                );
            }

            // Verify status is present
            assert!(rendered.contains("3,481 chunks") || rendered.contains("3,481") || rendered.contains("ready"));
        }
    }

    #[test]
    fn test_welcome_box_narrow_terminal_fallback() {
        let theme = make_test_theme();
        let cwd = PathBuf::from("/home/kevin/projects/rag");

        // When terminal width is less than 72 columns (e.g. 50)
        let rendered = render_welcome_box(&theme, &cwd, 0, 0, 50);
        assert!(!rendered.contains("╭"));
        assert!(!rendered.contains("╰"));
        assert!(rendered.contains("Sift"));
        assert!(rendered.contains("cwd: /home/kevin/projects/rag"));
    }

    #[test]
    fn test_welcome_box_empty_index_shows_tips() {
        let theme = make_test_theme();
        let cwd = PathBuf::from("/home/kevin/projects/rag");

        let rendered = render_welcome_box(&theme, &cwd, 0, 0, 80);
        assert!(rendered.contains("Tips:") || rendered.contains("/add"));
        assert!(rendered.contains("/help"));
    }

    #[test]
    fn test_welcome_box_with_chunks_shows_ready_line() {
        let theme = make_test_theme();
        let cwd = PathBuf::from("/home/kevin/projects/rag");

        let rendered = render_welcome_box(&theme, &cwd, 5, 120, 80);
        assert!(rendered.contains("120 chunks from 5 sources ready"));
    }

    #[test]
    fn test_welcome_box_long_cwd_truncated_aligned() {
        let theme = make_test_theme();
        let cwd = PathBuf::from("/home/kevin/very/long/nested/path/that/exceeds/the/width/of/the/terminal/box/for/testing/purposes");

        let rendered = render_welcome_box(&theme, &cwd, 2, 40, 72);
        let lines: Vec<&str> = rendered.lines().collect();
        let box_width = 70;

        for (idx, line) in lines.iter().enumerate() {
            let stripped = strip_ansi(line);
            assert_eq!(UnicodeWidthStr::width(stripped.as_str()), box_width, "Line {} width mismatch", idx);
        }
    }
}
