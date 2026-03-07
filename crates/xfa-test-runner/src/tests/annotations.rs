use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

pub struct AnnotationsTest;

impl PdfTest for AnnotationsTest {
    fn name(&self) -> &str {
        "annotations"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();

        let pdf = match pdf_syntax::Pdf::new(pdf_data.to_vec()) {
            Ok(p) => p,
            Err(e) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some(format!("{e:?}")),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        let pages = pdf.pages();
        let mut total_annots = 0usize;
        let mut has_annots = false;

        for page in pages.iter() {
            let annots = pdf_annot::Annotation::from_page(page);
            if !annots.is_empty() {
                has_annots = true;
            }
            for annot in &annots {
                let _atype = annot.annotation_type();
                total_annots += 1;
            }
        }

        if !has_annots {
            return TestResult {
                status: TestStatus::Skip,
                error_message: None,
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        let mut metadata = HashMap::new();
        metadata.insert("annotation_count".to_string(), total_annots.to_string());

        TestResult {
            status: TestStatus::Pass,
            error_message: None,
            duration_ms: start.elapsed().as_millis() as u64,
            oracle_score: None,
            metadata,
        }
    }
}
