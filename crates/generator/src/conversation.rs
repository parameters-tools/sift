#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Turn {
    pub user: String,
    pub assistant: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationMemory {
    turns: Vec<Turn>,
    max_turns: usize,
}

impl Default for ConversationMemory {
    fn default() -> Self {
        Self::new(6)
    }
}

impl ConversationMemory {
    pub fn new(max_turns: usize) -> Self {
        Self {
            turns: Vec::new(),
            max_turns,
        }
    }

    pub fn add_turn(&mut self, user: impl Into<String>, assistant: impl Into<String>) {
        if self.max_turns == 0 {
            return;
        }
        if self.turns.len() >= self.max_turns {
            self.turns.remove(0);
        }
        self.turns.push(Turn {
            user: user.into(),
            assistant: assistant.into(),
        });
    }

    pub fn turns(&self) -> &[Turn] {
        &self.turns
    }

    pub fn is_empty(&self) -> bool {
        self.turns.is_empty()
    }

    pub fn len(&self) -> usize {
        self.turns.len()
    }

    pub fn clear(&mut self) {
        self.turns.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversation_memory_caps_at_max_turns() {
        let mut mem = ConversationMemory::new(6);
        assert!(mem.is_empty());

        for i in 1..=8 {
            mem.add_turn(format!("question {i}"), format!("answer {i}"));
        }

        assert_eq!(mem.len(), 6);
        let turns = mem.turns();
        assert_eq!(turns[0].user, "question 3");
        assert_eq!(turns[5].user, "question 8");
    }

    #[test]
    fn test_conversation_memory_clear() {
        let mut mem = ConversationMemory::new(6);
        mem.add_turn("hi", "hello");
        assert_eq!(mem.len(), 1);
        mem.clear();
        assert!(mem.is_empty());
        assert_eq!(mem.len(), 0);
    }
}
