use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};
use crate::oracles::poppler::{self, PopplerOracle};

/// Compares our text extraction against Poppler's `pdftotext`.
pub struct TextOracleTest;

impl PdfTest for TextOracleTest {
    fn name(&self) -> &str {
        "text_oracle"
    }

    fn run(&self, pdf_data: &[u8], path: &Path) -> TestResult {
        let start = std::time::Instant::now();

        if !PopplerOracle::is_available() {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some("pdftotext not available".to_string()),
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        // 1. Extract with our engine
        let doc = match pdf_engine::PdfDocument::open(pdf_data.to_vec()) {
            Ok(d) => d,
            Err(e) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some(format!("Our engine failed: {e}")),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        let pages_to_extract = doc.page_count().min(5);
        let mut our_text = String::new();
        for i in 0..pages_to_extract {
            match doc.extract_text(i) {
                Ok(text) => our_text.push_str(&text),
                Err(e) => {
                    return TestResult {
                        status: TestStatus::Fail,
                        error_message: Some(format!("Our extraction page {i} failed: {e}")),
                        duration_ms: start.elapsed().as_millis() as u64,
                        oracle_score: None,
                        metadata: HashMap::new(),
                    };
                }
            }
        }

        // 2. Extract with Poppler
        let poppler_text = match PopplerOracle::extract_all_text(path) {
            Ok(t) => t,
            Err(e) => {
                // Poppler can't handle this PDF either — skip oracle comparison
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some(format!("pdftotext failed: {e}")),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        // 3. Normalize both
        let our_normalized = poppler::normalize_text(&our_text);
        let poppler_normalized = poppler::normalize_text(&poppler_text);

        // Both empty → scanned PDF, skip comparison
        if our_normalized.is_empty() && poppler_normalized.is_empty() {
            let mut metadata = HashMap::new();
            metadata.insert("skip_reason".to_string(), "both_empty".to_string());
            return TestResult {
                status: TestStatus::Skip,
                error_message: None,
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: Some(1.0),
                metadata,
            };
        }

        // 4. Calculate similarity
        let similarity = poppler::text_similarity(&our_normalized, &poppler_normalized);

        // 5. Save diff if similarity is low
        if similarity < 0.95 {
            let diff_dir = path.parent().unwrap_or(Path::new(".")).join("diffs");
            let _ = poppler::save_text_diff(
                path,
                &our_normalized,
                &poppler_normalized,
                similarity,
                &diff_dir,
            );
        }

        let status = if similarity >= 0.95 {
            TestStatus::Pass
        } else {
            TestStatus::Fail
        };

        let mut metadata = HashMap::new();
        metadata.insert("similarity".to_string(), format!("{similarity:.4}"));
        metadata.insert("our_chars".to_string(), our_normalized.len().to_string());
        metadata.insert(
            "poppler_chars".to_string(),
            poppler_normalized.len().to_string(),
        );
        metadata.insert("pages_compared".to_string(), pages_to_extract.to_string());

        TestResult {
            status,
            error_message: if similarity < 0.95 {
                Some(format!("Text similarity {similarity:.4} < 0.95"))
            } else {
                None
            },
            duration_ms: start.elapsed().as_millis() as u64,
            oracle_score: Some(similarity),
            metadata,
        }
    }
}
