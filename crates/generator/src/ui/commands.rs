#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandDef {
    pub name: &'static str,
    pub args: &'static str,
    pub description: &'static str,
    pub aliases: &'static [&'static str],
}

pub struct CommandRegistry;

impl CommandRegistry {
    pub const COMMANDS: &'static [CommandDef] = &[
        CommandDef {
            name: "/add",
            args: "<path|url>",
            description: "Index documents from a path or URL",
            aliases: &["/index"],
        },
        CommandDef {
            name: "/sources",
            args: "",
            description: "List indexed sources",
            aliases: &[],
        },
        CommandDef {
            name: "/status",
            args: "",
            description: "Show models, API base and index stats",
            aliases: &[],
        },
        CommandDef {
            name: "/model",
            args: "[chat|embed] [name]",
            description: "Switch chat or embedding model",
            aliases: &[],
        },
        CommandDef {
            name: "/chunks",
            args: "",
            description: "Show retrieved chunks and snippets from the last query",
            aliases: &[],
        },
        CommandDef {
            name: "/cite",
            args: "<n>",
            description: "Display the full text of chunk n in a bordered box",
            aliases: &[],
        },
        CommandDef {
            name: "/debug",
            args: "",
            description: "Show raw retriever scores and debug info for last query",
            aliases: &[],
        },
        CommandDef {
            name: "/clear",
            args: "",
            description: "Clear the screen and reset conversation history",
            aliases: &["/cls"],
        },
        CommandDef {
            name: "/new",
            args: "",
            description: "Start a fresh conversation without clearing the screen",
            aliases: &["/reset"],
        },
        CommandDef {
            name: "/help",
            args: "",
            description: "Show all commands",
            aliases: &["/h", "/?"],
        },
        CommandDef {
            name: "/exit",
            args: "",
            description: "Quit the shell",
            aliases: &["/quit", "/q"],
        },
    ];

    pub fn all() -> &'static [CommandDef] {
        Self::COMMANDS
    }

    pub fn filter(query: &str) -> Vec<&'static CommandDef> {
        let q = query.trim().to_lowercase();
        if q.is_empty() || q == "/" {
            return Self::COMMANDS.iter().collect();
        }

        Self::COMMANDS
            .iter()
            .filter(|cmd| {
                if q.starts_with('/') {
                    cmd.name.to_lowercase().starts_with(&q)
                        || cmd.aliases.iter().any(|a| a.to_lowercase().starts_with(&q))
                } else {
                    let slash_q = format!("/{q}");
                    cmd.name.to_lowercase().starts_with(&slash_q)
                        || cmd.aliases.iter().any(|a| a.to_lowercase().starts_with(&slash_q))
                        || cmd.description.to_lowercase().contains(&q)
                }
            })
            .collect()
    }

    pub fn find(name: &str) -> Option<&'static CommandDef> {
        let n = name.trim().to_lowercase();
        let target = if n.starts_with('/') { n } else { format!("/{n}") };

        Self::COMMANDS
            .iter()
            .find(|cmd| cmd.name == target || cmd.aliases.iter().any(|a| *a == target))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_empty_or_slash() {
        assert_eq!(CommandRegistry::filter("").len(), 11);
        assert_eq!(CommandRegistry::filter("/").len(), 11);
    }

    #[test]
    fn test_filter_prefix() {
        let s_cmds = CommandRegistry::filter("/s");
        let names: Vec<_> = s_cmds.iter().map(|c| c.name).collect();
        assert!(names.contains(&"/sources"));
        assert!(names.contains(&"/status"));
        assert!(!names.contains(&"/add"));

        let add = CommandRegistry::filter("/a");
        assert_eq!(add.len(), 1);
        assert_eq!(add[0].name, "/add");
    }

    #[test]
    fn test_filter_alias() {
        let idx = CommandRegistry::filter("/index");
        assert_eq!(idx.len(), 1);
        assert_eq!(idx[0].name, "/add");
    }

    #[test]
    fn test_find_command() {
        assert_eq!(CommandRegistry::find("/add").map(|c| c.name), Some("/add"));
        assert_eq!(CommandRegistry::find("add").map(|c| c.name), Some("/add"));
        assert_eq!(CommandRegistry::find("/index").map(|c| c.name), Some("/add"));
        assert_eq!(CommandRegistry::find("/status").map(|c| c.name), Some("/status"));
        assert_eq!(CommandRegistry::find("/unknown"), None);
    }
}
