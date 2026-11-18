use anyhow::Result;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub enum ReplAction {
    Ask(String),
    Index(PathBuf),
    Search(String),
    Stats,
    Sources,
    Help,
    Clear,
    Exit,
    Empty,
    Unknown(String),
}

pub fn parse_line(line: &str) -> ReplAction {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return ReplAction::Empty;
    }

    if let Some(cmd) = trimmed.strip_prefix('/') {
        let (name, arg) = cmd.split_once(char::is_whitespace).unwrap_or((cmd, ""));
        let arg = arg.trim();

        match name.to_ascii_lowercase().as_str() {
            "help" | "h" | "?" => ReplAction::Help,
            "exit" | "quit" | "q" => ReplAction::Exit,
            "clear" | "cls" => ReplAction::Clear,
            "stats" => ReplAction::Stats,
            "sources" => ReplAction::Sources,
            "index" => {
                if arg.is_empty() {
                    ReplAction::Unknown("Usage: /index <path-to-file-or-dir>".to_string())
                } else {
                    ReplAction::Index(PathBuf::from(arg))
                }
            }
            "search" => {
                if arg.is_empty() {
                    ReplAction::Unknown("Usage: /search <query>".to_string())
                } else {
                    ReplAction::Search(arg.to_string())
                }
            }
            _ => ReplAction::Unknown(format!("Unknown command '/{}'. Type /help for available commands.", name)),
        }
    } else {
        ReplAction::Ask(trimmed.to_string())
    }
}

pub async fn run_interactive(db_path: &Path) -> Result<()> {
    crate::ui::app::run(db_path).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_line_empty() {
        assert_eq!(parse_line(""), ReplAction::Empty);
        assert_eq!(parse_line("   \n\t"), ReplAction::Empty);
    }

    #[test]
    fn test_parse_line_help() {
        assert_eq!(parse_line("/help"), ReplAction::Help);
        assert_eq!(parse_line("/h"), ReplAction::Help);
        assert_eq!(parse_line("/?"), ReplAction::Help);
    }

    #[test]
    fn test_parse_line_exit() {
        assert_eq!(parse_line("/exit"), ReplAction::Exit);
        assert_eq!(parse_line("/quit"), ReplAction::Exit);
        assert_eq!(parse_line("/q"), ReplAction::Exit);
    }

    #[test]
    fn test_parse_line_clear_and_stats() {
        assert_eq!(parse_line("/clear"), ReplAction::Clear);
        assert_eq!(parse_line("/cls"), ReplAction::Clear);
        assert_eq!(parse_line("/stats"), ReplAction::Stats);
        assert_eq!(parse_line("/sources"), ReplAction::Sources);
    }

    #[test]
    fn test_parse_line_index() {
        assert_eq!(parse_line("/index ./docs"), ReplAction::Index(PathBuf::from("./docs")));
        assert!(matches!(parse_line("/index"), ReplAction::Unknown(_)));
    }

    #[test]
    fn test_parse_line_search() {
        assert_eq!(parse_line("/search refund policy"), ReplAction::Search("refund policy".to_string()));
        assert!(matches!(parse_line("/search"), ReplAction::Unknown(_)));
    }

    #[test]
    fn test_parse_line_ask() {
        assert_eq!(
            parse_line("What is the refund timeline?"),
            ReplAction::Ask("What is the refund timeline?".to_string())
        );
    }

    #[test]
    fn test_parse_line_unknown() {
        assert!(matches!(parse_line("/foobar"), ReplAction::Unknown(_)));
    }
}
