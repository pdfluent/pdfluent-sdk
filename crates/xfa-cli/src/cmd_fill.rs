//! Fill AcroForm fields from a JSON file.

use anyhow::{Context, Result};
use std::path::Path;

pub fn run(input: &Path, output: &Path, data: &Path) -> Result<()> {
    let json_str = std::fs::read_to_string(data).context("failed to read data JSON")?;
    let values: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&json_str).context("failed to parse JSON as object")?;

    let pdf_bytes = std::fs::read(input).context("failed to read input PDF")?;
    let mut doc = lopdf::Document::load_mem(&pdf_bytes).context("failed to parse PDF")?;

    let mut filled = 0usize;
    for (name, val) in &values {
        let value_str = match val {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        if set_acroform_field(&mut doc, name, &value_str) {
            filled += 1;
            println!("  {name} = {value_str}");
        } else {
            eprintln!("  warning: field '{name}' not found");
        }
    }

    doc.save(output).context("failed to save output PDF")?;
    println!(
        "Filled {filled}/{} fields -> {}",
        values.len(),
        output.display()
    );
    Ok(())
}

/// Set an AcroForm field value by fully-qualified name.
fn set_acroform_field(doc: &mut lopdf::Document, name: &str, value: &str) -> bool {
    let field_ids: Vec<lopdf::ObjectId> = match get_acroform_field_ids(doc) {
        Some(ids) => ids,
        None => return false,
    };

    for id in field_ids {
        let fq = get_field_name(doc, id);
        if fq == name {
            if let Ok(lopdf::Object::Dictionary(ref mut dict)) = doc.get_object_mut(id) {
                dict.set(
                    "V",
                    lopdf::Object::String(value.as_bytes().to_vec(), lopdf::StringFormat::Literal),
                );
                dict.remove(b"AP");
            }
            // Tell viewers to regenerate appearances for all fields.
            set_need_appearances(doc);
            return true;
        }
    }
    false
}

/// Set /NeedAppearances on the AcroForm dict so viewers regenerate /AP streams.
fn set_need_appearances(doc: &mut lopdf::Document) {
    let acroform_ref = doc
        .catalog()
        .ok()
        .and_then(|cat| cat.get(b"AcroForm").ok().cloned());
    if let Some(lopdf::Object::Reference(af_id)) = acroform_ref {
        if let Ok(lopdf::Object::Dictionary(ref mut af_dict)) = doc.get_object_mut(af_id) {
            af_dict.set("NeedAppearances", lopdf::Object::Boolean(true));
        }
    }
}

/// Collect all field object IDs by walking /Fields and /Kids recursively.
fn get_acroform_field_ids(doc: &lopdf::Document) -> Option<Vec<lopdf::ObjectId>> {
    let catalog = doc.catalog().ok()?;
    let acroform_ref = catalog.get(b"AcroForm").ok()?;
    let acroform = doc.dereference(acroform_ref).ok()?.1;
    let acroform_dict = acroform.as_dict().ok()?;
    let fields = acroform_dict.get(b"Fields").ok()?;
    let fields_arr = match fields {
        lopdf::Object::Array(arr) => arr.clone(),
        lopdf::Object::Reference(r) => {
            let obj = doc.get_object(*r).ok()?;
            match obj {
                lopdf::Object::Array(arr) => arr.clone(),
                _ => return None,
            }
        }
        _ => return None,
    };

    let mut result = Vec::new();
    collect_field_ids(doc, &fields_arr, &mut result);
    Some(result)
}

fn collect_field_ids(doc: &lopdf::Document, arr: &[lopdf::Object], out: &mut Vec<lopdf::ObjectId>) {
    for obj in arr {
        let id = match obj {
            lopdf::Object::Reference(r) => *r,
            _ => continue,
        };
        out.push(id);
        if let Ok(field_obj) = doc.get_object(id) {
            if let Ok(dict) = field_obj.as_dict() {
                if let Ok(kids) = dict.get(b"Kids") {
                    if let Ok(kids_arr) = kids.as_array() {
                        collect_field_ids(doc, kids_arr, out);
                    } else if let lopdf::Object::Reference(r) = kids {
                        if let Ok(kids_obj) = doc.get_object(*r) {
                            if let Ok(arr) = kids_obj.as_array() {
                                collect_field_ids(doc, arr, out);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Get the fully-qualified field name by walking /Parent chain.
fn get_field_name(doc: &lopdf::Document, id: lopdf::ObjectId) -> String {
    let mut parts = Vec::new();
    let mut current_id = Some(id);

    while let Some(cid) = current_id {
        let obj = match doc.get_object(cid) {
            Ok(o) => o,
            Err(_) => break,
        };
        let dict = match obj.as_dict() {
            Ok(d) => d,
            Err(_) => break,
        };

        if let Ok(t) = dict.get(b"T") {
            let name = match t {
                lopdf::Object::String(bytes, _) => String::from_utf8_lossy(bytes).to_string(),
                lopdf::Object::Reference(r) => {
                    if let Ok(lopdf::Object::String(bytes, _)) = doc.get_object(*r) {
                        String::from_utf8_lossy(bytes).to_string()
                    } else {
                        String::new()
                    }
                }
                _ => String::new(),
            };
            if !name.is_empty() {
                parts.push(name);
            }
        }

        current_id = dict.get(b"Parent").ok().and_then(|p| {
            if let lopdf::Object::Reference(r) = p {
                Some(*r)
            } else {
                None
            }
        });
    }

    parts.reverse();
    parts.join(".")
}
