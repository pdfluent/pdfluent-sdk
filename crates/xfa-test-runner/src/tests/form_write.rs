use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

/// Roundtrip test: fill text, checkbox, radio, and choice fields via lopdf,
/// save, reopen, verify values via pdf-forms.  Covers #541.
pub struct FormWriteTest;

impl PdfTest for FormWriteTest {
    fn name(&self) -> &str {
        "form_write"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();
        let elapsed = || start.elapsed().as_millis() as u64;

        // 1. Parse with pdf-syntax to build field tree.
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

        // Run each sub-test.  Results are aggregated at the end.
        let text_res = run_text_roundtrip(&tree, pdf_data);
        let checkbox_res = run_checkbox_roundtrip(&tree, pdf_data);
        let radio_res = run_radio_roundtrip(&tree, pdf_data);
        let choice_res = run_choice_roundtrip(&tree, pdf_data);

        let sub_results = [
            ("text", &text_res),
            ("checkbox", &checkbox_res),
            ("radio", &radio_res),
            ("choice", &choice_res),
        ];

        let any_fail = sub_results.iter().any(|(_, r)| r.0 == TestStatus::Fail);
        let all_skip = sub_results.iter().all(|(_, r)| r.0 == TestStatus::Skip);

        let mut metadata = HashMap::new();
        for (name, (status, msg)) in &sub_results {
            metadata.insert(format!("{name}_status"), format!("{:?}", status));
            if let Some(m) = msg {
                metadata.insert(format!("{name}_msg"), m.clone());
            }
        }

        let error_message = if any_fail {
            sub_results
                .iter()
                .find(|(_, r)| r.0 == TestStatus::Fail)
                .and_then(|(name, (_, msg))| msg.as_ref().map(|m| format!("{name}: {m}")))
        } else {
            None
        };

        TestResult {
            status: if any_fail {
                TestStatus::Fail
            } else if all_skip {
                TestStatus::Skip
            } else {
                TestStatus::Pass
            },
            error_message,
            duration_ms: elapsed(),
            oracle_score: None,
            metadata,
        }
    }
}

// ─── Sub-test helpers ────────────────────────────────────────────────────────

type SubResult = (TestStatus, Option<String>);

fn skip(msg: impl Into<String>) -> SubResult {
    (TestStatus::Skip, Some(msg.into()))
}

fn fail(msg: impl Into<String>) -> SubResult {
    (TestStatus::Fail, Some(msg.into()))
}

fn pass() -> SubResult {
    (TestStatus::Pass, None)
}

// ─── Text roundtrip ──────────────────────────────────────────────────────────

fn run_text_roundtrip(tree: &pdf_forms::FieldTree, pdf_data: &[u8]) -> SubResult {
    use pdf_forms::FormAccess;

    let test_value = "__xfa_roundtrip_test__";

    // Find first writable text field whose MaxLen can hold the test value.
    // Fixes #459 (MaxLen inheritance) and #471 (short MaxLen skip).
    let names = tree.field_names();
    let field_name = match names.iter().find(|name| {
        if let Some(id) = tree.find_by_name(name) {
            matches!(
                tree.effective_field_type(id),
                Some(pdf_forms::FieldType::Text)
            ) && !tree.get(id).flags.read_only()
                && tree
                    .effective_max_len(id)
                    .is_none_or(|ml| ml as usize >= test_value.len())
        } else {
            false
        }
    }) {
        Some(n) => n.clone(),
        None => return skip("no writable text fields"),
    };

    let mut doc = match lopdf::Document::load_mem(pdf_data) {
        Ok(d) => d,
        Err(e) => return skip(format!("lopdf load: {e}")),
    };

    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        set_field_value_lopdf(&mut doc, &field_name, test_value)
    })) {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return skip(format!("field set: {e}")),
        Err(_) => return fail("panic setting text field"),
    }

    let mut saved = Vec::new();
    if let Err(e) = doc.save_to(&mut saved) {
        return fail(format!("save: {e}"));
    }

    let pdf2 = match pdf_syntax::Pdf::new(saved) {
        Ok(p) => p,
        Err(e) => return fail(format!("reopen: {e:?}")),
    };
    let tree2 = match pdf_forms::parse_acroform(&pdf2) {
        Some(t) => t,
        None => return fail("AcroForm lost after save"),
    };

    match tree2.get_value(&field_name) {
        Some(v) if v == test_value => pass(),
        Some(v) => fail(format!("got '{v}', expected '{test_value}'")),
        None => fail("field value None after roundtrip"),
    }
}

// ─── Checkbox roundtrip ──────────────────────────────────────────────────────

fn run_checkbox_roundtrip(tree: &pdf_forms::FieldTree, pdf_data: &[u8]) -> SubResult {
    use pdf_forms::FormAccess;

    // Find a checkbox: Button, not radio, not push-button, not read-only.
    let names = tree.field_names();
    let field_name = match names.iter().find(|name| {
        if let Some(id) = tree.find_by_name(name) {
            let flags = tree.effective_flags(id);
            matches!(
                tree.effective_field_type(id),
                Some(pdf_forms::FieldType::Button)
            ) && !flags.read_only()
                && !flags.radio()
                && !flags.push_button()
        } else {
            false
        }
    }) {
        Some(n) => n.clone(),
        None => return skip("no writable checkbox fields"),
    };

    // Check → verify → uncheck → verify.
    for (checked, label) in [(true, "check"), (false, "uncheck")] {
        let mut doc = match lopdf::Document::load_mem(pdf_data) {
            Ok(d) => d,
            Err(e) => return skip(format!("lopdf load: {e}")),
        };

        if let Err(e) = set_button_name_lopdf(&mut doc, &field_name, checked) {
            return skip(format!("{label}: {e}"));
        }

        let mut saved = Vec::new();
        if let Err(e) = doc.save_to(&mut saved) {
            return fail(format!("{label} save: {e}"));
        }

        let pdf2 = match pdf_syntax::Pdf::new(saved) {
            Ok(p) => p,
            Err(e) => return fail(format!("{label} reopen: {e:?}")),
        };
        let tree2 = match pdf_forms::parse_acroform(&pdf2) {
            Some(t) => t,
            None => return fail(format!("{label}: AcroForm lost after save")),
        };
        let id2 = match tree2.find_by_name(&field_name) {
            Some(id) => id,
            None => return fail(format!("{label}: field not found after reload")),
        };

        let actual = pdf_forms::button::is_checked(&tree2, id2);
        if actual != checked {
            return fail(format!(
                "checkbox after {label}: expected checked={checked}, got {actual}"
            ));
        }
    }

    pass()
}

// ─── Radio button roundtrip ──────────────────────────────────────────────────

fn run_radio_roundtrip(tree: &pdf_forms::FieldTree, pdf_data: &[u8]) -> SubResult {
    use pdf_forms::FormAccess;

    // Find a radio group: Button with radio flag, ≥2 children, not read-only.
    let names = tree.field_names();
    let group_name = match names.iter().find(|name| {
        if let Some(id) = tree.find_by_name(name) {
            let flags = tree.effective_flags(id);
            matches!(
                tree.effective_field_type(id),
                Some(pdf_forms::FieldType::Button)
            ) && flags.radio()
                && !flags.read_only()
                && tree.get(id).children.len() >= 2
        } else {
            false
        }
    }) {
        Some(n) => n.clone(),
        None => return skip("no radio button groups"),
    };

    let group_id = tree.find_by_name(&group_name).unwrap();
    let children = tree.get(group_id).children.clone();

    // Use first child's on-state as the value to select.
    let target_child = children[0];
    let on_state = pdf_forms::button::on_state_name(tree, target_child);

    // Collect (object_id, is_target) for each child widget.
    let child_oids: Vec<((i32, i32), bool)> = children
        .iter()
        .enumerate()
        .filter_map(|(i, &cid)| tree.get(cid).object_id.map(|oid| (oid, i == 0)))
        .collect();

    if child_oids.is_empty() {
        return skip("radio widgets have no object_ids");
    }

    let mut doc = match lopdf::Document::load_mem(pdf_data) {
        Ok(d) => d,
        Err(e) => return skip(format!("lopdf load: {e}")),
    };

    // Set parent /V = Name(on_state).
    if let Err(e) = set_button_name_lopdf(&mut doc, &group_name, true) {
        // set_button_name_lopdf uses "Yes" — we need the actual on_state.
        // Fall back to direct object manipulation if on_state != "Yes".
        if on_state != "Yes" {
            if let Err(e2) = set_radio_parent_v_lopdf(&mut doc, &group_name, &on_state) {
                return skip(format!("set radio parent /V: {e} / {e2}"));
            }
        } else {
            return skip(format!("set radio parent: {e}"));
        }
    }

    // Set each widget's /AS.
    for ((obj_num, gen_num), is_target) in &child_oids {
        let lopdf_id = (*obj_num as u32, *gen_num as u16);
        let as_name = if *is_target {
            on_state.as_bytes().to_vec()
        } else {
            b"Off".to_vec()
        };
        if let Err(e) = set_widget_as_lopdf(&mut doc, lopdf_id, as_name) {
            return skip(format!("set widget /AS: {e}"));
        }
    }

    let mut saved = Vec::new();
    if let Err(e) = doc.save_to(&mut saved) {
        return fail(format!("save: {e}"));
    }

    let pdf2 = match pdf_syntax::Pdf::new(saved) {
        Ok(p) => p,
        Err(e) => return fail(format!("reopen: {e:?}")),
    };
    let tree2 = match pdf_forms::parse_acroform(&pdf2) {
        Some(t) => t,
        None => return fail("AcroForm lost after save"),
    };
    let group_id2 = match tree2.find_by_name(&group_name) {
        Some(id) => id,
        None => return fail("radio group not found after reload"),
    };

    // First child should be checked; at least one sibling should be off.
    let children2 = tree2.get(group_id2).children.clone();
    if children2.is_empty() {
        return fail("radio group has no children after reload");
    }
    let first_checked = pdf_forms::button::is_checked(&tree2, children2[0]);
    if !first_checked {
        return fail(format!(
            "radio first option not checked after roundtrip (on_state='{on_state}')"
        ));
    }
    if children2.len() > 1 && pdf_forms::button::is_checked(&tree2, children2[1]) {
        return fail("radio sibling unexpectedly checked after roundtrip");
    }

    pass()
}

// ─── Choice (dropdown / listbox) roundtrip ───────────────────────────────────

fn run_choice_roundtrip(tree: &pdf_forms::FieldTree, pdf_data: &[u8]) -> SubResult {
    use pdf_forms::FormAccess;

    // Find a choice field (combo or list) that is writable and has ≥1 option.
    let names = tree.field_names();
    let (field_name, first_export) = match names.iter().find_map(|name| {
        let id = tree.find_by_name(name)?;
        if !matches!(
            tree.effective_field_type(id),
            Some(pdf_forms::FieldType::Choice)
        ) || tree.effective_flags(id).read_only()
        {
            return None;
        }
        let opts = pdf_forms::choice::get_options(tree, id);
        if opts.is_empty() {
            return None;
        }
        Some((name.clone(), opts[0].export.clone()))
    }) {
        Some(pair) => pair,
        None => return skip("no writable choice fields with options"),
    };

    // Choice /V is a String (same as text field write path).
    let mut doc = match lopdf::Document::load_mem(pdf_data) {
        Ok(d) => d,
        Err(e) => return skip(format!("lopdf load: {e}")),
    };

    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        set_field_value_lopdf(&mut doc, &field_name, &first_export)
    })) {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return skip(format!("field set: {e}")),
        Err(_) => return fail("panic setting choice field"),
    }

    let mut saved = Vec::new();
    if let Err(e) = doc.save_to(&mut saved) {
        return fail(format!("save: {e}"));
    }

    let pdf2 = match pdf_syntax::Pdf::new(saved) {
        Ok(p) => p,
        Err(e) => return fail(format!("reopen: {e:?}")),
    };
    let tree2 = match pdf_forms::parse_acroform(&pdf2) {
        Some(t) => t,
        None => return fail("AcroForm lost after save"),
    };
    let id2 = match tree2.find_by_name(&field_name) {
        Some(id) => id,
        None => return fail("choice field not found after reload"),
    };

    let selection = pdf_forms::choice::get_selection(&tree2, id2);
    if selection.contains(&first_export) {
        pass()
    } else {
        fail(format!(
            "choice selection mismatch: expected '{}', got {:?}",
            first_export, selection
        ))
    }
}

// ─── lopdf field mutation helpers ────────────────────────────────────────────

/// Set a form field value directly via lopdf by walking the AcroForm field tree.
/// Value is written as a PDF String (for text/choice fields).
fn set_field_value_lopdf(
    doc: &mut lopdf::Document,
    target_name: &str,
    value: &str,
) -> Result<(), String> {
    use lopdf::Object;

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

        let partial = field_dict.get(b"T").ok().and_then(|o| match o {
            Object::String(s, _) => Some(String::from_utf8_lossy(s).to_string()),
            _ => None,
        });

        if name_parts.is_empty() {
            continue;
        }

        let partial = match partial {
            Some(p) => p,
            None => {
                if let Ok(Object::Array(kids_arr)) = field_dict.get(b"Kids") {
                    let kids_clone = kids_arr.clone();
                    if find_and_set_field(doc, &kids_clone, name_parts, value).is_ok() {
                        return Ok(());
                    }
                }
                continue;
            }
        };

        if partial != name_parts[0] {
            continue;
        }

        if name_parts.len() == 1 {
            // Terminal field — set /V on this object and propagate to all unnamed
            // descendants.  Fixes #478 (unmerged fields with unnamed widget children).
            set_value_deep(doc, field_id, value)?;
            return Ok(());
        }

        if let Ok(Object::Array(kids_arr)) = field_dict.get(b"Kids") {
            let kids_clone = kids_arr.clone();
            return find_and_set_field(doc, &kids_clone, &name_parts[1..], value);
        }
    }

    Err(format!("field '{}' not found", name_parts.join(".")))
}

/// Write /V to `field_id` and recursively to all unnamed descendants.
/// Fixes #478 (unmerged widgets store displayable /V on unnamed child).
fn set_value_deep(
    doc: &mut lopdf::Document,
    field_id: lopdf::ObjectId,
    value: &str,
) -> Result<(), String> {
    use lopdf::Object;

    {
        let obj = doc
            .get_object_mut(field_id)
            .map_err(|e| format!("get_mut {field_id:?}: {e}"))?;
        if let Object::Dictionary(d) = obj {
            d.set(
                b"V".to_vec(),
                Object::String(value.as_bytes().to_vec(), lopdf::StringFormat::Literal),
            );
        }
    }

    let kids: Vec<lopdf::ObjectId> = match doc.get_object(field_id) {
        Ok(Object::Dictionary(d)) => match d.get(b"Kids") {
            Ok(Object::Array(arr)) => arr
                .iter()
                .filter_map(|o| match o {
                    Object::Reference(id) => Some(*id),
                    _ => None,
                })
                .collect(),
            _ => vec![],
        },
        _ => vec![],
    };

    for kid_id in kids {
        let has_name: bool = match doc.get_object(kid_id) {
            Ok(Object::Dictionary(d)) => {
                matches!(d.get(b"T"), Ok(Object::String(s, _)) if !s.is_empty())
            }
            _ => false,
        };
        if !has_name {
            set_value_deep(doc, kid_id, value)?;
        }
    }

    Ok(())
}

/// Set a button field's /V and /AS as PDF Name objects.
/// `checked=true`  → Name("Yes") (standard on-state for new checkboxes)
/// `checked=false` → Name("Off")
///
/// For radio group parents, use `set_radio_parent_v_lopdf` for custom on-state names.
fn set_button_name_lopdf(
    doc: &mut lopdf::Document,
    target_name: &str,
    checked: bool,
) -> Result<(), String> {
    use lopdf::Object;

    let name_val = if checked {
        b"Yes".to_vec()
    } else {
        b"Off".to_vec()
    };

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
    let field_refs = match acroform.get(b"Fields") {
        Ok(Object::Array(arr)) => arr.clone(),
        _ => return Err("no Fields array in AcroForm".into()),
    };

    let parts: Vec<&str> = target_name.split('.').collect();
    find_and_set_button_name(doc, &field_refs, &parts, name_val)
}

fn find_and_set_button_name(
    doc: &mut lopdf::Document,
    refs: &[lopdf::Object],
    name_parts: &[&str],
    name_val: Vec<u8>,
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

        let partial = field_dict.get(b"T").ok().and_then(|o| match o {
            Object::String(s, _) => Some(String::from_utf8_lossy(s).to_string()),
            _ => None,
        });

        let partial = match partial {
            Some(p) => p,
            None => {
                if let Ok(Object::Array(kids_arr)) = field_dict.get(b"Kids") {
                    let kids_clone = kids_arr.clone();
                    if find_and_set_button_name(doc, &kids_clone, name_parts, name_val.clone())
                        .is_ok()
                    {
                        return Ok(());
                    }
                }
                continue;
            }
        };

        if partial != name_parts[0] {
            continue;
        }

        if name_parts.len() == 1 {
            set_button_name_deep(doc, field_id, name_val)?;
            return Ok(());
        }

        if let Ok(Object::Array(kids_arr)) = field_dict.get(b"Kids") {
            let kids_clone = kids_arr.clone();
            return find_and_set_button_name(doc, &kids_clone, &name_parts[1..], name_val);
        }
    }

    Err(format!("button field '{}' not found", name_parts.join(".")))
}

/// Write /V and /AS as Name objects to `field_id` and its unnamed descendants.
fn set_button_name_deep(
    doc: &mut lopdf::Document,
    field_id: lopdf::ObjectId,
    name_val: Vec<u8>,
) -> Result<(), String> {
    use lopdf::Object;

    {
        let obj = doc
            .get_object_mut(field_id)
            .map_err(|e| format!("get_mut {field_id:?}: {e}"))?;
        if let Object::Dictionary(d) = obj {
            d.set(b"V".to_vec(), Object::Name(name_val.clone()));
            d.set(b"AS".to_vec(), Object::Name(name_val.clone()));
        }
    }

    let kids: Vec<lopdf::ObjectId> = match doc.get_object(field_id) {
        Ok(Object::Dictionary(d)) => match d.get(b"Kids") {
            Ok(Object::Array(arr)) => arr
                .iter()
                .filter_map(|o| match o {
                    Object::Reference(id) => Some(*id),
                    _ => None,
                })
                .collect(),
            _ => vec![],
        },
        _ => vec![],
    };

    for kid_id in kids {
        let has_name: bool = match doc.get_object(kid_id) {
            Ok(Object::Dictionary(d)) => {
                matches!(d.get(b"T"), Ok(Object::String(s, _)) if !s.is_empty())
            }
            _ => false,
        };
        if !has_name {
            set_button_name_deep(doc, kid_id, name_val.clone())?;
        }
    }

    Ok(())
}

/// Set the parent radio group field's /V to the given Name value.
fn set_radio_parent_v_lopdf(
    doc: &mut lopdf::Document,
    target_name: &str,
    on_state: &str,
) -> Result<(), String> {
    use lopdf::Object;

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
    let field_refs = match acroform.get(b"Fields") {
        Ok(Object::Array(arr)) => arr.clone(),
        _ => return Err("no Fields array in AcroForm".into()),
    };

    let parts: Vec<&str> = target_name.split('.').collect();
    find_and_set_radio_v(doc, &field_refs, &parts, on_state.as_bytes().to_vec())
}

fn find_and_set_radio_v(
    doc: &mut lopdf::Document,
    refs: &[lopdf::Object],
    name_parts: &[&str],
    name_val: Vec<u8>,
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

        let partial = field_dict.get(b"T").ok().and_then(|o| match o {
            Object::String(s, _) => Some(String::from_utf8_lossy(s).to_string()),
            _ => None,
        });

        let partial = match partial {
            Some(p) => p,
            None => {
                if let Ok(Object::Array(kids_arr)) = field_dict.get(b"Kids") {
                    let kids_clone = kids_arr.clone();
                    if find_and_set_radio_v(doc, &kids_clone, name_parts, name_val.clone()).is_ok()
                    {
                        return Ok(());
                    }
                }
                continue;
            }
        };

        if partial != name_parts[0] {
            continue;
        }

        if name_parts.len() == 1 {
            let obj = doc
                .get_object_mut(field_id)
                .map_err(|e| format!("get_mut {field_id:?}: {e}"))?;
            if let Object::Dictionary(d) = obj {
                d.set(b"V".to_vec(), Object::Name(name_val));
            }
            return Ok(());
        }

        if let Ok(Object::Array(kids_arr)) = field_dict.get(b"Kids") {
            let kids_clone = kids_arr.clone();
            return find_and_set_radio_v(doc, &kids_clone, &name_parts[1..], name_val);
        }
    }

    Err(format!("radio field '{}' not found", name_parts.join(".")))
}

/// Set /AS on a widget dict identified directly by its lopdf ObjectId.
fn set_widget_as_lopdf(
    doc: &mut lopdf::Document,
    id: lopdf::ObjectId,
    as_name: Vec<u8>,
) -> Result<(), String> {
    use lopdf::Object;
    let obj = doc
        .get_object_mut(id)
        .map_err(|e| format!("get_mut {id:?}: {e}"))?;
    if let Object::Dictionary(d) = obj {
        d.set(b"AS".to_vec(), Object::Name(as_name));
        Ok(())
    } else {
        Err(format!("object {id:?} is not a dict"))
    }
}
