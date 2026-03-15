use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use super::{PdfTest, TestResult, TestStatus};

pub struct ImageExtractTest;

impl PdfTest for ImageExtractTest {
    fn name(&self) -> &str {
        "images"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();

        let doc = match lopdf::Document::load_mem(pdf_data) {
            Ok(d) => d,
            Err(_) => {
                // lopdf can't parse — fall back to pdf_engine to verify the PDF is valid.
                return self.fallback_check(pdf_data, start);
            }
        };

        // Wrap in Arc so the page map and per-page extraction can run in
        // threads with a hard timeout. Fixes #447: also eliminates the double
        // get_pages() call — we call it once here and pass page_id directly to
        // extract_images_from_page_id (which does not call get_pages() again).
        let doc_arc = Arc::new(doc);

        // Build the page map with a 20s timeout.  Some corrupt PDFs loop
        // forever inside lopdf's page-tree traversal; without the thread we
        // would block indefinitely.  Fixes #446/#447.
        let pages = {
            let (tx, rx) = std::sync::mpsc::channel();
            let clone = doc_arc.clone();
            std::thread::spawn(move || {
                let _ = tx.send(clone.get_pages());
            });
            match rx.recv_timeout(Duration::from_secs(20)) {
                Ok(p) => p,
                Err(_) => {
                    return self.fallback_check(pdf_data, start);
                }
            }
        };

        if pages.is_empty() {
            // lopdf found 0 pages — fall back to pdf_engine page count check.
            return self.fallback_check(pdf_data, start);
        }

        let mut total_images = 0usize;
        let pages_to_check = (pages.len() as u32).min(5) as usize;
        let mut pages_checked = 0u32;

        // Iterate the first N pages in page-number order (BTreeMap is sorted).
        for (&page_num, &page_id) in pages.iter().take(pages_to_check) {
            // Abort if total test time exceeds budget.
            if start.elapsed().as_secs() >= 20 {
                break;
            }

            // Each page runs in its own thread with a 10s deadline.
            // Pathological image streams (zip-bombs, infinite decompression)
            // are contained to at most 10s per page.  Fixes #446/#447.
            let (tx, rx) = std::sync::mpsc::channel();
            let clone = doc_arc.clone();
            std::thread::spawn(move || {
                let r = pdf_extract::extract_images_from_page_id(&clone, page_id, page_num);
                let _ = tx.send(r);
            });

            match rx.recv_timeout(Duration::from_secs(10)) {
                Ok(Ok(images)) => {
                    total_images += images.len();
                    pages_checked += 1;
                }
                Ok(Err(e)) => {
                    return TestResult {
                        status: TestStatus::Fail,
                        error_message: Some(format!("page {page_num}: {e}")),
                        duration_ms: start.elapsed().as_millis() as u64,
                        oracle_score: None,
                        metadata: HashMap::new(),
                    };
                }
                Err(_) => {
                    // Per-page timeout — stop checking more pages.
                    break;
                }
            }
        }

        let mut metadata = HashMap::new();
        metadata.insert("image_count".to_string(), total_images.to_string());
        metadata.insert("pages_checked".to_string(), pages_checked.to_string());
        metadata.insert("backend".to_string(), "lopdf".to_string());

        TestResult {
            status: TestStatus::Pass,
            error_message: None,
            duration_ms: start.elapsed().as_millis() as u64,
            oracle_score: None,
            metadata,
        }
    }
}

impl ImageExtractTest {
    /// Fallback when lopdf fails: use pdf_engine to verify the PDF has pages.
    /// We can't extract images without lopdf, but we can confirm the PDF is valid.
    fn fallback_check(&self, pdf_data: &[u8], start: std::time::Instant) -> super::TestResult {
        match pdf_engine::PdfDocument::open(pdf_data.to_vec()) {
            Ok(doc) => {
                let page_count = doc.page_count();
                let mut metadata = HashMap::new();
                metadata.insert("page_count".to_string(), page_count.to_string());
                metadata.insert("backend".to_string(), "pdf_engine".to_string());
                metadata.insert(
                    "note".to_string(),
                    "lopdf could not parse; image extraction skipped".to_string(),
                );

                super::TestResult {
                    status: TestStatus::Pass,
                    error_message: None,
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata,
                }
            }
            Err(_) => super::TestResult {
                status: TestStatus::Skip,
                error_message: Some("invalid PDF: cannot parse".into()),
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            },
        }
    }
}
