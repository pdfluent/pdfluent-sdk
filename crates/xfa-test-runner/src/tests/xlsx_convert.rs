//! PDF → XLSX table conversion corpus test.
//!
//! Detects tables in each corpus PDF using `pdf_xlsx::extract_tables`, then
//! converts them to XLSX and verifies that the output is a valid OOXML
//! spreadsheet (ZIP containing at least one `xl/worksheets/sheet*.xml` entry).
//!
//! Skip policy:
//! - lopdf cannot load the PDF → Skip
//! - No tables detected → Skip (not a tabular document)
//!
//! Fail conditions:
//! - `pdf_xlsx::pdf_to_xlsx` returns an error despite tables being found
//! - Output is not a ZIP
//! - No `xl/worksheets/` entry in the ZIP

use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

pub struct XlsxConvertTest;

impl PdfTest for XlsxConvertTest {
    fn name(&self) -> &str {
        "xlsx_convert"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        // Skip large PDFs to avoid OOM under concurrent load.
        // Each worker thread uses ~16 MB stack; with multiple concurrent workers
        // very large PDFs cause memory exhaustion and panics.
        const MAX_PDF_BYTES: usize = 5 * 1024 * 1024; // 5 MB
        if pdf_data.len() > MAX_PDF_BYTES {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some(format!(
                    "PDF too large for xlsx test ({} bytes > {})",
                    pdf_data.len(),
                    MAX_PDF_BYTES
                )),
                duration_ms: 0,
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        let pdf_owned = pdf_data.to_vec();
        let (tx, rx) = std::sync::mpsc::channel();
        // Thread spawn can fail with EAGAIN under concurrent load (OS thread limit).
        // Return Skip rather than panicking — this is a transient resource constraint,
        // not a bug in the PDF or our code.
        if std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024) // 16 MB — sufficient; 64 MB was causing OOM under concurrent load
            .spawn(move || {
                let r =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_inner(pdf_owned)));
                let _ = tx.send(r);
            })
            .is_err()
        {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some("thread spawn failed (resource temporarily unavailable)".into()),
                duration_ms: 0,
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
        match rx.recv_timeout(std::time::Duration::from_secs(25)) {
            Ok(Ok(result)) => result,
            Ok(Err(e)) => {
                let panic_msg = e
                    .downcast_ref::<String>()
                    .map(|s| s.as_str())
                    .or_else(|| e.downcast_ref::<&str>().copied())
                    .unwrap_or("unknown panic");
                TestResult {
                    status: TestStatus::Crash,
                    error_message: Some(format!("panic in XLSX conversion: {panic_msg}")),
                    duration_ms: 0,
                    oracle_score: None,
                    metadata: HashMap::new(),
                }
            }
            Err(_) => TestResult {
                status: TestStatus::Timeout,
                error_message: Some("XLSX conversion timed out (>25s)".into()),
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

    let doc = match lopdf::Document::load_mem(&pdf) {
        Ok(d) => d,
        Err(e) => {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some(format!("lopdf load failed: {e}")),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            }
        }
    };

    // Detect tables: skip if none found (not a tabular document).
    let tables = pdf_xlsx::extract_tables(&doc);
    if tables.is_empty() {
        return TestResult {
            status: TestStatus::Skip,
            error_message: None,
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    let table_count = tables.len();
    let total_rows: usize = tables.iter().map(|t| t.rows.len()).sum();

    // Skip documents with extreme row counts to bound memory and XLSX size.
    const MAX_TOTAL_ROWS: usize = 8_000;
    if total_rows > MAX_TOTAL_ROWS {
        return TestResult {
            status: TestStatus::Skip,
            error_message: Some(format!(
                "too many rows for xlsx test ({total_rows} > {MAX_TOTAL_ROWS})"
            )),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    match pdf_xlsx::pdf_to_xlsx(&doc) {
        Err(e) => TestResult {
            status: TestStatus::Fail,
            error_message: Some(format!("XLSX conversion failed: {e}")),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        },
        Ok(xlsx_bytes) => {
            // Verify ZIP magic bytes.
            if xlsx_bytes.len() < 4 || &xlsx_bytes[..2] != b"PK" {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some(
                        "XLSX output is not a valid ZIP (missing PK header)".into(),
                    ),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }

            // Verify at least one worksheet entry exists.
            let has_worksheet = {
                let cursor = std::io::Cursor::new(&xlsx_bytes);
                match zip::ZipArchive::new(cursor) {
                    Ok(archive) => {
                        // Collect names first to avoid borrow conflicts with `any`.
                        let names: Vec<String> = (0..archive.len())
                            .filter_map(|i| archive.name_for_index(i).map(str::to_string))
                            .collect();
                        names.iter().any(|n| n.starts_with("xl/worksheets/"))
                    }
                    Err(_) => false,
                }
            };

            if !has_worksheet {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some("XLSX output has no xl/worksheets/ entry".into()),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }

            let mut metadata = HashMap::new();
            metadata.insert("table_count".to_string(), table_count.to_string());
            metadata.insert("total_rows".to_string(), total_rows.to_string());
            metadata.insert("xlsx_size_bytes".to_string(), xlsx_bytes.len().to_string());

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
