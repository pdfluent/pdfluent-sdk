use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

/// Roundtrip test: fill first text field via lopdf, save, reopen, verify value.
pub struct FormWriteTest;

impl PdfTest for FormWriteTest {
    fn name(&self) -> &str {
        "form_write"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();
        let elapsed = || start.elapsed().as_millis() as u64;

        // 1. Parse with pdf-syntax to find AcroForm text fields.
        let pdf = match pdf_syntax::Pdf::new(pdf_data.to_vec()) {
            Ok(p) => p,
            Err(_) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some("pdf-syntax parse failed".into()),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        let tree = match pdf_forms::parse_acroform(&pdf) {
            Some(t) => t,
            None => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: None,
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        use pdf_forms::FormAccess;
        let names = tree.field_names();

        // Find first text field that is writable.
        let text_field = names.iter().find(|name| {
            if let Some(id) = tree.find_by_name(name) {
                let node = tree.get(id);
                matches!(node.field_type, Some(pdf_forms::FieldType::Text))
                    && !node.flags.read_only()
            } else {
                false
            }
        });

        let field_name = match text_field {
            Some(n) => n.clone(),
            None => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some("no writable text fields".into()),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        // 2. Load via lopdf and set field value.
        let mut doc = match lopdf::Document::load_mem(pdf_data) {
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

        let test_value = "__xfa_roundtrip_test__";

        let set_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            set_field_value_lopdf(&mut doc, &field_name, test_value)
        }));

        match set_result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some(format!("field set failed: {e}")),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
            Err(_) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some("panic setting field value".into()),
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

        // 4. Reopen and verify.
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

        let tree2 = match pdf_forms::parse_acroform(&pdf2) {
            Some(t) => t,
            None => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some("AcroForm lost after save".into()),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        let readback = tree2.get_value(&field_name);
        let mut metadata = HashMap::new();
        metadata.insert("field_name".into(), field_name.clone());
        metadata.insert("readback".into(), readback.clone().unwrap_or_default());

        match readback {
            Some(v) if v == test_value => TestResult {
                status: TestStatus::Pass,
                error_message: None,
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
            },
            Some(v) => TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!(
                    "value mismatch: expected '{test_value}', got '{v}'"
                )),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
            },
            None => TestResult {
                status: TestStatus::Fail,
                error_message: Some("field value is None after roundtrip".into()),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
            },
        }
    }
}

/// Set a form field value directly via lopdf by walking the AcroForm field tree.
fn set_field_value_lopdf(
    doc: &mut lopdf::Document,
    target_name: &str,
    value: &str,
) -> Result<(), String> {
    use lopdf::Object;

    // Find AcroForm → Fields array.
    let catalog = doc.catalog().map_err(|e| format!("no catalog: {e}"))?;

    let acroform_ref = catalog
        .get(b"AcroForm")
        .map_err(|_| "no AcroForm in catalog")?
        .clone();

    let acroform = doc
        .dereference(&acroform_ref)
        .map_err(|e| format!("deref AcroForm: {e}"))?
        .1
        .as_dict()
        .map_err(|_| "AcroForm not a dict")?
        .clone();

    let fields = acroform
        .get(b"Fields")
        .map_err(|_| "no Fields in AcroForm")?;
    let field_refs = match fields {
        Object::Array(arr) => arr.clone(),
        _ => return Err("Fields is not an array".into()),
    };

    // Walk fields looking for matching name.
    let parts: Vec<&str> = target_name.split('.').collect();
    find_and_set_field(doc, &field_refs, &parts, value)
}

fn find_and_set_field(
    doc: &mut lopdf::Document,
    refs: &[lopdf::Object],
    name_parts: &[&str],
    value: &str,
) -> Result<(), String> {
    use lopdf::Object;

    for obj in refs {
        let field_id = match obj {
            Object::Reference(id) => *id,
            _ => continue,
        };

        let field_dict = match doc.get_object(field_id) {
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => continue,
        };

        let partial = field_dict
            .get(b"T")
            .ok()
            .and_then(|o| match o {
                Object::String(s, _) => Some(String::from_utf8_lossy(s).to_string()),
                _ => None,
            })
            .unwrap_or_default();

        if name_parts.is_empty() {
            continue;
        }

        if partial != name_parts[0] {
            continue;
        }

        if name_parts.len() == 1 {
            // Terminal field — set value.
            let obj = doc
                .get_object_mut(field_id)
                .map_err(|e| format!("get_mut: {e}"))?;
            if let Object::Dictionary(d) = obj {
                d.set(
                    b"V".to_vec(),
                    Object::String(value.as_bytes().to_vec(), lopdf::StringFormat::Literal),
                );
            }
            return Ok(());
        }

        // Not terminal — recurse into Kids.
        if let Ok(Object::Array(kids_arr)) = field_dict.get(b"Kids") {
            let kids_clone = kids_arr.clone();
            return find_and_set_field(doc, &kids_clone, &name_parts[1..], value);
        }
    }

    Err(format!("field '{}' not found", name_parts.join(".")))
}
