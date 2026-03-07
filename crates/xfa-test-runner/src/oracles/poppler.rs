use std::path::Path;
use std::process::Command;

/// Parsed output from `pdfinfo`.
pub struct PdfInfo {
    pub page_count: Option<usize>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub producer: Option<String>,
    pub pdf_version: Option<String>,
}

/// Wrapper around Poppler CLI tools (`pdftotext`, `pdfinfo`).
pub struct PopplerOracle;

impl PopplerOracle {
    /// Check whether `pdftotext` is available on the system.
    pub fn is_available() -> bool {
        Command::new("pdftotext").arg("-v").output().is_ok()
    }

    /// Extract text from a single page (0-indexed) via `pdftotext`.
    #[allow(dead_code)]
    pub fn extract_text(pdf_path: &Path, page: usize) -> Result<String, String> {
        let page_1indexed = (page + 1).to_string();
        let output = Command::new("pdftotext")
            .args([
                "-f",
                &page_1indexed,
                "-l",
                &page_1indexed,
                "-enc",
                "UTF-8",
                pdf_path.to_str().unwrap_or_default(),
                "-", // stdout
            ])
            .output()
            .map_err(|e| format!("pdftotext spawn failed: {e}"))?;

        if !output.status.success() {
            return Err(format!(
                "pdftotext exit {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Extract text from all pages via `pdftotext` (no page range).
    pub fn extract_all_text(pdf_path: &Path) -> Result<String, String> {
        let output = Command::new("pdftotext")
            .args(["-enc", "UTF-8", pdf_path.to_str().unwrap_or_default(), "-"])
            .output()
            .map_err(|e| format!("pdftotext spawn failed: {e}"))?;

        if !output.status.success() {
            return Err(format!(
                "pdftotext exit {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Get PDF metadata via `pdfinfo`.
    pub fn get_info(pdf_path: &Path) -> Result<PdfInfo, String> {
        let output = Command::new("pdfinfo")
            .arg(pdf_path.to_str().unwrap_or_default())
            .output()
            .map_err(|e| format!("pdfinfo spawn failed: {e}"))?;

        if !output.status.success() {
            return Err(format!(
                "pdfinfo exit {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut info = PdfInfo {
            page_count: None,
            title: None,
            author: None,
            producer: None,
            pdf_version: None,
        };

        for line in stdout.lines() {
            if let Some((key, val)) = line.split_once(':') {
                let key = key.trim();
                let val = val.trim();
                if val.is_empty() {
                    continue;
                }
                match key {
                    "Pages" => info.page_count = val.parse().ok(),
                    "Title" => info.title = Some(val.to_string()),
                    "Author" => info.author = Some(val.to_string()),
                    "Producer" => info.producer = Some(val.to_string()),
                    "PDF version" => info.pdf_version = Some(val.to_string()),
                    _ => {}
                }
            }
        }

        Ok(info)
    }
}

/// Normalize text for comparison: collapse whitespace, lowercase, trim.
pub fn normalize_text(text: &str) -> String {
    text.replace('\r', "")
        .replace('\t', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Normalized Levenshtein similarity: 1.0 = identical, 0.0 = completely different.
/// For very long texts, only compares the first `max_chars` characters.
pub fn text_similarity(a: &str, b: &str) -> f64 {
    const MAX_CHARS: usize = 10_000;

    let a_capped = if a.len() > MAX_CHARS {
        &a[..MAX_CHARS]
    } else {
        a
    };
    let b_capped = if b.len() > MAX_CHARS {
        &b[..MAX_CHARS]
    } else {
        b
    };

    if a_capped.is_empty() && b_capped.is_empty() {
        return 1.0;
    }

    strsim::normalized_levenshtein(a_capped, b_capped)
}

/// Save a side-by-side diff when similarity is low.
pub fn save_text_diff(
    pdf_path: &Path,
    our_text: &str,
    poppler_text: &str,
    similarity: f64,
    diff_dir: &Path,
) -> std::io::Result<()> {
    std::fs::create_dir_all(diff_dir)?;

    let stem = pdf_path.file_stem().unwrap_or_default().to_string_lossy();

    let diff_path = diff_dir.join(format!("{stem}.diff.txt"));
    let content = format!(
        "PDF: {}\nSimilarity: {:.4}\n\n=== OUR ENGINE ===\n{}\n\n=== POPPLER (pdftotext) ===\n{}\n",
        pdf_path.display(),
        similarity,
        &our_text[..our_text.len().min(5000)],
        &poppler_text[..poppler_text.len().min(5000)],
    );
    std::fs::write(diff_path, content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_collapses_whitespace() {
        assert_eq!(normalize_text("  hello   world  "), "hello world");
    }

    #[test]
    fn normalize_handles_tabs_and_newlines() {
        assert_eq!(normalize_text("hello\t\tworld\r\nfoo"), "hello world foo");
    }

    #[test]
    fn normalize_lowercases() {
        assert_eq!(normalize_text("Hello World"), "hello world");
    }

    #[test]
    fn similarity_identical() {
        assert!((text_similarity("hello world", "hello world") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn similarity_empty_both() {
        assert!((text_similarity("", "") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn similarity_completely_different() {
        let score = text_similarity("aaaa", "zzzz");
        assert!(score < 0.5);
    }

    #[test]
    fn similarity_minor_difference() {
        let score = text_similarity("the quick brown fox", "the quick brown foy");
        assert!(score > 0.90);
    }

    #[test]
    fn similarity_caps_long_text() {
        let a = "x".repeat(20_000);
        let b = "x".repeat(20_000);
        // Should not panic or take forever — capped to 10K chars
        let score = text_similarity(&a, &b);
        assert!((score - 1.0).abs() < 1e-9);
    }
}
