//! Flatten AcroForm fields — remove interactive form elements.

use anyhow::{Context, Result};
use std::path::Path;

pub fn run(input: &Path, output: &Path) -> Result<()> {
    let pdf_bytes = std::fs::read(input).context("failed to read input PDF")?;
    let mut doc = lopdf::Document::load_mem(&pdf_bytes).context("failed to parse PDF")?;

    let removed = flatten_acroform(&mut doc);

    doc.save(output).context("failed to save output PDF")?;
    println!("Flattened {removed} form fields -> {}", output.display());
    Ok(())
}

/// Remove Widget annotations from pages and AcroForm from the catalog.
fn flatten_acroform(doc: &mut lopdf::Document) -> usize {
    let mut removed = 0usize;

    // First pass: identify Widget annotation object IDs.
    let mut widget_ids = std::collections::HashSet::new();
    for &id in doc.objects.keys() {
        if let Ok(obj) = doc.get_object(id) {
            if let Ok(dict) = obj.as_dict() {
                let is_widget = dict
                    .get(b"Subtype")
                    .ok()
                    .is_some_and(|st| matches!(st, lopdf::Object::Name(ref n) if n == b"Widget"));
                if is_widget {
                    widget_ids.insert(id);
                }
            }
        }
    }

    // Second pass: remove Widget refs from page Annots arrays.
    let page_ids: Vec<lopdf::ObjectId> = doc.page_iter().collect();
    for page_id in page_ids {
        if let Ok(lopdf::Object::Dictionary(ref mut dict)) = doc.get_object_mut(page_id) {
            if let Ok(annots) = dict.get(b"Annots") {
                if let Ok(arr) = annots.as_array() {
                    let filtered: Vec<lopdf::Object> = arr
                        .iter()
                        .filter(|obj| {
                            if let lopdf::Object::Reference(r) = obj {
                                if widget_ids.contains(r) {
                                    removed += 1;
                                    return false;
                                }
                            }
                            true
                        })
                        .cloned()
                        .collect();
                    dict.set("Annots", lopdf::Object::Array(filtered));
                }
            }
        }
    }

    // Remove AcroForm from catalog.
    let root_id = doc.trailer.get(b"Root").ok().and_then(|r| {
        if let lopdf::Object::Reference(id) = r {
            Some(*id)
        } else {
            None
        }
    });
    if let Some(rid) = root_id {
        if let Ok(lopdf::Object::Dictionary(ref mut dict)) = doc.get_object_mut(rid) {
            dict.remove(b"AcroForm");
        }
    }

    removed
}
