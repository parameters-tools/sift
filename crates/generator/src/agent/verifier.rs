use std::collections::{HashMap, HashSet};
use crate::ui::messages::{MessageFormatter, RetrievedChunk};

#[derive(Debug, Clone, PartialEq)]
pub struct VerificationResult {
    pub is_verified: bool,
    pub verified_citations: Vec<usize>,
    pub ungrounded_citations: Vec<usize>,
    pub conflicts: Vec<String>,
    pub detail: String,
    pub elapsed: f64,
}

/// Verifies whether citations [n] in the generated answer are grounded in retrieved chunks,
/// and inspects chunks across distinct sources for potential factual discrepancies.
pub fn verify_answer(
    answer: &str,
    chunks: &[RetrievedChunk],
    elapsed: f64,
) -> VerificationResult {
    let cited_ranks = MessageFormatter::extract_citations(answer);
    if cited_ranks.is_empty() {
        return VerificationResult {
            is_verified: true,
            verified_citations: Vec::new(),
            ungrounded_citations: Vec::new(),
            conflicts: Vec::new(),
            detail: "No citations present to verify".to_string(),
            elapsed,
        };
    }

    let chunk_map: HashMap<usize, &RetrievedChunk> =
        chunks.iter().map(|c| (c.rank, c)).collect();

    let mut verified = Vec::new();
    let mut ungrounded = Vec::new();

    // Map sentences to their inline citations
    let sentences = split_sentences(answer);
    for rank in &cited_ranks {
        if let Some(chunk) = chunk_map.get(rank) {
            let marker = format!("[{rank}]");
            let citing_sentences: Vec<&str> = sentences
                .iter()
                .filter(|s| s.contains(&marker))
                .copied()
                .collect();

            let is_grounded = if citing_sentences.is_empty() {
                // Citation in list or standalone
                true
            } else {
                citing_sentences.iter().any(|s| check_token_overlap(s, &chunk.text))
            };

            if is_grounded {
                verified.push(*rank);
            } else {
                ungrounded.push(*rank);
            }
        } else {
            ungrounded.push(*rank);
        }
    }

    // Conflict detection across different source documents
    let conflicts = detect_source_conflicts(chunks);

    let is_verified = ungrounded.is_empty() && conflicts.is_empty();

    // Unique source count of verified citations
    let mut verified_sources = HashSet::new();
    for rank in &verified {
        if let Some(chunk) = chunk_map.get(rank) {
            verified_sources.insert(&chunk.file);
        }
    }

    let detail = if !ungrounded.is_empty() {
        format!(
            "⚠ {} ungrounded citation(s) [{}] detected",
            ungrounded.len(),
            ungrounded.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ")
        )
    } else if !conflicts.is_empty() {
        format!("⚠ Discrepancy detected across sources: {}", conflicts[0])
    } else {
        let count = verified_sources.len();
        let s_word = if count == 1 { "source" } else { "sources" };
        format!("✔ Verified {} citation(s) across {count} {s_word}", verified.len())
    };

    VerificationResult {
        is_verified,
        verified_citations: verified,
        ungrounded_citations: ungrounded,
        conflicts,
        detail,
        elapsed,
    }
}

fn split_sentences(text: &str) -> Vec<&str> {
    text.split(['.', '\n', '!', '?'])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect()
}

fn check_token_overlap(sentence: &str, chunk_text: &str) -> bool {
    let chunk_lower = chunk_text.to_lowercase();
    let sentence_tokens: Vec<&str> = sentence
        .split_whitespace()
        .map(|s| s.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|s| s.len() >= 4 && !is_stopword(s))
        .collect();

    if sentence_tokens.is_empty() {
        return true;
    }

    let matched_count = sentence_tokens
        .iter()
        .filter(|&&t| chunk_lower.contains(&t.to_lowercase()))
        .count();

    // At least 25% of significant content tokens or at least 2 tokens match
    matched_count >= 2 || (matched_count as f32 / sentence_tokens.len() as f32) >= 0.25
}

fn is_stopword(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "this" | "that" | "with" | "from" | "have" | "were" | "which" | "their" | "there" | "about" | "would" | "these" | "other"
    )
}

/// Detects conflicting statements or polarities across different document sources.
fn detect_source_conflicts(chunks: &[RetrievedChunk]) -> Vec<String> {
    let mut conflicts = Vec::new();
    let mut by_source: HashMap<&str, Vec<&RetrievedChunk>> = HashMap::new();
    for c in chunks {
        by_source.entry(c.file.as_str()).or_default().push(c);
    }

    if by_source.len() < 2 {
        return conflicts;
    }

    // Check for opposing polarity keywords on common topics
    let polarity_pairs = [
        ("allowed", "prohibited"),
        ("permitted", "forbidden"),
        ("required", "optional"),
        ("mandatory", "voluntary"),
    ];

    let source_keys: Vec<&str> = by_source.keys().copied().collect();
    for i in 0..source_keys.len() {
        for j in (i + 1)..source_keys.len() {
            let src_a = source_keys[i];
            let src_b = source_keys[j];
            let text_a: String = by_source[src_a].iter().map(|c| c.text.to_lowercase()).collect::<Vec<_>>().join(" ");
            let text_b: String = by_source[src_b].iter().map(|c| c.text.to_lowercase()).collect::<Vec<_>>().join(" ");

            for &(pos_word, neg_word) in &polarity_pairs {
                if (text_a.contains(pos_word) && text_b.contains(neg_word))
                    || (text_a.contains(neg_word) && text_b.contains(pos_word))
                {
                    conflicts.push(format!(
                        "Polarity contrast ('{pos_word}' vs '{neg_word}') between {src_a} and {src_b}"
                    ));
                    break;
                }
            }
        }
    }

    conflicts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_grounded_answer() {
        let chunks = vec![
            RetrievedChunk {
                rank: 1,
                file: "refund_policy.md".into(),
                full_path: "/docs/refund_policy.md".into(),
                page: None,
                text: "Full refunds are guaranteed within 30 days of initial purchase with valid receipt.".into(),
                score: 0.8,
                is_cited: true,
            },
        ];

        let answer = "Customers are guaranteed a full refund within 30 days of purchase with a receipt [1].";
        let res = verify_answer(answer, &chunks, 0.012);
        assert!(res.is_verified);
        assert_eq!(res.verified_citations, vec![1]);
        assert!(res.ungrounded_citations.is_empty());
        assert!(res.detail.contains("Verified 1 citation(s) across 1 source"));
    }

    #[test]
    fn test_verify_ungrounded_citation() {
        let chunks = vec![
            RetrievedChunk {
                rank: 1,
                file: "refund_policy.md".into(),
                full_path: "/docs/refund_policy.md".into(),
                page: None,
                text: "Refunds take 5-7 business days to process.".into(),
                score: 0.8,
                is_cited: true,
            },
        ];

        // Citation 2 does not exist
        let answer = "The policy states that cryptocurrency payments are accepted [2].";
        let res = verify_answer(answer, &chunks, 0.012);
        assert!(!res.is_verified);
        assert_eq!(res.ungrounded_citations, vec![2]);
        assert!(res.detail.contains("ungrounded citation(s) [2]"));
    }

    #[test]
    fn test_detect_source_conflicts() {
        let chunks = vec![
            RetrievedChunk {
                rank: 1,
                file: "2023_policy.md".into(),
                full_path: "/docs/2023_policy.md".into(),
                page: None,
                text: "Remote work is allowed for all full-time employees.".into(),
                score: 0.8,
                is_cited: true,
            },
            RetrievedChunk {
                rank: 2,
                file: "2024_policy.md".into(),
                full_path: "/docs/2024_policy.md".into(),
                page: None,
                text: "Remote work is strictly prohibited without executive approval.".into(),
                score: 0.75,
                is_cited: true,
            },
        ];

        let conflicts = detect_source_conflicts(&chunks);
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].contains("Polarity contrast"));
        assert!(conflicts[0].contains("2023_policy.md"));
        assert!(conflicts[0].contains("2024_policy.md"));
    }
}
