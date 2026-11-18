use anyhow::{Context, Result, bail};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chunk {
    pub id: String,
    pub source: String,
    pub text: String,
    pub vector: Vec<f32>,
}

pub fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Could not create {}", parent.display()))?;
    }
    let db = Connection::open(path)
        .with_context(|| format!("Could not open database {}", path.display()))?;
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
        CREATE TABLE IF NOT EXISTS chunks (id TEXT PRIMARY KEY, source TEXT NOT NULL, text TEXT NOT NULL, vector TEXT NOT NULL);
        CREATE INDEX IF NOT EXISTS chunks_source_idx ON chunks(source);")?;
    Ok(db)
}

pub fn replace_source(db: &mut Connection, source: &str, items: &[Chunk]) -> Result<()> {
    let tx = db.transaction()?;
    tx.execute("DELETE FROM chunks WHERE source=?1", [source])?;
    for item in items {
        tx.execute(
            "INSERT INTO chunks(id,source,text,vector) VALUES (?1,?2,?3,?4)",
            params![
                item.id,
                item.source,
                item.text,
                serde_json::to_string(&item.vector)?
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn all(db: &Connection) -> Result<Vec<Chunk>> {
    let mut stmt = db.prepare("SELECT id,source,text,vector FROM chunks")?;
    let rows = stmt.query_map([], |row| {
        let raw: String = row.get(3)?;
        let vector = serde_json::from_str(&raw).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
        })?;
        Ok(Chunk {
            id: row.get(0)?,
            source: row.get(1)?,
            text: row.get(2)?,
            vector,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .context("Invalid vector data in local database")
}

pub fn get_stats(conn: &Connection) -> Result<(i64, i64)> {
    let chunks: i64 = conn.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))?;
    let sources: i64 = conn.query_row("SELECT COUNT(DISTINCT source) FROM chunks", [], |r| r.get(0))?;
    Ok((sources, chunks))
}

pub fn ensure_compatible(chunks: &[Chunk], query: &[f32]) -> Result<()> {
    if let Some(item) = chunks.iter().find(|c| c.vector.len() != query.len()) {
        bail!(
            "Embedding dimension mismatch for {}. Re-index with the current embedding model.",
            item.source
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_chunk(id: &str, source: &str, text: &str, vector: Vec<f32>) -> Chunk {
        Chunk {
            id: id.to_string(),
            source: source.to_string(),
            text: text.to_string(),
            vector,
        }
    }

    #[test]
    fn test_open_memory_database() {
        let conn = open(Path::new(":memory:")).expect("Failed to open in-memory database");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))
            .expect("Failed to query chunks table");
        assert_eq!(count, 0);
    }

    #[test]
    fn test_open_creates_nested_directories() {
        let tmp = std::env::temp_dir().join(format!("rag-test-db-{}", uuid::Uuid::new_v4()));
        let db_path = tmp.join("nested").join("sub").join("index.sqlite");
        assert!(!db_path.exists());

        let conn = open(&db_path).expect("Failed to create nested db");
        drop(conn);
        assert!(db_path.exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_replace_source_and_all() {
        let mut conn = open(Path::new(":memory:")).unwrap();
        let chunks = vec![
            make_chunk("c1", "source1.txt", "first chunk", vec![0.1, 0.2, 0.3]),
            make_chunk("c2", "source1.txt", "second chunk", vec![0.4, 0.5, 0.6]),
        ];

        replace_source(&mut conn, "source1.txt", &chunks).unwrap();
        let retrieved = all(&conn).unwrap();
        assert_eq!(retrieved.len(), 2);
        assert_eq!(retrieved, chunks);
    }

    #[test]
    fn test_reindexing_replaces_old_chunks_atomically() {
        let mut conn = open(Path::new(":memory:")).unwrap();
        let original_chunks = vec![
            make_chunk("c1", "doc.txt", "old chunk 1", vec![1.0, 2.0]),
            make_chunk("c2", "doc.txt", "old chunk 2", vec![3.0, 4.0]),
        ];
        replace_source(&mut conn, "doc.txt", &original_chunks).unwrap();
        assert_eq!(all(&conn).unwrap().len(), 2);

        // Re-index with new chunks
        let new_chunks = vec![
            make_chunk("c3", "doc.txt", "new replacement chunk", vec![5.0, 6.0]),
        ];
        replace_source(&mut conn, "doc.txt", &new_chunks).unwrap();

        let updated = all(&conn).unwrap();
        assert_eq!(updated.len(), 1);
        assert_eq!(updated[0].id, "c3");
        assert_eq!(updated[0].text, "new replacement chunk");
    }

    #[test]
    fn test_replace_source_empty_deletes_chunks() {
        let mut conn = open(Path::new(":memory:")).unwrap();
        let chunks = vec![make_chunk("c1", "doc.txt", "some text", vec![1.0])];
        replace_source(&mut conn, "doc.txt", &chunks).unwrap();
        assert_eq!(all(&conn).unwrap().len(), 1);

        // Clear with empty slice
        replace_source(&mut conn, "doc.txt", &[]).unwrap();
        assert_eq!(all(&conn).unwrap().len(), 0);
    }

    #[test]
    fn test_multiple_sources_isolation() {
        let mut conn = open(Path::new(":memory:")).unwrap();
        let doc1 = vec![make_chunk("c1", "doc1.txt", "doc1 chunk", vec![1.0])];
        let doc2 = vec![make_chunk("c2", "doc2.txt", "doc2 chunk", vec![2.0])];

        replace_source(&mut conn, "doc1.txt", &doc1).unwrap();
        replace_source(&mut conn, "doc2.txt", &doc2).unwrap();
        assert_eq!(all(&conn).unwrap().len(), 2);

        // Reindex doc1 only
        let doc1_new = vec![
            make_chunk("c1_new_1", "doc1.txt", "doc1 new 1", vec![1.1]),
            make_chunk("c1_new_2", "doc1.txt", "doc1 new 2", vec![1.2]),
        ];
        replace_source(&mut conn, "doc1.txt", &doc1_new).unwrap();

        let all_chunks = all(&conn).unwrap();
        assert_eq!(all_chunks.len(), 3);
        let doc2_chunks: Vec<_> = all_chunks.into_iter().filter(|c| c.source == "doc2.txt").collect();
        assert_eq!(doc2_chunks, doc2);
    }

    #[test]
    fn test_ensure_compatible() {
        let chunks = vec![
            make_chunk("c1", "doc1.txt", "text", vec![0.1, 0.2, 0.3]),
            make_chunk("c2", "doc2.txt", "text", vec![0.4, 0.5, 0.6]),
        ];
        let query_matching = vec![0.7, 0.8, 0.9];
        let query_mismatched = vec![0.7, 0.8];

        assert!(ensure_compatible(&chunks, &query_matching).is_ok());
        let err = ensure_compatible(&chunks, &query_mismatched).unwrap_err();
        assert!(err.to_string().contains("dimension mismatch"));
    }

    #[test]
    fn test_ensure_compatible_empty_chunks() {
        assert!(ensure_compatible(&[], &[0.1, 0.2]).is_ok());
    }

    #[test]
    fn test_all_invalid_vector_json() {
        let conn = open(Path::new(":memory:")).unwrap();
        conn.execute(
            "INSERT INTO chunks(id, source, text, vector) VALUES ('bad', 'doc', 'text', 'not-json')",
            [],
        )
        .unwrap();

        assert!(all(&conn).is_err());
    }
}
