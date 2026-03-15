//! Diagnostic: verify the form_write round-trip on maxLenInheritanceTest.pdf.
//!
//! Checks that:
//!   1. The terminal widget is correctly identified as a Text field via
//!      effective_field_type() (inherited /FT /Tx from parent).
//!   2. Writing 22 chars via set_field_value_lopdf is NOT blocked by the
//!      inherited /MaxLen 20.
//!   3. Readback matches the written value.
//!
//! Run with:
//!   cargo run -p xfa-test-runner --example diag_maxlen_fixture

use std::path::PathBuf;

fn main() {
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/maxLenInheritanceTest.pdf");

    let pdf_data =
        std::fs::read(&fixture).unwrap_or_else(|_| panic!("Cannot read {}", fixture.display()));

    // ── Step 1: parse acroform ────────────────────────────────────────────
    let pdf = pdf_syntax::Pdf::new(pdf_data.clone()).expect("pdf-syntax parse");
    let tree = pdf_forms::parse_acroform(&pdf).expect("parse_acroform: no AcroForm");

    use pdf_forms::FormAccess;
    let names = tree.field_names();
    println!("Fields: {:?}", names);

    for name in &names {
        if let Some(id) = tree.find_by_name(name) {
            let node = tree.get(id);
            println!(
                "  '{}': own_type={:?}  eff_type={:?}  own_maxlen={:?}  eff_maxlen={:?}",
                name,
                node.field_type,
                tree.effective_field_type(id),
                node.max_len,
                tree.effective_max_len(id),
            );
        }
    }

    // ── Step 2: field selection (OLD — uses node.field_type) ──────────────
    let old_found = names.iter().find(|name| {
        if let Some(id) = tree.find_by_name(name) {
            let node = tree.get(id);
            matches!(node.field_type, Some(pdf_forms::FieldType::Text)) && !node.flags.read_only()
        } else {
            false
        }
    });

    // ── Step 3: field selection (NEW — uses effective_field_type) ─────────
    let new_found = names.iter().find(|name| {
        if let Some(id) = tree.find_by_name(name) {
            matches!(
                tree.effective_field_type(id),
                Some(pdf_forms::FieldType::Text)
            ) && !tree.get(id).flags.read_only()
        } else {
            false
        }
    });

    println!("Old selection (broken): {:?}", old_found);
    println!("New selection (fixed):  {:?}", new_found);
    assert!(
        old_found.is_none(),
        "Old code should NOT find the inherited-type field"
    );
    assert!(
        new_found.is_some(),
        "New code MUST find the inherited-type field"
    );

    let field_name = new_found.unwrap().clone();
    println!("Selected field: '{field_name}'");

    // ── Step 4: write via lopdf ───────────────────────────────────────────
    let test_value = "__xfa_roundtrip_test__"; // 22 chars — exceeds /MaxLen 20

    let mut doc = lopdf::Document::load_mem(&pdf_data).expect("lopdf load");
    set_field_value_lopdf(&mut doc, &field_name, test_value).expect("set field");

    let mut saved = Vec::new();
    doc.save_to(&mut saved).expect("lopdf save");

    // ── Step 5: readback ──────────────────────────────────────────────────
    let pdf2 = pdf_syntax::Pdf::new(saved).expect("reopen after write");
    let tree2 = pdf_forms::parse_acroform(&pdf2).expect("parse_acroform after write");

    let readback = tree2.get_value(&field_name);
    println!("Readback: {:?}", readback);

    assert_eq!(
        readback.as_deref(),
        Some(test_value),
        "Round-trip value mismatch!"
    );

    println!("✓ form_write round-trip PASSED for maxLenInheritanceTest.pdf");
}

// ── Copied verbatim from form_write.rs ────────────────────────────────────

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
        if let Ok(Object::Array(kids_arr)) = field_dict.get(b"Kids") {
            let kids_clone = kids_arr.clone();
            return find_and_set_field(doc, &kids_clone, &name_parts[1..], value);
        }
    }
    Err(format!("field '{}' not found", name_parts.join(".")))
}
