use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

pub struct ParseTest;

impl PdfTest for ParseTest {
    fn name(&self) -> &str {
        "parse"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();

        match pdf_syntax::Pdf::new(pdf_data.to_vec()) {
            Ok(pdf) => {
                let page_count = pdf.pages().len();
                let mut metadata = HashMap::new();
                metadata.insert("page_count".to_string(), page_count.to_string());
                TestResult {
                    status: TestStatus::Pass,
                    error_message: None,
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata,
                }
            }
            // A fundamentally unreadable PDF (corrupt, truncated, not a PDF) cannot be
            // tested — return Skip so it does not show as a regression. (#467)
            Err(pdf_syntax::LoadPdfError::Invalid) => TestResult {
                status: TestStatus::Skip,
                error_message: Some("PDF is invalid or could not be parsed".to_string()),
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            },
            // Encrypted PDFs without a known password cannot be tested — Skip.
            Err(pdf_syntax::LoadPdfError::Decryption(_)) => TestResult {
                status: TestStatus::Skip,
                error_message: Some("PDF is encrypted (no password available)".to_string()),
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            },
            // PDFs exceeding object/page limits are rejected to prevent OOM. (#497)
            Err(pdf_syntax::LoadPdfError::TooLarge(obj, pg)) => TestResult {
                status: TestStatus::Skip,
                error_message: Some(format!(
                    "PDF too large ({obj} objects, {pg} pages) — skipping"
                )),
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            },
        }
    }
}
