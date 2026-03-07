use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

pub struct GeometryTest;

impl PdfTest for GeometryTest {
    fn name(&self) -> &str {
        "geometry"
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

        for i in 0..page_count {
            match doc.page_geometry(i) {
                Ok(geom) => {
                    let w = geom.media_box.width();
                    let h = geom.media_box.height();

                    if w <= 0.0
                        || h <= 0.0
                        || w.is_nan()
                        || h.is_nan()
                        || w.is_infinite()
                        || h.is_infinite()
                    {
                        return TestResult {
                            status: TestStatus::Fail,
                            error_message: Some(format!(
                                "Page {i} has invalid MediaBox dimensions: {w}x{h}"
                            )),
                            duration_ms: start.elapsed().as_millis() as u64,
                            oracle_score: None,
                            metadata: HashMap::new(),
                        };
                    }
                }
                Err(e) => {
                    return TestResult {
                        status: TestStatus::Fail,
                        error_message: Some(format!("Page geometry {i} failed: {e}")),
                        duration_ms: start.elapsed().as_millis() as u64,
                        oracle_score: None,
                        metadata: HashMap::new(),
                    };
                }
            }
        }

        let mut metadata = HashMap::new();
        metadata.insert("pages_checked".to_string(), page_count.to_string());

        TestResult {
            status: TestStatus::Pass,
            error_message: None,
            duration_ms: start.elapsed().as_millis() as u64,
            oracle_score: None,
            metadata,
        }
    }
}
