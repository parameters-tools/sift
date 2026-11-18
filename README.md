# Sift

> **Ask your documents. Get cited answers.**

A local-first, API-connected hybrid RAG (Retrieval-Augmented Generation) engine and interactive terminal shell built in Rust. Documents, chunks, and vector embeddings remain stored locally in SQLite, while embedding and answer generation connect to any OpenAI-compatible provider.

---

## Key Features

- **Local-First & Private**: Embeddings, chunk metadata, and document text are persisted in a local SQLite database (`.rag/index.sqlite`). API keys are never stored on disk.
- **Hybrid Retrieval (RRF)**: Combines dense vector search (cosine similarity) with lexical keyword matching via Reciprocal Rank Fusion (RRF) for balanced retrieval accuracy.
- **Interactive Terminal Shell**: Inline viewport powered by Ratatui and Crossterm featuring dynamic terminal width adaptation, hanging indents, and clean line rendering.
- **Precision Inline Citations**: Answers include `[n]` citations in dim cyan. The sources block lists only cited documents with page references, while uncited retrieved chunks are summarized.
- **Conversation Memory & Rewriting**: Retains a rolling 6-turn history. Follow-up questions are automatically rewritten into standalone search queries before retrieval.
- **Inspection Tools**:
  - `/chunks` &mdash; inspect retrieved snippets and citation flags.
  - `/cite <n>` &mdash; view full text of chunk `n` in a framed box.
  - `/sources` & `/stats` &mdash; inspect indexed files and database counts.
  - `/new` &mdash; start a fresh conversation session without clearing screen history.
- **Flexible Endpoints**: Supports unified providers (OpenAI, Ollama, vLLM, LM Studio) or split configurations (e.g. cloud chat model paired with local Ollama embeddings).
- **Document Ingestion**: Recursively parses `.txt`, `.md`, `.markdown`, `.rst`, `.html`, `.htm`, and `.pdf` files. Atomic re-indexing updates modified files cleanly.

---

## Project Architecture

The workspace is organized into modular Rust crates:

- `crates/generator`: The primary CLI and terminal shell (`rag`). Handles chunking, local SQLite vector storage, hybrid RRF search, LLM orchestration, and Ratatui terminal UI.
- `crates/indexer`: Ingestion pipeline supporting document parsing and optional Docling microservice integration for layout-aware PDF extraction.
- `crates/shared`: Common data structures (`ParsedDocument`, `ParsedElement`, `BoundingBox`, `ProvenanceSpan`).

---

## Quickstart

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (edition 2024 / Rust 1.85+)
- `pdftotext` from Poppler (optional, for PDF indexing: `apt-get install poppler-utils` or `brew install poppler`)
- An OpenAI-compatible embedding endpoint and chat completion endpoint

### Installation

```bash
git clone https://github.com/parameters-tools/sift.git
cd sift

# Build the release binary
cargo build --release --workspace

# The executable will be located at:
./target/release/rag
```

You can optionally install it into your Cargo bin path:

```bash
cargo install --path crates/generator
```

---

## Configuration

Sift reads configuration from environment variables or a local `.env` file. Copy the example configuration to get started:

```bash
cp .env.example .env
```

### Option A: Unified Provider (OpenAI, Ollama, vLLM, LM Studio)

```bash
export OPENAI_API_KEY="your-api-key"
export RAG_API_BASE="https://api.openai.com/v1"
export RAG_EMBEDDING_MODEL="text-embedding-3-small"
export RAG_CHAT_MODEL="gpt-4o-mini"
```

### Option B: Split Providers (Cloud Chat + Local Embeddings)

```bash
# Chat Model (e.g., Sarvam AI or OpenRouter)
export RAG_CHAT_BASE="https://api.sarvam.ai/v2"
export RAG_CHAT_API_KEY="your-chat-key"
export RAG_CHAT_MODEL="deepseekv4.1-flash-1017"

# Embedding Model (e.g., Local Ollama)
export RAG_EMBEDDING_BASE="http://127.0.0.1:11434/v1"
export RAG_EMBEDDING_API_KEY="ollama"
export RAG_EMBEDDING_MODEL="embeddinggemma-2"
```

Optional settings:
- `RAG_DB`: Path to SQLite index file (defaults to `.rag/index.sqlite`).

---

## Usage

### 1. Interactive Terminal Shell

Launch the interactive shell by running `rag` with no arguments:

```bash
rag
```

Inside the shell, enter questions directly or use slash commands:

| Command | Description |
|---|---|
| `<question>` | Asks a question across indexed documents with synthesized, cited answers |
| `/search <query>` | Runs hybrid search and displays ranked chunk scores without calling the LLM |
| `/chunks` | Shows the last retrieved chunk passages with citation status |
| `/cite <n>` | Prints the full passage text of chunk `n` in an indented box |
| `/index <path>` | Indexes or reindexes documents from a file or folder on the fly |
| `/sources` | Lists all indexed files and chunk tallies |
| `/stats` | Shows database path, total sources, and total chunks |
| `/new` | Resets conversation memory to begin a new topic |
| `/clear` | Clears the terminal screen and redraws the header |
| `/help` | Displays interactive commands reference |
| `/exit` | Exits the shell (or press `Ctrl+D`) |

### 2. Command Line Interface

You can also use Sift directly from script or command line workflows:

```bash
# Initialize index directory and database tables
rag init

# Recursively index a directory of documents
rag index ./docs --chunk-words 420 --overlap 70

# Query the index and generate an answer with citations
rag ask "What are the key policy requirements?" --top-k 6

# Search relevant chunks without calling generation endpoint
rag search "policy requirements" --top-k 8

# Display database statistics
rag stats
```

---

## Development & Testing

All unit, integration, and end-to-end tests run locally using in-memory databases and local mock HTTP servers without requiring external network access or real API keys.

```bash
# Run the entire test suite (119 tests across crates)
cargo test --workspace

# Run Clippy checks
cargo clippy --workspace --all-targets

# Run end-to-end integration tests
cargo test -p generator --test api_tests
cargo test -p generator --test e2e_pipeline_tests
```

---

## License

This project is licensed under the MIT License.
