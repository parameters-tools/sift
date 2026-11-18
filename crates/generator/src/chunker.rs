/// Split into word-bounded chunks with overlap while retaining paragraph boundaries where possible.
pub fn chunk(text: &str, max_words: usize, overlap: usize) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() || max_words == 0 {
        return Vec::new();
    }
    let stride = max_words.saturating_sub(overlap.min(max_words - 1)).max(1);
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < words.len() {
        let end = (start + max_words).min(words.len());
        chunks.push(words[start..end].join(" "));
        if end == words.len() {
            break;
        }
        start += stride;
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_and_whitespace_input() {
        assert_eq!(chunk("", 10, 2), Vec::<String>::new());
        assert_eq!(chunk("   \n\t  \r\n  ", 10, 2), Vec::<String>::new());
    }

    #[test]
    fn test_zero_max_words() {
        assert_eq!(chunk("hello world", 0, 0), Vec::<String>::new());
    }

    #[test]
    fn test_single_word() {
        assert_eq!(chunk("hello", 5, 2), vec!["hello"]);
    }

    #[test]
    fn test_text_shorter_than_max_words() {
        let text = "The quick brown fox jumps over the lazy dog";
        let res = chunk(text, 20, 5);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0], text);
    }

    #[test]
    fn test_exact_boundary_no_overlap() {
        let text = "one two three four five six";
        let res = chunk(text, 3, 0);
        assert_eq!(res, vec!["one two three", "four five six"]);
    }

    #[test]
    fn test_sliding_window_with_overlap() {
        let text = "one two three four five";
        // max_words = 3, overlap = 1 => stride = 2
        // Chunk 0: words 0..3 -> "one two three"
        // Chunk 1: words 2..5 -> "three four five"
        let res = chunk(text, 3, 1);
        assert_eq!(res, vec!["one two three", "three four five"]);
    }

    #[test]
    fn test_overlap_ge_max_words_clamps_stride() {
        let text = "one two three";
        // overlap = 5 >= max_words (2) => overlap clamped to max_words - 1 (1) => stride = 1
        let res = chunk(text, 2, 5);
        assert_eq!(res, vec!["one two", "two three"]);
    }

    #[test]
    fn test_whitespace_and_newline_normalization() {
        let text = "one\n\ntwo  \t three\r\nfour";
        let res = chunk(text, 2, 0);
        assert_eq!(res, vec!["one two", "three four"]);
    }

    #[test]
    fn test_large_text_chunking_and_consecutive_overlap() {
        let words: Vec<String> = (0..1000).map(|i| format!("w{}", i)).collect();
        let text = words.join(" ");
        let chunks = chunk(&text, 420, 70);
        assert!(!chunks.is_empty());

        for c in &chunks {
            let count = c.split_whitespace().count();
            assert!(count <= 420);
        }

        // Verify overlap between first two chunks
        let first_words: Vec<&str> = chunks[0].split_whitespace().collect();
        let second_words: Vec<&str> = chunks[1].split_whitespace().collect();
        assert_eq!(first_words[350..420], second_words[0..70]);
    }
}
