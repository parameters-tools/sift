use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use textwrap::{Options, WordSplitter};
use unicode_width::UnicodeWidthStr;

use super::markdown::MarkdownRenderer;
use super::theme::Theme;
use super::welcome::format_count;

#[derive(Debug, Clone, PartialEq)]
pub struct SourceRef {
    pub citation: usize,
    pub path: String,
    pub score: f32,
    pub page: Option<u32>,
}

impl SourceRef {
    pub fn new(citation: usize, path: impl Into<String>, score: f32) -> Self {
        Self {
            citation,
            path: path.into(),
            score,
            page: None,
        }
    }

    pub fn with_page(citation: usize, path: impl Into<String>, score: f32, page: u32) -> Self {
        Self {
            citation,
            path: path.into(),
            score,
            page: Some(page),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RetrievedChunk {
    pub rank: usize,
    pub file: String,
    pub full_path: String,
    pub page: Option<u32>,
    pub text: String,
    pub score: f32,
    pub is_cited: bool,
}

pub struct MessageFormatter;

impl MessageFormatter {
    pub fn render_user_message(text: &str, theme: &Theme) -> Vec<Line<'static>> {
        let content = text.trim();
        vec![Line::from(vec![
            Span::styled("> ", theme.style_muted()),
            Span::raw(content.to_string()),
        ])]
    }

    pub fn compute_content_width(terminal_width: usize) -> usize {
        (terminal_width.saturating_sub(4)).min(100)
    }

    pub fn wrap_line_with_indent(line_str: &str, width: usize, subsequent_indent: &str) -> Vec<String> {
        if UnicodeWidthStr::width(line_str) <= width || width <= subsequent_indent.len() {
            return vec![line_str.to_string()];
        }
        let opts = Options::new(width)
            .initial_indent("")
            .subsequent_indent(subsequent_indent)
            .word_splitter(WordSplitter::NoHyphenation);
        textwrap::wrap(line_str, opts)
            .into_iter()
            .map(|cow| cow.into_owned())
            .collect()
    }

    pub fn render_tool_done(tool_name: &str, args: &str, detail: &str, theme: &Theme) -> Vec<Line<'static>> {
        Self::render_tool_done_wrapped(tool_name, args, detail, theme, 100)
    }

    pub fn render_tool_done_wrapped(
        tool_name: &str,
        args: &str,
        detail: &str,
        theme: &Theme,
        content_width: usize,
    ) -> Vec<Line<'static>> {
        let lines: Vec<&str> = detail.lines().collect();
        let detail_str = if lines.len() > 1 {
            format!("{} … +{} lines", lines[0], lines.len() - 1)
        } else {
            lines.first().copied().unwrap_or("").to_string()
        };

        let mut res = Vec::new();
        let title_content = if args.is_empty() {
            format!("{} {tool_name}", theme.symbols.bullet)
        } else {
            format!("{} {tool_name}({args})", theme.symbols.bullet)
        };

        if UnicodeWidthStr::width(title_content.as_str()) <= content_width {
            let mut title_spans = vec![
                Span::styled(format!("{} ", theme.symbols.bullet), theme.style_success()),
                Span::styled(tool_name.to_string(), theme.style_bold()),
            ];
            if !args.is_empty() {
                title_spans.push(Span::raw(format!("({args})")));
            }
            res.push(Line::from(title_spans));
        } else {
            let wrapped = Self::wrap_line_with_indent(&title_content, content_width, "  ");
            for (idx, w_line) in wrapped.into_iter().enumerate() {
                if idx == 0 {
                    let mut title_spans = vec![
                        Span::styled(format!("{} ", theme.symbols.bullet), theme.style_success()),
                        Span::styled(tool_name.to_string(), theme.style_bold()),
                    ];
                    let prefix = format!("{} {tool_name}", theme.symbols.bullet);
                    if let Some(rem) = w_line.strip_prefix(&prefix).filter(|s| !s.is_empty()) {
                        title_spans.push(Span::raw(rem.to_string()));
                    }
                    res.push(Line::from(title_spans));
                } else {
                    res.push(Line::from(vec![Span::raw(w_line)]));
                }
            }
        }

        let sub_prefix = format!("  {}  ", theme.symbols.sub_item);
        let detail_full = format!("{sub_prefix}{detail_str}");
        if UnicodeWidthStr::width(detail_full.as_str()) <= content_width {
            res.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{}  ", theme.symbols.sub_item), theme.style_muted()),
                Span::styled(detail_str, theme.style_muted()),
            ]));
        } else {
            let wrapped = Self::wrap_line_with_indent(&detail_full, content_width, "  ");
            for (idx, w_line) in wrapped.into_iter().enumerate() {
                if idx == 0 {
                    let rem = w_line.strip_prefix(&sub_prefix).unwrap_or(&w_line);
                    res.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(format!("{}  ", theme.symbols.sub_item), theme.style_muted()),
                        Span::styled(rem.to_string(), theme.style_muted()),
                    ]));
                } else {
                    res.push(Line::from(vec![Span::styled(w_line, theme.style_muted())]));
                }
            }
        }

        res
    }

    pub fn format_tool_args(arg: &str) -> String {
        let trimmed = arg.trim();
        if trimmed.is_empty() {
            return String::new();
        }
        let char_count = trimmed.chars().count();
        if char_count > 60 {
            let prefix: String = trimmed.chars().take(59).collect();
            format!("\"{prefix}…\"")
        } else {
            format!("\"{trimmed}\"")
        }
    }

    pub fn render_retrieve_tool_done(
        user_message: &str,
        retrieved_query: &str,
        detail: &str,
        is_weak: bool,
        no_strong_match: bool,
        theme: &Theme,
    ) -> Vec<Line<'static>> {
        Self::render_retrieve_tool_done_wrapped(
            user_message,
            retrieved_query,
            detail,
            is_weak,
            no_strong_match,
            theme,
            100,
        )
    }

    pub fn render_retrieve_tool_done_wrapped(
        user_message: &str,
        retrieved_query: &str,
        detail: &str,
        is_weak: bool,
        no_strong_match: bool,
        theme: &Theme,
        content_width: usize,
    ) -> Vec<Line<'static>> {
        let args = if user_message.trim() == retrieved_query.trim() {
            String::new()
        } else {
            Self::format_tool_args(retrieved_query)
        };
        let mut lines = Self::render_tool_done_wrapped("Retrieve", &args, detail, theme, content_width);
        if no_strong_match {
            let warn_text = "No strong match in your sources";
            let sub_prefix = format!("  {} ⚠ ", theme.symbols.sub_item);
            let full_warn = format!("{sub_prefix}{warn_text}");
            if UnicodeWidthStr::width(full_warn.as_str()) <= content_width {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(format!("{} ⚠ ", theme.symbols.sub_item), theme.style_warning()),
                    Span::styled(warn_text, theme.style_warning()),
                ]));
            } else {
                let wrapped = Self::wrap_line_with_indent(&full_warn, content_width, "  ");
                for (idx, w_line) in wrapped.into_iter().enumerate() {
                    if idx == 0 {
                        let rem = w_line.strip_prefix(&sub_prefix).unwrap_or(&w_line);
                        lines.push(Line::from(vec![
                            Span::raw("  "),
                            Span::styled(format!("{} ⚠ ", theme.symbols.sub_item), theme.style_warning()),
                            Span::styled(rem.to_string(), theme.style_warning()),
                        ]));
                    } else {
                        lines.push(Line::from(vec![Span::styled(w_line, theme.style_warning())]));
                    }
                }
            }
        } else if is_weak {
            let warn_text = "Weak match: answers may be incomplete";
            let sub_prefix = format!("  {} ⚠ ", theme.symbols.sub_item);
            let full_warn = format!("{sub_prefix}{warn_text}");
            if UnicodeWidthStr::width(full_warn.as_str()) <= content_width {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(format!("{} ⚠ ", theme.symbols.sub_item), theme.style_warning()),
                    Span::styled(warn_text, theme.style_warning()),
                ]));
            } else {
                let wrapped = Self::wrap_line_with_indent(&full_warn, content_width, "  ");
                for (idx, w_line) in wrapped.into_iter().enumerate() {
                    if idx == 0 {
                        let rem = w_line.strip_prefix(&sub_prefix).unwrap_or(&w_line);
                        lines.push(Line::from(vec![
                            Span::raw("  "),
                            Span::styled(format!("{} ⚠ ", theme.symbols.sub_item), theme.style_warning()),
                            Span::styled(rem.to_string(), theme.style_warning()),
                        ]));
                    } else {
                        lines.push(Line::from(vec![Span::styled(w_line, theme.style_warning())]));
                    }
                }
            }
        }
        lines
    }

    pub fn render_tool_error(tool_name: &str, args: &str, error: &str, theme: &Theme) -> Vec<Line<'static>> {
        let lines_vec: Vec<&str> = error.lines().collect();
        let err_str = if lines_vec.len() > 1 {
            format!("{} … +{} lines", lines_vec[0], lines_vec.len() - 1)
        } else {
            error.to_string()
        };
        let mut title_spans = vec![
            Span::styled(format!("{} ", theme.symbols.bullet), theme.style_error()),
            Span::styled(tool_name.to_string(), theme.style_bold()),
        ];
        if !args.is_empty() {
            title_spans.push(Span::raw(format!("({args})")));
        }
        vec![
            Line::from(title_spans),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{}  ", theme.symbols.sub_item), theme.style_muted()),
                Span::styled(err_str, theme.style_error()),
            ]),
        ]
    }

    pub fn render_empty_index_notice(theme: &Theme) -> Vec<Line<'static>> {
        vec![
            Line::from(vec![
                Span::styled(format!("{} ", theme.symbols.bullet), theme.style_warning()),
                Span::styled("Index is empty.", theme.style_bold()),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{}  ", theme.symbols.sub_item), theme.style_muted()),
                Span::styled("Add sources first with /add <path|url>", theme.style_muted()),
            ]),
        ]
    }

    pub fn render_interrupted(theme: &Theme) -> Vec<Line<'static>> {
        vec![
            Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{}  ", theme.symbols.sub_item), theme.style_muted()),
                Span::styled("Interrupted by user", theme.style_muted()),
            ]),
        ]
    }

    pub fn extract_citations(answer: &str) -> Vec<usize> {
        let mut citations = Vec::new();
        let chars: Vec<char> = answer.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '[' {
                let start = i + 1;
                let mut j = start;
                while j < chars.len() && chars[j] != ']' && chars[j] != '\n' {
                    j += 1;
                }
                if j < chars.len() && chars[j] == ']' {
                    let inside: String = chars[start..j].iter().collect();
                    for part in inside.split(',') {
                        let trimmed = part.trim();
                        if let Ok(num) = trimmed.parse::<usize>()
                            && num > 0
                            && !citations.contains(&num)
                        {
                            citations.push(num);
                        }
                    }
                    i = j + 1;
                    continue;
                }
            }
            i += 1;
        }
        citations.sort_unstable();
        citations
    }

    pub fn render_assistant_answer(
        answer: &str,
        sources: &[(usize, String, f32)],
        theme: &Theme,
        content_width: usize,
    ) -> Vec<Line<'static>> {
        let refs: Vec<SourceRef> = sources
            .iter()
            .map(|(c, p, s)| SourceRef::new(*c, p.clone(), *s))
            .collect();
        Self::render_assistant_answer_with_refs(answer, &refs, theme, content_width)
    }

    pub fn render_assistant_answer_with_refs(
        answer: &str,
        sources: &[SourceRef],
        theme: &Theme,
        content_width: usize,
    ) -> Vec<Line<'static>> {
        let mut lines = Self::wrap_assistant_answer(answer, theme, content_width);
        lines.extend(Self::render_sources_block_filtered_wrapped(answer, sources, theme, content_width));
        lines
    }

    pub fn render_sources_block_filtered(
        answer: &str,
        sources: &[SourceRef],
        theme: &Theme,
    ) -> Vec<Line<'static>> {
        Self::render_sources_block_filtered_wrapped(answer, sources, theme, 100)
    }

    pub fn render_sources_block_filtered_wrapped(
        answer: &str,
        sources: &[SourceRef],
        theme: &Theme,
        content_width: usize,
    ) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        if sources.is_empty() {
            return lines;
        }

        let cited_nums = Self::extract_citations(answer);
        let cited_sources: Vec<SourceRef> = sources
            .iter()
            .filter(|s| cited_nums.contains(&s.citation))
            .cloned()
            .collect();

        let uncited_count = sources.len().saturating_sub(cited_sources.len());

        lines.push(Line::default());
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled("Sources", theme.style_muted().add_modifier(Modifier::BOLD)),
        ]));

        if !cited_sources.is_empty() {
            let grouped = Self::group_source_refs(&cited_sources);
            for (file_name, citations, pages) in grouped {
                let citations_str: String = citations.iter().map(|c| format!("[{c}]")).collect();
                let line_content = if pages.is_empty() {
                    format!("{citations_str} {file_name}")
                } else {
                    let page_str = pages
                        .iter()
                        .map(|p| p.to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("{citations_str} {file_name} · p.{page_str}")
                };
                let full_line = format!("  {line_content}");
                if UnicodeWidthStr::width(full_line.as_str()) <= content_width {
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(line_content, theme.style_muted()),
                    ]));
                } else {
                    let wrapped = Self::wrap_line_with_indent(&full_line, content_width, "  ");
                    for w_line in wrapped {
                        lines.push(Line::from(vec![Span::styled(w_line, theme.style_muted())]));
                    }
                }
            }
        }

        if uncited_count > 0 {
            let chunk_word = if uncited_count == 1 { "chunk" } else { "chunks" };
            let uncited_str = format!("{uncited_count} other {chunk_word} retrieved, not cited · /chunks to view");
            let full_uncited = format!("  {uncited_str}");
            if UnicodeWidthStr::width(full_uncited.as_str()) <= content_width {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(uncited_str, theme.style_muted()),
                ]));
            } else {
                let wrapped = Self::wrap_line_with_indent(&full_uncited, content_width, "  ");
                for w_line in wrapped {
                    lines.push(Line::from(vec![Span::styled(w_line, theme.style_muted())]));
                }
            }
        }

        lines
    }

    pub fn group_source_refs(
        sources: &[SourceRef],
    ) -> Vec<(String, Vec<usize>, Vec<u32>)> {
        let mut grouped: Vec<(String, Vec<usize>, Vec<u32>)> = Vec::new();
        for s in sources {
            let file_name = std::path::Path::new(&s.path)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| s.path.clone());

            if let Some((_, citations, pages)) = grouped.iter_mut().find(|(name, _, _)| name == &file_name) {
                citations.push(s.citation);
                if let Some(p) = s.page.filter(|p| !pages.contains(p)) {
                    pages.push(p);
                    pages.sort_unstable();
                }
            } else {
                let pages = if let Some(p) = s.page { vec![p] } else { Vec::new() };
                grouped.push((file_name, vec![s.citation], pages));
            }
        }
        grouped
    }

    pub fn group_sources(sources: &[(usize, String, f32)]) -> Vec<(String, Vec<usize>)> {
        let refs: Vec<SourceRef> = sources
            .iter()
            .map(|(c, p, s)| SourceRef::new(*c, p.clone(), *s))
            .collect();
        Self::group_source_refs(&refs)
            .into_iter()
            .map(|(name, citations, _)| (name, citations))
            .collect()
    }

    pub fn render_source_refs_block(
        sources: &[SourceRef],
        theme: &Theme,
    ) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        if sources.is_empty() {
            return lines;
        }

        let grouped = Self::group_source_refs(sources);
        lines.push(Line::default());
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled("Sources", theme.style_muted().add_modifier(Modifier::BOLD)),
        ]));

        for (file_name, citations, pages) in grouped {
            let citations_str: String = citations.iter().map(|c| format!("[{c}]")).collect();
            let line_content = if pages.is_empty() {
                format!("{citations_str} {file_name}")
            } else {
                let page_str = pages
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{citations_str} {file_name} · p.{page_str}")
            };
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(line_content, theme.style_muted()),
            ]));
        }

        lines
    }

    pub fn render_sources_block(
        sources: &[(usize, String, f32)],
        theme: &Theme,
    ) -> Vec<Line<'static>> {
        let refs: Vec<SourceRef> = sources
            .iter()
            .map(|(c, p, s)| SourceRef::new(*c, p.clone(), *s))
            .collect();
        Self::render_source_refs_block(&refs, theme)
    }

    pub fn render_chunks_inspection(
        chunks: &[RetrievedChunk],
        content_width: usize,
        theme: &Theme,
    ) -> Vec<Line<'static>> {
        if chunks.is_empty() {
            return vec![Line::from(vec![
                Span::raw("  "),
                Span::styled("No chunks retrieved yet.", theme.style_muted()),
            ])];
        }

        let width = content_width.clamp(20, 100);
        let mut lines = Vec::new();

        for (idx, c) in chunks.iter().enumerate() {
            if idx > 0 {
                lines.push(Line::default());
            }

            let page_str = c.page.map(|p| format!(" · p.{p}")).unwrap_or_default();
            let header = format!("[{}] {}{}", c.rank, c.file, page_str);
            let mut header_spans = vec![Span::raw("  ")];
            if c.is_cited {
                header_spans.push(Span::styled("● ", theme.style_accent()));
                header_spans.push(Span::styled(
                    header,
                    theme.style_accent().add_modifier(Modifier::BOLD),
                ));
            } else {
                header_spans.push(Span::styled(
                    header,
                    theme.style_muted().add_modifier(Modifier::BOLD),
                ));
            }
            lines.push(Line::from(header_spans));

            // Build a 2-3 line snippet, wrapped, truncated with "…"
            let clean_text: String = c.text.split_whitespace().collect::<Vec<_>>().join(" ");
            let snippet_width = width.saturating_sub(4).max(10);
            let options = Options::new(snippet_width)
                .word_splitter(WordSplitter::NoHyphenation);
            let wrapped = textwrap::wrap(&clean_text, &options);

            let max_lines = 3;
            let display_lines = wrapped.iter().take(max_lines).collect::<Vec<_>>();
            let has_more = wrapped.len() > max_lines;

            for (l_idx, line) in display_lines.iter().enumerate() {
                let is_last = l_idx == display_lines.len() - 1;
                let text = if is_last && has_more {
                    let s = line.as_ref();
                    if s.chars().count() > snippet_width.saturating_sub(1) {
                        let truncated: String = s.chars().take(snippet_width.saturating_sub(2)).collect();
                        format!("{truncated}…")
                    } else {
                        format!("{s}…")
                    }
                } else {
                    line.to_string()
                };

                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(text, theme.style_muted()),
                ]));
            }
        }

        lines
    }

    pub fn render_cite_inspection(
        chunk: &RetrievedChunk,
        content_width: usize,
        theme: &Theme,
    ) -> Vec<Line<'static>> {
        let box_width = (content_width.saturating_sub(2)).clamp(20, 100);
        let inner_width = box_width.saturating_sub(4).max(10);

        let page_str = chunk.page.map(|p| format!(" · p.{p}")).unwrap_or_default();
        let title = format!("[{}] {}{}", chunk.rank, chunk.file, page_str);

        // Header border: ╭─ [1] filename ──────╮
        let title_len = UnicodeWidthStr::width(title.as_str());
        let header_border = if title_len + 6 <= box_width {
            let dashes = box_width.saturating_sub(title_len + 6);
            let fill = theme.borders.horizontal.to_string().repeat(dashes);
            format!(
                "{}{} {} {}{}",
                theme.borders.top_left,
                theme.borders.horizontal,
                title,
                fill,
                theme.borders.top_right
            )
        } else {
            let max_t = box_width.saturating_sub(7);
            let truncated: String = title.chars().take(max_t).collect();
            let truncated_w = UnicodeWidthStr::width(truncated.as_str());
            let fill = theme
                .borders
                .horizontal
                .to_string()
                .repeat(box_width.saturating_sub(truncated_w + 7));
            format!(
                "{}{} {}… {}{}",
                theme.borders.top_left,
                theme.borders.horizontal,
                truncated,
                fill,
                theme.borders.top_right
            )
        };

        let bottom_dashes = theme
            .borders
            .horizontal
            .to_string()
            .repeat(box_width.saturating_sub(2));
        let bottom_border = format!(
            "{}{}{}",
            theme.borders.bottom_left, bottom_dashes, theme.borders.bottom_right
        );

        let mut lines = Vec::new();
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(header_border, theme.style_muted()),
        ]));

        // Wrap full text to inner width
        let options = Options::new(inner_width)
            .word_splitter(WordSplitter::NoHyphenation);

        for raw_line in chunk.text.lines() {
            let trimmed = raw_line.trim();
            if trimmed.is_empty() {
                let pad = " ".repeat(inner_width);
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(format!("{} ", theme.borders.vertical), theme.style_muted()),
                    Span::raw(pad),
                    Span::styled(format!(" {}", theme.borders.vertical), theme.style_muted()),
                ]));
                continue;
            }
            let wrapped = textwrap::wrap(trimmed, &options);
            for w in wrapped {
                let w_len = UnicodeWidthStr::width(w.as_ref());
                let pad = " ".repeat(inner_width.saturating_sub(w_len));
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(format!("{} ", theme.borders.vertical), theme.style_muted()),
                    Span::styled(w.into_owned(), theme.style_muted()),
                    Span::raw(pad),
                    Span::styled(format!(" {}", theme.borders.vertical), theme.style_muted()),
                ]));
            }
        }

        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(bottom_border, theme.style_muted()),
        ]));

        lines
    }

    pub fn format_answer_footer(
        elapsed_secs: f64,
        tokens: usize,
        is_approx_tokens: bool,
        chat_model: &str,
    ) -> String {
        let elapsed = Self::format_elapsed(elapsed_secs);
        let token_label = if is_approx_tokens {
            format!("~{tokens} tokens")
        } else {
            format!("{tokens} tokens")
        };
        format!("{elapsed} · {token_label} · {chat_model}")
    }

    pub fn render_answer_footer(
        elapsed_secs: f64,
        tokens: usize,
        is_approx_tokens: bool,
        chat_model: &str,
        theme: &Theme,
    ) -> Vec<Line<'static>> {
        let text = Self::format_answer_footer(elapsed_secs, tokens, is_approx_tokens, chat_model);
        vec![Line::from(vec![
            Span::raw("  "),
            Span::styled(text, theme.style_muted()),
        ])]
    }

    pub fn format_elapsed(secs: f64) -> String {
        let ms = (secs * 1000.0).round() as u64;
        if ms < 100 {
            format!("{ms}ms")
        } else {
            format!("{secs:.1}s")
        }
    }

    pub fn format_retrieve_detail(
        chunks_count: usize,
        sources_count: usize,
        elapsed_secs: f64,
        filtered_count: usize,
    ) -> String {
        let chunk_word = if chunks_count == 1 { "chunk" } else { "chunks" };
        let source_word = if sources_count == 1 { "source" } else { "sources" };
        let timing = Self::format_elapsed(elapsed_secs);
        if filtered_count > 0 {
            format!("{chunks_count} {chunk_word} from {sources_count} {source_word} · {timing} ({filtered_count} filtered)")
        } else {
            format!("{chunks_count} {chunk_word} from {sources_count} {source_word} · {timing}")
        }
    }

    pub fn estimate_tokens(text: &str) -> usize {
        let char_count = text.chars().count();
        if char_count == 0 {
            0
        } else {
            char_count.div_ceil(4)
        }
    }

    pub fn format_index_done(
        indexed_chunks: usize,
        indexed_files: usize,
        elapsed_secs: f64,
    ) -> String {
        let chunk_word = if indexed_chunks == 1 { "chunk" } else { "chunks" };
        let source_word = if indexed_files == 1 { "source" } else { "sources" };
        let timing = Self::format_elapsed(elapsed_secs);
        format!("Done ({timing}) · Indexed {indexed_chunks} {chunk_word} from {indexed_files} {source_word}")
    }

    pub fn wrap_assistant_answer(
        answer: &str,
        theme: &Theme,
        content_width: usize,
    ) -> Vec<Line<'static>> {
        let width = content_width.max(20);
        let mut lines = Vec::new();
        let mut is_first_line = true;
        let mut in_code_block = false;
        let mut code_lang = String::new();

        let raw_lines: Vec<&str> = answer.lines().collect();
        let mut current_para: Vec<String> = Vec::new();

        let flush_para = |current_para: &mut Vec<String>,
                              lines: &mut Vec<Line<'static>>,
                              is_first_line: &mut bool| {
            if current_para.is_empty() {
                return;
            }
            let text = current_para.join(" ");
            current_para.clear();

            let initial_indent = if *is_first_line {
                format!("{} ", theme.symbols.bullet)
            } else {
                "  ".to_string()
            };
            let subsequent_indent = "  ";

            let options = Options::new(width)
                .initial_indent(&initial_indent)
                .subsequent_indent(subsequent_indent)
                .word_splitter(WordSplitter::NoHyphenation);

            let wrapped = textwrap::wrap(&text, &options);
            for w in wrapped {
                let w_str = w.as_ref();
                let bullet_prefix = format!("{} ", theme.symbols.bullet);
                if *is_first_line && w_str.starts_with(&bullet_prefix) {
                    *is_first_line = false;
                    let remainder = &w_str[bullet_prefix.len()..];
                    let mut spans = vec![Span::styled(bullet_prefix, theme.style_accent())];
                    spans.extend(MarkdownRenderer::parse_inline_spans(remainder, theme));
                    lines.push(Line::from(spans));
                } else if let Some(rem) = w_str.strip_prefix("  ") {
                    let mut spans = vec![Span::raw("  ")];
                    spans.extend(MarkdownRenderer::parse_inline_spans(rem, theme));
                    lines.push(Line::from(spans));
                } else {
                    let spans = MarkdownRenderer::parse_inline_spans(w_str, theme);
                    lines.push(Line::from(spans));
                }
            }
        };

        for raw_line in raw_lines {
            let trimmed = raw_line.trim();

            if trimmed.starts_with("```") {
                flush_para(&mut current_para, &mut lines, &mut is_first_line);
                if in_code_block {
                    in_code_block = false;
                    code_lang.clear();
                    lines.push(Line::from(vec![
                        Span::styled("  └", Style::default().fg(Color::DarkGray)),
                        Span::styled(
                            "────────────────────────────────",
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]));
                } else {
                    in_code_block = true;
                    is_first_line = false;
                    code_lang = trimmed.trim_start_matches('`').trim().to_string();
                    let header = if code_lang.is_empty() {
                        "─── code ───────────────────────".to_string()
                    } else {
                        format!("─── {} ───────────────────────", code_lang)
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

            if trimmed.is_empty() {
                flush_para(&mut current_para, &mut lines, &mut is_first_line);
                if !lines.is_empty()
                    && lines.last().map(|l| !l.spans.is_empty()).unwrap_or(false)
                {
                    lines.push(Line::default());
                }
                continue;
            }

            // Headings
            if trimmed.starts_with('#') {
                flush_para(&mut current_para, &mut lines, &mut is_first_line);
                is_first_line = false;
                let heading_text = if let Some(r) = trimmed.strip_prefix("### ") {
                    r
                } else if let Some(r) = trimmed.strip_prefix("## ") {
                    r
                } else if let Some(r) = trimmed.strip_prefix("# ") {
                    r
                } else {
                    trimmed
                };
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        heading_text.to_string(),
                        theme.style_accent().add_modifier(Modifier::BOLD),
                    ),
                ]));
                continue;
            }

            // Bullet list items: - , * , +
            if trimmed.starts_with("- ")
                || trimmed.starts_with("* ")
                || trimmed.starts_with("+ ")
            {
                flush_para(&mut current_para, &mut lines, &mut is_first_line);
                is_first_line = false;
                let item_text = trimmed[2..].trim();
                let options = Options::new(width)
                    .initial_indent("  • ")
                    .subsequent_indent("    ")
                    .word_splitter(WordSplitter::NoHyphenation);

                let wrapped = textwrap::wrap(item_text, &options);
                for w in wrapped {
                    let w_str = w.as_ref();
                    if let Some(rem) = w_str.strip_prefix("  • ") {
                        let mut spans = vec![
                            Span::raw("  "),
                            Span::styled("• ", theme.style_accent()),
                        ];
                        spans.extend(MarkdownRenderer::parse_inline_spans(rem, theme));
                        lines.push(Line::from(spans));
                    } else if let Some(rem) = w_str.strip_prefix("    ") {
                        let mut spans = vec![Span::raw("    ")];
                        spans.extend(MarkdownRenderer::parse_inline_spans(rem, theme));
                        lines.push(Line::from(spans));
                    } else {
                        let spans = MarkdownRenderer::parse_inline_spans(w_str, theme);
                        lines.push(Line::from(spans));
                    }
                }
                continue;
            }

            // Numbered list items: 1. etc.
            if trimmed
                .chars()
                .next()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false)
                && trimmed.contains(". ")
            {
                flush_para(&mut current_para, &mut lines, &mut is_first_line);
                let initial_indent = if is_first_line {
                    format!("{} ", theme.symbols.bullet)
                } else {
                    "  ".to_string()
                };
                let subsequent_indent = "  ";
                let options = Options::new(width)
                    .initial_indent(&initial_indent)
                    .subsequent_indent(subsequent_indent)
                    .word_splitter(WordSplitter::NoHyphenation);

                let wrapped = textwrap::wrap(trimmed, &options);
                for w in wrapped {
                    let w_str = w.as_ref();
                    let bullet_prefix = format!("{} ", theme.symbols.bullet);
                    if is_first_line && w_str.starts_with(&bullet_prefix) {
                        is_first_line = false;
                        let remainder = &w_str[bullet_prefix.len()..];
                        let mut spans = vec![Span::styled(bullet_prefix, theme.style_accent())];
                        spans.extend(MarkdownRenderer::parse_inline_spans(remainder, theme));
                        lines.push(Line::from(spans));
                    } else if let Some(rem) = w_str.strip_prefix("  ") {
                        let mut spans = vec![Span::raw("  ")];
                        spans.extend(MarkdownRenderer::parse_inline_spans(rem, theme));
                        lines.push(Line::from(spans));
                    } else {
                        let spans = MarkdownRenderer::parse_inline_spans(w_str, theme);
                        lines.push(Line::from(spans));
                    }
                }
                continue;
            }

            // Regular prose line
            current_para.push(trimmed.to_string());
        }

        flush_para(&mut current_para, &mut lines, &mut is_first_line);

        lines
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_status_panel(
        db_path: &str,
        sources: i64,
        chunks: i64,
        chat_model: &str,
        embed_model: &str,
        api_base: &str,
        api_health: bool,
        version: &str,
        theme: &Theme,
    ) -> Vec<Line<'static>> {
        let health_span = if api_health {
            Span::styled("Reachable", theme.style_success())
        } else {
            Span::styled("Unreachable", theme.style_error())
        };

        let mut lines = vec![
            Line::from(vec![Span::styled("Sift Status", theme.style_bold())]),
            Line::from(format!("  Database:     {db_path}")),
            Line::from(format!(
                "  Index:        {} sources · {} chunks",
                format_count(sources),
                format_count(chunks)
            )),
            Line::from(format!("  Chat Model:   {chat_model}")),
            Line::from(format!("  Embed Model:  {embed_model}")),
        ];
        if api_base.contains(" · ") {
            let parts: Vec<&str> = api_base.split(" · ").collect();
            lines.push(Line::from(format!("  Chat Base:    {}", parts[0])));
            lines.push(Line::from(format!("  Embed Base:   {}", parts[1])));
        } else {
            lines.push(Line::from(format!("  API Base:     {api_base}")));
        }
        lines.push(Line::from(vec![
            Span::raw("  API Health:   "),
            health_span,
        ]));
        let version_str = if version.starts_with("Sift") {
            version.to_string()
        } else {
            format!("Sift {version}")
        };
        lines.push(Line::from(format!("  Version:      {version_str}")));
        lines
    }

    pub fn render_sources_list(sources: &[(String, i64)], theme: &Theme) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        lines.push(Line::from(vec![Span::styled("Indexed Sources", theme.style_bold())]));
        if sources.is_empty() {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("(no sources indexed yet)", theme.style_muted()),
            ]));
        } else {
            for (src, count) in sources {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(src.clone(), theme.style_accent()),
                    Span::styled(format!("  ({} chunks)", format_count(*count)), theme.style_muted()),
                ]));
            }
        }
        lines
    }

    pub fn render_help(theme: &Theme) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        lines.push(Line::from(vec![Span::styled("Sift Help", theme.style_bold())]));
        for cmd in super::commands::CommandRegistry::all() {
            let name_with_args = if cmd.args.is_empty() {
                cmd.name.to_string()
            } else {
                format!("{} {}", cmd.name, cmd.args)
            };
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{:<26}", name_with_args), theme.style_accent()),
                Span::styled(cmd.description.to_string(), theme.style_muted()),
            ]));
        }
        lines
    }
    pub fn format_user_message(text: &str, theme: &Theme) -> String {
        let prompt_symbol = theme.paint_muted(">");
        let content = text.trim();
        format!("{prompt_symbol} {content}")
    }

    pub fn format_tool_start(tool_name: &str, args: &str, theme: &Theme) -> String {
        let bullet = theme.paint_muted(theme.symbols.bullet);
        let name = theme.paint_bold(tool_name);
        format!("{bullet} {name}({args})")
    }

    pub fn format_tool_done(tool_name: &str, args: &str, detail: &str, theme: &Theme) -> String {
        let bullet = theme.paint_success(theme.symbols.bullet);
        let name = theme.paint_bold(tool_name);
        let sub = theme.paint_muted(theme.symbols.sub_item);

        let lines: Vec<&str> = detail.lines().collect();
        let detail_str = if lines.len() > 1 {
            format!("{} … +{} lines", lines[0], lines.len() - 1)
        } else {
            lines.first().copied().unwrap_or("").to_string()
        };

        let painted_detail = theme.paint_muted(&detail_str);
        format!("{bullet} {name}({args})\n  {sub}  {painted_detail}")
    }

    pub fn format_tool_error(tool_name: &str, args: &str, error: &str, theme: &Theme) -> String {
        let bullet = theme.paint_error(theme.symbols.bullet);
        let name = theme.paint_bold(tool_name);
        let sub = theme.paint_muted(theme.symbols.sub_item);
        let err_painted = theme.paint_error(error);

        format!("{bullet} {name}({args})\n  {sub}  {err_painted}")
    }

    pub fn format_empty_index_notice(theme: &Theme) -> String {
        let bullet = theme.paint_warning(theme.symbols.bullet);
        let title = theme.paint_bold("Index is empty.");
        let sub = theme.paint_muted(theme.symbols.sub_item);
        let help = theme.paint_muted("Add sources first with /add <path|url>");

        format!("{bullet} {title}\n  {sub}  {help}")
    }

    pub fn format_assistant_answer(
        answer: &str,
        sources: &[(usize, String, f32)], // (citation_num, source, score)
        theme: &Theme,
    ) -> String {
        let bullet = theme.symbols.bullet;
        let mut out = Vec::new();

        let lines: Vec<&str> = answer.lines().collect();
        if let Some((first, rest)) = lines.split_first() {
            out.push(format!("{bullet} {first}"));
            for line in rest {
                out.push(format!("  {line}"));
            }
        }

        if !sources.is_empty() {
            out.push(String::new());
            out.push(theme.paint_muted("  Sources"));
            for (file_name, citations) in Self::group_sources(sources) {
                let citations_str: String = citations.iter().map(|c| format!("[{c}]")).collect();
                out.push(theme.paint_muted(&format!("  {citations_str} {file_name}")));
            }
        }

        out.join("\n")
    }

    pub fn format_index_progress(
        path: &str,
        done: usize,
        total: usize,
        elapsed_secs: u64,
        theme: &Theme,
    ) -> String {
        let bullet = theme.paint_accent(theme.symbols.bullet);
        let name = theme.paint_bold("Index");
        let sub = theme.paint_muted(theme.symbols.sub_item);

        let pct = done.checked_mul(100).and_then(|v| v.checked_div(total)).unwrap_or(0);
        let bar_width = 16;
        let filled = (bar_width * pct) / 100;
        let empty = bar_width.saturating_sub(filled);

        let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
        let done_str = format_count(done as i64);
        let total_str = format_count(total as i64);

        let progress = format!(
            "Embedding {done_str}/{total_str} chunks  {bar}  {pct}%  ·  {elapsed_secs}s"
        );
        let painted_progress = theme.paint_muted(&progress);

        format!("{bullet} {name}(\"{path}\")\n  {sub}  {painted_progress}")
    }

    #[allow(clippy::too_many_arguments)]
    pub fn format_status_panel(
        db_path: &str,
        sources: i64,
        chunks: i64,
        chat_model: &str,
        embed_model: &str,
        api_base: &str,
        api_health: bool,
        version: &str,
        theme: &Theme,
    ) -> String {
        let health_str = if api_health {
            theme.paint_success("Reachable")
        } else {
            theme.paint_error("Unreachable")
        };

        let mut lines = Vec::new();
        lines.push(theme.paint_bold("Status"));
        lines.push(format!("  Database:     {}", db_path));
        lines.push(format!(
            "  Index:        {} sources · {} chunks",
            format_count(sources),
            format_count(chunks)
        ));
        lines.push(format!("  Chat Model:   {}", chat_model));
        lines.push(format!("  Embed Model:  {}", embed_model));
        lines.push(format!("  API Base:     {}", api_base));
        lines.push(format!("  API Health:   {}", health_str));
        lines.push(format!("  Version:      {}", version));

        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_user_message() {
        let theme = Theme::detect();
        let msg = MessageFormatter::format_user_message("how does pricing work?", &theme);
        assert!(msg.contains("> how does pricing work?"));
    }

    #[test]
    fn test_format_tool_done() {
        let theme = Theme::detect();
        let row = MessageFormatter::format_tool_done(
            "Retrieve",
            "\"Q3 revenue\"",
            "5 chunks from 3 sources (top score 0.82)",
            &theme,
        );
        assert!(row.contains("Retrieve"));
        assert!(row.contains("\"Q3 revenue\""));
        assert!(row.contains("5 chunks from 3 sources"));
    }

    #[test]
    fn test_render_tool_done() {
        let theme = Theme::detect();
        let text = MessageFormatter::render_tool_done(
            "Retrieve",
            "\"Q3 revenue\"",
            "5 chunks from 3 sources (top score 0.82)",
            &theme,
        );
        assert_eq!(text.len(), 2);
    }

    #[test]
    fn test_format_empty_index_notice() {
        let theme = Theme::detect();
        let notice = MessageFormatter::format_empty_index_notice(&theme);
        assert!(notice.contains("Index is empty."));
        assert!(notice.contains("/add <path|url>"));
    }

    #[test]
    fn test_wrapping_never_splits_words_and_hanging_indent() {
        let theme = Theme::detect();
        let answer = "The Advanced RAG shell provides interactive document retrieval and answer synthesis using local SQLite storage and OpenAI compatible endpoints. This sentence contains moderately long words like comprehensive, documentation, and architecture to verify that wrapping occurs strictly at word boundaries.\n\nThis is a subsequent paragraph that demonstrates that every new paragraph begins indented with two spaces, and any continuation lines created by wrapping also retain the two space hanging indent properly.";

        let original_words: std::collections::HashSet<String> = answer
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| c.is_ascii_punctuation()).to_string())
            .filter(|w| !w.is_empty())
            .collect();

        for term_width in [60, 80, 120] {
            let content_width = (term_width - 2).min(100);
            let lines = MessageFormatter::wrap_assistant_answer(answer, &theme, content_width);

            assert!(!lines.is_empty(), "Lines should not be empty for terminal width {term_width}");

            // 1. First line starts with "● "
            let first_line_str = lines[0].spans.iter().map(|s| s.content.as_ref()).collect::<String>();
            assert!(
                first_line_str.starts_with("● "),
                "First line must start with '● ', got: '{first_line_str}' at width {term_width}"
            );

            for (idx, line) in lines.iter().enumerate() {
                let line_str = line.spans.iter().map(|s| s.content.as_ref()).collect::<String>();
                let visual_w = unicode_width::UnicodeWidthStr::width(line_str.as_str());

                // Content width must not exceed content_width
                assert!(
                    visual_w <= content_width,
                    "Line {idx} visual width {visual_w} exceeds content width {content_width} at term width {term_width}: '{line_str}'"
                );

                if line.spans.is_empty() {
                    continue;
                }

                // Hanging indent: every line after the first must start with 2 spaces
                if idx > 0 {
                    assert!(
                        line_str.starts_with("  "),
                        "Line {idx} must start with 2 spaces hanging indent, got: '{line_str}' at width {term_width}"
                    );
                }

                // Verify that no words were broken in the middle
                for token in line_str.split_whitespace() {
                    let cleaned = token.trim_matches(|c: char| c.is_ascii_punctuation() || c == '●');
                    if cleaned.is_empty() {
                        continue;
                    }
                    assert!(
                        original_words.contains(cleaned),
                        "Word was split or modified: '{cleaned}' (from '{token}') at term width {term_width} line {idx}: '{line_str}'"
                    );
                }
            }
        }
    }

    #[test]
    fn test_singular_plural_formatting() {
        assert_eq!(
            MessageFormatter::format_retrieve_detail(1, 1, 0.4, 0),
            "1 chunk from 1 source · 0.4s"
        );
        assert_eq!(
            MessageFormatter::format_retrieve_detail(5, 1, 0.4, 0),
            "5 chunks from 1 source · 0.4s"
        );
        assert_eq!(
            MessageFormatter::format_retrieve_detail(5, 3, 1.25, 0),
            "5 chunks from 3 sources · 1.2s"
        );
        assert_eq!(
            MessageFormatter::format_retrieve_detail(0, 0, 0.0, 0),
            "0 chunks from 0 sources · 0ms"
        );
        assert_eq!(
            MessageFormatter::format_retrieve_detail(2, 1, 0.045, 0),
            "2 chunks from 1 source · 45ms"
        );
        // With filtered chunks
        assert_eq!(
            MessageFormatter::format_retrieve_detail(2, 1, 0.004, 3),
            "2 chunks from 1 source · 4ms (3 filtered)"
        );
        assert_eq!(
            MessageFormatter::format_index_done(1, 1, 0.2),
            "Done (0.2s) · Indexed 1 chunk from 1 source"
        );
        assert_eq!(
            MessageFormatter::format_index_done(12, 3, 0.5),
            "Done (0.5s) · Indexed 12 chunks from 3 sources"
        );
        assert_eq!(
            MessageFormatter::format_index_done(4, 1, 0.08),
            "Done (80ms) · Indexed 4 chunks from 1 source"
        );
    }

    #[test]
    fn test_elapsed_formatting_boundaries() {
        // Under 100ms: formatted as ms
        assert_eq!(MessageFormatter::format_elapsed(0.099), "99ms");
        assert_eq!(MessageFormatter::format_elapsed(0.012), "12ms");
        assert_eq!(MessageFormatter::format_elapsed(0.0), "0ms");

        // 100ms and above: formatted as seconds with one decimal
        assert_eq!(MessageFormatter::format_elapsed(0.100), "0.1s");
        assert_eq!(MessageFormatter::format_elapsed(1.04), "1.0s");
        assert_eq!(MessageFormatter::format_elapsed(2.56), "2.6s");

        // Never show "0.0s"
        assert_ne!(MessageFormatter::format_elapsed(0.0), "0.0s");
        assert_ne!(MessageFormatter::format_elapsed(0.005), "0.0s");
        assert_ne!(MessageFormatter::format_elapsed(0.049), "0.0s");
    }

    #[test]
    fn test_retrieve_row_hides_unchanged_query() {
        let theme = Theme::detect();

        // 1. Unchanged query: prints "● Retrieve" with no arguments
        let lines_unchanged = MessageFormatter::render_retrieve_tool_done(
            "What is the net revenue for 2024?",
            "What is the net revenue for 2024?",
            "3 chunks from 1 source · 45ms",
            false,
            false,
            &theme,
        );
        let header = lines_unchanged[0].spans.iter().map(|s| s.content.as_ref()).collect::<String>();
        assert_eq!(header, "● Retrieve");
        assert_eq!(lines_unchanged.len(), 2);

        // 2. Changed query (e.g. rewritten): prints "● Retrieve(\"...\")"
        let lines_rewritten = MessageFormatter::render_retrieve_tool_done(
            "revenue?",
            "net revenue financial statements 2024",
            "3 chunks from 1 source · 45ms",
            false,
            false,
            &theme,
        );
        let header_rewritten = lines_rewritten[0].spans.iter().map(|s| s.content.as_ref()).collect::<String>();
        assert_eq!(header_rewritten, "● Retrieve(\"net revenue financial statements 2024\")");

        // 3. Changed query over 60 chars is truncated with "…"
        let long_query = "What is the comprehensive financial breakdown and EBITDA margin for Q3 2024 across all business segments?";
        assert!(long_query.chars().count() > 60);
        let lines_long = MessageFormatter::render_retrieve_tool_done(
            "summary?",
            long_query,
            "5 chunks from 2 sources · 0.2s",
            false,
            false,
            &theme,
        );
        let header_long = lines_long[0].spans.iter().map(|s| s.content.as_ref()).collect::<String>();
        assert!(header_long.contains('…'));
        // Inside quotes is at most 60 chars (59 chars + '…')
        let inner = header_long.strip_prefix("● Retrieve(\"").unwrap().strip_suffix("\")").unwrap();
        assert_eq!(inner.chars().count(), 60);

        // 4. Weak match emits the yellow warning row under Retrieve
        let lines_weak = MessageFormatter::render_retrieve_tool_done(
            "test query",
            "test query",
            "1 chunk from 1 source · 80ms",
            true,
            false,
            &theme,
        );
        assert_eq!(lines_weak.len(), 3);
        let warn_str = lines_weak[2].spans.iter().map(|s| s.content.as_ref()).collect::<String>();
        assert!(warn_str.contains("Weak match: answers may be incomplete"));

        // 5. No strong match emits the yellow warning row under Retrieve
        let lines_no_strong = MessageFormatter::render_retrieve_tool_done(
            "test query",
            "test query",
            "1 chunk from 1 source · 80ms (4 filtered)",
            false,
            true,
            &theme,
        );
        assert_eq!(lines_no_strong.len(), 3);
        let warn_no_strong = lines_no_strong[2].spans.iter().map(|s| s.content.as_ref()).collect::<String>();
        assert!(warn_no_strong.contains("No strong match in your sources"));
    }

    #[test]
    fn test_bullet_hanging_indent_at_60_80_120() {
        let theme = Theme::detect();
        let answer = "- High performance vector retrieval and BM25 lexical search with reciprocal rank fusion.\n- Short bullet item.\n- Another extensive bullet item designed specifically to wrap over several consecutive lines at narrow and standard terminal widths.";

        for term_width in [60, 80, 120] {
            let content_width = (term_width - 2).min(100);
            let lines = MessageFormatter::wrap_assistant_answer(answer, &theme, content_width);

            assert!(!lines.is_empty(), "Lines should not be empty for terminal width {term_width}");

            // Verify lines:
            // First line of each bullet must start with "  • " (4 columns from column 0)
            // Any wrapped continuation line must start with "    " (4 spaces from column 0)
            let mut in_bullet = false;
            for (idx, line) in lines.iter().enumerate() {
                let line_str = line.spans.iter().map(|s| s.content.as_ref()).collect::<String>();
                if line_str.is_empty() {
                    in_bullet = false;
                    continue;
                }

                if line_str.starts_with("  • ") {
                    in_bullet = true;
                    assert!(line_str.len() > 4, "Line {idx} bullet has no text");
                } else if in_bullet {
                    assert!(
                        line_str.starts_with("    "),
                        "Line {idx} wrapped bullet line must start with 4 spaces hanging indent, got: '{line_str}' at width {term_width}"
                    );
                    assert!(
                        !line_str.starts_with("     "),
                        "Line {idx} wrapped bullet line must not have excess indent, got: '{line_str}' at width {term_width}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_source_refs_with_pages() {
        let theme = Theme::detect();
        let sources = vec![
            SourceRef::with_page(1, "/path/to/DeepSeek_V41_Tech_Report.pdf", 0.018, 3),
            SourceRef::with_page(2, "/path/to/DeepSeek_V41_Tech_Report.pdf", 0.016, 7),
            SourceRef::new(3, "/path/to/README.md", 0.015),
            SourceRef::with_page(4, "/path/to/DeepSeek_V41_Tech_Report.pdf", 0.014, 12),
        ];

        let grouped = MessageFormatter::group_source_refs(&sources);
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped[0].0, "DeepSeek_V41_Tech_Report.pdf");
        assert_eq!(grouped[0].1, vec![1, 2, 4]);
        assert_eq!(grouped[0].2, vec![3, 7, 12]);
        assert_eq!(grouped[1].0, "README.md");
        assert_eq!(grouped[1].1, vec![3]);
        assert!(grouped[1].2.is_empty());

        let rendered = MessageFormatter::render_source_refs_block(&sources, &theme);
        assert_eq!(rendered.len(), 4);
        let doc_line = rendered[2].spans.iter().map(|s| s.content.as_ref()).collect::<String>();
        assert_eq!(doc_line, "  [1][2][4] DeepSeek_V41_Tech_Report.pdf · p.3, 7, 12");
        let readme_line = rendered[3].spans.iter().map(|s| s.content.as_ref()).collect::<String>();
        assert_eq!(readme_line, "  [3] README.md");
    }

    #[test]
    fn test_sources_grouping_merges_duplicate_files() {
        let theme = Theme::detect();
        let sources = vec![
            (1, "/home/kevin/docs/DeepSeek_V41_Tech_Report.pdf".to_string(), 0.018f32),
            (2, "/home/kevin/docs/DeepSeek_V41_Tech_Report.pdf".to_string(), 0.016f32),
            (3, "/home/kevin/other/README.md".to_string(), 0.015f32),
            (4, "/home/kevin/docs/DeepSeek_V41_Tech_Report.pdf".to_string(), 0.014f32),
            (5, "/home/kevin/other/README.md".to_string(), 0.012f32),
        ];

        let grouped = MessageFormatter::group_sources(&sources);
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped[0].0, "DeepSeek_V41_Tech_Report.pdf");
        assert_eq!(grouped[0].1, vec![1, 2, 4]);
        assert_eq!(grouped[1].0, "README.md");
        assert_eq!(grouped[1].1, vec![3, 5]);

        let rendered = MessageFormatter::render_sources_block(&sources, &theme);
        assert_eq!(rendered.len(), 4); // 1 blank line + header + 2 file lines
        let header_line = rendered[1].spans.iter().map(|s| s.content.as_ref()).collect::<String>();
        assert_eq!(header_line, "  Sources");

        let doc_line = rendered[2].spans.iter().map(|s| s.content.as_ref()).collect::<String>();
        assert_eq!(doc_line, "  [1][2][4] DeepSeek_V41_Tech_Report.pdf");

        let readme_line = rendered[3].spans.iter().map(|s| s.content.as_ref()).collect::<String>();
        assert_eq!(readme_line, "  [3][5] README.md");
    }

    #[test]
    fn test_chunks_and_cite_render_at_widths_60_80_120() {
        let theme = Theme::detect();
        let sample_chunks = vec![
            RetrievedChunk {
                rank: 1,
                file: "DeepSeek_V41_Tech_Report.pdf".to_string(),
                full_path: "/home/kevin/docs/DeepSeek_V41_Tech_Report.pdf".to_string(),
                page: Some(3),
                text: "DeepSeek-V4.1 represents a massive leap in open weights reasoning capabilities with an extensive context window and mixture-of-experts architecture. Benchmark evaluations demonstrate state of the art results across coding and math tasks.".to_string(),
                score: 0.018,
                is_cited: true,
            },
            RetrievedChunk {
                rank: 2,
                file: "README.md".to_string(),
                full_path: "/home/kevin/README.md".to_string(),
                page: None,
                text: "Quick start guide for the interactive terminal application.".to_string(),
                score: 0.015,
                is_cited: false,
            },
        ];

        for term_width in [60, 80, 120] {
            let content_width = (term_width - 2).min(100);

            // Test /chunks rendering
            let chunks_lines =
                MessageFormatter::render_chunks_inspection(&sample_chunks, content_width, &theme);
            assert!(!chunks_lines.is_empty());
            for line in &chunks_lines {
                let line_str = line
                    .spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>();
                let visual_w = UnicodeWidthStr::width(line_str.as_str());
                assert!(
                    visual_w <= term_width,
                    "/chunks line width {visual_w} exceeds terminal width {term_width}: '{line_str}'"
                );
            }

            // Test /cite rendering
            let cite_lines =
                MessageFormatter::render_cite_inspection(&sample_chunks[0], content_width, &theme);
            assert!(!cite_lines.is_empty());
            for line in &cite_lines {
                let line_str = line
                    .spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>();
                let visual_w = UnicodeWidthStr::width(line_str.as_str());
                assert!(
                    visual_w <= term_width,
                    "/cite line width {visual_w} exceeds terminal width {term_width}: '{line_str}'"
                );
            }
        }
    }

    #[test]
    fn test_answer_footer_formatting() {
        assert_eq!(
            MessageFormatter::format_answer_footer(0.045, 120, false, "deepseekv4.1-flash-1017"),
            "45ms · 120 tokens · deepseekv4.1-flash-1017"
        );
        assert_eq!(
            MessageFormatter::format_answer_footer(1.24, 84, true, "deepseekv4.1-flash-1017"),
            "1.2s · ~84 tokens · deepseekv4.1-flash-1017"
        );
    }

    #[test]
    fn test_token_estimate_fallback() {
        assert_eq!(MessageFormatter::estimate_tokens(""), 0);
        assert_eq!(MessageFormatter::estimate_tokens("a"), 1);
        assert_eq!(MessageFormatter::estimate_tokens("abcd"), 1);
        assert_eq!(MessageFormatter::estimate_tokens("abcde"), 2);
        assert_eq!(MessageFormatter::estimate_tokens("12345678"), 2);
        assert_eq!(MessageFormatter::estimate_tokens("123456789"), 3);
        // Multibyte characters counted by character count
        assert_eq!(MessageFormatter::estimate_tokens("こんにちは"), 2);

        // Verification of approximate "~" formatting in footer
        let estimated = MessageFormatter::estimate_tokens("This is a response that needs token estimation.");
        let footer = MessageFormatter::format_answer_footer(0.5, estimated, true, "deepseekv4.1-flash-1017");
        assert!(footer.contains(&format!("~{estimated} tokens")));
    }

    #[test]
    fn test_citation_parsing_and_sources_filtering() {
        let theme = Theme::detect();

        // 1. Citation parsing with single, adjacent [1][2], comma-separated, repeated, and out-of-range markers
        let text = "Here is statement one [1] and joint evidence [1][2]. Later we cite [3, 4], and repeat [2][1]. Also invalid or out-of-range [0], [99], and [abc].";
        let parsed = MessageFormatter::extract_citations(text);
        assert_eq!(parsed, vec![1, 2, 3, 4, 99]);

        // 2. Sources block shows only cited sources
        let sources = vec![
            SourceRef::new(1, "/path/to/DocA.pdf", 0.020),
            SourceRef::new(2, "/path/to/DocA.pdf", 0.018),
            SourceRef::new(3, "/path/to/DocB.txt", 0.015),
            SourceRef::new(4, "/path/to/DocC.md", 0.012),
            SourceRef::new(5, "/path/to/DocD.org", 0.010),
        ];

        // Answer cites only [1] and [2] (both from DocA.pdf)
        let answer = "According to the tech report [1][2], the model achieves high benchmarks.";
        let rendered = MessageFormatter::render_sources_block_filtered(answer, &sources, &theme);

        // Lines:
        // [0] empty line
        // [1] "  Sources"
        // [2] "  [1][2] DocA.pdf"
        // [3] "  3 other chunks retrieved, not cited · /chunks to view"
        assert_eq!(rendered.len(), 4);
        let s_title = rendered[1].spans.iter().map(|s| s.content.as_ref()).collect::<String>();
        assert_eq!(s_title, "  Sources");
        let s_cited = rendered[2].spans.iter().map(|s| s.content.as_ref()).collect::<String>();
        assert_eq!(s_cited, "  [1][2] DocA.pdf");
        let s_uncited = rendered[3].spans.iter().map(|s| s.content.as_ref()).collect::<String>();
        assert_eq!(s_uncited, "  3 other chunks retrieved, not cited · /chunks to view");

        // 3. Singular case: 4 cited out of 5 -> 1 uncited
        let answer_4 = "Information from [1][2][3][4].";
        let rendered_4 = MessageFormatter::render_sources_block_filtered(answer_4, &sources, &theme);
        let s_uncited_1 = rendered_4.last().unwrap().spans.iter().map(|s| s.content.as_ref()).collect::<String>();
        assert_eq!(s_uncited_1, "  1 other chunk retrieved, not cited · /chunks to view");

        // 4. All cited: no uncited line
        let answer_all = "Information from [1][2][3][4][5].";
        let rendered_all = MessageFormatter::render_sources_block_filtered(answer_all, &sources, &theme);
        for line in &rendered_all {
            let line_str = line.spans.iter().map(|s| s.content.as_ref()).collect::<String>();
            assert!(!line_str.contains("other chunk"));
        }

        // 5. /chunks marks cited ones with ●
        let retrieved = vec![
            RetrievedChunk {
                rank: 1,
                file: "DocA.pdf".to_string(),
                full_path: "/path/to/DocA.pdf".to_string(),
                page: None,
                text: "sample text 1".to_string(),
                score: 0.02,
                is_cited: true,
            },
            RetrievedChunk {
                rank: 2,
                file: "DocB.txt".to_string(),
                full_path: "/path/to/DocB.txt".to_string(),
                page: None,
                text: "sample text 2".to_string(),
                score: 0.015,
                is_cited: false,
            },
        ];
        let chunks_out = MessageFormatter::render_chunks_inspection(&retrieved, 80, &theme);
        let line_cited = chunks_out[0].spans.iter().map(|s| s.content.as_ref()).collect::<String>();
        assert!(line_cited.contains("●"));
        assert!(line_cited.contains("[1] DocA.pdf"));
        let line_uncited = chunks_out[3].spans.iter().map(|s| s.content.as_ref()).collect::<String>();
        assert!(!line_uncited.contains("●"));
        assert!(line_uncited.contains("[2] DocB.txt"));
    }
}
