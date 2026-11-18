pub mod theme;
pub mod banner;
pub mod welcome;
pub mod commands;
pub mod footer;
pub mod input;
pub mod messages;
pub mod spinner;
pub mod markdown;
pub mod app;

pub use app::emit;

#[cfg(test)]
mod tests {
    use std::path::Path;
    use super::*;
    use ratatui::{
        backend::TestBackend,
        layout::{Constraint, Direction, Layout, Rect},
        text::{Line, Span},
        widgets::{Block, BorderType, Borders, Paragraph},
        Terminal, TerminalOptions, Viewport,
    };
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn test_ratatui_inline_viewport() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Inline(8),
            },
        )
        .unwrap();

        terminal
            .insert_before(2, |buf| {
                buf.set_string(0, 0, "Hello", ratatui::style::Style::default());
            })
            .unwrap();
    }

    #[test]
    fn test_emit_no_trailing_padding() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Inline(8),
            },
        )
        .unwrap();

        // Pass lines where some have accidental trailing spaces or content shorter than 80
        let test_lines = vec![
            Line::from(vec![
                Span::styled("● First short line", ratatui::style::Style::default().fg(ratatui::style::Color::Yellow)),
                Span::styled("     ", ratatui::style::Style::default().fg(ratatui::style::Color::Yellow)), // trailing spaces with style
            ]),
            Line::from(vec![
                Span::styled("  Second line with real content", ratatui::style::Style::default().fg(ratatui::style::Color::Green)),
            ]),
        ];

        emit(&mut terminal, test_lines, 80).unwrap();

        let buffer = terminal.backend().buffer();
        // Line 0: "● First short line" has visual width 18
        // Columns beyond 18 must NOT have styled trailing spaces; they must be default empty cells
        for col in 18..80 {
            let cell = &buffer[(col, 0)];
            assert_eq!(cell.symbol(), " ");
            assert_eq!(cell.style().fg.unwrap_or(ratatui::style::Color::Reset), ratatui::style::Color::Reset);
            assert_eq!(cell.style().bg.unwrap_or(ratatui::style::Color::Reset), ratatui::style::Color::Reset);
        }

        // Line 1: "  Second line with real content" has visual width 31
        for col in 31..80 {
            let cell = &buffer[(col, 1)];
            assert_eq!(cell.symbol(), " ");
            assert_eq!(cell.style().fg.unwrap_or(ratatui::style::Color::Reset), ratatui::style::Color::Reset);
            assert_eq!(cell.style().bg.unwrap_or(ratatui::style::Color::Reset), ratatui::style::Color::Reset);
        }
    }

    #[test]
    fn test_viewport_equal_width_and_no_trailing_padding() {
        for &cols in &[60u16, 80u16, 120u16] {
            let backend = TestBackend::new(cols, 24);
            let mut terminal = Terminal::with_options(
                backend,
                TerminalOptions {
                    viewport: Viewport::Inline(11),
                },
            )
            .unwrap();

            let theme = theme::Theme::detect();
            let mut input = input::InputState::new(std::path::Path::new("/tmp"));
            let mut spinner = spinner::SpinnerState::new();
            let footer = footer::FooterState {
                is_busy: true,
                ctrl_c_primed: false,
                shortcuts_open: false,
                sources_count: 5,
                chunks_count: 120,
                api_reachable: true,
            };

            let box_width = (cols.saturating_sub(2)) as usize;

            terminal
                .draw(|f| {
                    let area = f.area();
                    let draw_area = Rect {
                        x: area.x,
                        y: area.y,
                        width: (box_width as u16).min(area.width),
                        height: area.height,
                    };

                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Length(1), // Spinner
                            Constraint::Length(3), // Input box
                            Constraint::Length(1), // Footer
                            Constraint::Min(0),
                        ])
                        .split(draw_area);

                    // 1. Spinner
                    let spinner_line = spinner.render_ratatui_line(&theme);
                    f.render_widget(Paragraph::new(spinner_line), chunks[0]);

                    // 2. Input box
                    let block = Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(theme.style_muted());
                    let inner = block.inner(chunks[1]);
                    f.render_widget(block, chunks[1]);
                    let prompt_span = Span::styled(" > ", theme.style_muted());
                    let ph_span = Span::styled(input.current_placeholder(), theme.style_muted());
                    f.render_widget(Paragraph::new(Line::from(vec![prompt_span, ph_span])), inner);

                    // 3. Footer
                    let footer_line = footer.render_ratatui_line(draw_area.width as usize, &theme);
                    f.render_widget(Paragraph::new(footer_line), chunks[2]);
                })
                .unwrap();

            let buffer = terminal.backend().buffer();

            // Verify input box rows (rows 1, 2, 3)
            for row in 1..=3 {
                // Assert cells within box_width have the box characters / content
                let left_cell = &buffer[(0, row)];
                assert!(!left_cell.symbol().is_empty());

                let right_cell = &buffer[(box_width as u16 - 1, row)];
                assert!(!right_cell.symbol().is_empty());

                // Assert cells beyond box_width have NO trailing padding (default empty spaces)
                for col in (box_width as u16)..cols {
                    let cell = &buffer[(col, row)];
                    assert_eq!(
                        cell.symbol(),
                        " ",
                        "Found non-empty cell beyond box_width at col {}, row {}",
                        col,
                        row
                    );
                    assert_eq!(cell.style().bg.unwrap_or(ratatui::style::Color::Reset), ratatui::style::Color::Reset);
                }
            }
        }
    }

    #[test]
    fn test_layout_component_widths_at_60_80_100_120_160() {
        use std::path::PathBuf;
        let mut theme = theme::Theme::detect();
        theme.use_color = true;
        theme.use_unicode = true;
        theme.is_tty = true;
        let cwd = PathBuf::from("/home/kevin/advanced-rag");

        for &width in &[60usize, 80usize, 100usize, 120usize, 160usize] {
            // 1. Input box and footer: full width, terminal width - 2
            let expected_input_footer_w = width - 2;
            let footer = footer::FooterState {
                is_busy: false,
                ctrl_c_primed: false,
                shortcuts_open: false,
                sources_count: 3,
                chunks_count: 76,
                api_reachable: true,
            };
            let footer_line = footer.render_ratatui_line(expected_input_footer_w, &theme);
            assert_eq!(
                footer_line.width(),
                expected_input_footer_w,
                "Footer width mismatch at terminal width {width}"
            );
            // Footer right status right-aligns to box edge
            let last_span = footer_line.spans.last().unwrap();
            assert_eq!(last_span.content, "3 sources · 76 chunks");

            // 2. Welcome/banner box: min(terminal width - 2, 80)
            let expected_banner_w = (width - 2).min(80);
            let banner_w = banner::compute_banner_box_width(width);
            assert_eq!(
                banner_w, expected_banner_w,
                "Banner width mismatch at terminal width {width}"
            );
            let banner_lines = banner::render_banner_lines(&theme, &cwd, 0, 0, width);
            assert!(banner_lines.len() >= 11);
            for (idx, line) in banner_lines[0..11].iter().enumerate() {
                assert_eq!(
                    line.width(),
                    expected_banner_w,
                    "Banner box line {idx} width mismatch at terminal width {width}: got {}, expected {}",
                    line.width(),
                    expected_banner_w
                );
            }

            // 3. Answer text, tool rows and sources block: wrap at min(terminal width - 4, 100)
            let expected_content_w = (width - 4).min(100);
            let content_w = messages::MessageFormatter::compute_content_width(width);
            assert_eq!(
                content_w, expected_content_w,
                "Content wrap width mismatch at terminal width {width}"
            );

            // Test answer text wrapping
            let long_answer = "This is a detailed response explaining the complex architecture and implementation details with very long sentences designed to verify wrapping boundaries.";
            let ans_lines = messages::MessageFormatter::wrap_assistant_answer(long_answer, &theme, content_w);
            for (idx, l) in ans_lines.iter().enumerate() {
                assert!(
                    l.width() <= expected_content_w,
                    "Answer line {idx} width {} exceeds max {} at terminal width {width}",
                    l.width(),
                    expected_content_w
                );
                let text = l.spans.iter().map(|s| s.content.as_ref()).collect::<String>();
                if idx > 0 && !text.is_empty() {
                    assert!(
                        text.starts_with("  "),
                        "Continuation line {idx} must start with 2-space hanging indent: '{text}'"
                    );
                }
            }

            // Test tool rows wrapping
            let long_query = "What is the comprehensive financial breakdown and EBITDA margin for Q3 2024 across all business segments?";
            let tool_lines = messages::MessageFormatter::render_retrieve_tool_done_wrapped(
                "summary?",
                long_query,
                "5 chunks from 2 sources · 0.2s (3 filtered)",
                true,
                false,
                &theme,
                content_w,
            );
            for (idx, l) in tool_lines.iter().enumerate() {
                assert!(
                    l.width() <= expected_content_w,
                    "Tool row line {idx} width {} exceeds max {} at terminal width {width}",
                    l.width(),
                    expected_content_w
                );
                let text = l.spans.iter().map(|s| s.content.as_ref()).collect::<String>();
                if idx > 0 && !text.is_empty() {
                    assert!(
                        text.starts_with("  "),
                        "Tool continuation line {idx} must start with 2-space hanging indent: '{text}'"
                    );
                }
            }

            // Test sources block wrapping
            let sources = vec![
                messages::SourceRef::with_page(1, "/path/to/A_Very_Long_Document_Name_Exceeding_Line_Widths.pdf", 0.02, 12),
                messages::SourceRef::with_page(2, "/path/to/A_Very_Long_Document_Name_Exceeding_Line_Widths.pdf", 0.02, 15),
                messages::SourceRef::with_page(3, "/path/to/A_Very_Long_Document_Name_Exceeding_Line_Widths.pdf", 0.02, 18),
            ];
            let sources_lines = messages::MessageFormatter::render_sources_block_filtered_wrapped(
                "Cited in [1][2][3].",
                &sources,
                &theme,
                content_w,
            );
            for (idx, l) in sources_lines.iter().enumerate() {
                assert!(
                    l.width() <= expected_content_w,
                    "Sources line {idx} width {} exceeds max {} at terminal width {width}",
                    l.width(),
                    expected_content_w
                );
                let text = l.spans.iter().map(|s| s.content.as_ref()).collect::<String>();
                if !text.is_empty() {
                    assert!(
                        text.starts_with("  "),
                        "Sources line {idx} must start with 2-space hanging indent: '{text}'"
                    );
                }
            }

            // 4. Slash menu: same width as input box (width - 2)
            let slash_menu_w = expected_input_footer_w;
            assert_eq!(slash_menu_w, width - 2);
        }

        // 5. Below 60 columns: compact layout (no boxes)
        let compact_w = 50usize;
        assert!(banner::use_compact_fallback(compact_w, &theme));
        let compact_banner = banner::render_banner_lines(&theme, &cwd, 0, 0, compact_w);
        let first_line = compact_banner[0].spans.iter().map(|s| s.content.as_ref()).collect::<String>();
        assert!(!first_line.contains('╭'));
        assert!(!first_line.contains('╰'));
        assert!(first_line.contains("Sift"));
    }

    #[test]
    fn test_emit_height_calculation_and_wrapping() {
        // Line wrapping checks at 80 cols
        let line_short = Line::from("Short line");
        assert_eq!(app::compute_line_height(&line_short, 80), 1);

        let line_exact = Line::from("a".repeat(80));
        assert_eq!(app::compute_line_height(&line_exact, 80), 1);

        let line_wrap_1 = Line::from("a".repeat(81));
        assert_eq!(app::compute_line_height(&line_wrap_1, 80), 2);

        let line_wrap_2 = Line::from("a".repeat(160));
        assert_eq!(app::compute_line_height(&line_wrap_2, 80), 2);

        let line_wrap_3 = Line::from("a".repeat(161));
        assert_eq!(app::compute_line_height(&line_wrap_3, 80), 3);

        // Empty line takes 1 row of height
        let line_empty = Line::default();
        assert_eq!(app::compute_line_height(&line_empty, 80), 1);

        // Wide unicode characters (CJK: 4 chars = 8 display width)
        let line_cjk = Line::from("你好世界");
        assert_eq!(UnicodeWidthStr::width("你好世界"), 8);
        assert_eq!(app::compute_line_height(&line_cjk, 80), 1);

        // 41 CJK characters = 82 display width -> wraps to 2 lines at 80 cols
        let cjk_41 = "你".repeat(41);
        assert_eq!(UnicodeWidthStr::width(cjk_41.as_str()), 82);
        let line_cjk_wrap = Line::from(cjk_41);
        assert_eq!(app::compute_line_height(&line_cjk_wrap, 80), 2);

        // Total height calculation
        let lines = vec![
            line_short,
            line_empty,
            line_exact,
            line_wrap_1,
            line_cjk_wrap,
        ];
        // 1 + 1 + 1 + 2 + 2 = 7
        assert_eq!(app::compute_total_height(&lines, 80), 7);
    }

    #[test]
    fn test_lint_no_print_in_ui_and_commands() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let ui_dir = manifest_dir.join("src/ui");

        assert!(ui_dir.exists(), "src/ui directory must exist");

        let p_macro = format!("{}!", "print");
        let pl_macro = format!("{}!", "println");

        for entry in walkdir::WalkDir::new(&ui_dir).into_iter().flatten() {
            if entry.path().extension().map(|e| e == "rs").unwrap_or(false) {
                // Don't check this test definition itself
                if entry.path().ends_with("ui/mod.rs") {
                    continue;
                }
                let content = std::fs::read_to_string(entry.path()).unwrap();
                for (line_idx, line) in content.lines().enumerate() {
                    let trimmed = line.trim();
                    // Skip comments
                    if trimmed.starts_with("//")
                        || trimmed.starts_with("/*")
                        || trimmed.starts_with('*')
                    {
                        continue;
                    }
                    assert!(
                        !trimmed.contains(&pl_macro) && !trimmed.contains(&p_macro),
                        "Prohibited print in {}:{} -> {}",
                        entry.path().strip_prefix(manifest_dir).unwrap_or(entry.path()).display(),
                        line_idx + 1,
                        trimmed
                    );
                }
            }
        }
    }

    #[test]
    fn test_ui_resize_handling() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        use ratatui::layout::{Constraint, Direction, Layout, Rect};
        use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
        use std::path::PathBuf;

        let mut theme = theme::Theme::detect();
        theme.use_color = true;
        theme.use_unicode = true;
        theme.is_tty = true;

        // Initialize terminal at 80x24
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        // Helper closure to simulate the draw frame logic
        let draw_frame = |terminal: &mut Terminal<TestBackend>, input_text: &str| {
            terminal.draw(|f| {
                let area = f.area();
                if area.height == 0 || area.width == 0 {
                    return;
                }
                f.render_widget(Clear, area);

                let term_width = area.width as usize;
                let is_compact = term_width < 60 || area.height < 3;
                let input_box_width = if is_compact {
                    area.width
                } else {
                    (area.width.saturating_sub(2)).max(1)
                };

                let draw_area = Rect {
                    x: area.x,
                    y: area.y,
                    width: input_box_width,
                    height: area.height,
                };

                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3),
                        Constraint::Length(1),
                        Constraint::Min(0),
                    ])
                    .split(draw_area);

                let block = if is_compact {
                    Block::default().borders(Borders::NONE)
                } else {
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(theme.style_muted())
                };

                let inner = block.inner(chunks[0]);
                f.render_widget(block, chunks[0]);
                f.render_widget(Paragraph::new(input_text), inner);

                let footer = footer::FooterState {
                    is_busy: false,
                    ctrl_c_primed: false,
                    shortcuts_open: false,
                    sources_count: 5,
                    chunks_count: 42,
                    api_reachable: true,
                };
                let footer_line = footer.render_ratatui_line(draw_area.width as usize, &theme);
                f.render_widget(Paragraph::new(footer_line), chunks[1]);
            }).unwrap();
        };

        // 1. Initial draw at 80x24
        draw_frame(&mut terminal, "initial query");
        assert_eq!(terminal.size().unwrap().width, 80);
        assert_eq!(terminal.size().unwrap().height, 24);

        // 2. Resize to 120x30 (wider terminal)
        terminal.backend_mut().resize(120, 30);
        terminal.resize(Rect::new(0, 0, 120, 30)).unwrap();
        terminal.clear().unwrap();
        draw_frame(&mut terminal, "after expanding resize");
        assert_eq!(terminal.size().unwrap().width, 120);
        assert_eq!(terminal.size().unwrap().height, 30);

        // Verify buffer width matches new width (120) and input box spans width - 2 (118)
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer.area().width, 120);
        assert_eq!(buffer.area().height, 30);
        // Col 117 is the right border of the rounded box at width 120
        assert_ne!(buffer[(117, 1)].symbol(), " ");
        // Col 118 & 119 are beyond the box width and have no trailing padding
        assert_eq!(buffer[(118, 1)].symbol(), " ");
        assert_eq!(buffer[(119, 1)].symbol(), " ");

        // 3. Resize to 50x15 (narrow terminal below 60 cols -> compact mode)
        terminal.backend_mut().resize(50, 15);
        terminal.resize(Rect::new(0, 0, 50, 15)).unwrap();
        terminal.clear().unwrap();
        draw_frame(&mut terminal, "compact mode query");
        assert_eq!(terminal.size().unwrap().width, 50);
        assert_eq!(terminal.size().unwrap().height, 15);

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer.area().width, 50);
        // In compact mode, borders are none, so col 0 row 0 is raw text
        assert_eq!(buffer[(0, 0)].symbol(), "c");

        // 4. Resize to tiny height (80x2) -> compact mode due to height < 3
        terminal.backend_mut().resize(80, 2);
        terminal.resize(Rect::new(0, 0, 80, 2)).unwrap();
        terminal.clear().unwrap();
        draw_frame(&mut terminal, "tiny height");
        assert_eq!(terminal.size().unwrap().height, 2);

        // 5. Test execute_command_with_width adapts output formatting on resize
        let mut status = app::ShellStatus {
            db_path: PathBuf::from(".rag/test.sqlite"),
            sources: 2,
            chunks: 10,
            chat_model: "test-model".into(),
            embed_model: "test-embed".into(),
            api_base: "https://test.api".into(),
            api_ok: true,
            last_query_debug: None,
            last_retrieval: Some(vec![
                messages::RetrievedChunk {
                    rank: 1,
                    file: "long_document_name_for_resize_verification_testing.pdf".into(),
                    full_path: "/path/to/long_document_name_for_resize_verification_testing.pdf".into(),
                    page: Some(4),
                    text: "Retrieval augmented generation systems combine information retrieval with sequence-to-sequence language modeling to answer questions grounded in knowledge.".into(),
                    score: 0.85,
                    is_cited: true,
                },
            ]),
        };

        let conn = rusqlite::Connection::open_in_memory().unwrap();

        for &width in &[60usize, 80usize, 120usize] {
            let output = app::execute_command_with_width("/chunks", &mut status, &mut None, &conn, &theme, width);
            if let app::CommandOutput::Lines(lines) = output {
                let max_content_w = messages::MessageFormatter::compute_content_width(width);
                for l in &lines {
                    assert!(
                        l.width() <= max_content_w,
                        "Chunk line width {} exceeds {} at terminal width {}",
                        l.width(),
                        max_content_w,
                        width
                    );
                }
            } else {
                panic!("Expected CommandOutput::Lines for /chunks");
            }

            let cite_out = app::execute_command_with_width("/cite 1", &mut status, &mut None, &conn, &theme, width);
            if let app::CommandOutput::Lines(lines) = cite_out {
                let max_content_w = messages::MessageFormatter::compute_content_width(width);
                for l in &lines {
                    assert!(
                        l.width() <= max_content_w,
                        "Cite line width {} exceeds {} at terminal width {}",
                        l.width(),
                        max_content_w,
                        width
                    );
                }
            } else {
                panic!("Expected CommandOutput::Lines for /cite 1");
            }
        }
    }
}
