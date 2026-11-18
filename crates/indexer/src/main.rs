// crates/indexer/src/main.rs
use anyhow::Result;
use indexer::parser::DoclingLoader;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let pdf_path = args
        .get(1)
        .map(Path::new)
        .unwrap_or_else(|| Path::new("sample.pdf"));

    let loader = DoclingLoader::new("http://127.0.0.1:5001");
    println!("Parsing {:?} via Docling...", pdf_path);

    let parsed = loader.load_pdf(pdf_path).await?;
    println!(
        "Parsed '{}' ({} pages, {} elements)",
        parsed.filename,
        parsed.page_count,
        parsed.elements.len()
    );
    println!(
        "\n--- Markdown Preview ---\n{}",
        &parsed.markdown[..parsed.markdown.len().min(600)]
    );

    Ok(())
}
