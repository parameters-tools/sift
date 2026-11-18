mod common;

use common::{MockResponse, MockServer};
use generator::{api::Api, chunker, db, retrieve, sources};
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn test_full_rag_pipeline_and_reindexing() {
    let server = MockServer::start().await;

    // 1. Set up temporary test directory and files
    let tmp_dir = std::env::temp_dir().join(format!("rag-pipeline-test-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let doc_policy = tmp_dir.join("refund_policy.md");
    std::fs::write(
        &doc_policy,
        "# Refund Policy\nCustomers can request a full refund within 30 days of purchase for any unused item.",
    )
    .unwrap();

    let doc_faq = tmp_dir.join("faq.txt");
    std::fs::write(
        &doc_faq,
        "Frequently Asked Questions: Shipping takes between 3 to 5 business days for domestic orders.",
    )
    .unwrap();

    // 2. Initialize database
    let db_path = tmp_dir.join("index.sqlite");
    let mut conn = db::open(&db_path).unwrap();

    // Verify initial stats
    let initial_chunks: i64 = conn.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0)).unwrap();
    assert_eq!(initial_chunks, 0);

    // 3. Mock dynamic handler: return semantic embeddings based on content
    // text with "refund" -> [1.0, 0.0, 0.0]
    // text with "shipping" -> [0.0, 1.0, 0.0]
    let make_emb_handler = || {
        |req: &common::RecordedRequest| {
            if req.path == "/embeddings" {
                let body = req.json_body().unwrap();
                let inputs = body["input"].as_array().unwrap();
                let data: Vec<_> = inputs
                    .iter()
                    .enumerate()
                    .map(|(i, val)| {
                        let text = val.as_str().unwrap().to_lowercase();
                        let emb = if text.contains("refund") {
                            vec![1.0, 0.0, 0.0]
                        } else {
                            vec![0.0, 1.0, 0.0]
                        };
                        json!({ "index": i, "embedding": emb })
                    })
                    .collect();
                MockResponse::json(200, json!({ "data": data }))
            } else if req.path == "/chat/completions" {
                MockResponse::json(
                    200,
                    json!({
                        "choices": [
                            {
                                "message": {
                                    "role": "assistant",
                                    "content": "You can request a full refund within 30 days of purchase [1]."
                                }
                            }
                        ]
                    }),
                )
            } else {
                MockResponse::json(404, json!({ "error": "not found" }))
            }
        }
    };

    // Embeddings for index (2 files)
    server.push_handler(make_emb_handler());
    server.push_handler(make_emb_handler());

    let api = Api::new(&server.url, "mock-api-key", "text-embedding-3-small", "gpt-4o-mini").unwrap();

    // 4. Index files
    let files = sources::collect(&tmp_dir).unwrap();
    assert_eq!(files.len(), 2);

    for file in &files {
        let source_path = file.to_string_lossy().to_string();
        let text = sources::read(file).unwrap();
        let pieces = chunker::chunk(&text, 420, 70);
        assert!(!pieces.is_empty());

        let vectors = api.embed(&pieces).await.unwrap();
        let chunks: Vec<_> = pieces
            .into_iter()
            .zip(vectors)
            .map(|(t, v)| db::Chunk {
                id: Uuid::new_v4().to_string(),
                source: source_path.clone(),
                text: t,
                vector: v,
            })
            .collect();
        db::replace_source(&mut conn, &source_path, &chunks).unwrap();
    }

    // Verify stats after indexing
    let total_chunks: i64 = conn.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0)).unwrap();
    let total_sources: i64 = conn.query_row("SELECT COUNT(DISTINCT source) FROM chunks", [], |r| r.get(0)).unwrap();
    assert_eq!(total_chunks, 2);
    assert_eq!(total_sources, 2);

    // 5. Search query
    // Query embedding
    server.push_handler(make_emb_handler());

    let query = "How many days do I have to request a refund?";
    let q_vectors = api.embed(&[query.to_string()]).await.unwrap();
    let all_chunks = db::all(&conn).unwrap();
    db::ensure_compatible(&all_chunks, &q_vectors[0]).unwrap();

    let search_hits = retrieve::hybrid(all_chunks, query, &q_vectors[0], 2);
    assert_eq!(search_hits.len(), 2);
    assert!(search_hits[0].chunk.source.contains("refund_policy.md"));
    assert!(search_hits[0].chunk.text.contains("full refund within 30 days"));

    // 6. Ask question
    server.push_handler(make_emb_handler());

    let contexts: Vec<(String, String)> = search_hits
        .iter()
        .map(|h| (h.chunk.source.clone(), h.chunk.text.clone()))
        .collect();
    let answer = api.answer(query, &contexts).await.unwrap();
    assert_eq!(answer, "You can request a full refund within 30 days of purchase [1].");

    // 7. Atomic Reindexing: Update policy and reindex
    std::fs::write(
        &doc_policy,
        "# Refund Policy Updated\nRefund window has been extended to 60 days.",
    )
    .unwrap();

    // Mock embedding for updated file
    server.push_handler(make_emb_handler());

    let updated_text = sources::read(&doc_policy).unwrap();
    let updated_pieces = chunker::chunk(&updated_text, 420, 70);
    let updated_vectors = api.embed(&updated_pieces).await.unwrap();
    let updated_chunks: Vec<_> = updated_pieces
        .into_iter()
        .zip(updated_vectors)
        .map(|(t, v)| db::Chunk {
            id: Uuid::new_v4().to_string(),
            source: doc_policy.to_string_lossy().to_string(),
            text: t,
            vector: v,
        })
        .collect();

    db::replace_source(&mut conn, &doc_policy.to_string_lossy(), &updated_chunks).unwrap();

    // Ensure total chunks count is still 2 (old chunk was replaced, not duplicated!)
    let reindexed_total: i64 = conn.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0)).unwrap();
    assert_eq!(reindexed_total, 2);

    let current_chunks = db::all(&conn).unwrap();
    let policy_chunks: Vec<_> = current_chunks
        .into_iter()
        .filter(|c| c.source == doc_policy.to_string_lossy())
        .collect();
    assert_eq!(policy_chunks.len(), 1);
    assert!(policy_chunks[0].text.contains("extended to 60 days"));
    assert!(!policy_chunks[0].text.contains("within 30 days"));

    let _ = std::fs::remove_dir_all(&tmp_dir);
}
