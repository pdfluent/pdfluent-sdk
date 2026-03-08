use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

const MAX_PAGES_TO_RENDER: usize = 3;

/// Maximum time in seconds for a single page render before aborting the rest.
const PER_PAGE_BUDGET_SECS: u64 = 10;

/// Maximum total time in seconds for the entire render test.
const TOTAL_BUDGET_SECS: u64 = 20;

pub struct RenderTest;

impl PdfTest for RenderTest {
    fn name(&self) -> &str {
        "render"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();

        let doc = match pdf_engine::PdfDocument::open(pdf_data.to_vec()) {
            Ok(d) => d,
            Err(e) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some(format!("{e}")),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        let page_count = doc.page_count();
        let pages_to_render = page_count.min(MAX_PAGES_TO_RENDER);
        let options = pdf_engine::RenderOptions::default();
        let mut pages_rendered = 0usize;

        for i in 0..pages_to_render {
            // Abort if total test time budget exceeded.
            if start.elapsed().as_secs() >= TOTAL_BUDGET_SECS {
                break;
            }

            let page_start = std::time::Instant::now();
            match doc.render_page(i, &options) {
                Ok(rendered) => {
                    if rendered.pixels.is_empty() {
                        return TestResult {
                            status: TestStatus::Fail,
                            error_message: Some(format!("Page {i} rendered to empty pixel data")),
                            duration_ms: start.elapsed().as_millis() as u64,
                            oracle_score: None,
                            metadata: HashMap::new(),
                        };
                    }
                    pages_rendered += 1;

                    // If this page was slow, skip remaining pages.
                    if page_start.elapsed().as_secs() >= PER_PAGE_BUDGET_SECS {
                        break;
                    }
                }
                Err(e) => {
                    return TestResult {
                        status: TestStatus::Fail,
                        error_message: Some(format!("Render page {i} failed: {e}")),
                        duration_ms: start.elapsed().as_millis() as u64,
                        oracle_score: None,
                        metadata: HashMap::new(),
                    };
                }
            }
        }

        let mut metadata = HashMap::new();
        metadata.insert("pages_rendered".to_string(), pages_rendered.to_string());

        TestResult {
            status: TestStatus::Pass,
            error_message: None,
            duration_ms: start.elapsed().as_millis() as u64,
            oracle_score: None,
            metadata,
        }
    }
}
