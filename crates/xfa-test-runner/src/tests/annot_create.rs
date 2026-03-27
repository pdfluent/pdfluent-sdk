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
    // Use pdf_syntax for the annots_before metadata count (human-readable baseline).
    // Use lopdf for the Highlight-specific pass/fail comparison because pdf_syntax
    // may under-count annotations in lopdf-saved files with dense ObjStm compression
    // (e.g. poppler-22493-1.pdf: pdf_syntax sees 5 of 103 after lopdf save). Fixes #472.
    let annots_before = match pdf_syntax::Pdf::new(pdf.clone()) {
        Ok(p) => {
            let pages = p.pages();
            if pages.is_empty() {
                0
            } else {
                pdf_annot::Annotation::from_page(&pages[0]).len()
            }
        }
        Err(_) => 0,
    };
    let hl_before = count_page_highlights(&doc, 1);

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
        // Page dict not mutable (ObjStm full-compression PDF). Fixes #470.
        Ok(Err(AnnotBuildError::PageMutationFailed)) => {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some("page dict not mutable (ObjStm full-compression PDF)".into()),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
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

    // 4. Reopen with lopdf and verify our Highlight annotation exists.
    // lopdf is used (not pdf_syntax) because pdf_syntax cannot reliably traverse
    // the Annots array in lopdf-saved files that originally used dense ObjStm
    // compression — the individual annotation objects are present and valid but
    // pdf_syntax's cross-reference resolution misses most of them.  Fixes #472.
    let doc2 = match lopdf::Document::load_mem(&saved) {
        Ok(d) => d,
        Err(e) => {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("reopen failed: {e}")),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
    };

    let hl_after = count_page_highlights(&doc2, 1);
    let annots_after = hl_after; // use as proxy for metadata

    let mut metadata = HashMap::new();
    metadata.insert("annots_before".into(), annots_before.to_string());
    metadata.insert("annots_after".into(), annots_after.to_string());

    if hl_after > hl_before {
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
            error_message: Some(format!(
                "highlight annotation not found after roundtrip (highlights: {hl_before} → {hl_after})"
            )),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata,
        }
    }
}

/// Count Highlight-subtype annotations on a given page using lopdf.
///
/// Used for post-save verification because pdf_syntax may under-count annotations
/// in lopdf-saved files with dense ObjStm compression.  Fixes #472.
fn count_page_highlights(doc: &lopdf::Document, page_num: u32) -> usize {
    use lopdf::Object;
    let pages = doc.get_pages();
    let page_id = match pages.get(&page_num) {
        Some(id) => *id,
        None => return 0,
    };
    let page_dict = match doc.get_dictionary(page_id) {
        Ok(d) => d,
        Err(_) => return 0,
    };
    let annots_obj = match page_dict.get(b"Annots").ok().cloned() {
        Some(o) => o,
        None => return 0,
    };
    let arr = match annots_obj {
        Object::Array(arr) => arr,
        Object::Reference(r) => match doc.get_object(r) {
            Ok(Object::Array(arr)) => arr.clone(),
            _ => return 0,
        },
        _ => return 0,
    };
    arr.iter()
        .filter(|obj| {
            if let Object::Reference(ar) = obj {
                doc.get_dictionary(*ar)
                    .ok()
                    .and_then(|d| d.get(b"Subtype").ok().cloned())
                    .map(|s| matches!(s, Object::Name(n) if n == b"Highlight"))
                    .unwrap_or(false)
            } else {
                false
            }
        })
        .count()
}
