use std::path::Path;
use ratatui::style::{Color, Style};
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

pub fn compute_banner_box_width_two_panel(terminal_width: usize) -> usize {
    (terminal_width.saturating_sub(2)).min(100)
}

pub fn use_compact_header(terminal_width: usize, theme: &Theme) -> bool {
    terminal_width < 72
        || std::env::var_os("NO_COLOR").is_some()
        || !theme.use_color
        || !theme.use_unicode
        || !theme.is_tty
}

pub fn use_compact_fallback(terminal_width: usize, theme: &Theme) -> bool {
    use_compact_header(terminal_width, theme)
}

pub fn use_two_panel(terminal_width: usize, terminal_height: usize, theme: &Theme) -> bool {
    terminal_width >= 90 && terminal_height >= 30 && !use_compact_header(terminal_width, theme)
}

pub fn is_truecolor() -> bool {
    if let Ok(colorterm) = std::env::var("COLORTERM") {
        let ct = colorterm.trim().to_lowercase();
        ct == "truecolor" || ct == "24bit"
    } else {
        false
    }
}

pub fn get_logo_row_style(row_idx: usize, theme: &Theme) -> Style {
    if !theme.use_color {
        return Style::default();
    }
    if is_truecolor() {
        // Accent: #D97757 (217, 119, 87)
        // ~25% dimmer shade (75% of original): (163, 89, 65)
        let t = (row_idx.min(5) as f32) / 5.0;
        let r = (217.0 * (1.0 - t) + 163.0 * t).round() as u8;
        let g = (119.0 * (1.0 - t) + 89.0 * t).round() as u8;
        let b = (87.0 * (1.0 - t) + 65.0 * t).round() as u8;
        Style::default().fg(Color::Rgb(r, g, b))
    } else {
        theme.style_accent()
    }
}

pub fn truncate_to_width(s: &str, max_width: usize) -> (String, usize) {
    if max_width == 0 {
        return (String::new(), 0);
    }
    if UnicodeWidthStr::width(s) <= max_width {
        return (s.to_string(), UnicodeWidthStr::width(s));
    }
    let mut current_width = 0;
    let mut out = String::new();
    let ellipsis = "…";
    let ellipsis_w = 1;
    let target = max_width.saturating_sub(ellipsis_w);

    for c in s.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if current_width + cw > target {
            break;
        }
        out.push(c);
        current_width += cw;
    }
    out.push_str(ellipsis);
    (out, current_width + ellipsis_w)
}

pub fn render_banner_lines(
    theme: &Theme,
    cwd: &Path,
    sources_count: i64,
    chunks_count: i64,
    terminal_width: usize,
) -> Vec<Line<'static>> {
    render_banner_lines_full(
        theme,
        cwd,
        sources_count,
        chunks_count,
        "deepseekv4.1-flash-1017",
        true,
        &[],
        terminal_width,
        30,
    )
}

pub fn render_banner_box(
    theme: &Theme,
    cwd: &Path,
    sources_count: i64,
    chunks_count: i64,
    terminal_width: usize,
) -> String {
    let lines = render_banner_lines(theme, cwd, sources_count, chunks_count, terminal_width);
    lines
        .into_iter()
        .map(|line| {
            line.spans
                .into_iter()
                .map(|span| span.content.into_owned())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[allow(clippy::too_many_arguments)]
pub fn render_banner_lines_full(
    theme: &Theme,
    cwd: &Path,
    sources_count: i64,
    chunks_count: i64,
    chat_model: &str,
    api_ok: bool,
    top_sources: &[(String, usize)],
    terminal_width: usize,
    terminal_height: usize,
) -> Vec<Line<'static>> {
    let cwd_str = cwd.to_string_lossy();

    // 1. Below 72 cols, NO_COLOR, non-UTF-8, or not a TTY -> one-line header + cwd line
    if use_compact_header(terminal_width, theme) {
        let sparkle_str = if theme.use_unicode { "✻" } else { "*" };
        let sparkle = Span::styled(format!("{sparkle_str} "), theme.style_accent());
        let sep = if theme.use_unicode { " · " } else { " - " };
        let chunks_fmt = format_count(chunks_count);
        let sources_fmt = format_count(sources_count);
        let header_text = format!("Sift v0.1.0{sep}{chunks_fmt} chunks{sep}{sources_fmt} sources");
        let header = Line::from(vec![sparkle, Span::styled(header_text, theme.style_bold())]);

        let cwd_line = Line::from(vec![
            Span::styled("cwd: ", theme.style_muted()),
            Span::raw(cwd_str.to_string()),
        ]);

        return vec![header, cwd_line];
    }

    // 2. Two-panel banner (width >= 90 columns and height >= 30 rows)
    if use_two_panel(terminal_width, terminal_height, theme) {
        return render_two_panel_banner(
            theme,
            &cwd_str,
            sources_count,
            chunks_count,
            chat_model,
            api_ok,
            top_sources,
            terminal_width,
        );
    }

    // 3. Fallback single-panel banner (72-89 columns, or height < 30 rows)
    render_single_panel_banner(
        theme,
        &cwd_str,
        sources_count,
        chunks_count,
        terminal_width,
    )
}

fn pad_spans_to_width(mut spans: Vec<Span<'static>>, content_w: usize, target_w: usize) -> Vec<Span<'static>> {
    if content_w < target_w {
        spans.push(Span::raw(" ".repeat(target_w - content_w)));
    }
    spans
}

#[allow(clippy::too_many_arguments)]
fn render_two_panel_banner(
    theme: &Theme,
    cwd_str: &str,
    sources_count: i64,
    chunks_count: i64,
    chat_model: &str,
    api_ok: bool,
    top_sources: &[(String, usize)],
    terminal_width: usize,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let box_width = compute_banner_box_width_two_panel(terminal_width);
    let inner_total = box_width.saturating_sub(3);
    let left_width = inner_total / 2;
    let right_width = inner_total - left_width;

    let h = theme.borders.horizontal;
    let top_div = if theme.use_unicode { '┬' } else { '+' };
    let bot_div = if theme.use_unicode { '┴' } else { '+' };

    // --- Top border: "╭─ Sift v0.1.0 ───...─┬─...─╮" ---
    let title = "Sift v0.1.0";
    let title_prefix = format!("{}{h} ", theme.borders.top_left);
    let prefix_w = UnicodeWidthStr::width(title_prefix.as_str());
    let title_w = UnicodeWidthStr::width(title);
    let title_mid = " ";
    let title_mid_w = 1;
    let used_left = prefix_w + title_w + title_mid_w;
    let left_dashes = (left_width + 1).saturating_sub(used_left);
    let left_part_dash = h.to_string().repeat(left_dashes);
    let right_part_dash = h.to_string().repeat(right_width);

    let top_line = Line::from(vec![
        Span::styled(title_prefix, theme.style_accent()),
        Span::styled(title, theme.style_bold()),
        Span::styled(
            format!("{title_mid}{left_part_dash}{top_div}{right_part_dash}{}", theme.borders.top_right),
            theme.style_accent(),
        ),
    ]);
    lines.push(top_line);

    // --- Left Panel Rows (11 rows) ---
    let mut left_rows: Vec<(Vec<Span<'static>>, usize)> = Vec::new();

    // Row 0: blank
    left_rows.push((Vec::new(), 0));

    // Rows 1..6: 6 Logo rows (indent 3, accent / gradient)
    for (i, row) in LOGO.iter().enumerate() {
        let spans = vec![
            Span::raw("   "),
            Span::styled(*row, get_logo_row_style(i, theme)),
        ];
        left_rows.push((spans, 3 + LOGO_WIDTH));
    }

    // Row 7: blank
    left_rows.push((Vec::new(), 0));

    // Row 8: "Ask your documents. Get cited answers." (bold)
    let subtitle = "Ask your documents. Get cited answers.";
    let sub_w = UnicodeWidthStr::width(subtitle);
    if sub_w + 3 <= left_width {
        left_rows.push((vec![
            Span::raw("   "),
            Span::styled(subtitle, theme.style_bold()),
        ], 3 + sub_w));
    } else {
        let (disp, dw) = truncate_to_width(subtitle, left_width.saturating_sub(3));
        left_rows.push((vec![
            Span::raw("   "),
            Span::styled(disp, theme.style_bold()),
        ], 3 + dw));
    }

    // Row 9: chat model name (dim)
    let model_str = chat_model;
    let model_w = UnicodeWidthStr::width(model_str);
    if model_w + 3 <= left_width {
        left_rows.push((vec![
            Span::raw("   "),
            Span::styled(model_str.to_string(), theme.style_muted()),
        ], 3 + model_w));
    } else {
        let (disp, dw) = truncate_to_width(model_str, left_width.saturating_sub(3));
        left_rows.push((vec![
            Span::raw("   "),
            Span::styled(disp, theme.style_muted()),
        ], 3 + dw));
    }

    // Row 10: "cwd: <path>" (dim)
    let cwd_prefix = "   cwd: ";
    let cwd_pref_w = UnicodeWidthStr::width(cwd_prefix);
    let max_path = left_width.saturating_sub(cwd_pref_w);
    let (disp_path, dw) = truncate_to_width(cwd_str, max_path);
    left_rows.push((vec![
        Span::styled(cwd_prefix, theme.style_muted()),
        Span::styled(disp_path, theme.style_muted()),
    ], cwd_pref_w + dw));

    // --- Right Panel Rows (11 rows) ---
    let mut right_rows: Vec<(Vec<Span<'static>>, usize)> = Vec::new();

    // Row 0: blank
    right_rows.push((Vec::new(), 0));

    // Row 1: "Index   ● ready" (green dot; yellow "○ empty"; red "✗ API unreachable" when health check fails)
    let (dot, status_label, status_style) = if !api_ok {
        (if theme.use_unicode { "✗ " } else { "x " }, "API unreachable", theme.style_error())
    } else if chunks_count == 0 {
        (if theme.use_unicode { "○ " } else { "o " }, "empty", theme.style_warning())
    } else {
        (if theme.use_unicode { "● " } else { "* " }, "ready", theme.style_success())
    };
    let r1_prefix = "   Index   ";
    let r1_prefix_w = UnicodeWidthStr::width(r1_prefix);
    let dot_w = UnicodeWidthStr::width(dot);
    let label_w = UnicodeWidthStr::width(status_label);
    right_rows.push((vec![
        Span::styled(r1_prefix, theme.style_normal()),
        Span::styled(dot, status_style),
        Span::styled(status_label, status_style),
    ], r1_prefix_w + dot_w + label_w));

    // Row 2: "{chunks} chunks · {sources} sources"
    let chunks_fmt = format_count(chunks_count);
    let sources_fmt = format_count(sources_count);
    let chunk_unit = if chunks_count == 1 { "chunk" } else { "chunks" };
    let source_unit = if sources_count == 1 { "source" } else { "sources" };
    let sep = if theme.use_unicode { " · " } else { " - " };
    let r2_str = format!("   {chunks_fmt} {chunk_unit}{sep}{sources_fmt} {source_unit}");
    let r2_w = UnicodeWidthStr::width(r2_str.as_str());
    right_rows.push((vec![
        Span::styled(r2_str, theme.style_muted()),
    ], r2_w));

    // Row 3: blank row
    right_rows.push((Vec::new(), 0));

    // Rows 4..7: Source list or empty-state hints
    if chunks_count == 0 {
        let h1 = "   /add <path|url> to index documents";
        right_rows.push((vec![Span::styled(h1, theme.style_muted())], UnicodeWidthStr::width(h1)));

        let h2 = "   Ask a question once sources are indexed";
        right_rows.push((vec![Span::styled(h2, theme.style_muted())], UnicodeWidthStr::width(h2)));

        let h3 = "   /model to switch models";
        right_rows.push((vec![Span::styled(h3, theme.style_muted())], UnicodeWidthStr::width(h3)));

        right_rows.push((Vec::new(), 0));
    } else {
        // Up to 3 sources with most chunks
        for i in 0..3 {
            if i < top_sources.len() {
                let (src_path, cnt) = &top_sources[i];
                let fname = Path::new(src_path).file_name().and_then(|n| n.to_str()).unwrap_or(src_path);
                let count_str = format!("{cnt}");
                let count_w = count_str.len();

                let margin = 3; // "   "
                let end_margin = 2; // "  "
                let min_gap = 2; // "  "
                let avail_for_name = right_width.saturating_sub(margin + count_w + min_gap + end_margin);
                let (disp_name, name_w) = if UnicodeWidthStr::width(fname) > avail_for_name {
                    truncate_to_width(fname, avail_for_name)
                } else {
                    (fname.to_string(), UnicodeWidthStr::width(fname))
                };

                let gap = right_width.saturating_sub(margin + name_w + count_w + end_margin).max(min_gap);
                let spans = vec![
                    Span::raw("   "),
                    Span::styled(disp_name, theme.style_normal()),
                    Span::raw(" ".repeat(gap)),
                    Span::styled(count_str, theme.style_muted()),
                    Span::raw("  "),
                ];
                let total_w = margin + name_w + gap + count_w + end_margin;
                right_rows.push((spans, total_w));
            } else {
                right_rows.push((Vec::new(), 0));
            }
        }

        // Row 7: "+ N more (/sources)" when there are more than 3
        if sources_count > 3 {
            let more_cnt = sources_count - 3;
            let more_str = format!("   + {more_cnt} more  (/sources)");
            let mw = UnicodeWidthStr::width(more_str.as_str());
            right_rows.push((vec![Span::styled(more_str, theme.style_muted())], mw));
        } else {
            right_rows.push((Vec::new(), 0));
        }
    }

    // Row 8: blank row
    right_rows.push((Vec::new(), 0));

    // Row 9: "/add  index files   /help  commands"
    let cmd_str = "   /add  index files   /help  commands";
    let cmd_w = UnicodeWidthStr::width(cmd_str);
    right_rows.push((vec![Span::styled(cmd_str, theme.style_muted())], cmd_w));

    // Row 10: blank row
    right_rows.push((Vec::new(), 0));

    // Combine left and right panels row by row
    let max_rows = left_rows.len().max(right_rows.len());
    for i in 0..max_rows {
        let (l_spans, l_w) = if i < left_rows.len() {
            left_rows[i].clone()
        } else {
            (Vec::new(), 0)
        };
        let (r_spans, r_w) = if i < right_rows.len() {
            right_rows[i].clone()
        } else {
            (Vec::new(), 0)
        };

        let mut row_spans = Vec::new();
        row_spans.push(Span::styled(theme.borders.vertical.to_string(), theme.style_accent()));
        row_spans.extend(pad_spans_to_width(l_spans, l_w, left_width));
        row_spans.push(Span::styled(theme.borders.vertical.to_string(), theme.style_accent()));
        row_spans.extend(pad_spans_to_width(r_spans, r_w, right_width));
        row_spans.push(Span::styled(theme.borders.vertical.to_string(), theme.style_accent()));

        lines.push(Line::from(row_spans));
    }

    // --- Bottom border: "╰─...─┴─...─╯" ---
    let bot_left = theme.borders.bottom_left;
    let bot_right = theme.borders.bottom_right;
    let b_left_dash = h.to_string().repeat(left_width);
    let b_right_dash = h.to_string().repeat(right_width);
    lines.push(Line::from(vec![Span::styled(
        format!("{bot_left}{b_left_dash}{bot_div}{b_right_dash}{bot_right}"),
        theme.style_accent(),
    )]));

    lines
}

fn render_single_panel_banner(
    theme: &Theme,
    cwd_str: &str,
    sources_count: i64,
    chunks_count: i64,
    terminal_width: usize,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let box_width = compute_banner_box_width(terminal_width);
    let inner_width = box_width.saturating_sub(2);

    // Top border
    let h = theme.borders.horizontal.to_string().repeat(inner_width);
    let top = format!("{}{}{}", theme.borders.top_left, h, theme.borders.top_right);
    lines.push(Line::from(vec![Span::styled(top, theme.style_accent())]));

    // Blank line inside box
    lines.push(make_banner_box_line(Vec::new(), 0, inner_width, theme));

    // 6 Logo lines
    for (i, logo_row) in LOGO.iter().enumerate() {
        let mut spans = Vec::new();
        spans.push(Span::raw("   "));
        spans.push(Span::styled(*logo_row, get_logo_row_style(i, theme)));

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
    let (display_path, path_w) = if UnicodeWidthStr::width(cwd_str) > max_path_w {
        truncate_to_width(cwd_str, max_path_w)
    } else {
        (cwd_str.to_string(), UnicodeWidthStr::width(cwd_str))
    };
    let cwd_w = cwd_prefix_w + path_w;
    let cwd_spans = vec![
        Span::styled(cwd_prefix, theme.style_muted()),
        Span::styled(display_path, theme.style_muted()),
    ];
    lines.push(make_banner_box_line(cwd_spans, cwd_w, inner_width, theme));

    // Blank line inside box
    lines.push(make_banner_box_line(Vec::new(), 0, inner_width, theme));

    // Status line inside the box before bottom border
    let (status_text, status_w) = if chunks_count > 0 {
        let chunks_fmt = format_count(chunks_count);
        let sources_fmt = format_count(sources_count);
        let chunk_word = if chunks_count == 1 { "chunk" } else { "chunks" };
        let source_word = if sources_count == 1 { "source" } else { "sources" };
        let msg = format!("   {chunks_fmt} {chunk_word} from {sources_fmt} {source_word} ready");
        let w = UnicodeWidthStr::width(msg.as_str());
        (msg, w)
    } else {
        let msg = "   Tips: /add <path> to index documents, /help for commands".to_string();
        let w = UnicodeWidthStr::width(msg.as_str());
        (msg, w)
    };
    let status_spans = vec![Span::styled(status_text, theme.style_muted())];
    lines.push(make_banner_box_line(status_spans, status_w, inner_width, theme));

    // Bottom border
    let bottom = format!("{}{}{}", theme.borders.bottom_left, h, theme.borders.bottom_right);
    lines.push(Line::from(vec![Span::styled(bottom, theme.style_accent())]));

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
    fn test_banner_rows_have_equal_display_width_at_90_100_120_two_panel_and_80_single_panel() {
        let theme = make_test_theme();
        let cwd = PathBuf::from("/home/kevin/advanced-rag");
        let top_sources = vec![
            ("DeepSeek_V41_Tech_Report.pdf".to_string(), 42),
            ("SnehilResume.pdf".to_string(), 25),
            ("QuarterlyFinancials.xlsx".to_string(), 9),
            ("ExtraSource.txt".to_string(), 2),
        ];

        // 1. Two-panel banner at widths 90, 100, 120 (height = 30)
        for term_width in [90, 100, 120] {
            let expected_width = compute_banner_box_width_two_panel(term_width);
            let expected_calc = (term_width - 2).min(100);
            assert_eq!(expected_width, expected_calc);

            let lines = render_banner_lines_full(
                &theme,
                &cwd,
                4,
                78,
                "deepseekv4.1-flash-1017",
                true,
                &top_sources,
                term_width,
                30,
            );

            assert!(lines.len() >= 13, "Two-panel banner must have at least 13 lines");
            for (idx, line) in lines.iter().enumerate() {
                let display_w = line.width();
                assert_eq!(
                    display_w, expected_width,
                    "Line {idx} at terminal width {term_width} (height 30) has width {display_w}, expected {expected_width}. Text: '{line:?}'"
                );
            }
        }

        // 2. Single-panel banner at width 80 (height = 30)
        let term_width = 80;
        let expected_width = compute_banner_box_width(term_width);
        let expected_calc = (term_width - 2).min(80);
        assert_eq!(expected_width, expected_calc);

        let lines = render_banner_lines_full(
            &theme,
            &cwd,
            4,
            78,
            "deepseekv4.1-flash-1017",
            true,
            &top_sources,
            term_width,
            30,
        );

        assert!(lines.len() >= 13, "Single-panel banner must have at least 13 lines");
        for (idx, line) in lines.iter().enumerate() {
            let display_w = line.width();
            assert_eq!(
                display_w, expected_width,
                "Single-panel line {idx} at terminal width {term_width} has width {display_w}, expected {expected_width}. Text: '{line:?}'"
            );
        }
    }

    #[test]
    fn test_fallback_selection_by_width_height_no_color() {
        let theme = make_test_theme();
        let cwd = PathBuf::from("/home/kevin/advanced-rag");

        // 1. Two-panel: width >= 90 and height >= 30
        assert!(use_two_panel(90, 30, &theme));
        assert!(use_two_panel(100, 35, &theme));

        // 2. Fallback to single-panel when width 72-89 OR height < 30
        assert!(!use_two_panel(89, 30, &theme));
        assert!(!use_compact_header(89, &theme)); // Single-panel, not compact

        assert!(!use_two_panel(100, 29, &theme)); // Height < 30 collapses to single-panel
        assert!(!use_compact_header(100, &theme)); // Single-panel, not compact

        assert!(!use_two_panel(72, 30, &theme));
        assert!(!use_compact_header(72, &theme)); // Single-panel

        // 3. Compact fallback below 72 cols
        assert!(use_compact_header(71, &theme));
        assert!(use_compact_header(50, &theme));

        let compact_lines = render_banner_lines_full(
            &theme,
            &cwd,
            4,
            120,
            "deepseekv4.1-flash-1017",
            true,
            &[],
            70,
            30,
        );
        let first_text = compact_lines[0].spans.iter().map(|s| s.content.as_ref()).collect::<String>();
        assert!(
            first_text.contains("Sift v0.1.0") && first_text.contains("120 chunks") && first_text.contains("4 sources"),
            "Compact header should match format, got: '{first_text}'"
        );

        // 4. NO_COLOR forces compact fallback regardless of width
        let nc_theme = Theme::no_color();
        assert!(use_compact_header(120, &nc_theme));
        let nc_lines = render_banner_lines_full(
            &nc_theme,
            &cwd,
            4,
            120,
            "deepseekv4.1-flash-1017",
            true,
            &[],
            120,
            30,
        );
        let nc_first = nc_lines[0].spans.iter().map(|s| s.content.as_ref()).collect::<String>();
        assert!(nc_first.contains("Sift v0.1.0") && nc_first.contains("120 chunks"));
    }

    #[test]
    fn test_source_list_truncation_and_more_than_three() {
        let theme = make_test_theme();
        let cwd = PathBuf::from("/home/kevin/advanced-rag");
        let top_sources = vec![
            ("ThisIsAnExtremelyLongDocumentFileNameThatMustBeTruncatedProperly.pdf".to_string(), 99),
            ("SecondDocument.md".to_string(), 45),
            ("ThirdDocument.txt".to_string(), 12),
            ("FourthDocument.rst".to_string(), 5),
            ("FifthDocument.html".to_string(), 1),
        ];

        let lines = render_banner_lines_full(
            &theme,
            &cwd,
            5,
            162,
            "deepseekv4.1-flash-1017",
            true,
            &top_sources,
            100,
            30,
        );

        let combined = lines.iter().map(|l| l.to_string()).collect::<Vec<_>>().join("\n");
        // Verify truncation with ellipsis
        assert!(combined.contains('…'), "Long file name must be truncated with '…'");
        // Verify "+ N more (/sources)"
        assert!(combined.contains("+ 2 more  (/sources)"), "Expected '+ 2 more  (/sources)'");
    }

    #[test]
    fn test_empty_state_hints() {
        let theme = make_test_theme();
        let cwd = PathBuf::from("/home/kevin/advanced-rag");

        let lines = render_banner_lines_full(
            &theme,
            &cwd,
            0,
            0,
            "deepseekv4.1-flash-1017",
            true,
            &[],
            100,
            30,
        );

        let combined = lines.iter().map(|l| l.to_string()).collect::<Vec<_>>().join("\n");
        assert!(combined.contains("empty") || combined.contains('○'), "Empty status must be shown");
        assert!(combined.contains("/add <path|url> to index documents"));
        assert!(combined.contains("Ask a question once sources are indexed"));
        assert!(combined.contains("/model to switch models"));
    }
}
