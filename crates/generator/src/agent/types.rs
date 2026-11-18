#[derive(Debug, Clone, PartialEq)]
pub enum AgentStep {
    /// Sub-goal decomposition (Plan-and-solve)
    Plan {
        goals: Vec<String>,
        elapsed: f64,
    },
    /// Tool invocation (Retrieve, Calculate, Index, etc.)
    Tool {
        name: String,
        args: String,
        detail: String,
        elapsed: f64,
        is_weak: bool,
        no_strong_match: bool,
    },
    /// Self-RAG verification & conflict check
    Verification {
        is_verified: bool,
        verified_count: usize,
        total_citations: usize,
        detail: String,
        conflicts: Vec<String>,
        elapsed: f64,
    },
    /// Interactive clarification / disambiguation
    Clarification {
        topic: String,
        options: Vec<String>,
        detail: String,
        elapsed: f64,
    },
}
