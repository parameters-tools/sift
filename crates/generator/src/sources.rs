use anyhow::{Context, Result, bail};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
use walkdir::WalkDir;

pub fn collect(path: &Path) -> Result<Vec<PathBuf>> {
    if !path.exists() {
        bail!("Input path does not exist: {}", path.display());
    }
    let mut files = if path.is_file() {
        vec![path.to_path_buf()]
    } else {
        WalkDir::new(path)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                e.depth() == 0 || !e.file_name().to_string_lossy().starts_with('.')
            })
            .collect::<std::result::Result<Vec<_>, _>>()
            .with_context(|| format!("Could not walk {}", path.display()))?
            .into_iter()
            .filter(|entry| entry.file_type().is_file())
            .map(|entry| entry.into_path())
            .collect()
    };
    files.retain(|p| {
        matches!(
            p.extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str(),
            "txt" | "md" | "markdown" | "rst" | "html" | "htm" | "pdf"
        )
    });
    files.sort();
    if files.is_empty() {
        bail!("No supported files found (supported: txt, md, rst, html, pdf)");
    }
    Ok(files)
}

pub fn read(path: &Path) -> Result<String> {
    if path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
    {
        let output = Command::new("pdftotext")
            .args([
                "-layout",
                path.to_str().context("PDF path is not valid UTF-8")?,
                "-",
            ])
            .output()
            .context("Could not run pdftotext. Install Poppler (pdftotext) to index PDFs")?;
        if !output.status.success() {
            bail!(
                "pdftotext failed for {}: {}",
                path.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        return String::from_utf8(output.stdout).context("PDF text was not valid UTF-8");
    }
    std::fs::read_to_string(path)
        .with_context(|| format!("Could not read {} as UTF-8 text", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_nonexistent_path() {
        let path = PathBuf::from("/nonexistent/path/for/sure");
        assert!(collect(&path).is_err());
    }

    #[test]
    fn test_collect_single_supported_file() {
        let tmp = std::env::temp_dir().join(format!("rag-test-sources-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        let file = tmp.join("doc.md");
        std::fs::write(&file, "# Hello World").unwrap();

        let files = collect(&file).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], file);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_collect_directory_filtering() {
        let tmp = std::env::temp_dir().join(format!("rag-test-sources-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();

        // Supported files
        std::fs::write(tmp.join("doc1.txt"), "txt file").unwrap();
        std::fs::write(tmp.join("doc2.md"), "md file").unwrap();
        std::fs::write(tmp.join("doc3.html"), "html file").unwrap();
        std::fs::write(tmp.join("doc4.rst"), "rst file").unwrap();

        // Unsupported files
        std::fs::write(tmp.join("image.png"), "png file").unwrap();
        std::fs::write(tmp.join("binary.bin"), "bin file").unwrap();

        // Hidden file
        std::fs::write(tmp.join(".hidden.txt"), "hidden").unwrap();

        // Hidden directory
        let hidden_dir = tmp.join(".git");
        std::fs::create_dir_all(&hidden_dir).unwrap();
        std::fs::write(hidden_dir.join("inside_git.txt"), "inside git").unwrap();

        // Nested supported file
        let nested = tmp.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("doc5.markdown"), "markdown file").unwrap();

        let files = collect(&tmp).unwrap();
        assert_eq!(files.len(), 5);
        let names: Vec<_> = files.iter().map(|f| f.file_name().unwrap().to_str().unwrap()).collect();
        assert!(names.contains(&"doc1.txt"));
        assert!(names.contains(&"doc2.md"));
        assert!(names.contains(&"doc3.html"));
        assert!(names.contains(&"doc4.rst"));
        assert!(names.contains(&"doc5.markdown"));
        assert!(!names.contains(&"image.png"));
        assert!(!names.contains(&".hidden.txt"));
        assert!(!names.contains(&"inside_git.txt"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_read_text_file() {
        let tmp = std::env::temp_dir().join(format!("rag-test-sources-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        let file = tmp.join("content.txt");
        std::fs::write(&file, "Hello from local file").unwrap();

        let content = read(&file).unwrap();
        assert_eq!(content, "Hello from local file");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
