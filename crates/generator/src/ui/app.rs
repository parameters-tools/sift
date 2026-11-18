use std::{
    io::{self, stdout, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{self, disable_raw_mode, enable_raw_mode, ClearType},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph},
    Terminal, TerminalOptions, Viewport,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{api::Api, chunker, db, retrieve, sources};
use super::{
    commands::{CommandDef, CommandRegistry},
    footer::FooterState,
    input::InputState,
    messages::MessageFormatter,
    spinner::SpinnerState,
    theme::Theme,
    banner::render_banner_lines,
};

pub type QueryDebugList = Vec<(usize, String, f32)>;

#[derive(Debug, Clone)]
pub struct ShellStatus {
    pub db_path: PathBuf,
    pub sources: i64,
    pub chunks: i64,
    pub chat_model: String,
    pub embed_model: String,
    pub api_base: String,
    pub api_ok: bool,
    pub last_query_debug: Option<(String, QueryDebugList)>,
    pub last_retrieval: Option<Vec<crate::ui::messages::RetrievedChunk>>,
}

impl ShellStatus {
    pub fn refresh(&mut self, conn: &rusqlite::Connection) {
        if let Ok((s, c)) = db::get_stats(conn) {
            self.sources = s;
            self.chunks = c;
        }
    }
}

#[derive(Debug, Clone)]
pub struct QuerySuccess {
    pub answer: String,
    pub tool_detail: String,
    pub sources: Vec<(usize, String, f32)>,
    pub retrieved_chunks: Vec<crate::ui::messages::RetrievedChunk>,
    pub is_weak: bool,
    pub no_strong_match: bool,
    pub answer_elapsed: f64,
    pub token_count: usize,
    pub is_approx_tokens: bool,
    pub chat_model: String,
}

pub type QueryResult = Result<QuerySuccess>;

pub enum UiEvent {
    Token(String),
    HealthResult(bool),
    QueryDone {
        user_query: String,
        retrieved_query: String,
        result: QueryResult,
    },
    IndexDone {
        path: String,
        result: Result<(usize, usize, f32)>,
    },
}

pub enum CommandOutput {
    Lines(Vec<Line<'static>>),
    Clear,
    New,
    Exit,
    Index { path: String },
    Question { query: String },
}

pub fn setup_panic_hook() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), cursor::Show);
        original_hook(panic_info);
    }));
}

pub fn compute_line_height(line: &Line, term_width: u16) -> u16 {
    let w = line.width() as u16;
    if term_width == 0 || w == 0 {
        1
    } else {
        w.div_ceil(term_width)
    }
}

pub fn compute_total_height(lines: &[Line<'static>], term_width: u16) -> u16 {
    if lines.is_empty() {
        return 0;
    }
    lines.iter().map(|l| compute_line_height(l, term_width)).sum()
}

pub fn emit<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    lines: Vec<Line<'static>>,
    term_width: u16,
) -> io::Result<()> {
    if lines.is_empty() {
        return Ok(());
    }
    let total_height = compute_total_height(&lines, term_width);
    if total_height == 0 {
        return Ok(());
    }

    terminal.insert_before(total_height, |buf| {
        let max_w = if term_width > 0 {
            term_width.min(buf.area.width)
        } else {
            buf.area.width
        };
        let mut row: u16 = 0;
        for line in &lines {
            // Emit only real content per line: trim any trailing spaces from the end of the line
            let mut line_spans: Vec<Span> = line.spans.clone();
            while let Some(last) = line_spans.last_mut() {
                let trimmed = last.content.trim_end_matches(' ');
                if trimmed.is_empty() {
                    line_spans.pop();
                } else if trimmed.len() != last.content.len() {
                    last.content = trimmed.to_string().into();
                    break;
                } else {
                    break;
                }
            }

            let mut col: u16 = 0;
            for span in &line_spans {
                for ch in span.content.chars() {
                    let ch_w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0) as u16;
                    if ch_w == 0 {
                        continue;
                    }
                    if max_w > 0 && col + ch_w > max_w {
                        row += 1;
                        col = 0;
                        if row >= buf.area.height {
                            break;
                        }
                    }
                    if col + ch_w <= buf.area.width && row < buf.area.height {
                        let cell = &mut buf[(col, row)];
                        cell.set_char(ch);
                        cell.set_style(span.style);
                    }
                    col += ch_w;
                }
                if row >= buf.area.height {
                    break;
                }
            }
            row += 1;
            if row >= buf.area.height {
                break;
            }
        }
    })
}

pub fn handle_model_subcommand(
    cmd_arg: &str,
    status: &mut ShellStatus,
    api: &mut Option<Api>,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let parts: Vec<&str> = cmd_arg.split_whitespace().collect();
    if parts.is_empty() {
        vec![
            Line::from(vec![Span::styled("Current Models:", theme.style_bold())]),
            Line::from(vec![
                Span::raw("  Chat:      "),
                Span::styled(status.chat_model.clone(), theme.style_accent()),
            ]),
            Line::from(vec![
                Span::raw("  Embedding: "),
                Span::styled(status.embed_model.clone(), theme.style_accent()),
            ]),
            Line::from(vec![
                Span::styled("  Switch: /model chat <name>  or  /model embed <name>", theme.style_muted()),
            ]),
        ]
    } else if parts.len() >= 2 {
        let target = parts[0].to_ascii_lowercase();
        let new_model = parts[1];
        if target == "chat" {
            status.chat_model = new_model.to_string();
            if let Some(api_client) = api {
                api_client.set_chat_model(new_model.to_string());
            }
            vec![Line::from(vec![
                Span::styled("✔ ", theme.style_success()),
                Span::raw("Chat model set to: "),
                Span::styled(new_model.to_string(), theme.style_accent()),
            ])]
        } else if target == "embed" || target == "embedding" {
            status.embed_model = new_model.to_string();
            if let Some(api_client) = api {
                api_client.set_embedding_model(new_model.to_string());
            }
            vec![Line::from(vec![
                Span::styled("✔ ", theme.style_success()),
                Span::raw("Embedding model set to: "),
                Span::styled(new_model.to_string(), theme.style_accent()),
            ])]
        } else {
            vec![Line::from("Usage: /model [chat|embed] [model-name]")]
        }
    } else {
        vec![Line::from("Usage: /model [chat|embed] [model-name]")]
    }
}

pub fn execute_command(
    submitted: &str,
    status: &mut ShellStatus,
    api: &mut Option<Api>,
    conn: &rusqlite::Connection,
    theme: &Theme,
) -> CommandOutput {
    let line = submitted.trim();
    if let Some(cmd_line) = line.strip_prefix('/') {
        let (cmd_name, cmd_arg) = cmd_line.split_once(char::is_whitespace).unwrap_or((cmd_line, ""));
        let cmd_arg = cmd_arg.trim();

        match cmd_name.to_ascii_lowercase().as_str() {
            "exit" | "quit" | "q" => CommandOutput::Exit,
            "clear" | "cls" => CommandOutput::Clear,
            "new" | "reset" => CommandOutput::New,
            "help" | "h" | "?" => CommandOutput::Lines(MessageFormatter::render_help(theme)),
            "status" => CommandOutput::Lines(MessageFormatter::render_status_panel(
                &status.db_path.display().to_string(),
                status.sources,
                status.chunks,
                &status.chat_model,
                &status.embed_model,
                &status.api_base,
                status.api_ok,
                env!("CARGO_PKG_VERSION"),
                theme,
            )),
            "sources" => {
                let mut list = Vec::new();
                if let Ok(mut stmt) = conn.prepare("SELECT source, COUNT(*) FROM chunks GROUP BY source ORDER BY source")
                    && let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
                {
                    for r in rows.flatten() {
                        list.push(r);
                    }
                }
                CommandOutput::Lines(MessageFormatter::render_sources_list(&list, theme))
            }
            "model" => CommandOutput::Lines(handle_model_subcommand(cmd_arg, status, api, theme)),
            "chunks" => {
                let lines = if let Some(ref chunks) = status.last_retrieval {
                    MessageFormatter::render_chunks_inspection(chunks, 80, theme)
                } else {
                    vec![Line::from(vec![
                        Span::raw("  "),
                        Span::styled("No chunks retrieved yet.", theme.style_muted()),
                    ])]
                };
                CommandOutput::Lines(lines)
            }
            "cite" => {
                let lines = if let Some(ref chunks) = status.last_retrieval {
                    if let Ok(n) = cmd_arg.parse::<usize>() {
                        if n >= 1 && n <= chunks.len() {
                            let chunk = &chunks[n - 1];
                            MessageFormatter::render_cite_inspection(chunk, 80, theme)
                        } else {
                            vec![Line::from(vec![
                                Span::raw("  "),
                                Span::styled(
                                    format!("Chunk [{n}] not found. Available chunks: 1..{}", chunks.len()),
                                    theme.style_error(),
                                ),
                            ])]
                        }
                    } else {
                        vec![Line::from(vec![
                            Span::raw("  "),
                            Span::styled("Usage: /cite <n> (e.g. /cite 1)", theme.style_error()),
                        ])]
                    }
                } else {
                    vec![Line::from(vec![
                        Span::raw("  "),
                        Span::styled("No chunks retrieved yet.", theme.style_muted()),
                    ])]
                };
                CommandOutput::Lines(lines)
            }
            "debug" => {
                let lines = if let Some((ref q, ref list)) = status.last_query_debug {
                    let mut out = vec![
                        Line::from(vec![Span::styled("Query Debug Info", theme.style_bold())]),
                        Line::from(format!("  Query: \"{q}\"")),
                        Line::from("  Retrieved chunks (raw RRF scores):"),
                    ];
                    for (num, src, score) in list {
                        let file_name = std::path::Path::new(src)
                            .file_name()
                            .map(|f| f.to_string_lossy().to_string())
                            .unwrap_or_else(|| src.clone());
                        out.push(Line::from(vec![
                            Span::raw("    "),
                            Span::styled(format!("[{num}] {file_name}"), theme.style_accent()),
                            Span::raw(format!(" · raw score: {score:.5} · full path: {src}")),
                        ]));
                    }
                    out
                } else {
                    vec![Line::from("No queries executed yet.")]
                };
                CommandOutput::Lines(lines)
            }
            "add" | "index" => {
                if cmd_arg.is_empty() {
                    CommandOutput::Lines(MessageFormatter::render_tool_error(
                        "Index",
                        "\"\"",
                        "Missing path: /add <path|url>",
                        theme,
                    ))
                } else {
                    CommandOutput::Index {
                        path: cmd_arg.to_string(),
                    }
                }
            }
            unknown => CommandOutput::Lines(vec![
                Line::from(vec![
                    Span::styled(format!("Unknown command '/{unknown}'."), theme.style_error()),
                    Span::raw(" Type "),
                    Span::styled("/help", theme.style_accent()),
                    Span::raw(" for available commands."),
                ]),
            ]),
        }
    } else if status.chunks == 0 {
        CommandOutput::Lines(MessageFormatter::render_empty_index_notice(theme))
    } else {
        CommandOutput::Question {
            query: line.to_string(),
        }
    }
}

pub async fn run(db_path: &Path) -> Result<()> {
    let theme = Theme::detect();

    if !theme.is_tty {
        return run_plain_repl(db_path).await;
    }

    setup_panic_hook();

    let conn = db::open(db_path)?;
    let (sources_cnt, chunks_cnt) = db::get_stats(&conn).unwrap_or((0, 0));
    let mut api = Api::from_env().ok();

    let chat_model = api.as_ref().map(|a| a.chat_model().to_string()).unwrap_or_else(|| {
        std::env::var("RAG_CHAT_MODEL")
            .or_else(|_| std::env::var("SARVAM_MODEL"))
            .unwrap_or_else(|_| "gpt-4o-mini".into())
    });
    let embed_model = api.as_ref().map(|a| a.embedding_model().to_string()).unwrap_or_else(|| {
        std::env::var("RAG_EMBEDDING_MODEL")
            .unwrap_or_else(|_| "text-embedding-3-small".into())
    });
    let api_base = if let Some(ref a) = api {
        if a.chat_base() == a.embed_base() {
            a.chat_base().to_string()
        } else {
            format!("{} · {}", a.chat_base(), a.embed_base())
        }
    } else {
        std::env::var("RAG_API_BASE")
            .or_else(|_| std::env::var("SARVAM_BASE_URL"))
            .unwrap_or_else(|_| "https://api.openai.com/v1".into())
    };

    let mut status = ShellStatus {
        db_path: db_path.to_path_buf(),
        sources: sources_cnt,
        chunks: chunks_cnt,
        chat_model,
        embed_model,
        api_base,
        api_ok: true, // Optimistically true until health check returns
        last_query_debug: None,
        last_retrieval: None,
    };

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let (ui_tx, mut ui_rx) = mpsc::channel::<UiEvent>(100);

    // Startup health check in background (~1.5s timeout)
    if let Some(ref api_client) = api {
        let client_clone = api_client.clone();
        let tx = ui_tx.clone();
        tokio::spawn(async move {
            let ok = client_clone.health_check().await;
            let _ = tx.send(UiEvent::HealthResult(ok)).await;
        });
    } else {
        status.api_ok = false;
    }

    let history_dir = if let Some(p) = db_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        p.to_path_buf()
    } else {
        PathBuf::from(".rag")
    };
    let mut input = InputState::new(&history_dir);
    let mut spinner = SpinnerState::new();
    let mut memory = crate::conversation::ConversationMemory::new(6);

    let mut menu_dismissed = false;
    let mut menu_selected = 0usize;
    let mut is_busy = false;

    enable_raw_mode()?;
    let stdout = std::io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let viewport_height = 11u16;
    let mut terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Inline(viewport_height),
        },
    )?;

    // Emit banner box and tips through ui::emit into scrollback at startup
    let current_width = terminal.size().map(|s| s.width).unwrap_or(80);
    let welcome = render_banner_lines(&theme, &cwd, status.sources, status.chunks, current_width as usize);
    emit(&mut terminal, welcome, current_width)?;

    let mut cancel_token: Option<CancellationToken> = None;
    let mut turns_completed = 0usize;

    loop {
        // Drain any incoming background UI events
        while let Ok(event) = ui_rx.try_recv() {
            match event {
                UiEvent::Token(_token) => {
                    spinner.add_tokens(1);
                }
                UiEvent::HealthResult(ok) => {
                    status.api_ok = ok;
                }
                UiEvent::QueryDone {
                    user_query,
                    retrieved_query,
                    result,
                } => {
                    is_busy = false;
                    cancel_token = None;
                    let current_width = terminal.size().map(|s| s.width).unwrap_or(80);
                    let content_width = MessageFormatter::compute_content_width(current_width as usize);
                    match result {
                        Ok(success) => {
                            status.last_query_debug =
                                Some((retrieved_query.clone(), success.sources.clone()));
                            status.last_retrieval = Some(success.retrieved_chunks.clone());
                            let tool_row = MessageFormatter::render_retrieve_tool_done_wrapped(
                                &user_query,
                                &retrieved_query,
                                &success.tool_detail,
                                success.is_weak,
                                success.no_strong_match,
                                &theme,
                                content_width,
                            );
                            emit(&mut terminal, tool_row, current_width)?;

                            // Blank line between tool row and assistant answer
                            emit(&mut terminal, vec![Line::default()], current_width)?;

                            let assistant_row = MessageFormatter::render_assistant_answer(
                                &success.answer,
                                &success.sources,
                                &theme,
                                content_width,
                            );
                            emit(&mut terminal, assistant_row, current_width)?;

                            // Blank line between assistant answer and footer
                            emit(&mut terminal, vec![Line::default()], current_width)?;

                            let footer = MessageFormatter::render_answer_footer(
                                success.answer_elapsed,
                                success.token_count,
                                success.is_approx_tokens,
                                &success.chat_model,
                                &theme,
                            );
                            emit(&mut terminal, footer, current_width)?;

                            memory.add_turn(user_query.clone(), success.answer);
                            turns_completed += 1;
                        }
                        Err(e) => {
                            if e.to_string().contains("Interrupted") {
                                let interrupted = MessageFormatter::render_interrupted(&theme);
                                emit(&mut terminal, interrupted, current_width)?;
                            } else {
                                let args = if user_query.trim() == retrieved_query.trim() {
                                    String::new()
                                } else {
                                    MessageFormatter::format_tool_args(&retrieved_query)
                                };
                                let err_row = MessageFormatter::render_tool_error(
                                    "Retrieve",
                                    &args,
                                    &format!("{e:#}"),
                                    &theme,
                                );
                                emit(&mut terminal, err_row, current_width)?;
                            }
                            turns_completed += 1;
                        }
                    }
                }
                UiEvent::IndexDone { path, result } => {
                    is_busy = false;
                    cancel_token = None;
                    status.refresh(&conn);
                    let current_width = terminal.size().map(|s| s.width).unwrap_or(80);
                    let content_width = MessageFormatter::compute_content_width(current_width as usize);
                    match result {
                        Ok((indexed_chunks, indexed_files, elapsed)) => {
                            let detail = MessageFormatter::format_index_done(
                                indexed_chunks,
                                indexed_files,
                                elapsed as f64,
                            );
                            let msg = MessageFormatter::render_tool_done_wrapped(
                                "Index",
                                &format!("\"{path}\""),
                                &detail,
                                &theme,
                                content_width,
                            );
                            emit(&mut terminal, msg, current_width)?;
                            turns_completed += 1;
                        }
                        Err(e) => {
                            if e.to_string().contains("Interrupted") {
                                let interrupted = MessageFormatter::render_interrupted(&theme);
                                emit(&mut terminal, interrupted, current_width)?;
                            } else {
                                let err = MessageFormatter::render_tool_error(
                                    "Index",
                                    &format!("\"{path}\""),
                                    &format!("{e:#}"),
                                    &theme,
                                );
                                emit(&mut terminal, err, current_width)?;
                            }
                        }
                    }
                }
            }
        }

        // Check if last ctrl_c has timed out (2s window)
        if let Some(last) = input.last_ctrl_c
            && last.elapsed() > Duration::from_secs(2)
        {
            input.last_ctrl_c = None;
        }

        // Update menu state
        let is_slash_command = input.text.starts_with('/') && !input.text.contains(' ');
        let menu_active = is_slash_command && !is_busy && !menu_dismissed;

        let filtered_cmds: Vec<&'static CommandDef> = if menu_active {
            CommandRegistry::filter(&input.text)
        } else {
            Vec::new()
        };

        if menu_active && !filtered_cmds.is_empty() {
            menu_selected = menu_selected.min(filtered_cmds.len() - 1);
        } else {
            menu_selected = 0;
        }

        // Draw live inline viewport
        let footer_state = FooterState {
            is_busy,
            ctrl_c_primed: input.last_ctrl_c.is_some(),
            shortcuts_open: input.shortcuts_open,
            sources_count: status.sources,
            chunks_count: status.chunks,
            api_reachable: status.api_ok,
        };

        let num_lines = input.text.split('\n').count().max(1);
        let input_box_height = ((num_lines + 2) as u16).clamp(3, 8);

        terminal.draw(|f| {
            let area = f.area();
            if area.height == 0 || area.width == 0 {
                return;
            }
            f.render_widget(Clear, area);

            let term_width = area.width as usize;
            let is_compact = term_width < 60;

            // Input box and slash menu: full width, terminal width - 2 (when >= 60)
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

            if is_busy {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(1),                // Spinner row
                        Constraint::Length(input_box_height), // Input box
                        Constraint::Length(1),                // Footer line
                        Constraint::Min(0),                   // Remaining space below
                    ])
                    .split(draw_area);

                let spinner_line = spinner.render_ratatui_line(&theme);
                f.render_widget(Paragraph::new(spinner_line), chunks[0]);
                render_input_box(f, chunks[1], &mut input, &theme);
                let footer_line = footer_state.render_ratatui_line(draw_area.width as usize, &theme);
                f.render_widget(Paragraph::new(footer_line), chunks[2]);
            } else if menu_active && !filtered_cmds.is_empty() {
                let menu_len = filtered_cmds.len().min(7) as u16;
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(input_box_height), // Input box
                        Constraint::Length(menu_len),         // Slash menu
                        Constraint::Length(1),                // Footer line
                        Constraint::Min(0),                   // Remaining space below
                    ])
                    .split(draw_area);

                render_input_box(f, chunks[0], &mut input, &theme);

                let items: Vec<ListItem> = filtered_cmds
                    .iter()
                    .enumerate()
                    .map(|(i, cmd)| {
                        let is_selected = i == menu_selected;
                        let name_span = if is_selected {
                            Span::styled(
                                format!("   {:<12}", cmd.name),
                                theme.style_accent().add_modifier(Modifier::BOLD),
                            )
                        } else {
                            Span::styled(
                                format!("   {:<12}", cmd.name),
                                Style::default().add_modifier(Modifier::BOLD),
                            )
                        };
                        let desc_span = Span::styled(cmd.description, theme.style_muted());
                        let item = ListItem::new(Line::from(vec![name_span, desc_span]));
                        if is_selected {
                            item.style(Style::default().bg(Color::Rgb(40, 40, 40)))
                        } else {
                            item
                        }
                    })
                    .collect();

                let list = List::new(items);
                f.render_widget(list, chunks[1]);

                let footer_line = footer_state.render_ratatui_line(draw_area.width as usize, &theme);
                f.render_widget(Paragraph::new(footer_line), chunks[2]);
            } else {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(input_box_height), // Input box
                        Constraint::Length(1),                // Footer line
                        Constraint::Min(0),                   // Remaining space below
                    ])
                    .split(draw_area);

                render_input_box(f, chunks[0], &mut input, &theme);

                let footer_line = footer_state.render_ratatui_line(draw_area.width as usize, &theme);
                f.render_widget(Paragraph::new(footer_line), chunks[1]);
            }
        })?;

        // Poll for user keyboard input with short timeout to keep spinner animating
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Resize(_cols, _rows) => {
                    terminal.autoresize()?;
                }
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    // Check Ctrl+C
                    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                        if is_busy {
                            if let Some(ref token) = cancel_token {
                                token.cancel();
                            }
                        } else if let Some(last) = input.last_ctrl_c {
                            if last.elapsed() < Duration::from_secs(2) {
                                break;
                            } else {
                                input.clear();
                                input.last_ctrl_c = Some(Instant::now());
                            }
                        } else {
                            input.clear();
                            input.last_ctrl_c = Some(Instant::now());
                        }
                        continue;
                    }

                    // Check Ctrl+D on empty input
                    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('d') && input.is_empty() && !is_busy {
                        break;
                    }

                    // Check other Control keys
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        if !is_busy {
                            match key.code {
                                KeyCode::Char('a') => {
                                    input.move_cursor_home();
                                }
                                KeyCode::Char('e') => {
                                    input.move_cursor_end();
                                }
                                KeyCode::Char('u') => {
                                    menu_dismissed = false;
                                    input.delete_to_start();
                                }
                                KeyCode::Char('k') => {
                                    menu_dismissed = false;
                                    input.delete_to_end();
                                }
                                KeyCode::Char('w') => {
                                    menu_dismissed = false;
                                    input.delete_word_backward();
                                }
                                KeyCode::Char('d') => {
                                    menu_dismissed = false;
                                    input.delete_forward();
                                }
                                KeyCode::Char('l') => {
                                    execute!(
                                        std::io::stdout(),
                                        terminal::Clear(ClearType::All),
                                        terminal::Clear(ClearType::Purge),
                                        cursor::MoveTo(0, 0)
                                    )?;
                                    drop(terminal);
                                    let backend = CrosstermBackend::new(std::io::stdout());
                                    terminal = Terminal::with_options(
                                        backend,
                                        TerminalOptions {
                                            viewport: Viewport::Inline(viewport_height),
                                        },
                                    )?;
                                    status.refresh(&conn);
                                    let current_w = terminal.size().map(|s| s.width).unwrap_or(80);
                                    let w_lines = render_banner_lines(&theme, &cwd, status.sources, status.chunks, current_w as usize);
                                    emit(&mut terminal, w_lines, current_w)?;
                                    input.clear();
                                }
                                _ => {}
                            }
                        }
                        continue;
                    }

                    // Check Esc
                    if key.code == KeyCode::Esc {
                        if is_busy {
                            if let Some(ref token) = cancel_token {
                                token.cancel();
                            }
                        } else if menu_active {
                            menu_dismissed = true;
                        } else if input.shortcuts_open {
                            input.shortcuts_open = false;
                        }
                        continue;
                    }

                    // While busy, ignore other inputs
                    if is_busy {
                        continue;
                    }

                    // Menu navigation
                    if menu_active && !filtered_cmds.is_empty() {
                        match key.code {
                            KeyCode::Up => {
                                if menu_selected == 0 {
                                    menu_selected = filtered_cmds.len() - 1;
                                } else {
                                    menu_selected -= 1;
                                }
                                continue;
                            }
                            KeyCode::Down => {
                                menu_selected = (menu_selected + 1) % filtered_cmds.len();
                                continue;
                            }
                            KeyCode::Tab => {
                                let cmd = filtered_cmds[menu_selected];
                                if cmd.args.is_empty() {
                                    input.text = cmd.name.to_string();
                                } else {
                                    input.text = format!("{} ", cmd.name);
                                }
                                input.cursor = input.text.chars().count();
                                menu_dismissed = true;
                                continue;
                            }
                            KeyCode::Enter => {
                                let cmd = filtered_cmds[menu_selected];
                                if cmd.name == "/add" {
                                    input.text = format!("{} ", cmd.name);
                                    input.cursor = input.text.chars().count();
                                    menu_dismissed = true;
                                    continue;
                                } else {
                                    input.text = cmd.name.to_string();
                                    input.cursor = input.text.chars().count();
                                    menu_dismissed = true;
                                    // Submit immediately
                                }
                            }
                            _ => {}
                        }
                    }

                    match key.code {
                        KeyCode::Enter => {
                            if key.modifiers.contains(KeyModifiers::SHIFT) || key.modifiers.contains(KeyModifiers::ALT) {
                                input.insert_newline();
                            } else if let Some(submitted) = input.submit() {
                                menu_dismissed = false;

                                // 1. Spacing: one blank line between turns
                                if turns_completed > 0 {
                                    emit(&mut terminal, vec![Line::default()], current_width)?;
                                }

                                // 2. User message is always emitted first into scrollback
                                let user_lines = MessageFormatter::render_user_message(&submitted, &theme);
                                emit(&mut terminal, user_lines, current_width)?;

                                // 3. Execute command via decoupled handler
                                match execute_command(&submitted, &mut status, &mut api, &conn, &theme) {
                                    CommandOutput::Lines(lines) => {
                                        emit(&mut terminal, lines, current_width)?;
                                        turns_completed += 1;
                                    }
                                    CommandOutput::Clear => {
                                        memory.clear();
                                        execute!(
                                            std::io::stdout(),
                                            terminal::Clear(ClearType::All),
                                            terminal::Clear(ClearType::Purge),
                                            cursor::MoveTo(0, 0)
                                        )?;
                                        drop(terminal);
                                        let backend = CrosstermBackend::new(std::io::stdout());
                                        terminal = Terminal::with_options(
                                            backend,
                                            TerminalOptions {
                                                viewport: Viewport::Inline(viewport_height),
                                            },
                                        )?;
                                        status.refresh(&conn);
                                        let current_w = terminal.size().map(|s| s.width).unwrap_or(80);
                                        let w_lines = render_banner_lines(&theme, &cwd, status.sources, status.chunks, current_w as usize);
                                        emit(&mut terminal, w_lines, current_w)?;
                                        input.clear();
                                    }
                                    CommandOutput::New => {
                                        memory.clear();
                                        let note = vec![Line::from(vec![
                                            Span::raw("  "),
                                            Span::styled("Started fresh conversation (history cleared)", theme.style_muted()),
                                        ])];
                                        emit(&mut terminal, note, current_width)?;
                                        turns_completed += 1;
                                    }
                                    CommandOutput::Exit => {
                                        break;
                                    }
                                    CommandOutput::Index { path } => {
                                        // Spacing: one blank line between echoed user message and first tool row
                                        emit(&mut terminal, vec![Line::default()], current_width)?;

                                        let token = CancellationToken::new();
                                        cancel_token = Some(token.clone());
                                        is_busy = true;
                                        spinner = SpinnerState::with_verb("Indexing");

                                        let path_str = path.clone();
                                        let api_opt = api.clone();
                                        let db_file = status.db_path.clone();
                                        let tx = ui_tx.clone();

                                        tokio::spawn(async move {
                                            let res = handle_indexing(path, db_file, api_opt, token).await;
                                            let _ = tx.send(UiEvent::IndexDone { path: path_str, result: res }).await;
                                        });
                                    }
                                    CommandOutput::Question { query } => {
                                        if let Some(ref api_client) = api {
                                            // Spacing: one blank line between echoed user message and first tool row
                                            emit(&mut terminal, vec![Line::default()], current_width)?;

                                            let token = CancellationToken::new();
                                            cancel_token = Some(token.clone());
                                            is_busy = true;
                                            spinner = SpinnerState::new();

                                            let query_str = query.clone();
                                            let history_turns = memory.turns().to_vec();
                                            let api_clone = api_client.clone();
                                            let db_file = status.db_path.clone();
                                            let tx = ui_tx.clone();

                                            tokio::spawn(async move {
                                                let (res, retrieved_q) = match handle_query(
                                                    query_str.clone(),
                                                    history_turns,
                                                    db_file,
                                                    api_clone,
                                                    token,
                                                    tx.clone(),
                                                )
                                                .await
                                                {
                                                    Ok((success, rq)) => (Ok(success), rq),
                                                    Err(e) => (Err(e), query_str.clone()),
                                                };
                                                let _ = tx
                                                    .send(UiEvent::QueryDone {
                                                        user_query: query_str,
                                                        retrieved_query: retrieved_q,
                                                        result: res,
                                                    })
                                                    .await;
                                            });
                                        } else {
                                            let err_lines = vec![Line::from("API not configured. Check your API keys and endpoints in /status.")];
                                            emit(&mut terminal, err_lines, current_width)?;
                                            turns_completed += 1;
                                        }
                                    }
                                }
                            }
                        }
                        KeyCode::Up => {
                            if !input.move_cursor_up_line() {
                                input.history_up();
                            }
                        }
                        KeyCode::Down => {
                            if !input.move_cursor_down_line() {
                                input.history_down();
                            }
                        }
                        KeyCode::Left => {
                            input.move_cursor_left();
                        }
                        KeyCode::Right => {
                            input.move_cursor_right();
                        }
                        KeyCode::Home => {
                            input.move_cursor_home();
                        }
                        KeyCode::End => {
                            input.move_cursor_end();
                        }
                        KeyCode::Backspace => {
                            menu_dismissed = false;
                            input.delete_backspace();
                        }
                        KeyCode::Delete => {
                            menu_dismissed = false;
                            input.delete_forward();
                        }
                        KeyCode::Char(c)
                            if !key.modifiers.contains(KeyModifiers::CONTROL)
                                && !key.modifiers.contains(KeyModifiers::ALT) =>
                        {
                            menu_dismissed = false;
                            input.insert_char(c);
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    execute!(std::io::stdout(), cursor::Show)?;

    Ok(())
}

fn render_input_box(f: &mut ratatui::Frame, area: Rect, input: &mut InputState, theme: &Theme) {
    let is_compact = area.width < 60;
    let block = if is_compact {
        Block::default().borders(Borders::NONE)
    } else {
        Block::default()
            .borders(Borders::ALL)
            .border_type(if theme.use_unicode {
                BorderType::Rounded
            } else {
                BorderType::Plain
            })
            .border_style(theme.style_muted())
    };

    let inner = block.inner(area);
    f.render_widget(block, area);

    if input.is_empty() {
        let placeholder = input.current_placeholder();
        let prompt_span = Span::styled(" > ", theme.style_muted());
        let ph_span = Span::styled(placeholder, theme.style_muted());
        f.render_widget(Paragraph::new(Line::from(vec![prompt_span, ph_span])), inner);
        f.set_cursor_position(Position {
            x: inner.x + 3,
            y: inner.y,
        });
    } else {
        let lines: Vec<&str> = input.text.split('\n').collect();
        let mut text_lines = Vec::new();
        let mut cursor_row = 0;
        let mut cursor_col = 0;
        let mut current_offset = 0;

        for (i, line_text) in lines.iter().enumerate() {
            let line_len = line_text.chars().count();
            if input.cursor >= current_offset && input.cursor <= current_offset + line_len {
                cursor_row = i;
                cursor_col = input.cursor - current_offset;
            }
            current_offset += line_len + 1; // +1 for '\n'

            let prefix = if i == 0 {
                Span::styled(" > ", theme.style_muted())
            } else {
                Span::styled("   ", theme.style_muted())
            };
            text_lines.push(Line::from(vec![prefix, Span::raw(*line_text)]));
        }

        f.render_widget(Paragraph::new(text_lines), inner);

        let max_x = inner.x + inner.width.saturating_sub(1);
        let cur_x = (inner.x + 3 + cursor_col as u16).min(max_x);
        let max_y = inner.y + inner.height.saturating_sub(1);
        let cur_y = (inner.y + cursor_row as u16).min(max_y);

        f.set_cursor_position(Position {
            x: cur_x,
            y: cur_y,
        });
    }
}

fn strip_html_tags(html: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
            text.push(' ');
        } else if !in_tag {
            text.push(c);
        }
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

async fn handle_indexing(
    target_path_str: String,
    db_file: PathBuf,
    api_opt: Option<Api>,
    token: CancellationToken,
) -> Result<(usize, usize, f32)> {
    let start = Instant::now();
    let api = api_opt.context("API not configured for embeddings")?;

    if target_path_str.starts_with("http://") || target_path_str.starts_with("https://") {
        let resp = reqwest::get(&target_path_str).await.context("Failed to fetch URL")?;
        let body = resp.text().await.context("Failed to read URL response")?;
        let text = strip_html_tags(&body);
        let pieces = chunker::chunk(&text, 420, 70);
        if pieces.is_empty() {
            return Ok((0, 1, start.elapsed().as_secs_f32()));
        }
        let mut items = Vec::with_capacity(pieces.len());
        for batch in pieces.chunks(64) {
            if token.is_cancelled() {
                anyhow::bail!("Interrupted by user");
            }
            let input = batch.to_vec();
            let vectors = api.embed(&input).await?;
            for (t, v) in batch.iter().zip(vectors) {
                items.push(db::Chunk {
                    id: uuid::Uuid::new_v4().to_string(),
                    source: target_path_str.clone(),
                    text: t.clone(),
                    vector: v,
                });
            }
        }
        let count = items.len();
        let mut conn = db::open(&db_file)?;
        db::replace_source(&mut conn, &target_path_str, &items)?;
        return Ok((count, 1, start.elapsed().as_secs_f32()));
    }

    let target_path = PathBuf::from(&target_path_str);
    let files = sources::collect(&target_path)?;
    if files.is_empty() {
        return Ok((0, 0, start.elapsed().as_secs_f32()));
    }

    let mut total_chunks = 0;
    let total_files = files.len();
    let mut conn = db::open(&db_file)?;

    for file in &files {
        if token.is_cancelled() {
            anyhow::bail!("Interrupted by user");
        }
        let source = std::fs::canonicalize(file)
            .unwrap_or_else(|_| file.clone())
            .to_string_lossy()
            .to_string();
        let text = sources::read(file)?;
        let pieces = chunker::chunk(&text, 420, 70);
        if pieces.is_empty() {
            db::replace_source(&mut conn, &source, &[])?;
            continue;
        }

        let mut items = Vec::with_capacity(pieces.len());
        for batch in pieces.chunks(64) {
            if token.is_cancelled() {
                anyhow::bail!("Interrupted by user");
            }
            let input = batch.to_vec();
            let vectors = api.embed(&input).await?;
            for (t, v) in batch.iter().zip(vectors) {
                items.push(db::Chunk {
                    id: uuid::Uuid::new_v4().to_string(),
                    source: source.clone(),
                    text: t.clone(),
                    vector: v,
                });
            }
        }
        let count = items.len();
        db::replace_source(&mut conn, &source, &items)?;
        total_chunks += count;
    }

    Ok((total_chunks, total_files, start.elapsed().as_secs_f32()))
}

async fn handle_query(
    question: String,
    history: Vec<crate::conversation::Turn>,
    db_file: PathBuf,
    api: Api,
    token: CancellationToken,
    ui_channel: mpsc::Sender<UiEvent>,
) -> Result<(QuerySuccess, String)> {
    if token.is_cancelled() {
        anyhow::bail!("Interrupted by user");
    }

    // Rewrite query if there is prior conversation history
    let retrieved_query = if history.is_empty() {
        question.clone()
    } else {
        api.rewrite_query(&question, &history).await?
    };

    if token.is_cancelled() {
        anyhow::bail!("Interrupted by user");
    }

    let conn = db::open(&db_file)?;
    let chunks = db::all(&conn)?;
    if chunks.is_empty() {
        return Ok((
            QuerySuccess {
                answer: "The index is empty. Add sources first with /add <path|url>".to_string(),
                tool_detail: "0 chunks retrieved".to_string(),
                sources: Vec::new(),
                retrieved_chunks: Vec::new(),
                is_weak: true,
                no_strong_match: true,
                answer_elapsed: 0.0,
                token_count: 0,
                is_approx_tokens: false,
                chat_model: api.chat_model().to_string(),
            },
            retrieved_query,
        ));
    }

    // Embed query
    let query_embs = api.embed(std::slice::from_ref(&retrieved_query)).await?;
    let query_emb = query_embs.into_iter().next().context("Failed to get query embedding")?;

    if token.is_cancelled() {
        anyhow::bail!("Interrupted by user");
    }

    db::ensure_compatible(&chunks, &query_emb)?;
    let retrieve_start = Instant::now();
    let initial_hits = retrieve::hybrid(chunks, &retrieved_query, &query_emb, 5);
    let retrieve_elapsed = retrieve_start.elapsed().as_secs_f64();

    if initial_hits.is_empty() {
        return Ok((
            QuerySuccess {
                answer: "I could not find any relevant information in the indexed documents.".to_string(),
                tool_detail: MessageFormatter::format_retrieve_detail(0, 0, retrieve_elapsed, 0),
                sources: Vec::new(),
                retrieved_chunks: Vec::new(),
                is_weak: true,
                no_strong_match: true,
                answer_elapsed: 0.0,
                token_count: 0,
                is_approx_tokens: false,
                chat_model: api.chat_model().to_string(),
            },
            retrieved_query,
        ));
    }

    let filter_config = retrieve::RetrievalFilterConfig::default();
    let filtered = retrieve::filter_by_relevance(initial_hits, &filter_config);
    let hits = filtered.hits;
    let filtered_count = filtered.filtered_count;
    let no_strong_match = filtered.no_strong_match;

    let weak_config = retrieve::WeakRetrievalConfig::default();
    let is_weak = retrieve::is_weak_retrieval(&hits, &weak_config);

    let mut unique_sources = std::collections::HashSet::new();
    for h in &hits {
        unique_sources.insert(h.chunk.source.clone());
    }

    let detail = MessageFormatter::format_retrieve_detail(
        hits.len(),
        unique_sources.len(),
        retrieve_elapsed,
        filtered_count,
    );

    let contexts: Vec<(String, String)> = hits
        .iter()
        .map(|h| (h.chunk.source.clone(), h.chunk.text.chars().take(5000).collect()))
        .collect();

    let mut sources_list = Vec::new();
    let mut retrieved_chunks = Vec::new();
    for (i, h) in hits.iter().enumerate() {
        sources_list.push((i + 1, h.chunk.source.clone(), h.score));
        let file_name = std::path::Path::new(&h.chunk.source)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| h.chunk.source.clone());
        retrieved_chunks.push(crate::ui::messages::RetrievedChunk {
            rank: i + 1,
            file: file_name,
            full_path: h.chunk.source.clone(),
            page: None,
            text: h.chunk.text.clone(),
            score: h.score,
            is_cited: false,
        });
    }

    let tx = ui_channel.clone();
    let answer_start = Instant::now();
    let answer_res = api
        .answer_stream_with_history(
            &question,
            &contexts,
            &history,
            token.clone(),
            move |token_str| {
                let _ = tx.try_send(UiEvent::Token(token_str.to_string()));
            },
        )
        .await?;

    let answer = answer_res.text;
    let cited_ranks = MessageFormatter::extract_citations(&answer);
    for chunk in &mut retrieved_chunks {
        chunk.is_cited = cited_ranks.contains(&chunk.rank);
    }

    let answer_elapsed = answer_start.elapsed().as_secs_f64();
    let (token_count, is_approx_tokens) = match answer_res.completion_tokens {
        Some(tokens) => (tokens, false),
        None => (crate::ui::messages::MessageFormatter::estimate_tokens(&answer), true),
    };
    let chat_model = api.chat_model().to_string();

    Ok((
        QuerySuccess {
            answer,
            tool_detail: detail,
            sources: sources_list,
            retrieved_chunks,
            is_weak,
            no_strong_match,
            answer_elapsed,
            token_count,
            is_approx_tokens,
            chat_model,
        },
        retrieved_query,
    ))
}

pub async fn run_plain_repl(db_path: &Path) -> Result<()> {
    use std::io::BufRead;

    let conn = db::open(db_path)?;
    let (sources_cnt, chunks_cnt) = db::get_stats(&conn).unwrap_or((0, 0));
    let mut out = std::io::stdout().lock();
    writeln!(out, "Sift (Non-TTY Mode)")?;
    writeln!(out, "Database: {} ({} sources, {} chunks)", db_path.display(), sources_cnt, chunks_cnt)?;
    writeln!(out, "Type a question or /help for commands, /exit to quit.\n")?;
    out.flush()?;

    let stdin = std::io::stdin();
    let lines = stdin.lock().lines();

    let mut plain_memory = crate::conversation::ConversationMemory::new(6);
    for line_res in lines {
        let line = line_res?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed == "/exit" || trimmed == "/quit" {
            writeln!(out, "Goodbye!")?;
            break;
        }
        if trimmed == "/help" {
            writeln!(out, "Sift Help")?;
            for cmd in CommandRegistry::all() {
                writeln!(out, "  {:<12} {}", cmd.name, cmd.description)?;
            }
            continue;
        }
        if trimmed == "/status" {
            writeln!(out, "Sift Status")?;
            writeln!(out, "Database:     {}", db_path.display())?;
            writeln!(out, "Index:        {sources_cnt} sources · {chunks_cnt} chunks")?;
            writeln!(out, "Version:      Sift 0.1.0")?;
            continue;
        }
        if trimmed == "/sources" {
            let mut stmt = conn.prepare("SELECT source, COUNT(*) FROM chunks GROUP BY source ORDER BY source")?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
            for r in rows {
                let (src, count) = r?;
                writeln!(out, "  {src} ({count} chunks)")?;
            }
            continue;
        }
        if trimmed.starts_with("/model") {
            let api = Api::from_env().ok();
            let chat_m = api.as_ref().map(|a| a.chat_model().to_string()).unwrap_or_else(|| "gpt-4o-mini".into());
            let emb_m = api.as_ref().map(|a| a.embedding_model().to_string()).unwrap_or_else(|| "text-embedding-3-small".into());
            writeln!(out, "Chat Model:  {chat_m}")?;
            writeln!(out, "Embed Model: {emb_m}")?;
            continue;
        }
        if let Some(arg) = trimmed.strip_prefix("/add ").or_else(|| trimmed.strip_prefix("/index ")) {
            let api = Api::from_env().ok();
            match handle_indexing(arg.trim().to_string(), db_path.to_path_buf(), api, CancellationToken::new()).await {
                Ok((chunks, files, elapsed)) => writeln!(out, "Indexed {chunks} chunks from {files} sources in {elapsed:.1}s")?,
                Err(e) => writeln!(out, "Error indexing: {e}")?,
            }
            continue;
        }
        if !trimmed.starts_with('/') {
            writeln!(out, "> {trimmed}")?;
            let (_sc, cc) = db::get_stats(&conn).unwrap_or((0, 0));
            if cc == 0 {
                writeln!(out, "Index is empty. Add sources first with /add <path|url>")?;
            } else if let Ok(api) = Api::from_env() {
                let (tx, _) = mpsc::channel(100);
                match handle_query(
                    trimmed.to_string(),
                    plain_memory.turns().to_vec(),
                    db_path.to_path_buf(),
                    api,
                    CancellationToken::new(),
                    tx,
                )
                .await
                {
                    Ok((success, _)) => {
                        writeln!(out, "\n{}\n", success.answer)?;
                        plain_memory.add_turn(trimmed, success.answer);
                    }
                    Err(e) => writeln!(out, "Error: {e}")?,
                }
            } else {
                writeln!(out, "API not configured.")?;
            }
            continue;
        }
        writeln!(out, "> {trimmed}")?;
    }

    Ok(())
}
