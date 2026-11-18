use std::time::{Duration, Instant};

use ratatui::text::{Line, Span};

use super::theme::Theme;

#[derive(Debug, Clone)]
pub struct SpinnerState {
    pub start_time: Instant,
    pub token_count: usize,
    pub verb_idx: usize,
    pub last_verb_change: Instant,
    pub custom_verb: Option<String>,
}

impl Default for SpinnerState {
    fn default() -> Self {
        Self::new()
    }
}

impl SpinnerState {
    pub const VERBS: &'static [&'static str] = &["Retrieving", "Thinking", "Synthesizing", "Reading"];

    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            token_count: 0,
            verb_idx: 0,
            last_verb_change: Instant::now(),
            custom_verb: None,
        }
    }

    pub fn with_verb(verb: impl Into<String>) -> Self {
        let mut s = Self::new();
        s.custom_verb = Some(verb.into());
        s
    }

    pub fn add_tokens(&mut self, n: usize) {
        self.token_count += n;
    }

    pub fn current_verb(&mut self) -> &str {
        if let Some(ref v) = self.custom_verb {
            return v;
        }

        if self.last_verb_change.elapsed() > Duration::from_secs(3) {
            self.verb_idx = (self.verb_idx + 1) % Self::VERBS.len();
            self.last_verb_change = Instant::now();
        }
        Self::VERBS[self.verb_idx]
    }

    pub fn current_frame<'a>(&self, theme: &'a Theme) -> &'a str {
        let elapsed_ms = self.start_time.elapsed().as_millis();
        let frame_idx = ((elapsed_ms / 125) as usize) % theme.symbols.spinner.len();
        theme.symbols.spinner[frame_idx]
    }

    pub fn render_ratatui_line(&mut self, theme: &Theme) -> Line<'static> {
        let frame = self.current_frame(theme).to_string();
        let verb = format!("{}…", self.current_verb());
        let elapsed_secs = self.start_time.elapsed().as_secs();

        let mut stats = vec![
            "esc to interrupt".to_string(),
            format!("{elapsed_secs}s"),
        ];

        if self.token_count > 0 {
            stats.push(format!("↓ {} tokens", self.token_count));
        }

        let stats_str = format!("({})", stats.join(" · "));

        Line::from(vec![
            Span::raw("  "),
            Span::styled(frame, theme.style_accent()),
            Span::raw(" "),
            Span::styled(verb, theme.style_accent()),
            Span::raw(" "),
            Span::styled(stats_str, theme.style_muted()),
        ])
    }

    pub fn render_line(&mut self, theme: &Theme) -> String {
        let frame = self.current_frame(theme);
        let verb = self.current_verb().to_string();
        let elapsed_secs = self.start_time.elapsed().as_secs();

        let painted_frame = theme.paint_accent(frame);
        let painted_verb = theme.paint_accent(&format!("{verb}…"));

        let mut stats = vec![
            "esc to interrupt".to_string(),
            format!("{elapsed_secs}s"),
        ];

        if self.token_count > 0 {
            stats.push(format!("↓ {} tokens", self.token_count));
        }

        let stats_str = theme.paint_muted(&format!("({})", stats.join(" · ")));

        format!("  {painted_frame} {painted_verb} {stats_str}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spinner_frame_cycling() {
        let theme = Theme::detect();
        let spinner = SpinnerState::new();

        let frame = spinner.current_frame(&theme);
        assert!(theme.symbols.spinner.contains(&frame));
    }

    #[test]
    fn test_spinner_verb_rotation() {
        let mut spinner = SpinnerState::new();
        assert_eq!(spinner.current_verb(), "Retrieving");

        // Force time advance
        spinner.last_verb_change = Instant::now() - Duration::from_secs(4);
        assert_eq!(spinner.current_verb(), "Thinking");
    }

    #[test]
    fn test_spinner_custom_verb() {
        let mut spinner = SpinnerState::with_verb("Indexing");
        assert_eq!(spinner.current_verb(), "Indexing");
    }
}
