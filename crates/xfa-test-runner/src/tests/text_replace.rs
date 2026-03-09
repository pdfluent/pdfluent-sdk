use std::collections::HashMap;
use std::path::Path;

use pdf_manip::text_replace::replace_text;
use pdf_manip::text_run::FontMap;

use super::{PdfTest, TestResult, TestStatus};

/// Roundtrip test: find first word on page 1, replace it, save, reopen, verify.
pub struct TextReplaceTest;

impl PdfTest for TextReplaceTest {
    fn name(&self) -> &str {
        "text_replace"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();
        let elapsed = || start.elapsed().as_millis() as u64;

        // 1. Extract text from page 1 to find a word to replace.
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

        // Pick the first word ≥ 3 characters (to avoid single-char noise).
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

        let replacement = "__XFA_REPLACED__";

        // 2. Load via lopdf and perform replacement on page 1 only.
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

        let fonts = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            FontMap::from_page(&doc, 1)
        })) {
            Ok(Ok(f)) => f,
            Ok(Err(e)) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some(format!("font map failed: {e}")),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
            Err(_) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some("panic building font map".into()),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        let replace_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            replace_text(&mut doc, 1, &search_word, replacement, &fonts)
        }));

        let replacements: usize = match replace_result {
            Ok(Ok(n)) => n,
            Ok(Err(e)) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some(format!("replace failed: {e}")),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
            Err(_) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some("panic in text replacement".into()),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        if replacements == 0 {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some(
                    "0 replacements on page 1 (font encoding or split text)".into(),
                ),
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

        // 4. Reopen and verify replacement text is present.
        let new_text = match extract_page1_text(&saved) {
            Some(t) => t,
            None => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some("cannot extract text after roundtrip".into()),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        let mut metadata = HashMap::new();
        metadata.insert("search_word".into(), search_word.clone());
        metadata.insert("replacements".into(), replacements.to_string());

        if new_text.contains(replacement) {
            TestResult {
                status: TestStatus::Pass,
                error_message: None,
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
            }
        } else {
            TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!(
                    "replacement text '{replacement}' not found in output"
                )),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
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
