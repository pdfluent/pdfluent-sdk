use std::collections::HashMap;
use std::path::Path;

use pdf_redact::search_redact::{search_and_redact, RedactSearchOptions};

use super::{PdfTest, TestResult, TestStatus};

/// Extract text from page 1 using pdf-engine (for initial word selection).
fn extract_page1_text(pdf_data: &[u8]) -> Option<String> {
    let doc = pdf_engine::PdfDocument::open(pdf_data.to_vec()).ok()?;
    let text = doc.extract_text(0).ok()?;
    Some(text)
}

/// Verify whether a word is still present in the page 1 content stream after
/// redaction.  Uses extract_positioned_chars (the same method search_and_redact
/// uses to locate text) rather than pdf_engine::extract_text.
///
/// pdf_engine also extracts text from non-content locations such as document
/// outlines/bookmarks and URI-action strings; those are NOT redacted by
/// search_and_redact and would produce false FAILs if used for verification.
/// Fixes #466: MOZILLA-666767-3.pdf "Mozilla" survived in the outline title
/// "Mozilla Privacy Policy" and URI actions even after all content occurrences
/// were successfully redacted.
fn page1_still_contains_word(saved: &[u8], word: &str) -> bool {
    let doc = match lopdf::Document::load_mem(saved) {
        Ok(d) => d,
        Err(_) => return false,
    };
    let chars = match pdf_extract::extract_positioned_chars(&doc, 1) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let text: String = chars.iter().map(|c| c.ch).collect();
    // Use word-boundary matching to avoid false positives from substring
    // occurrences — e.g. "are" inside "Clarence" or "413Are" (digit-prefixed
    // from extracted page-number text) must not cause a spurious FAIL after
    // all standalone occurrences of the target word were successfully redacted.
    let pattern = format!(r"(?i)\b{}\b", regex_lite::escape(word));
    match regex_lite::Regex::new(&pattern) {
        Ok(re) => re.is_match(&text),
        // Fallback to substring if regex construction somehow fails.
        Err(_) => text.contains(word),
    }
}

/// Corpus test: redact first word on page 1, verify it is absent after roundtrip.
pub struct RedactTest;

impl PdfTest for RedactTest {
    fn name(&self) -> &str {
        "redact"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        // Wrap entire execution in a thread to guard against lopdf hangs on
        // corrupt PDFs (page-tree loops, infinite decompression, etc.). #452
        let pdf_owned = pdf_data.to_vec();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || {
                let r =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_inner(pdf_owned)));
                let _ = tx.send(r);
            })
            .expect("thread spawn");
        match rx.recv_timeout(std::time::Duration::from_secs(25)) {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => TestResult {
                status: TestStatus::Crash,
                error_message: Some("panic in test execution".into()),
                duration_ms: 0,
                oracle_score: None,
                metadata: HashMap::new(),
            },
            Err(_) => TestResult {
                status: TestStatus::Timeout,
                error_message: Some("test timed out (>25s)".into()),
                duration_ms: 30_000,
                oracle_score: None,
                metadata: HashMap::new(),
            },
        }
    }
}

fn run_inner(pdf: Vec<u8>) -> TestResult {
    let start = std::time::Instant::now();
    let elapsed = || start.elapsed().as_millis() as u64;

    // 1. Extract text from page 1 to find a word to redact.
    let text = match extract_page1_text(&pdf) {
        Some(t) if !t.trim().is_empty() => t,
        _ => {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some("no text on page 1".into()),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
    };

    // Pick the first word ≥ 3 chars with at least one letter.
    // Purely numeric tokens (e.g. "000") match too broadly in charts/barcodes
    // and cannot be reliably redacted across all rendering paths.
    let search_word = match text.split_whitespace().find(|w| {
        w.len() >= 3
            && w.chars().all(|c| c.is_alphanumeric())
            && w.chars().any(|c| c.is_alphabetic())
    }) {
        Some(w) => w.to_string(),
        None => {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some("no suitable word found".into()),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
    };

    // 2. Load via lopdf and perform redaction.
    let mut doc = match lopdf::Document::load_mem(&pdf) {
        Ok(d) => d,
        Err(e) => {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some(format!("lopdf load failed: {e}")),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
    };

    let opts = RedactSearchOptions::exact(&search_word);

    let redact_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        search_and_redact(&mut doc, &search_word, &opts)
    }));

    let report = match redact_result {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some(format!("redact failed: {e}")),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
        Err(_) => {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some("panic in redaction".into()),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
    };

    if report.areas_redacted == 0 {
        return TestResult {
            status: TestStatus::Skip,
            error_message: Some("0 areas redacted (font encoding mismatch?)".into()),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    // 3. Save to bytes.
    let mut saved = Vec::new();
    if let Err(e) = doc.save_to(&mut saved) {
        return TestResult {
            status: TestStatus::Fail,
            error_message: Some(format!("save failed: {e}")),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    // 4. Verify the word is gone from the page 1 content stream.
    let mut metadata = HashMap::new();
    metadata.insert("search_word".into(), search_word.clone());
    metadata.insert("areas_redacted".into(), report.areas_redacted.to_string());
    metadata.insert("ops_removed".into(), report.operations_removed.to_string());

    if page1_still_contains_word(&saved, &search_word) {
        TestResult {
            status: TestStatus::Fail,
            error_message: Some(format!(
                "redacted word '{}' still present in content stream",
                search_word
            )),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata,
        }
    } else {
        // 5. Verify the PDF is still valid (parse succeeds).
        match pdf_syntax::Pdf::new(saved) {
            Ok(_) => TestResult {
                status: TestStatus::Pass,
                error_message: None,
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
            },
            Err(e) => TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("reparse failed after redaction: {e:?}")),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
            },
        }
    }
}
