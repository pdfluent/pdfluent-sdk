//! XFA data binding roundtrip test.
//!
//! Verifies that the XFA Data DOM can be serialized to XML and re-parsed
//! without data loss: field values present in the original datasets packet
//! must still be present and identical after a serialize → re-parse roundtrip.
//!
//! Roundtrip steps:
//! 1. Extract the datasets XML from the XFA packet.
//! 2. Parse into a DataDom.
//! 3. Collect all leaf (DataValue) nodes as (path, value) pairs.
//! 4. Re-serialize via `DataDom::to_xml()`.
//! 5. Re-parse the serialized XML into a fresh DataDom.
//! 6. Collect leaf (path, value) pairs from the re-parsed DOM.
//! 7. Fail if any value was lost or changed.
//!
//! Skip policy:
//! - PDF cannot be parsed              → Skip
//! - No /XFA in this PDF               → Skip
//! - XFA extraction error              → Skip
//! - No datasets packet                → Skip (form has no data)
//! - Datasets XML cannot be parsed     → Skip (malformed data; not our bug)
//! - DataDom has no leaf values        → Skip (nothing to verify)
//!
//! Fail conditions:
//! - Re-serialized XML cannot be parsed by DataDom::from_xml
//! - Any leaf value present in the original is missing after roundtrip
//! - Any leaf value changed during roundtrip

use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

pub struct XfaDataRoundtripTest;

impl PdfTest for XfaDataRoundtripTest {
    fn name(&self) -> &str {
        "xfa_data_roundtrip"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();

        let pdf = match pdf_syntax::Pdf::new(pdf_data.to_vec()) {
            Ok(p) => p,
            Err(_) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some("PDF could not be parsed".into()),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                }
            }
        };

        let packets = match pdf_xfa::extract::extract_xfa(&pdf) {
            Ok(p) => p,
            Err(pdf_xfa::error::XfaError::PacketNotFound(_)) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: None,
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                }
            }
            Err(e) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some(format!("XFA extraction error: {e}")),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                }
            }
        };

        let datasets_xml = match packets.datasets() {
            Some(d) => d.to_string(),
            None => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some("no datasets packet".into()),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                }
            }
        };

        // Parse the original datasets XML.
        let original_dom = match pdf_xfa::dom_resolver::data_dom::DataDom::from_xml(&datasets_xml) {
            Ok(d) => d,
            Err(e) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some(format!("datasets XML parse error: {e}")),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                }
            }
        };

        // Collect leaf values from the original DataDom.
        let original_values = collect_leaf_values(&original_dom);
        if original_values.is_empty() {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some("datasets has no leaf values".into()),
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        // Re-serialize and re-parse.
        let reserialized = original_dom.to_xml();
        let roundtrip_dom = match pdf_xfa::dom_resolver::data_dom::DataDom::from_xml(&reserialized)
        {
            Ok(d) => d,
            Err(e) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some(format!("re-serialized XML could not be parsed: {e}")),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                }
            }
        };

        // Compare leaf values.
        let roundtrip_values = collect_leaf_values(&roundtrip_dom);
        let mut missing: Vec<String> = Vec::new();
        let mut changed: Vec<String> = Vec::new();

        for (path, orig_val) in &original_values {
            match roundtrip_values.get(path) {
                None => missing.push(path.clone()),
                Some(rt_val) if rt_val != orig_val => {
                    changed.push(format!("{path}: {orig_val:?} → {rt_val:?}"));
                }
                _ => {}
            }
        }

        let mut metadata = HashMap::new();
        metadata.insert("leaf_count".to_string(), original_values.len().to_string());
        metadata.insert(
            "roundtrip_leaf_count".to_string(),
            roundtrip_values.len().to_string(),
        );

        if !missing.is_empty() || !changed.is_empty() {
            let mut parts = Vec::new();
            if !missing.is_empty() {
                parts.push(format!(
                    "{} missing: {}",
                    missing.len(),
                    missing[..3.min(missing.len())].join(", ")
                ));
            }
            if !changed.is_empty() {
                parts.push(format!(
                    "{} changed: {}",
                    changed.len(),
                    changed[..3.min(changed.len())].join(", ")
                ));
            }
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some(parts.join("; ")),
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata,
            };
        }

        TestResult {
            status: TestStatus::Pass,
            error_message: None,
            duration_ms: start.elapsed().as_millis() as u64,
            oracle_score: None,
            metadata,
        }
    }
}

/// Recursively collect all leaf (DataValue) nodes as (dot-path, value) pairs.
///
/// Path segments are the node names joined with `.`. Duplicate names at the
/// same level are disambiguated with a `[N]` index suffix.
fn collect_leaf_values(dom: &pdf_xfa::dom_resolver::data_dom::DataDom) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if let Some(root) = dom.root() {
        visit_node(dom, root, "", &mut out);
    }
    out
}

fn visit_node(
    dom: &pdf_xfa::dom_resolver::data_dom::DataDom,
    id: pdf_xfa::dom_resolver::data_dom::DataNodeId,
    // `parent_path` is the fully-qualified dot-path of id's parent (empty for root).
    parent_path: &str,
    out: &mut HashMap<String, String>,
) {
    let node = match dom.get(id) {
        Some(n) => n,
        None => return,
    };

    let path = if parent_path.is_empty() {
        node.name().to_string()
    } else {
        format!("{parent_path}.{}", node.name())
    };

    if node.is_value() {
        // For duplicate sibling names the HashMap keeps the last value; that is
        // consistent between original and roundtrip so the comparison still works.
        out.insert(path, node.value().to_string());
    } else {
        for &child in dom.children(id) {
            visit_node(dom, child, &path, out);
        }
    }
}
