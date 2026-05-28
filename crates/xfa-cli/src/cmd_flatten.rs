//! Flatten AcroForm fields — remove interactive form elements.

use anyhow::{Context, Result};
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
struct LayoutDumpJson {
    pages: Vec<LayoutDumpEntryJson>,
}

#[derive(Serialize)]
struct LayoutDumpEntryJson {
    page_num: u32,
    page_height: f64,
    used_height: f64,
    overflow_to_next: bool,
    first_overflow_element: Option<String>,
}

fn write_layout_dump(path: &Path, dump: pdf_xfa::LayoutDump) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).context("failed to create dump-layout directory")?;
    }

    let json = LayoutDumpJson {
        pages: dump
            .pages
            .into_iter()
            .map(|entry| LayoutDumpEntryJson {
                page_num: entry.page_num,
                page_height: entry.page_height,
                used_height: entry.used_height,
                overflow_to_next: entry.overflow_to_next,
                first_overflow_element: entry.first_overflow_element,
            })
            .collect(),
    };

    let bytes = serde_json::to_vec_pretty(&json).context("failed to serialise layout dump")?;
    std::fs::write(path, bytes).context("failed to write layout dump JSON")?;
    Ok(())
}

pub fn run(
    input: &Path,
    output: &Path,
    dump_layout: Option<&Path>,
    xfa_rendering_policy: &str,
) -> Result<()> {
    // D11/D14: explicit rendering policy. `flatten` is the production command and
    // applies the default `SavedStateFaithful` policy only — it does not thread a
    // policy through the flattener. `FreshMergeExperimental` is experimental,
    // opt-in, and pending corpus-scale (D13) validation; it is reproduced for
    // measurement via the policy-aware API
    // (`pdf_xfa::flatten_xfa_to_pdf_with_policy`) or `pdfluent measure --policy
    // fresh-merge`, never by `flatten`. Reject any non-saved-state token loudly
    // rather than silently producing saved-state output under the wrong label.
    let policy =
        pdf_xfa::XfaRenderingPolicy::from_token(xfa_rendering_policy).with_context(|| {
            format!(
                "unknown --xfa-rendering-policy '{xfa_rendering_policy}' \
             (expected 'saved-state' or 'fresh-merge')"
            )
        })?;
    if policy != pdf_xfa::XfaRenderingPolicy::SavedStateFaithful {
        eprintln!(
            "`flatten` applies the production 'saved-state' (SavedStateFaithful) \
             policy only. '{}' is experimental and not produced by `flatten` \
             (pending D13 corpus-scale validation); use \
             `pdfluent measure --policy fresh-merge` for experimental measurement. \
             SavedStateFaithful remains the default.",
            policy.as_str()
        );
        std::process::exit(2);
    }

    let pdf_bytes = std::fs::read(input).context("failed to read input PDF")?;

    // Reject non-PDF input loudly. `flatten_xfa_to_pdf` returns the input bytes
    // unchanged when it finds no PDF/XFA structure, so without this guard a
    // non-PDF file was copied through and reported as a successful flatten
    // (exit 0) — the same class of silent failure that `info`/`measure` already
    // reject. Encrypted PDFs carry a valid header and still pass here, so the
    // encrypted-skip path (exit 2) below stays authoritative.
    if !has_pdf_header(&pdf_bytes) {
        anyhow::bail!(
            "input is not a PDF file (missing %PDF header): {}",
            input.display()
        );
    }

    match dump_layout {
        Some(dump_path) => match pdf_xfa::flatten_xfa_to_pdf_with_layout_dump(&pdf_bytes) {
            Ok((flattened_bytes, layout_dump)) => {
                std::fs::write(output, &flattened_bytes).context("failed to write output PDF")?;
                write_layout_dump(dump_path, layout_dump)?;
                println!("Flattened XFA/AcroForm -> {}", output.display());
            }
            Err(pdf_xfa::error::XfaError::Encrypted(msg)) => {
                eprintln!("SKIP: encrypted PDF — {msg}");
                std::process::exit(2);
            }
            Err(e) => {
                eprintln!("XFA flatten failed: {e:?}");
                // Fallback to regular acroform flatten
                let mut doc =
                    lopdf::Document::load_mem(&pdf_bytes).context("failed to parse PDF")?;
                let removed = flatten_acroform(&mut doc);
                doc.save(output).context("failed to save output PDF")?;
                write_layout_dump(dump_path, pdf_xfa::LayoutDump::default())?;
                println!("Flattened {removed} form fields -> {}", output.display());
            }
        },
        None => match pdf_xfa::flatten_xfa_to_pdf(&pdf_bytes) {
            Ok(flattened_bytes) => {
                std::fs::write(output, &flattened_bytes).context("failed to write output PDF")?;
                println!("Flattened XFA/AcroForm -> {}", output.display());
            }
            Err(pdf_xfa::error::XfaError::Encrypted(msg)) => {
                eprintln!("SKIP: encrypted PDF — {msg}");
                std::process::exit(2);
            }
            Err(e) => {
                eprintln!("XFA flatten failed: {e:?}");
                // Fallback to regular acroform flatten
                let mut doc =
                    lopdf::Document::load_mem(&pdf_bytes).context("failed to parse PDF")?;
                let removed = flatten_acroform(&mut doc);
                doc.save(output).context("failed to save output PDF")?;
                println!("Flattened {removed} form fields -> {}", output.display());
            }
        },
    }
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
    // Annots can be an inline array or an indirect reference to an array.
    let page_ids: Vec<lopdf::ObjectId> = doc.page_iter().collect();
    for page_id in page_ids {
        let annots_ref = {
            let page_dict = match doc.get_dictionary(page_id) {
                Ok(d) => d,
                Err(_) => continue,
            };
            match page_dict.get(b"Annots") {
                Ok(lopdf::Object::Reference(r)) => Some(*r),
                Ok(lopdf::Object::Array(_)) => None,
                _ => continue,
            }
        };

        // Resolve the array (inline or indirect)
        let arr = if let Some(ref_id) = annots_ref {
            match doc.get_object(ref_id) {
                Ok(lopdf::Object::Array(arr)) => arr.clone(),
                _ => continue,
            }
        } else {
            match doc.get_dictionary(page_id) {
                Ok(d) => match d.get(b"Annots") {
                    Ok(lopdf::Object::Array(arr)) => arr.clone(),
                    _ => continue,
                },
                Err(_) => continue,
            }
        };

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

        // Write back: update indirect object or inline array
        if let Some(ref_id) = annots_ref {
            doc.objects.insert(ref_id, lopdf::Object::Array(filtered));
        } else if let Ok(lopdf::Object::Dictionary(ref mut dict)) = doc.get_object_mut(page_id) {
            dict.set("Annots", lopdf::Object::Array(filtered));
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

/// Returns `true` when `bytes` carries a PDF signature (`%PDF-`) within the
/// first kibibyte — the tolerant window Adobe-class readers scan for the file
/// header. Used to reject non-PDF input before flattening.
fn has_pdf_header(bytes: &[u8]) -> bool {
    const SIGNATURE: &[u8] = b"%PDF-";
    const SCAN_WINDOW: usize = 1024;
    let scan = &bytes[..bytes.len().min(SCAN_WINDOW)];
    scan.windows(SIGNATURE.len())
        .any(|window| window == SIGNATURE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_pdf_bytes() {
        assert!(!has_pdf_header(b"not a pdf"));
        assert!(!has_pdf_header(b""));
        assert!(!has_pdf_header(b"%PD"));
    }

    #[test]
    fn accepts_pdf_header_at_start() {
        assert!(has_pdf_header(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n"));
    }

    #[test]
    fn accepts_pdf_header_within_scan_window() {
        let mut data = vec![b' '; 100];
        data.extend_from_slice(b"%PDF-1.4");
        assert!(has_pdf_header(&data));
    }

    #[test]
    fn ignores_header_past_scan_window() {
        let mut data = vec![b'\n'; 2000];
        data.extend_from_slice(b"%PDF-1.4");
        assert!(!has_pdf_header(&data));
    }
}
