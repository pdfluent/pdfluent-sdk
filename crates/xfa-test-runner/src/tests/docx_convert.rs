//! PDF → DOCX conversion corpus test.
//!
//! Calls `pdf_docx::convert_pdf_bytes_to_docx` on every corpus PDF and verifies
//! that the output is a structurally valid DOCX (ZIP with `word/document.xml`).
//!
//! Skip policy:
//! - `DocxError::Pdf` (lopdf load failure) → Skip (corrupt / encrypted PDF)
//!
//! Fail conditions:
//! - Any other conversion error
//! - Output is not a ZIP (missing PK magic bytes)
//! - `word/document.xml` not found inside the ZIP

use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

pub struct DocxConvertTest;

impl PdfTest for DocxConvertTest {
    fn name(&self) -> &str {
        "docx_convert"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        // Thread wrapper guards against lopdf hangs on corrupt page trees / streams.
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

    match pdf_docx::convert_pdf_bytes_to_docx(&pdf) {
        // lopdf could not parse the PDF — not a DOCX conversion failure.
        Err(pdf_docx::DocxError::Pdf(_)) => TestResult {
            status: TestStatus::Skip,
            error_message: Some("lopdf could not load PDF".into()),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        },
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

            // Verify word/document.xml is present.
            let has_document_xml = {
                let cursor = std::io::Cursor::new(&docx_bytes);
                zip::ZipArchive::new(cursor)
                    .map(|mut a| a.by_name("word/document.xml").is_ok())
                    .unwrap_or(false)
            };

            if !has_document_xml {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some("DOCX output is missing word/document.xml".into()),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }

            let mut metadata = HashMap::new();
            metadata.insert("docx_size_bytes".to_string(), docx_bytes.len().to_string());

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
