use ratatui::style::{Color, Modifier, Style};
use std::io::IsTerminal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BorderChars {
    pub top_left: char,
    pub top_right: char,
    pub bottom_left: char,
    pub bottom_right: char,
    pub horizontal: char,
    pub vertical: char,
}

impl BorderChars {
    pub const UNICODE_ROUNDED: Self = Self {
        top_left: '╭',
        top_right: '╮',
        bottom_left: '╰',
        bottom_right: '╯',
        horizontal: '─',
        vertical: '│',
    };

    pub const ASCII: Self = Self {
        top_left: '+',
        top_right: '+',
        bottom_left: '+',
        bottom_right: '+',
        horizontal: '-',
        vertical: '|',
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Symbols {
    pub bullet: &'static str,
    pub sub_item: &'static str,
    pub sparkle: &'static str,
    pub prompt: &'static str,
    pub spinner: &'static [&'static str],
}

impl Symbols {
    pub const UNICODE: Self = Self {
        bullet: "●",
        sub_item: "⎿",
        sparkle: "✻",
        prompt: "❯",
        spinner: &["·", "✢", "✳", "∗", "✻", "✽"],
    };

    pub const ASCII: Self = Self {
        bullet: "*",
        sub_item: "|_",
        sparkle: "*",
        prompt: ">",
        spinner: &["-", "\\", "|", "/", "-", "\\"],
    };
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub use_color: bool,
    pub use_unicode: bool,
    pub is_tty: bool,
    pub truecolor: bool,
    pub accent: Color,
    pub success: Color,
    pub error: Color,
    pub warning: Color,
    pub muted: Color,
    pub text: Color,
    pub borders: BorderChars,
    pub symbols: Symbols,
}

impl Default for Theme {
    fn default() -> Self {
        Self::detect()
    }
}

impl Theme {
    pub fn detect() -> Self {
        let is_tty = std::io::stdout().is_terminal() && std::io::stdin().is_terminal();
        let no_color = std::env::var_os("NO_COLOR").is_some() || std::env::var("TERM").map(|t| t == "dumb").unwrap_or(false);
        let use_color = is_tty && !no_color;

        let truecolor = use_color
            && (std::env::var("COLORTERM").map(|c| c.contains("truecolor") || c.contains("24bit")).unwrap_or(false)
                || std::env::var("TERM_PROGRAM").is_ok());

        let use_unicode = detect_utf8_support();

        let accent = if !use_color {
            Color::Reset
        } else if truecolor {
            Color::Rgb(0xD9, 0x77, 0x57) // Warm orange (#D97757)
        } else {
            Color::Indexed(208) // 256-color orange fallback
        };

        let success = if use_color { Color::Green } else { Color::Reset };
        let error = if use_color { Color::Red } else { Color::Reset };
        let warning = if use_color { Color::Yellow } else { Color::Reset };
        let muted = if use_color { Color::DarkGray } else { Color::Reset };
        let text = if use_color { Color::White } else { Color::Reset };

        let borders = if use_unicode {
            BorderChars::UNICODE_ROUNDED
        } else {
            BorderChars::ASCII
        };

        let symbols = if use_unicode {
            Symbols::UNICODE
        } else {
            Symbols::ASCII
        };

        Self {
            use_color,
            use_unicode,
            is_tty,
            truecolor,
            accent,
            success,
            error,
            warning,
            muted,
            text,
            borders,
            symbols,
        }
    }

    pub fn no_color() -> Self {
        let mut t = Self::detect();
        t.use_color = false;
        t.accent = Color::Reset;
        t.success = Color::Reset;
        t.error = Color::Reset;
        t.warning = Color::Reset;
        t.muted = Color::Reset;
        t.text = Color::Reset;
        t
    }

    pub fn ascii() -> Self {
        let mut t = Self::detect();
        t.use_unicode = false;
        t.borders = BorderChars::ASCII;
        t.symbols = Symbols::ASCII;
        t
    }

    // Ratatui style helpers
    pub fn style_accent(&self) -> Style {
        if self.use_color {
            Style::default().fg(self.accent)
        } else {
            Style::default()
        }
    }

    pub fn style_muted(&self) -> Style {
        if self.use_color {
            Style::default().fg(self.muted)
        } else {
            Style::default()
        }
    }

    pub fn style_success(&self) -> Style {
        if self.use_color {
            Style::default().fg(self.success)
        } else {
            Style::default()
        }
    }

    pub fn style_error(&self) -> Style {
        if self.use_color {
            Style::default().fg(self.error)
        } else {
            Style::default()
        }
    }

    pub fn style_warning(&self) -> Style {
        if self.use_color {
            Style::default().fg(self.warning)
        } else {
            Style::default()
        }
    }

    pub fn style_bold(&self) -> Style {
        Style::default().add_modifier(Modifier::BOLD)
    }

    // Terminal ANSI string helpers for scrollback printing
    pub fn paint_accent(&self, text: &str) -> String {
        if !self.use_color {
            text.to_string()
        } else if self.truecolor {
            format!("\x1b[38;2;217;119;87m{text}\x1b[0m")
        } else {
            format!("\x1b[38;5;208m{text}\x1b[0m")
        }
    }

    pub fn paint_muted(&self, text: &str) -> String {
        if self.use_color {
            format!("\x1b[2m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    pub fn paint_bold(&self, text: &str) -> String {
        if self.is_tty {
            format!("\x1b[1m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    pub fn paint_success(&self, text: &str) -> String {
        if self.use_color {
            format!("\x1b[32m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    pub fn paint_warning(&self, text: &str) -> String {
        if self.use_color {
            format!("\x1b[33m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    pub fn paint_error(&self, text: &str) -> String {
        if self.use_color {
            format!("\x1b[31m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }
}

fn detect_utf8_support() -> bool {
    // Check standard environment variables for utf-8 locale
    for var in ["LC_ALL", "LC_CTYPE", "LANG"] {
        if let Ok(val) = std::env::var(var) {
            let val_lower = val.to_lowercase();
            if val_lower.contains("utf-8") || val_lower.contains("utf8") {
                return true;
            }
        }
    }
    // Default to true on modern Linux / Unix systems unless explicitly set to C / POSIX
    if let Ok(lang) = std::env::var("LANG") {
        return lang != "C" && lang != "POSIX";
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_symbols() {
        let unicode = Symbols::UNICODE;
        assert_eq!(unicode.bullet, "●");
        assert_eq!(unicode.sub_item, "⎿");
        assert_eq!(unicode.sparkle, "✻");
        assert_eq!(unicode.prompt, "❯");
        assert_eq!(unicode.spinner.len(), 6);

        let ascii = Symbols::ASCII;
        assert_eq!(ascii.bullet, "*");
        assert_eq!(ascii.sub_item, "|_");
        assert_eq!(ascii.sparkle, "*");
        assert_eq!(ascii.prompt, ">");
        assert_eq!(ascii.spinner.len(), 6);
    }

    #[test]
    fn test_theme_borders() {
        let rounded = BorderChars::UNICODE_ROUNDED;
        assert_eq!(rounded.top_left, '╭');
        assert_eq!(rounded.top_right, '╮');
        assert_eq!(rounded.bottom_left, '╰');
        assert_eq!(rounded.bottom_right, '╯');
        assert_eq!(rounded.horizontal, '─');
        assert_eq!(rounded.vertical, '│');

        let ascii = BorderChars::ASCII;
        assert_eq!(ascii.top_left, '+');
        assert_eq!(ascii.top_right, '+');
        assert_eq!(ascii.bottom_left, '+');
        assert_eq!(ascii.bottom_right, '+');
        assert_eq!(ascii.horizontal, '-');
        assert_eq!(ascii.vertical, '|');
    }

    #[test]
    fn test_no_color_theme() {
        let theme = Theme::no_color();
        assert!(!theme.use_color);
        assert_eq!(theme.paint_accent("hello"), "hello");
        assert_eq!(theme.paint_muted("dim"), "dim");
        assert_eq!(theme.paint_success("ok"), "ok");
        assert_eq!(theme.paint_error("err"), "err");
        assert_eq!(theme.style_accent(), Style::default());
    }

    #[test]
    fn test_theme_ansi_painting() {
        let mut theme = Theme::detect();
        theme.use_color = true;
        theme.truecolor = true;
        let painted = theme.paint_accent("test");
        assert!(painted.contains("\x1b[38;2;217;119;87m"));
        assert!(painted.contains("test"));

        theme.truecolor = false;
        let painted_256 = theme.paint_accent("test");
        assert!(painted_256.contains("\x1b[38;5;208m"));
    }
}
