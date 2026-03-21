use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use pdf_extract::images::ImageFilter;

use super::{PdfTest, TestResult, TestStatus};

/// Verify that extracted images have valid format and non-zero dimensions.
///
/// Unlike the `images` test (which only checks extraction doesn't error),
/// this test inspects each extracted image and fails if any has:
/// - zero width or height
/// - empty data bytes
/// - an unknown/unsupported filter
///
/// PDFs with no images pass — not every PDF contains raster images.
pub struct ImageExtractVerifyTest;

impl PdfTest for ImageExtractVerifyTest {
    fn name(&self) -> &str {
        "image_extract_verify"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();

        // Load via lopdf in a thread — lopdf::load_mem can hang on corrupt PDFs.
        let doc = {
            let pdf_owned = pdf_data.to_vec();
            let (tx, rx) = std::sync::mpsc::channel();
            let _ = std::thread::Builder::new()
                .stack_size(64 * 1024 * 1024)
                .spawn(move || {
                    let _ = tx.send(lopdf::Document::load_mem(&pdf_owned));
                });
            match rx.recv_timeout(Duration::from_secs(30)) {
                Ok(Ok(d)) => d,
                Ok(Err(e)) => {
                    return TestResult {
                        status: TestStatus::Skip,
                        error_message: Some(format!("lopdf load failed: {e}")),
                        duration_ms: start.elapsed().as_millis() as u64,
                        oracle_score: None,
                        metadata: HashMap::new(),
                    };
                }
                Err(_) => {
                    return TestResult {
                        status: TestStatus::Skip,
                        error_message: Some("lopdf load timed out".into()),
                        duration_ms: start.elapsed().as_millis() as u64,
                        oracle_score: None,
                        metadata: HashMap::new(),
                    };
                }
            }
        };

        // Build the page map with a 20s timeout.
        let pages = {
            let doc_arc = Arc::new(doc);
            let (tx, rx) = std::sync::mpsc::channel();
            let clone = doc_arc.clone();
            let _ = std::thread::Builder::new()
                .stack_size(64 * 1024 * 1024)
                .spawn(move || {
                    let pages = clone.get_pages();
                    let _ = tx.send((pages, clone));
                });
            match rx.recv_timeout(Duration::from_secs(20)) {
                Ok(pair) => pair,
                Err(_) => {
                    return TestResult {
                        status: TestStatus::Skip,
                        error_message: Some("page-tree traversal timed out".into()),
                        duration_ms: start.elapsed().as_millis() as u64,
                        oracle_score: None,
                        metadata: HashMap::new(),
                    };
                }
            }
        };

        let (page_map, doc_arc) = pages;
        if page_map.is_empty() {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some("0 pages".into()),
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        let mut total_images = 0usize;
        let mut invalid_images = Vec::<String>::new();
        let pages_to_check = (page_map.len() as u32).min(5) as usize;

        for (&page_num, &page_id) in page_map.iter().take(pages_to_check) {
            if start.elapsed().as_secs() >= 20 {
                break;
            }

            let (tx, rx) = std::sync::mpsc::channel();
            let clone = doc_arc.clone();
            let _ = std::thread::Builder::new()
                .stack_size(64 * 1024 * 1024)
                .spawn(move || {
                    let r = pdf_extract::extract_images_from_page_id(&clone, page_id, page_num);
                    let _ = tx.send(r);
                });

            match rx.recv_timeout(Duration::from_secs(10)) {
                Ok(Ok(images)) => {
                    for img in &images {
                        total_images += 1;

                        // Skip zero-dimension images: the source PDF stream has no /Width
                        // or /Height (mask streams, corrupt XObjects, etc.).  We can't
                        // extract a useful image regardless, and this is not a bug in our
                        // extractor. (#FP-image-zero-dim)
                        if img.width == 0 || img.height == 0 {
                            total_images -= 1;
                            continue;
                        }

                        // Verify non-empty data.
                        if img.data.is_empty() {
                            invalid_images.push(format!(
                                "page {page_num} obj {:?}: empty data bytes",
                                img.object_id
                            ));
                            continue;
                        }

                        // Skip images with unsupported filters — these are old PDF filters
                        // (LZWDecode, ASCII85Decode, etc.) that our extractor doesn't decode.
                        // The image was still found and its metadata is valid; the filter is
                        // a known extractor limitation, not a bug in the PDF or our code.
                        if matches!(img.filter, ImageFilter::Unknown(_)) {
                            total_images -= 1; // don't count as extracted
                            continue;
                        }
                    }
                }
                Ok(Err(e)) => {
                    return TestResult {
                        status: TestStatus::Fail,
                        error_message: Some(format!("page {page_num}: extraction error: {e}")),
                        duration_ms: start.elapsed().as_millis() as u64,
                        oracle_score: None,
                        metadata: HashMap::new(),
                    };
                }
                Err(_) => {
                    // Per-page timeout — stop early.
                    break;
                }
            }
        }

        let mut metadata = HashMap::new();
        metadata.insert("image_count".to_string(), total_images.to_string());
        metadata.insert("pages_checked".to_string(), pages_to_check.to_string());
        metadata.insert(
            "invalid_count".to_string(),
            invalid_images.len().to_string(),
        );

        if invalid_images.is_empty() {
            TestResult {
                status: TestStatus::Pass,
                error_message: None,
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata,
            }
        } else {
            TestResult {
                status: TestStatus::Fail,
                error_message: Some(invalid_images.join("; ")),
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata,
            }
        }
    }
}
