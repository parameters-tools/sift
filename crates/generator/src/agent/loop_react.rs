use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;
use anyhow::{Context, Result};
use rusqlite::Connection;
use uuid::Uuid;

use crate::agent::calculator::{eval_math, find_math_intent, format_result};
use crate::agent::planner::analyze_and_plan;
use crate::agent::types::AgentStep;
use crate::agent::verifier::verify_answer;
use crate::api::Api;
use crate::db;
use crate::retrieve::{self, Hit};
use crate::sources;
use crate::chunker;
use crate::ui::messages::{MessageFormatter, RetrievedChunk};

pub struct AgentExecutionOutput {
    pub answer: String,
    pub steps: Vec<AgentStep>,
    pub sources_list: Vec<(usize, String, f32)>,
    pub retrieved_chunks: Vec<RetrievedChunk>,
    pub is_weak: bool,
    pub no_strong_match: bool,
    pub answer_elapsed: f64,
    pub token_count: usize,
    pub is_approx_tokens: bool,
    pub chat_model: String,
}

pub struct AgentRunner;

impl AgentRunner {
    pub async fn run_agent(
        question: &str,
        retrieved_query: &str,
        history: &[crate::conversation::Turn],
        db_file: &Path,
        api: &Api,
        token: &tokio_util::sync::CancellationToken,
        token_callback: impl Fn(&str) + Send + Sync + 'static,
    ) -> Result<AgentExecutionOutput> {
        let mut steps = Vec::new();
        let mut conn = db::open(db_file)?;

        // 1. On-the-fly Indexing check (Feature 5)
        if let Some(path_str) = find_referenced_path(question, db_file) {
            let path = PathBuf::from(&path_str);
            if path.exists() {
                // Check if already indexed
                let is_indexed = is_source_indexed(&conn, &path_str);
                if !is_indexed {
                    let idx_start = Instant::now();
                    let (indexed_chunks, indexed_files) = index_path_on_the_fly(&mut conn, &path, api).await?;
                    let idx_elapsed = idx_start.elapsed().as_secs_f64();
                    let detail = MessageFormatter::format_index_done(indexed_chunks, indexed_files, idx_elapsed);
                    steps.push(AgentStep::Tool {
                        name: "Index".to_string(),
                        args: format!("\"{path_str}\""),
                        detail,
                        elapsed: idx_elapsed,
                        is_weak: false,
                        no_strong_match: false,
                    });
                }
            }
        }

        // 2. Plan-and-solve sub-goal decomposition (Feature 3)
        let plan_opt = analyze_and_plan(question);
        if let Some(ref plan) = plan_opt {
            steps.push(AgentStep::Plan {
                goals: plan.goals.clone(),
                elapsed: 0.001,
            });
        }

        // 3. Hop 1: Primary Retrieval
        let chunks = db::all(&conn)?;
        if chunks.is_empty() {
            return Ok(AgentExecutionOutput {
                answer: "The index is empty. Add documents first with /index <path>".to_string(),
                steps,
                sources_list: Vec::new(),
                retrieved_chunks: Vec::new(),
                is_weak: true,
                no_strong_match: true,
                answer_elapsed: 0.0,
                token_count: 0,
                is_approx_tokens: false,
                chat_model: api.chat_model().to_string(),
            });
        }

        let hop1_start = Instant::now();
        let embs1 = api.embed(std::slice::from_ref(&retrieved_query.to_string())).await?;
        let emb1 = embs1.into_iter().next().context("Failed to get query embedding")?;
        db::ensure_compatible(&chunks, &emb1)?;

        let mut all_hits = retrieve::hybrid(chunks.clone(), retrieved_query, &emb1, 6);
        let hop1_elapsed = hop1_start.elapsed().as_secs_f64();

        let unique_src1: HashSet<String> = all_hits.iter().map(|h| h.chunk.source.clone()).collect();
        steps.push(AgentStep::Tool {
            name: "Retrieve".to_string(),
            args: if question.trim() == retrieved_query.trim() {
                String::new()
            } else {
                MessageFormatter::format_tool_args(retrieved_query)
            },
            detail: MessageFormatter::format_retrieve_detail(all_hits.len(), unique_src1.len(), hop1_elapsed, 0),
            elapsed: hop1_elapsed,
            is_weak: all_hits.is_empty(),
            no_strong_match: all_hits.is_empty(),
        });

        // 4. Multi-hop Iterative Research (Feature 1)
        let hop2_query = determine_hop2_query(question, &all_hits, plan_opt.as_ref());
        if let Some(h2_query) = hop2_query {
            let hop2_start = Instant::now();
            if let Ok(embs2) = api.embed(std::slice::from_ref(&h2_query)).await
                && let Some(emb2) = embs2.into_iter().next()
            {
                let hits2 = retrieve::hybrid(chunks.clone(), &h2_query, &emb2, 4);
                let hop2_elapsed = hop2_start.elapsed().as_secs_f64();
                let unique_src2: HashSet<String> = hits2.iter().map(|h| h.chunk.source.clone()).collect();
                steps.push(AgentStep::Tool {
                    name: "Retrieve".to_string(),
                    args: MessageFormatter::format_tool_args(&h2_query),
                    detail: MessageFormatter::format_retrieve_detail(hits2.len(), unique_src2.len(), hop2_elapsed, 0),
                    elapsed: hop2_elapsed,
                    is_weak: hits2.is_empty(),
                    no_strong_match: hits2.is_empty(),
                });

                // Merge and deduplicate chunks
                let mut existing_ids: HashSet<String> = all_hits.iter().map(|h| h.chunk.id.clone()).collect();
                for h in hits2 {
                    if !existing_ids.contains(&h.chunk.id) {
                        existing_ids.insert(h.chunk.id.clone());
                        all_hits.push(h);
                    }
                }
            }
        }

        // 5. Tool Use: Math Calculator (Feature 4)
        if let Some(expr) = find_math_intent(question) {
            let calc_start = Instant::now();
            match eval_math(&expr) {
                Ok(res) => {
                    let formatted = format_result(res);
                    let calc_elapsed = calc_start.elapsed().as_secs_f64();
                    steps.push(AgentStep::Tool {
                        name: "Calculate".to_string(),
                        args: format!("\"{expr}\""),
                        detail: format!("{formatted} · {:.1}ms", calc_elapsed * 1000.0),
                        elapsed: calc_elapsed,
                        is_weak: false,
                        no_strong_match: false,
                    });
                }
                Err(_) => {
                    // Ignored if expression parsing failed
                }
            }
        }

        // 6. Ambiguity / Clarification check (Feature 6)
        if let Some((topic, options)) = detect_topic_ambiguity(question, &all_hits) {
            steps.push(AgentStep::Clarification {
                topic: topic.clone(),
                options: options.clone(),
                detail: format!("Multiple scopes identified for '{topic}': {}", options.join(", ")),
                elapsed: 0.001,
            });
        }

        // Filter and score
        let filter_config = retrieve::RetrievalFilterConfig::default();
        let filtered = retrieve::filter_by_relevance(all_hits, &filter_config);
        let hits = filtered.hits;
        let no_strong_match = filtered.no_strong_match;
        let weak_config = retrieve::WeakRetrievalConfig::default();
        let is_weak = retrieve::is_weak_retrieval(&hits, &weak_config);

        let contexts: Vec<(String, String)> = hits
            .iter()
            .map(|h| (h.chunk.source.clone(), h.chunk.text.chars().take(5000).collect()))
            .collect();

        let mut sources_list = Vec::new();
        let mut retrieved_chunks = Vec::new();
        for (i, h) in hits.iter().enumerate() {
            sources_list.push((i + 1, h.chunk.source.clone(), h.score));
            let file_name = std::path::Path::new(&h.chunk.source)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| h.chunk.source.clone());
            retrieved_chunks.push(RetrievedChunk {
                rank: i + 1,
                file: file_name,
                full_path: h.chunk.source.clone(),
                page: None,
                text: h.chunk.text.clone(),
                score: h.score,
                is_cited: false,
            });
        }

        // 7. Generation
        let answer_start = Instant::now();
        let answer_res = api
            .answer_stream_with_history(
                question,
                &contexts,
                history,
                token.clone(),
                token_callback,
            )
            .await?;
        let answer_elapsed = answer_start.elapsed().as_secs_f64();
        let answer = answer_res.text;

        // Mark citations
        let cited_ranks = MessageFormatter::extract_citations(&answer);
        for chunk in &mut retrieved_chunks {
            chunk.is_cited = cited_ranks.contains(&chunk.rank);
        }

        // 8. Autonomous Verification & Self-Correction (Feature 2)
        let verify_start = Instant::now();
        let v_res = verify_answer(&answer, &retrieved_chunks, verify_start.elapsed().as_secs_f64());
        steps.push(AgentStep::Verification {
            is_verified: v_res.is_verified,
            verified_count: v_res.verified_citations.len(),
            total_citations: cited_ranks.len(),
            detail: v_res.detail,
            conflicts: v_res.conflicts,
            elapsed: v_res.elapsed,
        });

        let (token_count, is_approx_tokens) = match answer_res.completion_tokens {
            Some(tokens) => (tokens, false),
            None => (MessageFormatter::estimate_tokens(&answer), true),
        };

        Ok(AgentExecutionOutput {
            answer,
            steps,
            sources_list,
            retrieved_chunks,
            is_weak,
            no_strong_match,
            answer_elapsed,
            token_count,
            is_approx_tokens,
            chat_model: api.chat_model().to_string(),
        })
    }
}

/// Detects if the question references a specific file path on disk.
fn find_referenced_path(query: &str, _db_file: &Path) -> Option<String> {
    for word in query.split_whitespace() {
        let clean = word.trim_matches(|c: char| c == '"' || c == '\'' || c == '`' || c == ',' || c == '?' || c == '.');
        if (clean.contains('/') || clean.contains('\\') || clean.ends_with(".md") || clean.ends_with(".txt") || clean.ends_with(".pdf"))
            && Path::new(clean).exists()
        {
            return Some(clean.to_string());
        }
    }
    None
}

fn is_source_indexed(conn: &Connection, source_path: &str) -> bool {
    let mut stmt = match conn.prepare("SELECT COUNT(*) FROM chunks WHERE source = ?1") {
        Ok(s) => s,
        Err(_) => return false,
    };
    stmt.query_row([source_path], |row| row.get::<_, i64>(0))
        .map(|cnt| cnt > 0)
        .unwrap_or(false)
}

async fn index_path_on_the_fly(
    conn: &mut Connection,
    path: &Path,
    api: &Api,
) -> Result<(usize, usize)> {
    let files = sources::collect(path)?;
    let mut total_chunks = 0;
    for file in &files {
        let text = sources::read(file)?;
        let pieces = chunker::chunk(&text, 420, 70);
        let mut items = Vec::new();
        let source_str = file.to_string_lossy().to_string();

        for batch in pieces.chunks(64) {
            let vectors = api.embed(batch).await?;
            for (p_text, vector) in batch.iter().zip(vectors) {
                items.push(db::Chunk {
                    id: Uuid::new_v4().to_string(),
                    source: source_str.clone(),
                    text: p_text.clone(),
                    vector,
                });
            }
        }
        total_chunks += items.len();
        db::replace_source(conn, &source_str, &items)?;
    }
    Ok((total_chunks, files.len()))
}

/// Determines whether a second retrieval hop is needed.
fn determine_hop2_query(
    question: &str,
    hop1_hits: &[Hit],
    plan: Option<&crate::agent::planner::Plan>,
) -> Option<String> {
    // 1. If plan has multiple goals, use goal 2 as hop 2
    if let Some(p) = plan
        && p.goals.len() >= 2
    {
        let g2 = &p.goals[1];
        let clean = g2
            .trim_start_matches("Retrieve information regarding ")
            .trim_start_matches("Investigate related aspect: ")
            .trim_start_matches("Sub-query 2: ")
            .trim();
        if !clean.is_empty() && clean != question.trim() {
            return Some(clean.to_string());
        }
    }

    // 2. Check if Hop 1 chunks explicitly reference other documents or sections
    for h in hop1_hits.iter().take(3) {
        let text_lower = h.chunk.text.to_lowercase();
        let patterns = [
            "see section ",
            "refer to appendix ",
            "refer to ",
            "specified in ",
            "defined in ",
        ];
        for pat in patterns {
            if let Some(pos) = text_lower.find(pat) {
                let candidate = &h.chunk.text[pos + pat.len()..];
                let target: String = candidate
                    .chars()
                    .take_while(|&c| c != '.' && c != ',' && c != '\n' && c != ';')
                    .collect();
                let clean = target.trim();
                if clean.len() >= 3 && clean.len() <= 40 {
                    return Some(clean.to_string());
                }
            }
        }
    }

    None
}

/// Detects ambiguity across sources for broad questions.
fn detect_topic_ambiguity(
    query: &str,
    hits: &[Hit],
) -> Option<(String, Vec<String>)> {
    if hits.len() < 2 {
        return None;
    }
    let lower = query.to_lowercase();
    let broad_terms = ["policy", "guidelines", "rules", "schedule", "pricing", "status", "limits"];
    for term in broad_terms {
        if lower.contains(term) {
            let mut distinct_files = HashSet::new();
            for h in hits {
                if let Some(name) = Path::new(&h.chunk.source).file_name() {
                    let s = name.to_string_lossy().to_string();
                    if s.to_lowercase().contains(term) {
                        distinct_files.insert(s);
                    }
                }
            }
            if distinct_files.len() >= 2 {
                let mut options: Vec<String> = distinct_files.into_iter().collect();
                options.sort();
                return Some((term.to_string(), options));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_determine_hop2_from_plan() {
        let plan = crate::agent::planner::Plan {
            goals: vec![
                "Retrieve information regarding Enterprise plan".into(),
                "Retrieve information regarding Starter plan".into(),
                "Synthesize".into(),
            ],
        };
        let hop2 = determine_hop2_query("Compare plans", &[], Some(&plan));
        assert_eq!(hop2, Some("Starter plan".to_string()));
    }

    #[test]
    fn test_determine_hop2_from_reference() {
        let hits = vec![
            Hit {
                chunk: db::Chunk {
                    id: "1".into(),
                    source: "sla.md".into(),
                    text: "Uptime credits are calculated as specified in Annex B for all customers.".into(),
                    vector: vec![0.1; 4],
                },
                score: 0.9,
                cosine: 0.9,
            },
        ];
        let hop2 = determine_hop2_query("What are the credits?", &hits, None);
        assert_eq!(hop2, Some("Annex B for all customers".to_string()));
    }

    #[test]
    fn test_detect_topic_ambiguity() {
        let hits = vec![
            Hit {
                chunk: db::Chunk {
                    id: "1".into(),
                    source: "/docs/leave_policy_us.md".into(),
                    text: "US leave policy guidelines".into(),
                    vector: vec![0.1; 4],
                },
                score: 0.9,
                cosine: 0.9,
            },
            Hit {
                chunk: db::Chunk {
                    id: "2".into(),
                    source: "/docs/leave_policy_uk.md".into(),
                    text: "UK leave policy guidelines".into(),
                    vector: vec![0.1; 4],
                },
                score: 0.85,
                cosine: 0.85,
            },
        ];
        let amb = detect_topic_ambiguity("What is the leave policy?", &hits);
        assert!(amb.is_some());
        let (topic, options) = amb.unwrap();
        assert_eq!(topic, "policy");
        assert_eq!(options.len(), 2);
    }
}
