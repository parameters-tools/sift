// crates/shared/src/models.rs
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundingBox {
    pub l: f64, // left
    pub t: f64, // top
    pub r: f64, // right
    pub b: f64, // bottom
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProvenanceSpan {
    pub page_no: u32,
    pub bbox: Option<BoundingBox>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedElement {
    pub label: String, // e.g., "section_header", "text", "table", "list_item"
    pub text: String,
    pub provenance: Vec<ProvenanceSpan>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedDocument {
    pub doc_id: Uuid,
    pub filename: String,
    pub source_path: String,
    pub markdown: String, // Structured Markdown with preserved # headings & tables
    pub elements: Vec<ParsedElement>, // Granular layout blocks with page numbers
    pub page_count: u32,
}
