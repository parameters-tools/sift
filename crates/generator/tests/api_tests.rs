mod common;

use common::{MockResponse, MockServer};
use generator::api::Api;
use serde_json::json;

#[tokio::test]
async fn test_embed_success_single_and_multiple() {
    let server = MockServer::start().await;
    server.push_response(MockResponse::json(
        200,
        json!({
            "object": "list",
            "data": [
                {
                    "object": "embedding",
                    "index": 0,
                    "embedding": [0.1, 0.2, 0.3]
                },
                {
                    "object": "embedding",
                    "index": 1,
                    "embedding": [0.4, 0.5, 0.6]
                }
            ],
            "model": "text-embedding-3-small"
        }),
    ));

    let api = Api::new(&server.url, "test-key-123", "text-embedding-3-small", "gpt-4o-mini").unwrap();
    let inputs = vec!["first chunk".to_string(), "second chunk".to_string()];
    let embeddings = api.embed(&inputs).await.unwrap();

    assert_eq!(embeddings.len(), 2);
    assert_eq!(embeddings[0], vec![0.1, 0.2, 0.3]);
    assert_eq!(embeddings[1], vec![0.4, 0.5, 0.6]);

    let reqs = server.recorded_requests();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].method, "POST");
    assert_eq!(reqs[0].path, "/embeddings");
    assert_eq!(reqs[0].header("authorization"), Some("Bearer test-key-123"));
    let body = reqs[0].json_body().unwrap();
    assert_eq!(body["model"], "text-embedding-3-small");
    assert_eq!(body["input"], json!(["first chunk", "second chunk"]));
}

#[tokio::test]
async fn test_embed_out_of_order_indices_sorted() {
    let server = MockServer::start().await;
    // Server returns index 1 before index 0
    server.push_response(MockResponse::json(
        200,
        json!({
            "data": [
                {
                    "index": 1,
                    "embedding": [0.4, 0.5, 0.6]
                },
                {
                    "index": 0,
                    "embedding": [0.1, 0.2, 0.3]
                }
            ]
        }),
    ));

    let api = Api::new(&server.url, "test-key", "emb-model", "chat-model").unwrap();
    let inputs = vec!["a".to_string(), "b".to_string()];
    let embeddings = api.embed(&inputs).await.unwrap();

    // Must be sorted by index so vector 0 corresponds to "a" and vector 1 to "b"
    assert_eq!(embeddings[0], vec![0.1, 0.2, 0.3]);
    assert_eq!(embeddings[1], vec![0.4, 0.5, 0.6]);
}

#[tokio::test]
async fn test_embed_empty_input_no_http_call() {
    let server = MockServer::start().await;
    let api = Api::new(&server.url, "test-key", "emb-model", "chat-model").unwrap();

    let embeddings = api.embed(&[]).await.unwrap();
    assert!(embeddings.is_empty());
    assert_eq!(server.recorded_requests().len(), 0);
}

#[tokio::test]
async fn test_embed_http_error_401() {
    let server = MockServer::start().await;
    server.push_response(MockResponse::error(401, "Invalid API key"));

    let api = Api::new(&server.url, "invalid-key", "emb-model", "chat-model").unwrap();
    let err = api.embed(&["test".to_string()]).await.unwrap_err();

    assert!(err.to_string().contains("Embedding API returned an error"));
}

#[tokio::test]
async fn test_embed_http_error_429() {
    let server = MockServer::start().await;
    server.push_response(MockResponse::error(429, "Rate limit exceeded"));

    let api = Api::new(&server.url, "test-key", "emb-model", "chat-model").unwrap();
    let err = api.embed(&["test".to_string()]).await.unwrap_err();

    assert!(err.to_string().contains("Embedding API returned an error"));
}

#[tokio::test]
async fn test_embed_http_error_500() {
    let server = MockServer::start().await;
    server.push_response(MockResponse::error(500, "Internal Server Error"));

    let api = Api::new(&server.url, "test-key", "emb-model", "chat-model").unwrap();
    let err = api.embed(&["test".to_string()]).await.unwrap_err();

    assert!(err.to_string().contains("Embedding API returned an error"));
}

#[tokio::test]
async fn test_embed_count_mismatch() {
    let server = MockServer::start().await;
    // API returns only 1 vector when 2 inputs were sent
    server.push_response(MockResponse::json(
        200,
        json!({
            "data": [
                { "index": 0, "embedding": [0.1, 0.2] }
            ]
        }),
    ));

    let api = Api::new(&server.url, "test-key", "emb-model", "chat-model").unwrap();
    let inputs = vec!["one".to_string(), "two".to_string()];
    let err = api.embed(&inputs).await.unwrap_err();

    assert!(err.to_string().contains("Embedding API returned 1 vectors for 2 inputs"));
}

#[tokio::test]
async fn test_embed_invalid_vector_values() {
    let server = MockServer::start().await;
    // Empty embedding vector
    server.push_response(MockResponse::json(
        200,
        json!({
            "data": [
                { "index": 0, "embedding": [] }
            ]
        }),
    ));

    let api = Api::new(&server.url, "test-key", "emb-model", "chat-model").unwrap();
    let err = api.embed(&["one".to_string()]).await.unwrap_err();
    assert!(err.to_string().contains("Embedding API returned an empty or invalid vector"));
}

#[tokio::test]
async fn test_embed_duplicate_indices() {
    let server = MockServer::start().await;
    server.push_response(MockResponse::json(
        200,
        json!({
            "data": [
                { "index": 0, "embedding": [0.1] },
                { "index": 0, "embedding": [0.2] }
            ]
        }),
    ));

    let api = Api::new(&server.url, "test-key", "emb-model", "chat-model").unwrap();
    let inputs = vec!["one".to_string(), "two".to_string()];
    let err = api.embed(&inputs).await.unwrap_err();
    assert!(err.to_string().contains("Embedding API returned invalid indices"));
}

#[tokio::test]
async fn test_embed_malformed_json_response() {
    let server = MockServer::start().await;
    server.push_response(MockResponse::raw(
        200,
        "OK",
        vec![("Content-Type", "application/json".to_string())],
        b"not a json object".to_vec(),
    ));

    let api = Api::new(&server.url, "test-key", "emb-model", "chat-model").unwrap();
    let err = api.embed(&["one".to_string()]).await.unwrap_err();
    assert!(err.to_string().contains("Invalid embedding API response"));
}

#[tokio::test]
async fn test_answer_success() {
    let server = MockServer::start().await;
    server.push_response(MockResponse::json(
        200,
        json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "According to [1], refunds are permitted within 30 days."
                    }
                }
            ]
        }),
    ));

    let api = Api::new(&server.url, "test-key", "emb-model", "chat-model").unwrap();
    let contexts = vec![
        ("doc.txt".to_string(), "Refunds allowed in 30 days.".to_string()),
    ];
    let answer = api.answer("What is the refund policy?", &contexts).await.unwrap();

    assert_eq!(answer, "According to [1], refunds are permitted within 30 days.");

    let reqs = server.recorded_requests();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].path, "/chat/completions");
    let body = reqs[0].json_body().unwrap();
    assert_eq!(body["model"], "chat-model");
    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["role"], "system");
    let system_prompt = messages[0]["content"].as_str().unwrap();
    assert!(system_prompt.contains("[1] Source: doc.txt"));
    assert!(system_prompt.contains("Refunds allowed in 30 days."));
    assert!(system_prompt.contains("untrusted data"));
    assert_eq!(messages[1]["role"], "user");
    assert_eq!(messages[1]["content"], "What is the refund policy?");
}

#[tokio::test]
async fn test_answer_empty_choices() {
    let server = MockServer::start().await;
    server.push_response(MockResponse::json(
        200,
        json!({
            "choices": []
        }),
    ));

    let api = Api::new(&server.url, "test-key", "emb-model", "chat-model").unwrap();
    let err = api.answer("question", &[]).await.unwrap_err();
    assert!(err.to_string().contains("Chat API returned no answer"));
}

#[tokio::test]
async fn test_answer_null_or_whitespace_content() {
    let server = MockServer::start().await;
    server.push_response(MockResponse::json(
        200,
        json!({
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "   "
                    }
                }
            ]
        }),
    ));

    let api = Api::new(&server.url, "test-key", "emb-model", "chat-model").unwrap();
    let err = api.answer("question", &[]).await.unwrap_err();
    assert!(err.to_string().contains("Chat API returned no answer"));
}

#[tokio::test]
async fn test_answer_http_error_500() {
    let server = MockServer::start().await;
    server.push_response(MockResponse::error(500, "Chat service down"));

    let api = Api::new(&server.url, "test-key", "emb-model", "chat-model").unwrap();
    let err = api.answer("question", &[]).await.unwrap_err();
    assert!(err.to_string().contains("Chat API returned an error"));
}

#[tokio::test]
async fn test_answer_malformed_json() {
    let server = MockServer::start().await;
    server.push_response(MockResponse::raw(
        200,
        "OK",
        vec![("Content-Type", "application/json".to_string())],
        b"corrupt json".to_vec(),
    ));

    let api = Api::new(&server.url, "test-key", "emb-model", "chat-model").unwrap();
    let err = api.answer("question", &[]).await.unwrap_err();
    assert!(err.to_string().contains("Invalid chat API response"));
}

#[test]
fn test_api_new_invalid_header_character() {
    let res = Api::new("http://localhost", "key_with_\n_newline", "m", "c");
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("OPENAI_API_KEY contains invalid header characters"));
}

#[tokio::test]
async fn test_split_endpoints_embed_and_chat() {
    let embed_server = MockServer::start().await;
    let chat_server = MockServer::start().await;

    embed_server.push_response(MockResponse::json(
        200,
        json!({
            "data": [
                { "index": 0, "embedding": [0.1, 0.2] }
            ]
        }),
    ));

    chat_server.push_response(MockResponse::json(
        200,
        json!({
            "choices": [
                { "message": { "role": "assistant", "content": "Split answer." } }
            ]
        }),
    ));

    let api = Api::with_endpoints(
        &embed_server.url,
        "embed-key",
        "embed-model",
        &chat_server.url,
        "chat-key",
        "chat-model",
    )
    .unwrap();

    let emb = api.embed(&["test".to_string()]).await.unwrap();
    assert_eq!(emb.len(), 1);
    assert_eq!(embed_server.recorded_requests().len(), 1);
    assert_eq!(chat_server.recorded_requests().len(), 0);

    let ans = api.answer("query", &[]).await.unwrap();
    assert_eq!(ans, "Split answer.");
    assert_eq!(chat_server.recorded_requests().len(), 1);
}

#[tokio::test]
async fn test_rewrite_query_skipped_with_empty_history() {
    let server = MockServer::start().await;
    let api = Api::new(&server.url, "test-key", "emb-model", "chat-model").unwrap();

    let question = "what is the context window length?";
    let rewritten = api.rewrite_query(question, &[]).await.unwrap();

    // Must return the exact original query
    assert_eq!(rewritten, question);
    // Crucial: no HTTP calls should be made to the server
    assert_eq!(server.recorded_requests().len(), 0);
}

#[tokio::test]
async fn test_rewrite_query_called_when_history_present() {
    let server = MockServer::start().await;
    server.push_response(MockResponse::json(
        200,
        json!({
            "choices": [
                { "message": { "role": "assistant", "content": "deepseek v4.1 flash context length" } }
            ]
        }),
    ));

    let api = Api::new(&server.url, "test-key", "emb-model", "chat-model").unwrap();
    let history = vec![generator::conversation::Turn {
        user: "Tell me about DeepSeek V4.1 Flash".to_string(),
        assistant: "DeepSeek V4.1 Flash is an efficient MoE language model.".to_string(),
    }];

    let rewritten = api.rewrite_query("what is its context length?", &history).await.unwrap();
    assert_eq!(rewritten, "deepseek v4.1 flash context length");
    assert_eq!(server.recorded_requests().len(), 1);
    let req = &server.recorded_requests()[0];
    assert_eq!(req.path, "/chat/completions");
}
