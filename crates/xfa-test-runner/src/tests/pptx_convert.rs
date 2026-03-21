//! PDF → PPTX conversion corpus test.
//!
//! Calls `pdf_pptx::convert_pdf_bytes_to_pptx` on every corpus PDF and verifies
//! that the output is a structurally valid PPTX (ZIP with `ppt/presentation.xml`).
//!
//! Skip policy:
//! - `PptxError::Pdf` (lopdf load failure) → Skip (corrupt / encrypted PDF)
//!
//! Fail conditions:
//! - Any other conversion error
//! - Output is not a ZIP (missing PK magic bytes)
//! - `ppt/presentation.xml` not found inside the ZIP

use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

pub struct PptxConvertTest;

impl PdfTest for PptxConvertTest {
    fn name(&self) -> &str {
        "pptx_convert"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
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
                error_message: Some("panic in PPTX conversion".into()),
                duration_ms: 0,
                oracle_score: None,
                metadata: HashMap::new(),
            },
            Err(_) => TestResult {
                status: TestStatus::Timeout,
                error_message: Some("PPTX conversion timed out (>25s)".into()),
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

    match pdf_pptx::convert_pdf_bytes_to_pptx(&pdf) {
        Err(pdf_pptx::PptxError::Pdf(_)) => TestResult {
            status: TestStatus::Skip,
            error_message: Some("lopdf could not load PDF".into()),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        },
        Err(e) => TestResult {
            status: TestStatus::Fail,
            error_message: Some(format!("PPTX conversion failed: {e}")),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        },
        Ok(pptx_bytes) => {
            let pptx_bytes: Vec<u8> = pptx_bytes;
            if pptx_bytes.len() < 4 || &pptx_bytes[..2] != b"PK" {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some(
                        "PPTX output is not a valid ZIP (missing PK header)".into(),
                    ),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }

            let has_presentation_xml = {
                let cursor = std::io::Cursor::new(&pptx_bytes);
                zip::ZipArchive::new(cursor)
                    .map(|mut a: zip::ZipArchive<_>| a.by_name("ppt/presentation.xml").is_ok())
                    .unwrap_or(false)
            };

            if !has_presentation_xml {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some("PPTX output is missing ppt/presentation.xml".into()),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }

            let mut metadata = HashMap::new();
            metadata.insert("pptx_size_bytes".to_string(), pptx_bytes.len().to_string());

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
