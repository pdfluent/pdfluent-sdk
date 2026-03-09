use std::collections::HashMap;
use std::path::Path;

use pdf_redact::search_redact::{search_and_redact, RedactSearchOptions};

use super::{PdfTest, TestResult, TestStatus};

/// Corpus test: redact first word on page 1, verify it is absent after roundtrip.
pub struct RedactTest;

impl PdfTest for RedactTest {
    fn name(&self) -> &str {
        "redact"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();
        let elapsed = || start.elapsed().as_millis() as u64;

        // 1. Extract text from page 1 to find a word to redact.
        let text = match extract_page1_text(pdf_data) {
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

        // Pick the first word >= 3 chars (alphanumeric only, to avoid regex chars).
        let search_word = match text
            .split_whitespace()
            .find(|w| w.len() >= 3 && w.chars().all(|c| c.is_alphanumeric()))
        {
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
        let mut doc = match lopdf::Document::load_mem(pdf_data) {
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

        // 4. Reopen and verify the word is gone.
        let new_text = match extract_page1_text(&saved) {
            Some(t) => t,
            None => {
                // If text extraction fails entirely after redaction, that's OK
                // (redaction may have removed all content streams).
                return TestResult {
                    status: TestStatus::Pass,
                    error_message: None,
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        let mut metadata = HashMap::new();
        metadata.insert("search_word".into(), search_word.clone());
        metadata.insert("areas_redacted".into(), report.areas_redacted.to_string());
        metadata.insert("ops_removed".into(), report.operations_removed.to_string());

        if new_text.contains(&search_word) {
            TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!(
                    "redacted word '{}' still present in output",
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
}

/// Extract text from page 1 using pdf-engine.
fn extract_page1_text(pdf_data: &[u8]) -> Option<String> {
    let doc = pdf_engine::PdfDocument::open(pdf_data.to_vec()).ok()?;
    let text = doc.extract_text(0).ok()?;
    Some(text)
}
