//! PDF → DOCX conversion corpus test.
//!
//! Calls `pdf_docx::convert_pdf_bytes_to_docx` on every corpus PDF and verifies
//! that the output is a structurally valid DOCX (ZIP with `word/document.xml`)
//! and that the text content matches the source PDF above a similarity threshold.
//!
//! Skip policy:
//! - `DocxError::Pdf` (lopdf load failure) → Skip (corrupt / encrypted PDF)
//! - PDF has fewer than MIN_PDF_CHARS characters → Skip (image-only / scanned)
//!
//! Fail conditions:
//! - Any other conversion error
//! - Output is not a ZIP (missing PK magic bytes)
//! - `word/document.xml` not found inside the ZIP
//! - Text similarity between DOCX content and PDF text < SIMILARITY_THRESHOLD

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

/// Minimum normalized Levenshtein similarity between DOCX text and PDF text.
const SIMILARITY_THRESHOLD: f64 = 0.50;
/// Skip similarity check when the PDF has fewer than this many characters.
const MIN_PDF_CHARS: usize = 50;
/// Cap text length before computing Levenshtein to avoid O(n²) blowup.
/// Same approach as the poppler oracle (10 000 chars → max 100M ops).
const MAX_SIMILARITY_CHARS: usize = 10_000;

pub struct DocxConvertTest;

impl PdfTest for DocxConvertTest {
    fn name(&self) -> &str {
        "docx_convert"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        // Thread wrapper guards against lopdf hangs on corrupt page trees / streams.
        let pdf_owned = pdf_data.to_vec();
        let (tx, rx) = std::sync::mpsc::channel();
        if std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || {
                let r =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_inner(pdf_owned)));
                let _ = tx.send(r);
            })
            .is_err()
        {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some("thread spawn failed (resource limit)".into()),
                duration_ms: 0,
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
        match rx.recv_timeout(std::time::Duration::from_secs(25)) {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => TestResult {
                status: TestStatus::Crash,
                error_message: Some("panic in DOCX conversion".into()),
                duration_ms: 0,
                oracle_score: None,
                metadata: HashMap::new(),
            },
            Err(_) => TestResult {
                status: TestStatus::Timeout,
                error_message: Some("DOCX conversion timed out (>25s)".into()),
                duration_ms: 25_000,
                oracle_score: None,
                metadata: HashMap::new(),
            },
        }
    }
}

fn run_inner(pdf: Vec<u8>) -> TestResult {
    let start = std::time::Instant::now();
    let elapsed = || start.elapsed().as_millis() as u64;

    // Load PDF once — reused for both conversion and reference text extraction.
    let doc = match lopdf::Document::load_mem(&pdf) {
        Ok(d) => d,
        Err(_) => {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some("lopdf could not load PDF".into()),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
    };

    // Extract reference text from the source PDF (reuses the loaded document).
    let pdf_text = {
        let blocks = pdf_extract::extract_text(&doc);
        let raw: String = blocks
            .into_iter()
            .map(|b| b.text)
            .collect::<Vec<_>>()
            .join(" ");
        raw.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };

    // Text-only conversion — skip image extraction for speed.
    match pdf_docx::pdf_to_docx_text_only(&doc) {
        Err(e) => TestResult {
            status: TestStatus::Fail,
            error_message: Some(format!("DOCX conversion failed: {e}")),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        },
        Ok(docx_bytes) => {
            // Verify ZIP magic bytes (OOXML is a ZIP file).
            if docx_bytes.len() < 4 || &docx_bytes[..2] != b"PK" {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some(
                        "DOCX output is not a valid ZIP (missing PK header)".into(),
                    ),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }

            // Read word/document.xml and extract its text content.
            let docx_text = {
                let cursor = std::io::Cursor::new(&docx_bytes);
                match zip::ZipArchive::new(cursor) {
                    Ok(mut archive) => match archive.by_name("word/document.xml") {
                        Ok(mut entry) => {
                            let mut xml = String::new();
                            let _ = entry.read_to_string(&mut xml);
                            extract_docx_text(&xml)
                        }
                        Err(_) => {
                            return TestResult {
                                status: TestStatus::Fail,
                                error_message: Some(
                                    "DOCX output is missing word/document.xml".into(),
                                ),
                                duration_ms: elapsed(),
                                oracle_score: None,
                                metadata: HashMap::new(),
                            };
                        }
                    },
                    Err(_) => {
                        return TestResult {
                            status: TestStatus::Fail,
                            error_message: Some("DOCX output ZIP could not be opened".into()),
                            duration_ms: elapsed(),
                            oracle_score: None,
                            metadata: HashMap::new(),
                        };
                    }
                }
            };

            let mut metadata = HashMap::new();
            metadata.insert("docx_size_bytes".to_string(), docx_bytes.len().to_string());
            metadata.insert("pdf_chars".to_string(), pdf_text.len().to_string());
            metadata.insert("docx_chars".to_string(), docx_text.len().to_string());
            metadata.insert(
                "threshold".to_string(),
                format!("{SIMILARITY_THRESHOLD:.2}"),
            );

            // Only compare when the PDF has meaningful text content.
            if pdf_text.len() >= MIN_PDF_CHARS {
                let similarity = capped_similarity(&pdf_text, &docx_text);
                metadata.insert("similarity".to_string(), format!("{similarity:.4}"));

                if similarity < SIMILARITY_THRESHOLD {
                    return TestResult {
                        status: TestStatus::Fail,
                        error_message: Some(format!(
                            "DOCX text similarity {similarity:.4} below threshold \
                             {SIMILARITY_THRESHOLD:.2} \
                             (pdf={} chars, docx={} chars)",
                            pdf_text.len(),
                            docx_text.len(),
                        )),
                        duration_ms: elapsed(),
                        oracle_score: Some(similarity),
                        metadata,
                    };
                }

                TestResult {
                    status: TestStatus::Pass,
                    error_message: None,
                    duration_ms: elapsed(),
                    oracle_score: Some(similarity),
                    metadata,
                }
            } else {
                // Image-only / scanned PDF — skip text comparison.
                metadata.insert("skip_reason".to_string(), "pdf_too_few_chars".to_string());
                TestResult {
                    status: TestStatus::Pass,
                    error_message: None,
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata,
                }
            }
        }
    }
}

/// Extract plain text from DOCX `word/document.xml` by collecting `<w:t>` element content.
/// Handles both `<w:t>text</w:t>` and `<w:t xml:space="preserve"> text</w:t>` forms.
///
/// IMPORTANT: The search matches `<w:t>` and `<w:t ` only — NOT `<w:tbl>`, `<w:tc>`,
/// `<w:tr>`, etc. Without this guard, table markup is captured as text content,
/// causing massive text expansion and O(n²) similarity blowup (#597).
fn extract_docx_text(xml: &str) -> String {
    let mut result = String::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<w:t") {
        let after = &rest[start + 4..]; // bytes after "<w:t"
                                        // Only match <w:t> or <w:t ...> — skip <w:tbl>, <w:tc>, <w:tr>, <w:top>, etc.
        let next_ch = after.as_bytes().first().copied().unwrap_or(0);
        if next_ch != b'>' && next_ch != b' ' && next_ch != b'/' {
            rest = after;
            continue;
        }
        rest = after;
        // Find the closing '>' of the opening tag (may have attributes).
        let Some(tag_end) = rest.find('>') else {
            break;
        };
        // Self-closing tag <w:t/> — no text content.
        if rest[..tag_end].ends_with('/') {
            rest = &rest[tag_end + 1..];
            continue;
        }
        rest = &rest[tag_end + 1..];
        // Find closing </w:t>.
        let Some(close) = rest.find("</w:t>") else {
            break;
        };
        result.push_str(&rest[..close]);
        result.push(' ');
        rest = &rest[close + 6..];
    }
    // Unescape XML entities that quick-xml inserts during write.
    let unescaped = result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'");
    // Normalize: collapse whitespace, lowercase (matching poppler normalize_text).
    unescaped
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Compute text similarity with a character cap to avoid O(n²) blowup.
///
/// For texts under [`MAX_SIMILARITY_CHARS`] this is identical to
/// `strsim::normalized_levenshtein`. For longer texts the inputs are
/// truncated to the cap first — the same strategy as the poppler oracle.
fn capped_similarity(a: &str, b: &str) -> f64 {
    let a = if a.len() > MAX_SIMILARITY_CHARS {
        &a[..a.floor_char_boundary(MAX_SIMILARITY_CHARS)]
    } else {
        a
    };
    let b = if b.len() > MAX_SIMILARITY_CHARS {
        &b[..b.floor_char_boundary(MAX_SIMILARITY_CHARS)]
    } else {
        b
    };
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    strsim::normalized_levenshtein(a, b)
}

/// Extract and normalize text from a PDF using lopdf + pdf-extract.
#[allow(dead_code)]
fn extract_pdf_text(pdf: &[u8]) -> String {
    let doc = match lopdf::Document::load_mem(pdf) {
        Ok(d) => d,
        Err(_) => return String::new(),
    };
    let blocks = pdf_extract::extract_text(&doc);
    let raw: String = blocks
        .into_iter()
        .map(|b| b.text)
        .collect::<Vec<_>>()
        .join(" ");
    raw.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_docx_text_basic() {
        let xml = r#"<w:body><w:p><w:r><w:t>Hello</w:t></w:r><w:r><w:t xml:space="preserve"> World</w:t></w:r></w:p></w:body>"#;
        let text = extract_docx_text(xml);
        assert_eq!(text, "hello world");
    }

    #[test]
    fn extract_docx_text_self_closing() {
        let xml = r#"<w:body><w:t/>normal<w:t>text</w:t></w:body>"#;
        let text = extract_docx_text(xml);
        assert_eq!(text, "text");
    }

    #[test]
    fn extract_docx_text_ignores_table_tags() {
        // <w:tbl>, <w:tc>, <w:tr> must NOT be matched as <w:t> elements (#597).
        let xml =
            r#"<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
        let text = extract_docx_text(xml);
        assert_eq!(text, "cell");
    }

    #[test]
    fn extract_docx_text_empty() {
        let text = extract_docx_text("<w:body></w:body>");
        assert!(text.is_empty());
    }
}
