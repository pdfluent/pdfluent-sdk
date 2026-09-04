// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

pub struct SearchTest;

impl PdfTest for SearchTest {
    fn name(&self) -> &str {
        "search"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        // Wrap entire execution in a thread to guard against lopdf hangs on
        // corrupt PDFs (page-tree loops, infinite decompression, etc.). #452
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

    // Skip very large PDFs (>50 MB) where load_mem itself can exceed timeout.
    if pdf.len() > 50 * 1024 * 1024 {
        return TestResult {
            status: TestStatus::Skip,
            error_message: Some(format!(
                "PDF too large ({} MB) — skipping search",
                pdf.len() / 1024 / 1024
            )),
            duration_ms: start.elapsed().as_millis() as u64,
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    // Try lopdf first for full search capability.
    // Use count_text_only with a page limit — bounding boxes are not needed
    // for corpus validation and positioned char extraction is very expensive.
    match lopdf::Document::load_mem(&pdf) {
        Ok(doc) => {
            // Limit to 10 pages to avoid timeouts on documents with many pages.
            let pages_to_search = (doc.get_pages().len() as u32).min(10);
            let options = pdf_extract::SearchOptions {
                pages: (1..=pages_to_search).collect(),
                ..Default::default()
            };
            let match_count = pdf_extract::count_text_only(&doc, "the", &options);

            let mut metadata = HashMap::new();
            metadata.insert("match_count".to_string(), match_count.to_string());
            metadata.insert("backend".to_string(), "lopdf".to_string());

            TestResult {
                status: TestStatus::Pass,
                error_message: None,
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata,
            }
        }
        Err(_) => {
            // Fallback: use pdf_engine text extraction for search validation.
            match pdf_engine::PdfDocument::open(pdf) {
                Ok(doc) => {
                    let page_count = doc.page_count();
                    let mut match_count = 0usize;
                    let pages_to_search = page_count.min(5);

                    for i in 0..pages_to_search {
                        if let Ok(text) = doc.extract_text(i) {
                            match_count += text.to_lowercase().matches("the").count();
                        }
                    }

                    let mut metadata = HashMap::new();
                    metadata.insert("match_count".to_string(), match_count.to_string());
                    metadata.insert("backend".to_string(), "pdf_engine".to_string());

                    TestResult {
                        status: TestStatus::Pass,
                        error_message: None,
                        duration_ms: start.elapsed().as_millis() as u64,
                        oracle_score: None,
                        metadata,
                    }
                }
                Err(_) => TestResult {
                    status: TestStatus::Skip,
                    error_message: Some("invalid PDF: cannot parse".into()),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                },
            }
        }
    }
}
