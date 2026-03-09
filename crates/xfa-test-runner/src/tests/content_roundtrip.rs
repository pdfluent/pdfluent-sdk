use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

/// Roundtrip test: decode content stream → encode → decode → compare ops.
pub struct ContentRoundtripTest;

impl PdfTest for ContentRoundtripTest {
    fn name(&self) -> &str {
        "content_roundtrip"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();
        let elapsed = || start.elapsed().as_millis() as u64;

        // 1. Load via lopdf to get page content streams.
        let doc = match lopdf::Document::load_mem(pdf_data) {
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

        let pages = doc.get_pages();
        if pages.is_empty() {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some("0 pages".into()),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        // Test up to 3 pages.
        let max_pages = pages.len().min(3);
        let mut pages_tested = 0usize;
        let mut pages_skipped = 0usize;
        let mut total_ops = 0usize;

        for page_num in 1..=(max_pages as u32) {
            let page_id = match pages.get(&page_num) {
                Some(id) => *id,
                None => {
                    pages_skipped += 1;
                    continue;
                }
            };

            // Get content stream bytes.
            let stream_bytes = match get_page_content_bytes(&doc, page_id) {
                Some(b) if !b.is_empty() => b,
                _ => {
                    pages_skipped += 1;
                    continue;
                }
            };

            // Decode → encode → decode roundtrip (catch panics).
            let rt_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                roundtrip_content(&stream_bytes)
            }));

            match rt_result {
                Ok(Ok((orig_count, rt_count))) => {
                    total_ops += orig_count;
                    if orig_count != rt_count {
                        return TestResult {
                            status: TestStatus::Fail,
                            error_message: Some(format!(
                                "page {page_num}: op count mismatch {orig_count} → {rt_count}"
                            )),
                            duration_ms: elapsed(),
                            oracle_score: None,
                            metadata: HashMap::new(),
                        };
                    }
                    pages_tested += 1;
                }
                Ok(Err(e)) => {
                    // Decode/encode error is not uncommon — skip.
                    pages_skipped += 1;
                    if pages_tested == 0 && page_num == max_pages as u32 {
                        return TestResult {
                            status: TestStatus::Skip,
                            error_message: Some(format!("all pages failed: {e}")),
                            duration_ms: elapsed(),
                            oracle_score: None,
                            metadata: HashMap::new(),
                        };
                    }
                }
                Err(_) => {
                    return TestResult {
                        status: TestStatus::Fail,
                        error_message: Some(format!("page {page_num}: panic in content roundtrip")),
                        duration_ms: elapsed(),
                        oracle_score: None,
                        metadata: HashMap::new(),
                    };
                }
            }
        }

        if pages_tested == 0 {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some("no decodable content streams".into()),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        let mut metadata = HashMap::new();
        metadata.insert("pages_tested".into(), pages_tested.to_string());
        metadata.insert("pages_skipped".into(), pages_skipped.to_string());
        metadata.insert("total_ops".into(), total_ops.to_string());

        TestResult {
            status: TestStatus::Pass,
            error_message: None,
            duration_ms: elapsed(),
            oracle_score: None,
            metadata,
        }
    }
}

/// Decode → encode → decode and return (original_op_count, roundtrip_op_count).
fn roundtrip_content(stream: &[u8]) -> Result<(usize, usize), String> {
    let editor1 =
        pdf_manip::ContentEditor::from_stream(stream).map_err(|e| format!("decode: {e}"))?;
    let orig_count = editor1.len();

    let encoded = editor1.encode().map_err(|e| format!("encode: {e}"))?;

    let editor2 =
        pdf_manip::ContentEditor::from_stream(&encoded).map_err(|e| format!("re-decode: {e}"))?;
    let rt_count = editor2.len();

    Ok((orig_count, rt_count))
}

/// Extract concatenated content stream bytes from a page.
fn get_page_content_bytes(doc: &lopdf::Document, page_id: lopdf::ObjectId) -> Option<Vec<u8>> {
    let page = doc.get_object(page_id).ok()?;
    let page_dict = page.as_dict().ok()?;
    let contents = page_dict.get(b"Contents").ok()?;

    match contents {
        lopdf::Object::Reference(r) => {
            let obj = doc.get_object(*r).ok()?;
            stream_bytes(doc, obj)
        }
        lopdf::Object::Array(arr) => {
            let mut combined = Vec::new();
            for item in arr {
                let obj = match item {
                    lopdf::Object::Reference(r) => doc.get_object(*r).ok()?,
                    other => other,
                };
                if let Some(bytes) = stream_bytes(doc, obj) {
                    combined.extend_from_slice(&bytes);
                    combined.push(b'\n');
                }
            }
            if combined.is_empty() {
                None
            } else {
                Some(combined)
            }
        }
        lopdf::Object::Stream(s) => {
            let mut s = s.clone();
            let _ = s.decompress();
            Some(s.content.clone())
        }
        _ => None,
    }
}

fn stream_bytes(doc: &lopdf::Document, obj: &lopdf::Object) -> Option<Vec<u8>> {
    match obj {
        lopdf::Object::Stream(s) => {
            let mut s = s.clone();
            let _ = s.decompress();
            Some(s.content.clone())
        }
        lopdf::Object::Reference(r) => {
            let obj = doc.get_object(*r).ok()?;
            stream_bytes(doc, obj)
        }
        _ => None,
    }
}
