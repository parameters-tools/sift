use serde_json::Value;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

#[allow(dead_code)]
pub struct MockResponse {
    pub status: u16,
    pub status_text: &'static str,
    pub headers: Vec<(&'static str, String)>,
    pub body: Vec<u8>,
}

#[allow(dead_code)]
impl MockResponse {
    pub fn json(status: u16, json: Value) -> Self {
        let body = serde_json::to_vec(&json).unwrap();
        Self {
            status,
            status_text: match status {
                200 => "OK",
                201 => "Created",
                400 => "Bad Request",
                401 => "Unauthorized",
                403 => "Forbidden",
                404 => "Not Found",
                429 => "Too Many Requests",
                500 => "Internal Server Error",
                _ => "Unknown",
            },
            headers: vec![("Content-Type", "application/json".to_string())],
            body,
        }
    }

    pub fn error(status: u16, message: &str) -> Self {
        let json = serde_json::json!({
            "error": {
                "message": message,
                "type": "api_error"
            }
        });
        Self::json(status, json)
    }

    pub fn raw(status: u16, status_text: &'static str, headers: Vec<(&'static str, String)>, body: Vec<u8>) -> Self {
        Self {
            status,
            status_text,
            headers,
            body,
        }
    }
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct RecordedRequest {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[allow(dead_code)]
impl RecordedRequest {
    pub fn json_body(&self) -> Option<Value> {
        serde_json::from_slice(&self.body).ok()
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        let name_lower = name.to_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(&name_lower))
            .map(|(_, v)| v.as_str())
    }
}

type BoxedHandler = Box<dyn Fn(&RecordedRequest) -> MockResponse + Send + Sync>;

#[allow(dead_code)]
pub struct MockServer {
    pub url: String,
    pub requests: Arc<Mutex<Vec<RecordedRequest>>>,
    queue: Arc<Mutex<VecDeque<BoxedHandler>>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

#[allow(dead_code)]
impl MockServer {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let url = format!("http://127.0.0.1:{port}");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let queue: Arc<Mutex<VecDeque<BoxedHandler>>> = Arc::new(Mutex::new(VecDeque::new()));
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

        let reqs_clone = requests.clone();
        let queue_clone = queue.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        break;
                    }
                    Ok((mut stream, _)) = listener.accept() => {
                        let reqs = reqs_clone.clone();
                        let q = queue_clone.clone();
                        tokio::spawn(async move {
                            let mut buf = Vec::new();
                            let mut temp = [0u8; 4096];
                            let header_end;
                            let content_len;
                            loop {
                                let n = stream.read(&mut temp).await.unwrap_or(0);
                                if n == 0 { return; }
                                buf.extend_from_slice(&temp[..n]);
                                if let Some(pos) = find_header_end(&buf) {
                                    header_end = pos;
                                    content_len = parse_content_length(&buf[..pos]);
                                    break;
                                }
                            }
                            let total_needed = header_end + 4 + content_len;
                            while buf.len() < total_needed {
                                let n = stream.read(&mut temp).await.unwrap_or(0);
                                if n == 0 { break; }
                                buf.extend_from_slice(&temp[..n]);
                            }

                            let header_str = String::from_utf8_lossy(&buf[..header_end]);
                            let mut lines = header_str.lines();
                            let first_line = lines.next().unwrap_or("");
                            let mut parts = first_line.split_whitespace();
                            let method = parts.next().unwrap_or("GET").to_string();
                            let path = parts.next().unwrap_or("/").to_string();
                            let mut headers = Vec::new();
                            for line in lines {
                                if let Some((k, v)) = line.split_once(':') {
                                    headers.push((k.trim().to_lowercase(), v.trim().to_string()));
                                }
                            }
                            let body = if buf.len() >= header_end + 4 {
                                buf[header_end + 4..header_end + 4 + content_len].to_vec()
                            } else {
                                Vec::new()
                            };

                            let recorded = RecordedRequest {
                                method,
                                path,
                                headers,
                                body,
                            };
                            let resp = {
                                let mut guard = q.lock().unwrap();
                                if let Some(handler) = guard.pop_front() {
                                    handler(&recorded)
                                } else {
                                    MockResponse::json(404, serde_json::json!({"error": "not configured"}))
                                }
                            };
                            reqs.lock().unwrap().push(recorded);

                            let mut resp_bytes = format!(
                                "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n",
                                resp.status, resp.status_text, resp.body.len()
                            ).into_bytes();
                            for (k, v) in resp.headers {
                                resp_bytes.extend_from_slice(format!("{}: {}\r\n", k, v).as_bytes());
                            }
                            resp_bytes.extend_from_slice(b"\r\n");
                            resp_bytes.extend_from_slice(&resp.body);

                            let _ = stream.write_all(&resp_bytes).await;
                            let _ = stream.flush().await;
                        });
                    }
                }
            }
        });

        Self {
            url,
            requests,
            queue,
            shutdown_tx: Some(shutdown_tx),
        }
    }

    pub fn push_response(&self, resp: MockResponse) {
        self.queue.lock().unwrap().push_back(Box::new(move |_| MockResponse {
            status: resp.status,
            status_text: resp.status_text,
            headers: resp.headers.clone(),
            body: resp.body.clone(),
        }));
    }

    pub fn push_handler<F>(&self, f: F)
    where
        F: Fn(&RecordedRequest) -> MockResponse + Send + Sync + 'static,
    {
        self.queue.lock().unwrap().push_back(Box::new(f));
    }

    pub fn recorded_requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

fn parse_content_length(buf: &[u8]) -> usize {
    let s = String::from_utf8_lossy(buf);
    for line in s.lines() {
        if let Some((k, v)) = line.split_once(':')
            && k.trim().eq_ignore_ascii_case("content-length")
            && let Ok(len) = v.trim().parse::<usize>()
        {
            return len;
        }
    }
    0
}
