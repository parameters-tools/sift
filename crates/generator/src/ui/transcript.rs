use std::path::PathBuf;
use std::time::{Duration, Instant};

use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use super::{
    messages::MessageFormatter,
    theme::Theme,
};
use crate::agent::types::AgentStep;

#[derive(Debug, Clone, PartialEq)]
pub struct AnswerFooter {
    pub elapsed: f64,
    pub token_count: usize,
    pub is_approx: bool,
    pub model: String,
}

impl AnswerFooter {
    pub fn new(elapsed: f64, token_count: usize, is_approx: bool, model: impl Into<String>) -> Self {
        Self {
            elapsed,
            token_count,
            is_approx,
            model: model.into(),
        }
    }
}

impl From<(f64, usize, bool, String)> for AnswerFooter {
    fn from((elapsed, token_count, is_approx, model): (f64, usize, bool, String)) -> Self {
        Self {
            elapsed,
            token_count,
            is_approx,
            model,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Banner {
        cwd: PathBuf,
        sources: i64,
        chunks: i64,
        chat_model: String,
        api_ok: bool,
        top_sources: Vec<(String, usize)>,
        height: usize,
    },
    UserMessage(String),
    ToolRow {
        tool_name: String,
        user_query: String,
        args: String,
        detail: String,
        is_weak: bool,
        no_strong_match: bool,
        is_error: bool,
        agent_steps: Vec<AgentStep>,
    },
    Answer {
        markdown: String,
        sources: Vec<(usize, String, f32)>,
        footer: AnswerFooter,
    },
    CommandOutput(Vec<Line<'static>>),
    Notice(String),
}

impl Entry {
    pub fn banner(cwd: PathBuf, sources: i64, chunks: i64) -> Self {
        Self::Banner {
            cwd,
            sources,
            chunks,
            chat_model: "deepseekv4.1-flash-1017".to_string(),
            api_ok: true,
            top_sources: Vec::new(),
            height: 30,
        }
    }

    pub fn banner_full(
        cwd: PathBuf,
        sources: i64,
        chunks: i64,
        chat_model: impl Into<String>,
        api_ok: bool,
        top_sources: Vec<(String, usize)>,
        height: usize,
    ) -> Self {
        Self::Banner {
            cwd,
            sources,
            chunks,
            chat_model: chat_model.into(),
            api_ok,
            top_sources,
            height,
        }
    }

    pub fn user_message(text: impl Into<String>) -> Self {
        Self::UserMessage(text.into())
    }

    pub fn tool_retrieve(
        user_query: impl Into<String>,
        retrieved_query: impl Into<String>,
        detail: impl Into<String>,
        is_weak: bool,
        no_strong_match: bool,
    ) -> Self {
        Self::ToolRow {
            tool_name: "Retrieve".to_string(),
            user_query: user_query.into(),
            args: retrieved_query.into(),
            detail: detail.into(),
            is_weak,
            no_strong_match,
            is_error: false,
            agent_steps: Vec::new(),
        }
    }

    pub fn tool_agent_steps(
        user_query: impl Into<String>,
        retrieved_query: impl Into<String>,
        steps: Vec<AgentStep>,
    ) -> Self {
        Self::ToolRow {
            tool_name: "Agent".to_string(),
            user_query: user_query.into(),
            args: retrieved_query.into(),
            detail: String::new(),
            is_weak: false,
            no_strong_match: false,
            is_error: false,
            agent_steps: steps,
        }
    }

    pub fn tool_index(path: impl Into<String>, detail: impl Into<String>) -> Self {
        let p = path.into();
        Self::ToolRow {
            tool_name: "Index".to_string(),
            user_query: p.clone(),
            args: p,
            detail: detail.into(),
            is_weak: false,
            no_strong_match: false,
            is_error: false,
            agent_steps: Vec::new(),
        }
    }

    pub fn tool_error(
        tool_name: impl Into<String>,
        args: impl Into<String>,
        error: impl Into<String>,
    ) -> Self {
        let a = args.into();
        Self::ToolRow {
            tool_name: tool_name.into(),
            user_query: a.clone(),
            args: a,
            detail: error.into(),
            is_weak: false,
            no_strong_match: false,
            is_error: true,
            agent_steps: Vec::new(),
        }
    }

    pub fn answer(
        markdown: impl Into<String>,
        sources: Vec<(usize, String, f32)>,
        footer: impl Into<AnswerFooter>,
    ) -> Self {
        Self::Answer {
            markdown: markdown.into(),
            sources,
            footer: footer.into(),
        }
    }

    pub fn command_output(lines: Vec<Line<'static>>) -> Self {
        Self::CommandOutput(lines)
    }

    pub fn notice(text: impl Into<String>) -> Self {
        Self::Notice(text.into())
    }

    pub fn render(&self, width: usize) -> Vec<Line<'static>> {
        self.render_with_theme(width, &Theme::detect())
    }

    pub fn render_with_theme(&self, width: usize, theme: &Theme) -> Vec<Line<'static>> {
        let safe_width = width.max(1);
        let raw_lines = match self {
            Entry::Banner {
                cwd,
                sources,
                chunks,
                chat_model,
                api_ok,
                top_sources,
                height,
            } => {
                super::banner::render_banner_lines_full(
                    theme,
                    cwd,
                    *sources,
                    *chunks,
                    chat_model,
                    *api_ok,
                    top_sources,
                    safe_width,
                    *height,
                )
            }
            Entry::UserMessage(query) => {
                let mut lines = Vec::new();
                lines.push(Line::default()); // Blank line before user prompt
                let content_width = if safe_width < 60 {
                    safe_width
                } else {
                    MessageFormatter::compute_content_width(safe_width)
                };
                let user_lines = MessageFormatter::render_user_message_wrapped(query, theme, content_width);
                lines.extend(user_lines);
                lines
            }
            Entry::ToolRow {
                tool_name,
                user_query,
                args,
                detail,
                is_weak,
                no_strong_match,
                is_error,
                agent_steps,
            } => {
                let content_width = if safe_width < 60 {
                    safe_width
                } else {
                    MessageFormatter::compute_content_width(safe_width)
                };
                let tool_lines = if *is_error {
                    MessageFormatter::render_tool_error(tool_name, args, detail, theme)
                } else if !agent_steps.is_empty() {
                    MessageFormatter::render_agent_steps(user_query, args, agent_steps, theme, content_width)
                } else if tool_name == "Index" {
                    let formatted_path = if args.starts_with('"') {
                        args.clone()
                    } else {
                        format!("\"{args}\"")
                    };
                    MessageFormatter::render_tool_done_wrapped("Index", &formatted_path, detail, theme, content_width)
                } else {
                    MessageFormatter::render_retrieve_tool_done_wrapped(
                        user_query,
                        args,
                        detail,
                        *is_weak,
                        *no_strong_match,
                        theme,
                        content_width,
                    )
                };
                let mut lines = Vec::new();
                lines.push(Line::default()); // Blank line before tool row
                lines.extend(tool_lines);
                lines
            }
            Entry::Answer {
                markdown,
                sources,
                footer,
            } => {
                let content_width = if safe_width < 60 {
                    safe_width
                } else {
                    MessageFormatter::compute_content_width(safe_width)
                };
                let mut lines = Vec::new();
                lines.push(Line::default()); // Blank line before answer
                lines.extend(MessageFormatter::render_assistant_answer(
                    markdown,
                    sources,
                    theme,
                    content_width,
                ));
                lines.push(Line::default()); // Blank line between answer and footer
                lines.extend(MessageFormatter::render_answer_footer(
                    footer.elapsed,
                    footer.token_count,
                    footer.is_approx,
                    &footer.model,
                    theme,
                ));
                lines
            }
            Entry::CommandOutput(out_lines) => {
                let mut lines = Vec::new();
                lines.push(Line::default());
                lines.extend(out_lines.clone());
                lines
            }
            Entry::Notice(text) => {
                let mut lines = Vec::new();
                lines.push(Line::default());
                if text == "Interrupted" {
                    lines.extend(MessageFormatter::render_interrupted(theme));
                } else {
                    lines.push(Line::from(vec![
                        Span::styled("ℹ ", theme.style_accent()),
                        Span::raw(text.clone()),
                    ]));
                }
                lines
            }
        };

        // Enforce that EVERY line produces display width <= width
        let mut final_lines = Vec::new();
        for line in raw_lines {
            if line.width() <= safe_width {
                final_lines.push(line);
            } else {
                final_lines.extend(wrap_line_to_width(line, safe_width));
            }
        }
        final_lines
    }
}

pub fn wrap_line_to_width(line: Line<'static>, max_width: usize) -> Vec<Line<'static>> {
    if max_width == 0 {
        return vec![line];
    }
    if line.width() <= max_width {
        return vec![line];
    }

    let mut result = Vec::new();
    let mut current_line_spans: Vec<Span<'static>> = Vec::new();
    let mut current_width = 0;

    for span in line.spans {
        let style = span.style;
        let words: Vec<&str> = span.content.split_inclusive(' ').collect();

        for chunk in words {
            let chunk_width = UnicodeWidthStr::width(chunk);
            if current_width + chunk_width <= max_width {
                current_line_spans.push(Span::styled(chunk.to_string(), style));
                current_width += chunk_width;
            } else if chunk_width <= max_width {
                if !current_line_spans.is_empty() {
                    result.push(Line::from(std::mem::take(&mut current_line_spans)));
                }
                let trimmed = chunk.trim_start_matches(' ');
                let trimmed_w = UnicodeWidthStr::width(trimmed);
                current_line_spans.push(Span::styled(trimmed.to_string(), style));
                current_width = trimmed_w;
            } else {
                // Single chunk wider than max_width: break at character boundaries
                for ch in chunk.chars() {
                    let ch_w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
                    if current_width + ch_w > max_width && !current_line_spans.is_empty() {
                        result.push(Line::from(std::mem::take(&mut current_line_spans)));
                        current_width = 0;
                    }
                    current_line_spans.push(Span::styled(ch.to_string(), style));
                    current_width += ch_w;
                }
            }
        }
    }
    if !current_line_spans.is_empty() {
        result.push(Line::from(current_line_spans));
    }
    if result.is_empty() {
        result.push(Line::default());
    }
    result
}

#[derive(Debug, Clone)]
pub struct Transcript {
    entries: Vec<Entry>,
    max_entries: usize,
}

impl Default for Transcript {
    fn default() -> Self {
        Self::new()
    }
}

impl Transcript {
    pub const DEFAULT_MAX_ENTRIES: usize = 200;

    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            max_entries: Self::DEFAULT_MAX_ENTRIES,
        }
    }

    pub fn with_max_entries(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            max_entries,
        }
    }

    pub fn push(&mut self, entry: Entry) {
        if self.entries.len() >= self.max_entries {
            self.entries.remove(0);
        }
        self.entries.push(entry);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn update_banner_size(&mut self, height: usize) {
        for entry in &mut self.entries {
            if let Entry::Banner { height: h, .. } = entry {
                *h = height;
            }
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn render_all(&self, width: usize, theme: &Theme) -> Vec<Line<'static>> {
        let mut all_lines = Vec::new();
        for entry in &self.entries {
            all_lines.extend(entry.render_with_theme(width, theme));
        }
        all_lines
    }
}

#[derive(Debug, Clone)]
pub struct ResizeDebouncer {
    debounce_duration: Duration,
    pending: Option<(u16, u16, Instant)>,
}

impl ResizeDebouncer {
    pub fn new(debounce_duration: Duration) -> Self {
        Self {
            debounce_duration,
            pending: None,
        }
    }

    pub fn on_resize(&mut self, cols: u16, rows: u16, now: Instant) {
        self.pending = Some((cols, rows, now + self.debounce_duration));
    }

    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub fn pending_size(&self) -> Option<(u16, u16)> {
        self.pending.map(|(c, r, _)| (c, r))
    }

    pub fn check_settled(&mut self, now: Instant) -> Option<(u16, u16)> {
        match self.pending {
            Some((cols, rows, deadline)) if now >= deadline => {
                self.pending = None;
                Some((cols, rows))
            }
            _ => None,
        }
    }

    pub fn time_remaining(&self, now: Instant) -> Option<Duration> {
        self.pending.map(|(_, _, deadline)| {
            if deadline > now {
                deadline - now
            } else {
                Duration::ZERO
            }
        })
    }
}

#[derive(Debug, Clone)]
pub struct UiConfig {
    pub clear_scrollback_on_resize: bool,
    pub resize_debounce_ms: u64,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            clear_scrollback_on_resize: false,
            resize_debounce_ms: 120,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_burst_of_30_resize_events_within_100ms_produces_exactly_one_redraw() {
        let mut debouncer = ResizeDebouncer::new(Duration::from_millis(120));
        let start = Instant::now();
        let mut redraw_count = 0;

        // 30 resize events within 100ms (every 3ms over 90ms)
        for i in 0..30 {
            let t = start + Duration::from_millis(i * 3);
            debouncer.on_resize(80 + (i as u16), 24, t);
            if debouncer.check_settled(t).is_some() {
                redraw_count += 1;
            }
        }
        assert_eq!(redraw_count, 0, "No redraw should occur during resize burst");

        // At 100ms (only 10ms after last event at 90ms): still debouncing
        let t_burst_end = start + Duration::from_millis(100);
        if debouncer.check_settled(t_burst_end).is_some() {
            redraw_count += 1;
        }
        assert_eq!(redraw_count, 0, "No redraw before debounce expires");

        // At 215ms (125ms after last event at 90ms): debounce settled!
        let t_settled = start + Duration::from_millis(215);
        if let Some((cols, rows)) = debouncer.check_settled(t_settled) {
            redraw_count += 1;
            assert_eq!(cols, 109); // 80 + 29
            assert_eq!(rows, 24);
        }
        assert_eq!(redraw_count, 1, "Exactly one redraw must occur after settled");

        // Subsequent check: no extra redraws
        let t_later = start + Duration::from_millis(300);
        if debouncer.check_settled(t_later).is_some() {
            redraw_count += 1;
        }
        assert_eq!(redraw_count, 1, "No further redraws without new events");
    }

    #[test]
    fn test_entry_render_bounds_and_determinism() {
        let theme = Theme::detect();
        let entries: Vec<Entry> = vec![
            Entry::banner(PathBuf::from("/home/kevin/advanced-rag"), 3, 76),
            Entry::user_message("What is Retrieval-Augmented Generation, and how does hybrid reciprocal rank fusion combine dense and sparse search rankings?"),
            Entry::tool_retrieve(
                "What is RAG?",
                "Retrieval-Augmented Generation hybrid reciprocal rank fusion",
                "3 chunks · 42ms",
                false,
                false,
            ),
            Entry::tool_agent_steps(
                "Compare RAG and Fine-tuning",
                "RAG vs fine-tuning",
                vec![
                    crate::agent::types::AgentStep::Tool {
                        name: "Retrieve".into(),
                        args: "RAG definition".into(),
                        detail: "Found 2 chunks".into(),
                        elapsed: 0.05,
                        is_weak: false,
                        no_strong_match: false,
                    },
                ],
            ),
            Entry::tool_index("crates/generator/src/main.rs", "42 chunks in 3 files (0.15s)"),
            Entry::tool_error("Retrieve", "unreachable query", "Connection timed out after 5.0s"),
            Entry::answer(
                "Retrieval-Augmented Generation (RAG) is a technique that supplements LLM prompts with relevant document chunks [1].\n\nIt ensures answers are factual and cited [2].",
                vec![
                    (1, "docs/rag.md".into(), 0.95),
                    (2, "docs/architecture.md".into(), 0.88),
                ],
                AnswerFooter::new(0.42, 128, false, "gpt-4o"),
            ),
            Entry::command_output(vec![
                Line::from("Sift 0.1.0"),
                Line::from("Database: /home/kevin/advanced-rag/.rag/index.sqlite"),
                Line::from("Sources: 3 · Chunks: 76"),
            ]),
            Entry::notice("Interrupted"),
            Entry::notice("Started fresh conversation (history cleared)"),
        ];

        let widths = [40, 60, 80, 100, 120, 160];

        for &w in &widths {
            for (idx, entry) in entries.iter().enumerate() {
                let lines1 = entry.render_with_theme(w, &theme);
                let lines2 = entry.render_with_theme(w, &theme);
                let lines_detect = entry.render(w);

                // Deterministic check
                assert_eq!(
                    lines1, lines2,
                    "Entry {idx} render({w}) must be deterministic"
                );
                assert_eq!(
                    lines1, lines_detect,
                    "Entry {idx} render({w}) must match render_with_theme"
                );

                // Width bound check: EVERY line must have display width <= w
                for (line_idx, line) in lines1.iter().enumerate() {
                    let display_w = line.width();
                    assert!(
                        display_w <= w,
                        "Entry {idx} ({:?}) line {line_idx} has display width {display_w} > max width {w}: '{:?}'",
                        entry, display_w
                    );
                }
            }
        }
    }

    #[test]
    fn test_transcript_capacity_capped_at_200() {
        let mut transcript = Transcript::new();
        assert_eq!(transcript.len(), 0);

        for i in 0..250 {
            transcript.push(Entry::notice(format!("Notice {i}")));
        }

        assert_eq!(transcript.len(), 200);
        if let Some(Entry::Notice(msg)) = transcript.entries().first() {
            assert_eq!(msg, "Notice 50");
        } else {
            panic!("Expected Notice 50 as first entry");
        }
        if let Some(Entry::Notice(msg)) = transcript.entries().last() {
            assert_eq!(msg, "Notice 249");
        } else {
            panic!("Expected Notice 249 as last entry");
        }
    }
}
