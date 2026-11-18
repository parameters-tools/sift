// crates/indexer/src/parser/mod.rs
use anyhow::{Context, Result};
use reqwest::multipart;
use serde::Deserialize;
use serde_json::Value;
use shared::models::{BoundingBox, ParsedDocument, ParsedElement, ProvenanceSpan};
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct DoclingApiResponse {
    document: DoclingRawDoc,
    status: String,
}

#[derive(Debug, Deserialize)]
struct DoclingRawDoc {
    #[allow(dead_code)]
    filename: String,
    md_content: Option<String>,
    json_content: Option<Value>,
}

pub struct DoclingLoader {
    client: reqwest::Client,
    endpoint: String,
    table_mode: String,
    do_ocr: bool,
}

impl DoclingLoader {
    pub fn new(base_url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            endpoint: format!("{}/v1/convert/file", base_url.trim_end_matches('/')),
            table_mode: "fast".to_string(),
            do_ocr: true,
        }
    }

    pub fn with_options(mut self, table_mode: &str, do_ocr: bool) -> Self {
        self.table_mode = table_mode.to_string();
        self.do_ocr = do_ocr;
        self
    }

    pub async fn load_pdf(&self, pdf_path: &Path) -> Result<ParsedDocument> {
        let file_bytes = tokio::fs::read(pdf_path)
            .await
            .with_context(|| format!("Failed to read PDF at {:?}", pdf_path))?;

        let filename = pdf_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown.pdf")
            .to_string();

        let file_part = multipart::Part::bytes(file_bytes)
            .file_name(filename.clone())
            .mime_str("application/pdf")?;

        let form = multipart::Form::new()
            .part("files", file_part)
            .text("from_formats", "pdf")
            .text("to_formats", "md")
            .text("to_formats", "json")
            .text("do_table_structure", "true")
            .text("table_mode", self.table_mode.clone())
            .text("do_ocr", self.do_ocr.to_string())
            .text("image_export_mode", "placeholder");

        let resp = self
            .client
            .post(&self.endpoint)
            .multipart(form)
            .send()
            .await
            .context("Failed to connect to docling-serve")?
            .error_for_status()
            .context("docling-serve returned an HTTP error")?
            .json::<DoclingApiResponse>()
            .await
            .context("Failed to deserialize docling-serve JSON response")?;

        if resp.status != "success" {
            anyhow::bail!("Docling conversion failed with status: {}", resp.status);
        }

        let markdown = resp.document.md_content.unwrap_or_default();
        let (elements, page_count) = Self::extract_elements(resp.document.json_content.as_ref());

        Ok(ParsedDocument {
            doc_id: Uuid::new_v4(),
            filename,
            source_path: pdf_path.to_string_lossy().to_string(),
            markdown,
            elements,
            page_count,
        })
    }

    /// Extracts layout elements and page provenance from Docling's JSON tree
    fn extract_elements(json_val: Option<&Value>) -> (Vec<ParsedElement>, u32) {
        let mut elements = Vec::new();
        let mut max_page = 1u32;

        let Some(root) = json_val else {
            return (elements, max_page);
        };

        if let Some(pages_val) = root.get("pages") {
            if let Some(arr) = pages_val.as_array() {
                max_page = arr.len().max(1) as u32;
            } else if let Some(obj) = pages_val.as_object() {
                max_page = obj.len().max(1) as u32;
            }
        } else if let Some(pc) = root.get("page_count").and_then(|v| v.as_u64()) {
            max_page = (pc as u32).max(1);
        }

        if let Some(texts) = root.get("texts").and_then(|t| t.as_array()) {
            for item in texts {
                let label = item
                    .get("label")
                    .and_then(|l| l.as_str())
                    .unwrap_or("text")
                    .to_string();

                let text = item
                    .get("text")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();

                if text.is_empty() {
                    continue;
                }

                let provenance = Self::parse_provenance(item.get("prov"));
                for span in &provenance {
                    max_page = max_page.max(span.page_no);
                }
                elements.push(ParsedElement {
                    label,
                    text,
                    provenance,
                });
            }
        }

        (elements, max_page)
    }

    fn parse_provenance(prov_val: Option<&Value>) -> Vec<ProvenanceSpan> {
        let Some(arr) = prov_val.and_then(|v| v.as_array()) else {
            return Vec::new();
        };

        arr.iter()
            .filter_map(|p| {
                let page_no = p.get("page_no")?.as_u64()? as u32;
                let bbox = p.get("bbox").and_then(|b| {
                    Some(BoundingBox {
                        l: b.get("l")?.as_f64()?,
                        t: b.get("t")?.as_f64()?,
                        r: b.get("r")?.as_f64()?,
                        b: b.get("b")?.as_f64()?,
                    })
                });
                Some(ProvenanceSpan { page_no, bbox })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn test_extract_elements_empty_json() {
        let (elements, pages) = DoclingLoader::extract_elements(None);
        assert!(elements.is_empty());
        assert_eq!(pages, 1);
    }

    #[test]
    fn test_extract_elements_pages_and_texts() {
        let json_val = json!({
            "pages": { "1": {}, "2": {} },
            "texts": [
                {
                    "label": "section_header",
                    "text": "Introduction",
                    "prov": [{ "page_no": 1, "bbox": { "l": 10.0, "t": 20.0, "r": 100.0, "b": 50.0 } }]
                },
                {
                    "label": "text",
                    "text": "Body content here.",
                    "prov": [{ "page_no": 2 }]
                },
                {
                    "label": "text",
                    "text": "   " // should be skipped because it is empty after trim
                }
            ]
        });

        let (elements, pages) = DoclingLoader::extract_elements(Some(&json_val));
        assert_eq!(pages, 2);
        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0].label, "section_header");
        assert_eq!(elements[0].text, "Introduction");
        assert_eq!(elements[0].provenance.len(), 1);
        assert_eq!(elements[0].provenance[0].page_no, 1);
        let bbox = elements[0].provenance[0].bbox.as_ref().unwrap();
        assert_eq!(bbox.l, 10.0);
        assert_eq!(bbox.t, 20.0);
        assert_eq!(bbox.r, 100.0);
        assert_eq!(bbox.b, 50.0);

        assert_eq!(elements[1].label, "text");
        assert_eq!(elements[1].text, "Body content here.");
        assert_eq!(elements[1].provenance[0].page_no, 2);
        assert!(elements[1].provenance[0].bbox.is_none());
    }

    #[tokio::test]
    async fn test_load_pdf_mock_server_success() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let _ = stream.read(&mut buf).await.unwrap();

            let resp_body = json!({
                "status": "success",
                "document": {
                    "filename": "test.pdf",
                    "md_content": "# Mock Document\nThis is mock markdown.",
                    "json_content": {
                        "pages": { "1": {} },
                        "texts": [
                            {
                                "label": "section_header",
                                "text": "Mock Document",
                                "prov": [{ "page_no": 1 }]
                            }
                        ]
                    }
                }
            });
            let body_bytes = serde_json::to_vec(&resp_body).unwrap();
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body_bytes.len()
            );
            stream.write_all(resp.as_bytes()).await.unwrap();
            stream.write_all(&body_bytes).await.unwrap();
        });

        let tmp = std::env::temp_dir().join(format!("rag-test-pdf-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        let dummy_pdf = tmp.join("sample.pdf");
        std::fs::write(&dummy_pdf, b"%PDF-1.4 dummy content").unwrap();

        let loader = DoclingLoader::new(&format!("http://127.0.0.1:{port}"));
        let parsed = loader.load_pdf(&dummy_pdf).await.unwrap();

        assert_eq!(parsed.filename, "sample.pdf");
        assert_eq!(parsed.markdown, "# Mock Document\nThis is mock markdown.");
        assert_eq!(parsed.page_count, 1);
        assert_eq!(parsed.elements.len(), 1);
        assert_eq!(parsed.elements[0].text, "Mock Document");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn test_load_pdf_server_error() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let _ = stream.read(&mut buf).await.unwrap();

            let resp = "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            stream.write_all(resp.as_bytes()).await.unwrap();
        });

        let tmp = std::env::temp_dir().join(format!("rag-test-pdf-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        let dummy_pdf = tmp.join("sample.pdf");
        std::fs::write(&dummy_pdf, b"%PDF-1.4 dummy content").unwrap();

        let loader = DoclingLoader::new(&format!("http://127.0.0.1:{port}"));
        let err = loader.load_pdf(&dummy_pdf).await.unwrap_err();
        assert!(err.to_string().contains("docling-serve returned an HTTP error"));

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
