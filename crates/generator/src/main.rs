use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use generator::{api::Api, chunker, db, retrieve, sources};
use std::{
    path::{Path, PathBuf},
    time::Instant,
};
use uuid::Uuid;

#[derive(Parser)]
#[command(name="rag", version, about="Local-first hybrid RAG CLI. Connect any OpenAI-compatible API.", long_about=None)]
struct Cli {
    /// SQLite index path (also configurable with RAG_DB)
    #[arg(
        long,
        global = true,
        env = "RAG_DB",
        default_value = ".rag/index.sqlite"
    )]
    db: PathBuf,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Launch interactive Claude Code-like shell (default when no command specified).
    Chat,
    /// Create the local index and print API configuration instructions.
    Init,
    /// Add or refresh files from a path (PDF, Markdown, HTML, RST, or text).
    Index {
        path: PathBuf,
        #[arg(long, default_value_t = 420)]
        chunk_words: usize,
        #[arg(long, default_value_t = 70)]
        overlap: usize,
    },
    /// Ask a question and generate a cited answer from indexed sources.
    Ask {
        question: String,
        #[arg(long, default_value_t = 6)]
        top_k: usize,
    },
    /// Search indexed chunks without calling the chat completion endpoint.
    Search {
        query: String,
        #[arg(long, default_value_t = 8)]
        top_k: usize,
    },
    /// Show the number of indexed chunks and sources.
    Stats,
}

fn database(path: &Path) -> Result<rusqlite::Connection> {
    db::open(path)
}

fn load_env_file() {
    for path in [".env", "../.env"] {
        if let Ok(content) = std::fs::read_to_string(path) {
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((k, v)) = line.split_once('=') {
                    let k = k.trim();
                    let v = v.trim().trim_matches('"').trim_matches('\'');
                    if std::env::var(k).is_err() {
                        unsafe {
                            std::env::set_var(k, v);
                        }
                    }
                }
            }
            break;
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    load_env_file();
    let cli = Cli::parse();
    match cli.command {
        None | Some(Command::Chat) => {
            generator::interactive::run_interactive(&cli.db).await?;
        }
        Some(Command::Init) => {
            let conn = database(&cli.db)?;
            println!("Initialized local index at {}", cli.db.display());
            println!(
                "Set OPENAI_API_KEY (required), RAG_API_BASE (optional, default https://api.openai.com/v1), RAG_EMBEDDING_MODEL, and RAG_CHAT_MODEL."
            );
            println!(
                "The API must implement the OpenAI-compatible /embeddings and /chat/completions endpoints."
            );
            drop(conn);
        }
        Some(Command::Index {
            path,
            chunk_words,
            overlap,
        }) => {
            if chunk_words == 0 {
                bail!("--chunk-words must be greater than zero");
            }
            if overlap >= chunk_words {
                bail!("--overlap must be less than --chunk-words");
            }
            let files = sources::collect(&path)?;
            let file_count = files.len();
            let api = Api::from_env()?;
            let mut conn = database(&cli.db)?;
            let mut total = 0usize;
            for file in files {
                let source = std::fs::canonicalize(&file)
                    .unwrap_or_else(|_| file.clone())
                    .to_string_lossy()
                    .to_string();
                let text = sources::read(&file)?;
                let pieces = chunker::chunk(&text, chunk_words, overlap);
                if pieces.is_empty() {
                    db::replace_source(&mut conn, &source, &[])?;
                    println!("Skipped empty file {}", file.display());
                    continue;
                }
                let mut items = Vec::with_capacity(pieces.len());
                for batch in pieces.chunks(64) {
                    let input = batch.to_vec();
                    let vectors = api.embed(&input).await.with_context(|| {
                        format!("Embedding failed while indexing {}", file.display())
                    })?;
                    for (text, vector) in batch.iter().zip(vectors) {
                        items.push(db::Chunk {
                            id: Uuid::new_v4().to_string(),
                            source: source.clone(),
                            text: text.clone(),
                            vector,
                        });
                    }
                }
                let count = items.len();
                db::replace_source(&mut conn, &source, &items)?;
                total += count;
                println!("Indexed {} ({} chunks)", file.display(), count);
            }
            println!(
                "Done: {} files, {} chunks in {}",
                file_count,
                total,
                cli.db.display()
            );
        }
        Some(Command::Search { query, top_k }) => {
            if query.trim().is_empty() {
                bail!("Query cannot be empty");
            }
            if top_k == 0 {
                bail!("--top-k must be greater than zero");
            }
            let conn = database(&cli.db)?;
            let chunks = db::all(&conn)?;
            if chunks.is_empty() {
                bail!("The index is empty. Run `rag index <path>` first.");
            }
            let api = Api::from_env()?;
            let vectors = api.embed(std::slice::from_ref(&query)).await?;
            db::ensure_compatible(&chunks, &vectors[0])?;
            for (i, hit) in retrieve::hybrid(chunks, &query, &vectors[0], top_k)
                .into_iter()
                .enumerate()
            {
                println!(
                    "[{}] {}  (RRF {:.5})\n{}\n",
                    i + 1,
                    hit.chunk.source,
                    hit.score,
                    hit.chunk.text
                );
            }
        }
        Some(Command::Ask { question, top_k }) => {
            if question.trim().is_empty() {
                bail!("Question cannot be empty");
            }
            if top_k == 0 {
                bail!("--top-k must be greater than zero");
            }
            let started = Instant::now();
            let conn = database(&cli.db)?;
            let chunks = db::all(&conn)?;
            if chunks.is_empty() {
                bail!("The index is empty. Run `rag index <path>` first.");
            }
            let api = Api::from_env()?;
            let vectors = api.embed(std::slice::from_ref(&question)).await?;
            db::ensure_compatible(&chunks, &vectors[0])?;
            let hits = retrieve::hybrid(chunks, &question, &vectors[0], top_k);
            let contexts: Vec<(String, String)> = hits
                .iter()
                .map(|hit| {
                    (
                        hit.chunk.source.clone(),
                        hit.chunk.text.chars().take(5000).collect(),
                    )
                })
                .collect();
            let answer = api.answer(&question, &contexts).await?;
            println!("{answer}\n\nSources:");
            for (i, hit) in hits.iter().enumerate() {
                println!("[{}] {}", i + 1, hit.chunk.source);
            }
            println!(
                "\n({} retrieved chunks, {:.1}s)",
                hits.len(),
                started.elapsed().as_secs_f32()
            );
        }
        Some(Command::Stats) => {
            let conn = database(&cli.db)?;
            let chunks: i64 = conn.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))?;
            let sources: i64 =
                conn.query_row("SELECT COUNT(DISTINCT source) FROM chunks", [], |r| {
                    r.get(0)
                })?;
            println!(
                "Database: {}\nSources: {sources}\nChunks: {chunks}",
                cli.db.display()
            );
        }
    }
    Ok(())
}
