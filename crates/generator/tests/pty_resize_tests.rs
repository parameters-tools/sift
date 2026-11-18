mod common;

use std::io::{Read, Write};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use common::{MockResponse, MockServer};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde_json::json;
use unicode_width::UnicodeWidthStr;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_pty_resize_sequence_and_invariants() {
    // 1. Setup temporary directory for test index database
    let test_id = uuid::Uuid::new_v4();
    let temp_dir = std::env::temp_dir().join(format!("rag-pty-test-{test_id}"));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let db_path = temp_dir.join("index.sqlite");

    let mut conn = generator::db::open(&db_path).unwrap();
    let chunk = generator::db::Chunk {
        id: "chunk-1".to_string(),
        source: "docs/intro.md".to_string(),
        text: "Retrieval-Augmented Generation (RAG) is a technique that supplements language models with external knowledge.".to_string(),
        vector: vec![0.1f32; 1536],
    };
    generator::db::replace_source(&mut conn, "docs/intro.md", &[chunk]).unwrap();
    drop(conn);

    // 2. Start mock server
    let server = MockServer::start().await;
    server.set_default_handler(|req| {
        if req.path == "/models" {
            MockResponse::json(200, json!({
                "data": [
                    {"id": "mock-chat", "object": "model"},
                    {"id": "mock-embed", "object": "model"}
                ]
            }))
        } else if req.path == "/embeddings" {
            MockResponse::json(200, json!({
                "object": "list",
                "data": [
                    {
                        "object": "embedding",
                        "index": 0,
                        "embedding": vec![0.1f32; 1536]
                    }
                ],
                "model": "mock-embed"
            }))
        } else if req.path == "/chat/completions" {
            MockResponse::json(200, json!({
                "choices": [
                    {
                        "message": {
                            "role": "assistant",
                            "content": "RAG supplements LLMs with retrieved document chunks [1]."
                        }
                    }
                ],
                "model": "mock-chat"
            }))
        } else {
            MockResponse::json(404, json!({"error": "not found"}))
        }
    });

    // 3. Setup PTY at 120 cols x 24 rows
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();

    let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_rag"));
    cmd.cwd(temp_dir.clone());
    cmd.env("RAG_DB", db_path.to_str().unwrap());
    cmd.env("RAG_API_BASE", &server.url);
    cmd.env("RAG_CHAT_BASE", &server.url);
    cmd.env("RAG_EMBEDDING_BASE", &server.url);
    cmd.env("OPENAI_API_KEY", "test-key");
    cmd.env("RAG_CHAT_API_KEY", "test-key");
    cmd.env("RAG_EMBEDDING_API_KEY", "test-key");
    cmd.env("RAG_CHAT_MODEL", "mock-chat");
    cmd.env("RAG_EMBEDDING_MODEL", "mock-embed");
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLUMNS", "120");
    cmd.env("LINES", "24");

    let mut child = pair.slave.spawn_command(cmd).unwrap();
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().unwrap();
    let writer = std::sync::Arc::new(std::sync::Mutex::new(pair.master.take_writer().unwrap()));
    let writer_clone = writer.clone();

    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            // Terminal emulator response to DSR (Device Status Report: \x1b[6n)
            if buf[..n].windows(4).any(|w| w == b"\x1b[6n") {
                let mut w = writer_clone.lock().unwrap();
                let _ = w.write_all(b"\x1b[1;1R");
                let _ = w.flush();
            }
            if tx.send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
    });

    let mut parser = vt100::Parser::new(24, 120, 0);

    let pump = |parser: &mut vt100::Parser| {
        while let Ok(bytes) = rx.try_recv() {
            parser.process(&bytes);
        }
    };

    let wait_for = |parser: &mut vt100::Parser, predicate: &dyn Fn(&str) -> bool, timeout: Duration| -> bool {
        let start = Instant::now();
        loop {
            pump(parser);
            let contents = parser.screen().contents();
            if predicate(&contents) {
                return true;
            }
            if start.elapsed() >= timeout {
                return false;
            }
            thread::sleep(Duration::from_millis(20));
        }
    };

    println!("Mock server URL: {}", server.url);

    // 4. Wait for app startup
    let started = wait_for(
        &mut parser,
        &|contents| contents.contains("Ask your documents") || contents.contains("Sift"),
        Duration::from_secs(5),
    );
    if !started {
        eprintln!("Screen contents upon startup timeout:\n'{}'", parser.screen().contents());
        if let Ok(Some(status)) = child.try_wait() {
            eprintln!("Child process exited early with status: {:?}", status);
        }
    }
    assert!(started, "App must show initial banner within 5s");

    // 5. Submit question against mock backend (use \r for Enter key in raw terminal mode)
    {
        let mut w = writer.lock().unwrap();
        w.write_all(b"What is RAG?\r").unwrap();
        w.flush().unwrap();
    }

    let answered = wait_for(
        &mut parser,
        &|contents| contents.contains("RAG supplements LLMs") || contents.contains("mock-chat"),
        Duration::from_secs(10),
    );
    if !answered {
        eprintln!("Screen contents upon answer timeout:\n'{}'", parser.screen().contents());
        eprintln!("Recorded requests count: {}", server.recorded_requests().len());
        for req in server.recorded_requests() {
            eprintln!("Req: {} {}", req.method, req.path);
        }
    }
    assert!(answered, "App must display assistant answer from mock backend");

    // 6. Resize sequence: 120 -> 60 -> 120 -> 40 -> 100 columns with short delays
    let resize_steps: [u16; 4] = [60, 120, 40, 100];

    for &target_width in &resize_steps {
        // Trigger resize in PTY
        pair.master
            .resize(PtySize {
                rows: 24,
                cols: target_width,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        parser.set_size(24, target_width);

        // Wait for debounce (~120ms) + full re-anchor redraw (~350ms total)
        let settle_deadline = Instant::now() + Duration::from_millis(400);
        while Instant::now() < settle_deadline {
            pump(&mut parser);
            thread::sleep(Duration::from_millis(25));
        }
        pump(&mut parser);

        let contents = parser.screen().contents();
        let lines: Vec<&str> = contents.lines().collect();
        println!("=== Screen contents at width {} ===\n{}\n===================", target_width, contents);

        // Assertion 1: No screen row contains more than one "╭"
        for (row_idx, line) in lines.iter().enumerate() {
            let top_left_count = line.matches('╭').count();
            assert!(
                top_left_count <= 1,
                "Row {row_idx} at width {target_width} contains multiple '╭' ({top_left_count}): '{line}'"
            );
        }

        // Assertion 2: Every box row has equal display width
        let expected_banner = (target_width as usize).saturating_sub(2).min(80);
        let expected_input = (target_width as usize).saturating_sub(2).max(1);

        for (row_idx, line) in lines.iter().enumerate() {
            if let (Some(first_col), Some(last_col)) = (line.find(['╭', '│', '╰']), line.rfind(['╮', '│', '╯'])) {
                if first_col >= last_col {
                    continue;
                }
                let char_len = line[last_col..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                let box_slice = &line[first_col..last_col + char_len];
                let box_width = UnicodeWidthStr::width(box_slice);
                assert!(
                    box_width == expected_banner || box_width == expected_input,
                    "Row {row_idx} at width {target_width} has box width {box_width}, expected banner {expected_banner} or input {expected_input}. Row: '{line}'"
                );
            }
        }

        // Assertion 3: Input box is fully visible
        assert!(
            contents.contains('>') || contents.contains("Ask a question"),
            "Input box / prompt must be visible at width {target_width}"
        );
        if target_width >= 60 {
            assert!(
                contents.contains('╭') && contents.contains('╰'),
                "Input box borders must be visible at width {target_width}"
            );
        }

        // Assertion 4: Footer is on the last row
        let last_row = lines.last().copied().unwrap_or("");
        assert!(
            last_row.contains("shortcuts") || last_row.contains("chunks") || last_row.contains("sources") || last_row.contains('?'),
            "Footer must be on the last row at width {target_width}, got: '{last_row}'"
        );
    }

    // Clean exit
    {
        let mut w = writer.lock().unwrap();
        let _ = w.write_all(b"/exit\r");
        let _ = w.flush();
    }
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&temp_dir);
}
