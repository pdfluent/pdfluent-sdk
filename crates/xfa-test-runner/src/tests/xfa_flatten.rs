//! XFA flatten test.
//!
//! For each XFA PDF, strips the AcroForm / XFA layer using lopdf (the same
//! approach as `xfa-cli`'s `flatten` command), then re-parses the result
//! with `pdf_syntax` and checks that the output is a structurally valid PDF
//! with at least one page.
//!
//! When the iText 5 oracle script is present at `/opt/itext/itext-xfa-oracle.sh`,
//! the test also compares our page count against iText's flatten output and
//! fails if they differ.
//!
//! Skip policy:
//! - No /XFA key in AcroForm → Skip (not an XFA form)
//! - lopdf cannot load the PDF  → Skip (corrupt; defer to parse test)
//!
//! Fail conditions:
//! - lopdf cannot re-serialise the mutated document
//! - pdf_syntax cannot re-parse the saved bytes
//! - Re-parsed PDF has zero pages
//! - Page count differs from iText oracle (when oracle is available)

use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};
use crate::oracles::itext::ITextOracle;

pub struct XfaFlattenTest;

impl PdfTest for XfaFlattenTest {
    fn name(&self) -> &str {
        "xfa_flatten"
    }

    fn run(&self, pdf_data: &[u8], path: &Path) -> TestResult {
        let start = std::time::Instant::now();

        let mut doc = match lopdf::Document::load_mem(pdf_data) {
            Ok(d) => d,
            Err(_) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some("lopdf could not load PDF".into()),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                }
            }
        };

        if !has_xfa(&doc) {
            return TestResult {
                status: TestStatus::Skip,
                error_message: None,
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        // Query the iText oracle with the original PDF before we mutate it.
        // Stored as Option<(itext_page_count, flatten_success)>.
        let itext_result = ITextOracle::new().and_then(|oracle| oracle.call(path));

        // Strip the AcroForm (which contains the /XFA key) from the catalog.
        // This is the minimum "flatten" step: the page content streams remain
        // untouched so the PDF stays renderable.
        remove_acroform(&mut doc);

        let mut buf = Vec::new();
        if let Err(e) = doc.save_to(&mut buf) {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("re-serialisation failed: {e}")),
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        match pdf_syntax::Pdf::new(buf) {
            Ok(reparsed) => {
                let pages = reparsed.pages().len();
                let mut metadata = HashMap::new();
                metadata.insert("page_count".to_string(), pages.to_string());

                // Structural check: at least one page.
                if pages == 0 {
                    return TestResult {
                        status: TestStatus::Fail,
                        error_message: Some("flattened PDF has no pages".into()),
                        duration_ms: start.elapsed().as_millis() as u64,
                        oracle_score: None,
                        metadata,
                    };
                }

                // iText oracle comparison: fail if page count differs.
                if let Some(ref r) = itext_result {
                    metadata.insert("itext_has_xfa".to_string(), r.has_xfa.to_string());
                    metadata.insert("itext_page_count".to_string(), r.page_count.to_string());
                    metadata.insert(
                        "itext_flatten_success".to_string(),
                        r.flatten_success.to_string(),
                    );
                    if !r.errors.is_empty() {
                        metadata.insert("itext_errors".to_string(), r.errors.join("; "));
                    }
                    if r.has_xfa
                        && r.flatten_success
                        && r.page_count > 0
                        && r.page_count as usize != pages
                    {
                        return TestResult {
                            status: TestStatus::Fail,
                            error_message: Some(format!(
                                "page count mismatch: ours={pages}, iText={}",
                                r.page_count
                            )),
                            duration_ms: start.elapsed().as_millis() as u64,
                            oracle_score: None,
                            metadata,
                        };
                    }
                }

                TestResult {
                    status: TestStatus::Pass,
                    error_message: None,
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata,
                }
            }
            Err(e) => TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("re-parse of flattened PDF failed: {e:?}")),
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            },
        }
    }
}

/// Returns `true` when the PDF catalog has an AcroForm that contains a /XFA key.
fn has_xfa(doc: &lopdf::Document) -> bool {
    let root_id = match doc.trailer.get(b"Root") {
        Ok(lopdf::Object::Reference(id)) => *id,
        _ => return false,
    };

    // Resolve the AcroForm entry — it may be an indirect reference or inline dict.
    let acro_obj = {
        let catalog = match doc.get_dictionary(root_id) {
            Ok(d) => d,
            Err(_) => return false,
        };
        match catalog.get(b"AcroForm") {
            Ok(lopdf::Object::Reference(r)) => lopdf::Object::Reference(*r),
            Ok(lopdf::Object::Dictionary(d)) => {
                // Inline dict: check directly without a second lookup.
                return d.get(b"XFA").is_ok();
            }
            _ => return false,
        }
    };

    match acro_obj {
        lopdf::Object::Reference(r) => doc
            .get_dictionary(r)
            .map(|d| d.get(b"XFA").is_ok())
            .unwrap_or(false),
        _ => false,
    }
}

/// Remove the /AcroForm entry from the document catalog.
fn remove_acroform(doc: &mut lopdf::Document) {
    let root_id = match doc.trailer.get(b"Root") {
        Ok(lopdf::Object::Reference(id)) => *id,
        _ => return,
    };
    if let Ok(lopdf::Object::Dictionary(ref mut dict)) = doc.get_object_mut(root_id) {
        dict.remove(b"AcroForm");
    }
}
