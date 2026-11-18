use crate::db::Chunk;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub chunk: Chunk,
    pub score: f32,
    pub cosine: f32,
}

fn terms(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.chars().count() > 1)
        .map(|s| s.to_lowercase())
        .collect()
}
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let (mut dot, mut aa, mut bb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        dot += x * y;
        aa += x * x;
        bb += y * y;
    }
    if aa == 0.0 || bb == 0.0 {
        0.0
    } else {
        dot / (aa.sqrt() * bb.sqrt())
    }
}

/// Reciprocal rank fusion of dense cosine and lexical overlap rankings.
pub fn hybrid(chunks: Vec<Chunk>, question: &str, query: &[f32], limit: usize) -> Vec<Hit> {
    let qterms = terms(question);
    let cosine_scores: Vec<f32> = chunks
        .iter()
        .map(|c| cosine(query, &c.vector))
        .collect();
    let mut dense: Vec<(usize, f32)> = cosine_scores
        .iter()
        .copied()
        .enumerate()
        .collect();
    let mut lexical: Vec<(usize, f32)> = chunks
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let doc = terms(&c.text);
            let score = if qterms.is_empty() {
                0.0
            } else {
                qterms.intersection(&doc).count() as f32 / qterms.len() as f32
            };
            (i, score)
        })
        .collect();
    dense.sort_by(|a, b| b.1.total_cmp(&a.1));
    lexical.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut scores = vec![0.0; chunks.len()];
    for (rank, (i, _)) in dense.iter().enumerate() {
        scores[*i] += 0.65 / (60.0 + rank as f32 + 1.0);
    }
    for (rank, (i, score)) in lexical.iter().enumerate() {
        if *score > 0.0 {
            scores[*i] += 0.35 / (60.0 + rank as f32 + 1.0);
        }
    }
    let mut hits: Vec<Hit> = chunks
        .into_iter()
        .enumerate()
        .map(|(i, chunk)| Hit {
            chunk,
            score: scores[i],
            cosine: cosine_scores[i],
        })
        .collect();
    hits.sort_by(|a, b| b.score.total_cmp(&a.score));
    hits.truncate(limit);
    hits
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RetrievalFilterConfig {
    pub relevance_threshold: f32,
    pub top_k: usize,
}

impl Default for RetrievalFilterConfig {
    fn default() -> Self {
        let relevance_threshold = std::env::var("RAG_RELEVANCE_THRESHOLD")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.50);
        Self {
            relevance_threshold,
            top_k: 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FilteredRetrieval {
    pub hits: Vec<Hit>,
    pub filtered_count: usize,
    pub no_strong_match: bool,
}

pub fn filter_by_relevance(hits: Vec<Hit>, config: &RetrievalFilterConfig) -> FilteredRetrieval {
    if hits.is_empty() {
        return FilteredRetrieval {
            hits: Vec::new(),
            filtered_count: 0,
            no_strong_match: true,
        };
    }

    let initial_count = hits.len();
    let passing: Vec<Hit> = hits
        .iter()
        .filter(|h| h.cosine >= config.relevance_threshold)
        .take(config.top_k)
        .cloned()
        .collect();

    if passing.is_empty() {
        let top_hit = hits[0].clone();
        FilteredRetrieval {
            hits: vec![top_hit],
            filtered_count: initial_count.saturating_sub(1),
            no_strong_match: true,
        }
    } else {
        let kept = passing.len();
        FilteredRetrieval {
            hits: passing,
            filtered_count: initial_count.saturating_sub(kept),
            no_strong_match: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeakRetrievalConfig {
    pub min_chunks: usize,
    pub min_top_score: f32,
}

impl Default for WeakRetrievalConfig {
    fn default() -> Self {
        Self {
            min_chunks: 2,
            min_top_score: 0.012,
        }
    }
}

pub fn is_weak_retrieval(hits: &[Hit], config: &WeakRetrievalConfig) -> bool {
    if hits.len() < config.min_chunks {
        return true;
    }
    match hits.first() {
        Some(top) if top.score < config.min_top_score => true,
        Some(_) => false,
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_chunk(id: &str, text: &str, vector: Vec<f32>) -> Chunk {
        Chunk {
            id: id.to_string(),
            source: format!("doc_{id}.txt"),
            text: text.to_string(),
            vector,
        }
    }

    #[test]
    fn test_terms_extraction() {
        let extracted = terms("Hello, World! This is a test 123... A B C.");
        assert!(extracted.contains("hello"));
        assert!(extracted.contains("world"));
        assert!(extracted.contains("this"));
        assert!(extracted.contains("is"));
        assert!(extracted.contains("test"));
        assert!(extracted.contains("123"));
        // Single characters are excluded
        assert!(!extracted.contains("a"));
        assert!(!extracted.contains("b"));
        assert!(!extracted.contains("c"));
    }

    #[test]
    fn test_cosine_similarity() {
        // Identical vectors
        let sim = cosine(&[1.0, 0.0, 0.0], &[1.0, 0.0, 0.0]);
        assert!((sim - 1.0).abs() < 1e-5);

        // Orthogonal vectors
        let sim_ortho = cosine(&[1.0, 0.0], &[0.0, 1.0]);
        assert_eq!(sim_ortho, 0.0);

        // Opposite vectors
        let sim_opp = cosine(&[1.0, 0.0], &[-1.0, 0.0]);
        assert!((sim_opp - (-1.0)).abs() < 1e-5);

        // Zero vectors
        assert_eq!(cosine(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
        assert_eq!(cosine(&[0.0, 0.0], &[0.0, 0.0]), 0.0);
    }

    #[test]
    fn test_hybrid_empty_chunks() {
        let hits = hybrid(Vec::new(), "hello", &[1.0, 0.0], 5);
        assert!(hits.is_empty());
    }

    #[test]
    fn test_hybrid_limit_zero() {
        let chunks = vec![make_chunk("1", "hello world", vec![1.0, 0.0])];
        let hits = hybrid(chunks, "hello", &[1.0, 0.0], 0);
        assert!(hits.is_empty());
    }

    #[test]
    fn test_hybrid_dense_dominated() {
        // Lexical score is zero for both chunks (no term overlap with question)
        let c1 = make_chunk("1", "apple fruit banana", vec![1.0, 0.0]);
        let c2 = make_chunk("2", "orange citrus lemon", vec![0.0, 1.0]);

        // Query vector points towards c1
        let hits = hybrid(vec![c1, c2], "vehicle automotive", &[1.0, 0.0], 2);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].chunk.id, "1");
        assert_eq!(hits[1].chunk.id, "2");
        assert!(hits[0].score > hits[1].score);
    }

    #[test]
    fn test_hybrid_lexical_dominated() {
        // Both chunks have identical dense vectors
        let c1 = make_chunk("1", "The refund policy covers full returns within 30 days.", vec![1.0, 1.0]);
        let c2 = make_chunk("2", "Our company culture focuses on employee growth.", vec![1.0, 1.0]);

        let hits = hybrid(vec![c1, c2], "What is the refund policy?", &[1.0, 1.0], 2);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].chunk.id, "1");
        assert_eq!(hits[1].chunk.id, "2");
        assert!(hits[0].score > hits[1].score);
    }

    #[test]
    fn test_hybrid_reciprocal_rank_fusion_combined() {
        // c1: high dense match, zero lexical
        // c2: moderate dense match, high lexical match
        // c3: low dense match, zero lexical
        let c1 = make_chunk("1", "random text without any matches", vec![0.9, 0.1]);
        let c2 = make_chunk("2", "target keywords present in text", vec![0.7, 0.3]);
        let c3 = make_chunk("3", "unrelated text completely", vec![0.1, 0.9]);

        let hits = hybrid(vec![c1, c2, c3], "target keywords", &[1.0, 0.0], 3);
        assert_eq!(hits.len(), 3);
        // c2 gets boosted by both dense rank and lexical rank
        assert_eq!(hits[0].chunk.id, "2");
    }

    #[test]
    fn test_hybrid_limit_greater_than_total() {
        let chunks = vec![
            make_chunk("1", "first text", vec![1.0, 0.0]),
            make_chunk("2", "second text", vec![0.0, 1.0]),
        ];
        let hits = hybrid(chunks, "text", &[1.0, 0.0], 10);
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn test_weak_retrieval_thresholds_table_driven() {
        let config = WeakRetrievalConfig {
            min_chunks: 2,
            min_top_score: 0.012,
        };

        struct TestCase {
            name: &'static str,
            scores: Vec<f32>,
            expected_weak: bool,
        }

        let cases = vec![
            TestCase {
                name: "empty hits is weak",
                scores: vec![],
                expected_weak: true,
            },
            TestCase {
                name: "single hit with high score is weak (fewer than 2 chunks)",
                scores: vec![0.020],
                expected_weak: true,
            },
            TestCase {
                name: "single hit with low score is weak",
                scores: vec![0.008],
                expected_weak: true,
            },
            TestCase {
                name: "two hits with low top score (< 0.012) is weak",
                scores: vec![0.010, 0.009],
                expected_weak: true,
            },
            TestCase {
                name: "two hits exactly at threshold 0.012 is not weak",
                scores: vec![0.012, 0.010],
                expected_weak: false,
            },
            TestCase {
                name: "two hits above threshold is not weak",
                scores: vec![0.016, 0.014],
                expected_weak: false,
            },
            TestCase {
                name: "five hits with strong top score is not weak",
                scores: vec![0.018, 0.015, 0.013, 0.011, 0.009],
                expected_weak: false,
            },
            TestCase {
                name: "five hits with weak top score is weak",
                scores: vec![0.011, 0.010, 0.009, 0.008, 0.007],
                expected_weak: true,
            },
        ];

        for case in cases {
            let hits: Vec<Hit> = case
                .scores
                .into_iter()
                .enumerate()
                .map(|(i, s)| Hit {
                    chunk: make_chunk(&format!("chunk_{i}"), "sample text", vec![]),
                    score: s,
                    cosine: s,
                })
                .collect();

            let weak = is_weak_retrieval(&hits, &config);
            assert_eq!(
                weak, case.expected_weak,
                "Case '{}' failed: expected weak={}, got={}",
                case.name, case.expected_weak, weak
            );
        }
    }

    #[test]
    fn test_filter_by_relevance_threshold_and_min_one_chunk() {
        let config = RetrievalFilterConfig {
            relevance_threshold: 0.50,
            top_k: 3,
        };

        // 1. Partial match: 2 pass, 2 fail -> 2 kept, 2 filtered
        let hits = vec![
            Hit {
                chunk: make_chunk("1", "A", vec![]),
                score: 0.03,
                cosine: 0.85,
            },
            Hit {
                chunk: make_chunk("2", "B", vec![]),
                score: 0.02,
                cosine: 0.60,
            },
            Hit {
                chunk: make_chunk("3", "C", vec![]),
                score: 0.015,
                cosine: 0.40,
            },
            Hit {
                chunk: make_chunk("4", "D", vec![]),
                score: 0.010,
                cosine: 0.20,
            },
        ];
        let res = filter_by_relevance(hits, &config);
        assert_eq!(res.hits.len(), 2);
        assert_eq!(res.hits[0].chunk.id, "1");
        assert_eq!(res.hits[1].chunk.id, "2");
        assert_eq!(res.filtered_count, 2);
        assert!(!res.no_strong_match);

        // 2. No hit passes threshold: keep at least 1 chunk (the top-ranked hit)
        let weak_hits = vec![
            Hit {
                chunk: make_chunk("1", "A", vec![]),
                score: 0.03,
                cosine: 0.35,
            },
            Hit {
                chunk: make_chunk("2", "B", vec![]),
                score: 0.02,
                cosine: 0.25,
            },
            Hit {
                chunk: make_chunk("3", "C", vec![]),
                score: 0.01,
                cosine: 0.15,
            },
        ];
        let res_weak = filter_by_relevance(weak_hits, &config);
        assert_eq!(res_weak.hits.len(), 1);
        assert_eq!(res_weak.hits[0].chunk.id, "1");
        assert_eq!(res_weak.filtered_count, 2); // 3 total - 1 kept = 2 filtered
        assert!(res_weak.no_strong_match);

        // 3. All hits pass threshold and truncated to top_k
        let all_pass = vec![
            Hit {
                chunk: make_chunk("1", "A", vec![]),
                score: 0.04,
                cosine: 0.90,
            },
            Hit {
                chunk: make_chunk("2", "B", vec![]),
                score: 0.03,
                cosine: 0.80,
            },
            Hit {
                chunk: make_chunk("3", "C", vec![]),
                score: 0.02,
                cosine: 0.70,
            },
            Hit {
                chunk: make_chunk("4", "D", vec![]),
                score: 0.01,
                cosine: 0.60,
            },
        ];
        let res_top = filter_by_relevance(all_pass, &config);
        assert_eq!(res_top.hits.len(), 3); // top_k is 3
        assert_eq!(res_top.filtered_count, 1);
        assert!(!res_top.no_strong_match);

        // 4. Empty hits
        let empty_res = filter_by_relevance(vec![], &config);
        assert!(empty_res.hits.is_empty());
        assert_eq!(empty_res.filtered_count, 0);
        assert!(empty_res.no_strong_match);
    }
}
