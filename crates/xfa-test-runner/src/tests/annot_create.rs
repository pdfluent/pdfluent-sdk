use std::collections::HashMap;
use std::path::Path;

use pdf_annot::builder::{add_annotation_to_page, AnnotRect, AnnotationBuilder};
use pdf_annot::error::AnnotBuildError;

use super::{PdfTest, TestResult, TestStatus};

/// Roundtrip test: add a highlight annotation on page 1, save, reopen, verify.
pub struct AnnotCreateTest;

impl PdfTest for AnnotCreateTest {
    fn name(&self) -> &str {
        "annot_create"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        // Wrap entire execution in a thread to guard against lopdf hangs on
        // corrupt PDFs (page-tree loops, infinite decompression, etc.). #452
        let pdf_owned = pdf_data.to_vec();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_inner(pdf_owned)));
            let _ = tx.send(r);
        });
        match rx.recv_timeout(std::time::Duration::from_secs(30)) {
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
                error_message: Some("test timed out (>30s)".into()),
                duration_ms: 30_000,
                oracle_score: None,
                metadata: HashMap::new(),
            },
        }
    }
}

fn run_inner(pdf: Vec<u8>) -> TestResult {
    let start = std::time::Instant::now();
    let elapsed = || start.elapsed().as_millis() as u64;

    // 1. Load via lopdf.
    let mut doc = match lopdf::Document::load_mem(&pdf) {
        Ok(d) => d,
        Err(e) => {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some(format!("lopdf load failed: {e}")),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
    };

    let page_count = doc.get_pages().len();
    if page_count == 0 {
        return TestResult {
            status: TestStatus::Skip,
            error_message: Some("0 pages".into()),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    // Count existing annotations on page 1 before mutation.
    let annots_before = count_annotations_lopdf(&doc, 1);

    // 2. Add a highlight annotation on page 1.
    let build_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let rect = AnnotRect {
            x0: 72.0,
            y0: 700.0,
            x1: 200.0,
            y1: 720.0,
        };
        let annot_id = AnnotationBuilder::highlight(rect)
            .color(1.0, 1.0, 0.0)
            .opacity(0.5)
            .contents("test annotation")
            .quad_points_from_rect(&rect)
            .build(&mut doc)?;
        add_annotation_to_page(&mut doc, 1, annot_id)?;
        Ok::<_, AnnotBuildError>(())
    }));

    match build_result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("annotation build failed: {e}")),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
        Err(_) => {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some("panic building annotation".into()),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
    }

    // 3. Save to bytes.
    let mut saved = Vec::new();
    if let Err(e) = doc.save_to(&mut saved) {
        return TestResult {
            status: TestStatus::Fail,
            error_message: Some(format!("save failed: {e}")),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    // 4. Reopen with pdf-syntax and verify annotation exists.
    let pdf2 = match pdf_syntax::Pdf::new(saved) {
        Ok(p) => p,
        Err(e) => {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("reopen failed: {e:?}")),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
    };

    let pages = pdf2.pages();
    if pages.is_empty() {
        return TestResult {
            status: TestStatus::Fail,
            error_message: Some("no pages after reopen".into()),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    let annots = pdf_annot::Annotation::from_page(&pages[0]);
    let annots_after = annots.len();

    let mut metadata = HashMap::new();
    metadata.insert("annots_before".into(), annots_before.to_string());
    metadata.insert("annots_after".into(), annots_after.to_string());

    if annots_after > annots_before {
        // Check that at least one is a Highlight.
        let has_highlight = annots
            .iter()
            .any(|a| matches!(a.annotation_type(), pdf_annot::AnnotationType::Highlight));
        if has_highlight {
            TestResult {
                status: TestStatus::Pass,
                error_message: None,
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
            }
        } else {
            TestResult {
                status: TestStatus::Fail,
                error_message: Some("highlight annotation not found after roundtrip".into()),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
            }
        }
    } else {
        TestResult {
            status: TestStatus::Fail,
            error_message: Some(format!(
                "annotation count did not increase: {annots_before} → {annots_after}"
            )),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata,
        }
    }
}

/// Count annotations on a page via lopdf (fallible, returns 0 on error).
fn count_annotations_lopdf(doc: &lopdf::Document, page_num: u32) -> usize {
    let pages = doc.get_pages();
    let page_id = match pages.get(&page_num) {
        Some(id) => *id,
        None => return 0,
    };
    let page = match doc.get_object(page_id) {
        Ok(lopdf::Object::Dictionary(d)) => d,
        _ => return 0,
    };
    match page.get(b"Annots") {
        Ok(lopdf::Object::Array(arr)) => arr.len(),
        Ok(lopdf::Object::Reference(r)) => match doc.get_object(*r) {
            Ok(lopdf::Object::Array(arr)) => arr.len(),
            _ => 0,
        },
        _ => 0,
    }
}
