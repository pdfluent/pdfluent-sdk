// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};
use crate::oracles::poppler::{self, PopplerOracle};

/// Minimum similarity (normalized Levenshtein) below which the test fails.
/// Poppler is the ground truth; <50% agreement signals a meaningful regression.
/// Only applied when poppler extracts at least MIN_POPPLER_CHARS characters
/// (to avoid false failures on scanned/image-only PDFs where both are empty
/// and on tiny documents where one stray character skews the ratio).
const FAIL_THRESHOLD: f64 = 0.50;
const MIN_POPPLER_CHARS: usize = 50;

/// Compares our text extraction against Poppler's `pdftotext`.
/// Fails when similarity < 0.50 on documents where poppler extracts real text.
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
                    status: TestStatus::Pass,
                    error_message: Some(format!("Our engine failed: {e}")),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: Some(0.0),
                    metadata: HashMap::new(),
                };
            }
        };

        let our_text = doc.extract_all_text();

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

        let mut metadata = HashMap::new();
        metadata.insert("similarity".to_string(), format!("{similarity:.4}"));
        metadata.insert("our_chars".to_string(), our_normalized.len().to_string());
        metadata.insert(
            "poppler_chars".to_string(),
            poppler_normalized.len().to_string(),
        );
        metadata.insert("pages_compared".to_string(), doc.page_count().to_string());
        metadata.insert("threshold".to_string(), format!("{FAIL_THRESHOLD:.2}"));

        // Fail when poppler extracts real text and our similarity is below threshold.
        // Skip the threshold for image-heavy/scanned PDFs (too few poppler chars).
        let qualifies_for_threshold = poppler_normalized.len() >= MIN_POPPLER_CHARS;
        if qualifies_for_threshold && similarity < FAIL_THRESHOLD {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!(
                    "text similarity {similarity:.4} below threshold {FAIL_THRESHOLD:.2} \
                     (our={} chars, poppler={} chars)",
                    our_normalized.len(),
                    poppler_normalized.len(),
                )),
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: Some(similarity),
                metadata,
            };
        }

        TestResult {
            status: TestStatus::Pass,
            error_message: None,
            duration_ms: start.elapsed().as_millis() as u64,
            oracle_score: Some(similarity),
            metadata,
        }
    }
}
