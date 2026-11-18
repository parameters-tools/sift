use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub struct InputState {
    pub text: String,
    pub cursor: usize, // Character offset
    pub history: Vec<String>,
    pub history_idx: Option<usize>,
    pub saved_draft: String,
    pub history_path: PathBuf,
    pub placeholder_idx: usize,
    pub last_placeholder_tick: Instant,
    pub placeholders: Vec<String>,
    pub shortcuts_open: bool,
    pub last_ctrl_c: Option<Instant>,
}

pub fn generate_placeholders(sources: &[String]) -> Vec<String> {
    if sources.is_empty() {
        return vec!["Type /add <path> to index your documents".to_string()];
    }

    let templates: &[fn(&str) -> String] = &[
        |file| format!("Try \"Summarize {file}\""),
        |file| format!("Try \"What are the key points of {file}?\""),
        |file| format!("Try \"Explain the main findings in {file}\""),
        |file| format!("Try \"What does {file} say about...?\""),
    ];

    let mut placeholders = Vec::new();
    for (i, src) in sources.iter().enumerate() {
        let raw_name = std::path::Path::new(src)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(src);

        let file_name = if raw_name.chars().count() > 30 {
            let truncated: String = raw_name.chars().take(29).collect();
            format!("{truncated}…")
        } else {
            raw_name.to_string()
        };

        let template_fn = templates[i % templates.len()];
        placeholders.push(template_fn(&file_name));
    }

    placeholders
}

impl InputState {
    pub fn new(history_dir: &Path) -> Self {
        let history_path = history_dir.join("history");
        let history = Self::load_history(&history_path);

        Self {
            text: String::new(),
            cursor: 0,
            history,
            history_idx: None,
            saved_draft: String::new(),
            history_path,
            placeholder_idx: 0,
            last_placeholder_tick: Instant::now(),
            placeholders: generate_placeholders(&[]),
            shortcuts_open: false,
            last_ctrl_c: None,
        }
    }

    pub fn set_sources(&mut self, sources: &[String]) {
        self.placeholders = generate_placeholders(sources);
        self.placeholder_idx = 0;
        self.last_placeholder_tick = Instant::now();
    }

    pub fn current_placeholder(&mut self) -> &str {
        if self.placeholders.is_empty() {
            return "Type /add <path> to index your documents";
        }
        if self.last_placeholder_tick.elapsed() > Duration::from_secs(6) {
            self.placeholder_idx = (self.placeholder_idx + 1) % self.placeholders.len();
            self.last_placeholder_tick = Instant::now();
        }
        &self.placeholders[self.placeholder_idx]
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn insert_char(&mut self, c: char) {
        if self.is_empty() && c == '?' {
            // Typing '?' on empty input toggles shortcuts panel
            self.shortcuts_open = !self.shortcuts_open;
            return;
        }

        // Close shortcuts panel if typing regular text
        self.shortcuts_open = false;
        self.last_ctrl_c = None;

        let mut chars: Vec<char> = self.text.chars().collect();
        let idx = self.cursor.min(chars.len());
        chars.insert(idx, c);
        self.text = chars.into_iter().collect();
        self.cursor += 1;
    }

    pub fn insert_str(&mut self, s: &str) {
        self.shortcuts_open = false;
        self.last_ctrl_c = None;
        for c in s.chars() {
            self.insert_char(c);
        }
    }

    pub fn insert_newline(&mut self) {
        self.insert_char('\n');
    }

    pub fn delete_backspace(&mut self) {
        if self.cursor == 0 || self.text.is_empty() {
            return;
        }
        let mut chars: Vec<char> = self.text.chars().collect();
        if self.cursor <= chars.len() {
            chars.remove(self.cursor - 1);
            self.cursor -= 1;
            self.text = chars.into_iter().collect();
        }
    }

    pub fn delete_forward(&mut self) {
        let mut chars: Vec<char> = self.text.chars().collect();
        if self.cursor < chars.len() {
            chars.remove(self.cursor);
            self.text = chars.into_iter().collect();
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_cursor_right(&mut self) {
        let len = self.text.chars().count();
        if self.cursor < len {
            self.cursor += 1;
        }
    }

    pub fn move_cursor_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_cursor_end(&mut self) {
        self.cursor = self.text.chars().count();
    }

    pub fn delete_to_start(&mut self) {
        let chars: Vec<char> = self.text.chars().collect();
        if self.cursor <= chars.len() {
            let remainder: String = chars[self.cursor..].iter().collect();
            self.text = remainder;
            self.cursor = 0;
        }
    }

    pub fn delete_to_end(&mut self) {
        let chars: Vec<char> = self.text.chars().collect();
        if self.cursor <= chars.len() {
            let prefix: String = chars[..self.cursor].iter().collect();
            self.text = prefix;
        }
    }

    pub fn delete_word_backward(&mut self) {
        if self.cursor == 0 || self.text.is_empty() {
            return;
        }
        let chars: Vec<char> = self.text.chars().collect();
        let mut idx = self.cursor;
        while idx > 0 && chars[idx - 1].is_whitespace() {
            idx -= 1;
        }
        while idx > 0 && !chars[idx - 1].is_whitespace() {
            idx -= 1;
        }
        let mut new_chars = Vec::new();
        new_chars.extend_from_slice(&chars[..idx]);
        new_chars.extend_from_slice(&chars[self.cursor..]);
        self.cursor = idx;
        self.text = new_chars.into_iter().collect();
    }

    pub fn is_multiline(&self) -> bool {
        self.text.contains('\n')
    }

    pub fn cursor_line_and_col(&self) -> (usize, usize, usize) {
        let mut line_idx = 0;
        let mut col_idx = 0;
        let mut current_offset = 0;
        let lines: Vec<&str> = self.text.split('\n').collect();
        let total = lines.len();
        for (i, line) in lines.iter().enumerate() {
            let len = line.chars().count();
            if self.cursor >= current_offset && self.cursor <= current_offset + len {
                line_idx = i;
                col_idx = self.cursor - current_offset;
                break;
            }
            current_offset += len + 1;
        }
        (line_idx, col_idx, total)
    }

    pub fn move_cursor_up_line(&mut self) -> bool {
        if !self.is_multiline() {
            return false;
        }
        let (line_idx, col_idx, _) = self.cursor_line_and_col();
        if line_idx == 0 {
            return false;
        }
        let lines: Vec<&str> = self.text.split('\n').collect();
        let prev_line_len = lines[line_idx - 1].chars().count();
        let target_col = col_idx.min(prev_line_len);
        let mut offset = 0;
        for line in &lines[..line_idx - 1] {
            offset += line.chars().count() + 1;
        }
        self.cursor = offset + target_col;
        true
    }

    pub fn move_cursor_down_line(&mut self) -> bool {
        if !self.is_multiline() {
            return false;
        }
        let (line_idx, col_idx, total) = self.cursor_line_and_col();
        if line_idx + 1 >= total {
            return false;
        }
        let lines: Vec<&str> = self.text.split('\n').collect();
        let next_line_len = lines[line_idx + 1].chars().count();
        let target_col = col_idx.min(next_line_len);
        let mut offset = 0;
        for line in &lines[..=line_idx] {
            offset += line.chars().count() + 1;
        }
        self.cursor = offset + target_col;
        true
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.history_idx = None;
        self.saved_draft.clear();
    }

    pub fn submit(&mut self) -> Option<String> {
        let trimmed = self.text.trim();
        if trimmed.is_empty() {
            return None;
        }

        let submitted = self.text.clone();
        self.add_history(&submitted);
        self.clear();
        Some(submitted)
    }

    pub fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }

        match self.history_idx {
            None => {
                self.saved_draft = self.text.clone();
                let last_idx = self.history.len() - 1;
                self.history_idx = Some(last_idx);
                self.text = self.history[last_idx].clone();
                self.cursor = self.text.chars().count();
            }
            Some(idx) => {
                if idx > 0 {
                    let new_idx = idx - 1;
                    self.history_idx = Some(new_idx);
                    self.text = self.history[new_idx].clone();
                    self.cursor = self.text.chars().count();
                }
            }
        }
    }

    pub fn history_down(&mut self) {
        if let Some(idx) = self.history_idx {
            if idx + 1 < self.history.len() {
                let new_idx = idx + 1;
                self.history_idx = Some(new_idx);
                self.text = self.history[new_idx].clone();
                self.cursor = self.text.chars().count();
            } else {
                self.history_idx = None;
                self.text = self.saved_draft.clone();
                self.cursor = self.text.chars().count();
            }
        }
    }

    fn add_history(&mut self, entry: &str) {
        let s = entry.trim();
        if s.is_empty() {
            return;
        }
        if self.history.last().map(|last| last == s).unwrap_or(false) {
            return; // Don't duplicate consecutive identical entries
        }

        self.history.push(s.to_string());
        if self.history.len() > 1000 {
            self.history.remove(0);
        }
        self.save_history();
    }

    fn load_history(path: &Path) -> Vec<String> {
        if let Ok(content) = std::fs::read_to_string(path) {
            content
                .lines()
                .map(|line| line.trim().to_string())
                .filter(|line| !line.is_empty())
                .collect()
        } else {
            Vec::new()
        }
    }

    fn save_history(&self) {
        if let Some(parent) = self.history_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let content = self.history.join("\n");
        let _ = std::fs::write(&self.history_path, content);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_insert_and_delete() {
        let tmp = std::env::temp_dir().join(format!("rag-input-test-{}", uuid::Uuid::new_v4()));
        let mut input = InputState::new(&tmp);

        input.insert_char('h');
        input.insert_char('e');
        input.insert_char('l');
        input.insert_char('l');
        input.insert_char('o');
        assert_eq!(input.text, "hello");
        assert_eq!(input.cursor, 5);

        input.delete_backspace();
        assert_eq!(input.text, "hell");
        assert_eq!(input.cursor, 4);

        input.move_cursor_left();
        input.delete_forward();
        assert_eq!(input.text, "hel");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_input_history_navigation() {
        let tmp = std::env::temp_dir().join(format!("rag-input-test-{}", uuid::Uuid::new_v4()));
        let mut input = InputState::new(&tmp);

        input.insert_str("first question");
        input.submit();

        input.insert_str("second question");
        input.submit();

        assert_eq!(input.history.len(), 2);

        // Walk up history
        input.history_up();
        assert_eq!(input.text, "second question");

        input.history_up();
        assert_eq!(input.text, "first question");

        // Walk down history
        input.history_down();
        assert_eq!(input.text, "second question");

        input.history_down();
        assert_eq!(input.text, "");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_question_mark_toggles_shortcuts() {
        let tmp = std::env::temp_dir().join(format!("rag-input-test-{}", uuid::Uuid::new_v4()));
        let mut input = InputState::new(&tmp);

        assert!(!input.shortcuts_open);
        input.insert_char('?');
        assert!(input.shortcuts_open);
        assert!(input.text.is_empty()); // Should not add '?' to text

        input.insert_char('?');
        assert!(!input.shortcuts_open);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_line_editing_helpers() {
        let tmp = std::env::temp_dir().join(format!("rag-input-test-{}", uuid::Uuid::new_v4()));
        let mut input = InputState::new(&tmp);

        input.insert_str("hello wonderful world");
        assert_eq!(input.cursor, 21);

        input.delete_word_backward();
        assert_eq!(input.text, "hello wonderful ");
        assert_eq!(input.cursor, 16);

        input.move_cursor_home();
        input.move_cursor_right(); // cursor at 1 ("h|ello")
        input.delete_to_end();
        assert_eq!(input.text, "h");

        input.insert_str("ey there");
        input.move_cursor_end();
        input.move_cursor_left(); // at "e" in there
        input.delete_to_start();
        assert_eq!(input.text, "e");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_multiline_navigation() {
        let tmp = std::env::temp_dir().join(format!("rag-input-test-{}", uuid::Uuid::new_v4()));
        let mut input = InputState::new(&tmp);

        input.insert_str("line 1\nline 2 longer\nline 3");
        assert!(input.is_multiline());

        // Cursor starts at end of line 3
        let (row, col, total) = input.cursor_line_and_col();
        assert_eq!(row, 2);
        assert_eq!(col, 6);
        assert_eq!(total, 3);

        // Move up to line 2
        assert!(input.move_cursor_up_line());
        let (row, col, _) = input.cursor_line_and_col();
        assert_eq!(row, 1);
        assert_eq!(col, 6);

        // Move up to line 1
        assert!(input.move_cursor_up_line());
        let (row, col, _) = input.cursor_line_and_col();
        assert_eq!(row, 0);
        assert_eq!(col, 6);

        // Can't move up further
        assert!(!input.move_cursor_up_line());

        // Move down to line 2
        assert!(input.move_cursor_down_line());
        let (row, _, _) = input.cursor_line_and_col();
        assert_eq!(row, 1);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_placeholder_generation_0_1_and_5_sources() {
        // 0 sources
        let ph0 = generate_placeholders(&[]);
        assert_eq!(ph0.len(), 1);
        assert_eq!(ph0[0], "Type /add <path> to index your documents");

        // 1 source
        let ph1 = generate_placeholders(&["DeepSeek_V41_Tech_Report.pdf".to_string()]);
        assert_eq!(ph1.len(), 1);
        assert!(ph1[0].contains("DeepSeek_V41_Tech_Report.pdf"));
        assert!(ph1[0].starts_with("Try \""));

        // 5 sources (including one with a long name > 30 chars to test truncation with "…")
        let sources = vec![
            "ShortDoc.md".to_string(),
            "VeryLongDocumentNameExceedingThirtyCharactersTotalReport.pdf".to_string(),
            "SnehilResume.pdf".to_string(),
            "QuarterlyFinancials.xlsx".to_string(),
            "SystemArchitectureOverview.rst".to_string(),
        ];
        let ph5 = generate_placeholders(&sources);
        assert_eq!(ph5.len(), 5);
        assert!(ph5[0].contains("ShortDoc.md"));
        assert!(ph5[1].contains('…')); // truncated long file name
        assert!(ph5[2].contains("SnehilResume.pdf"));
        assert!(ph5[3].contains("QuarterlyFinancials.xlsx"));
        assert!(ph5[4].contains("SystemArchitectureOverview.rst"));

        for ph in &ph5 {
            assert!(ph.starts_with("Try \""));
        }
    }
}
