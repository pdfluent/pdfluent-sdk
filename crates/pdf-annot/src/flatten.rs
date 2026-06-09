#![cfg(feature = "write")]
//! Annotation flattening implementation.
//!
//! Flattens non-widget, non-link annotations containing appearance streams (/AP /N)
//! directly into the page content stream.

use lopdf::{dictionary, Object, ObjectId, Stream};
use std::collections::HashSet;

/// Convert a PDF object to a f64 number.
fn as_number(obj: &Object) -> Option<f64> {
    match obj {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(f) => Some(*f as f64),
        _ => None,
    }
}

/// Walk parent Pages chain to resolve inherited Resources dictionary.
fn resolve_inherited_resources_clone(
    doc: &lopdf::Document,
    page_id: ObjectId,
) -> Option<lopdf::Dictionary> {
    let mut current = page_id;
    let mut visited = HashSet::new();
    loop {
        if !visited.insert(current) {
            return None;
        }
        let dict = doc.get_dictionary(current).ok()?;
        if let Ok(res_obj) = dict.get(b"Resources") {
            return match res_obj {
                Object::Dictionary(d) => Some(d.clone()),
                Object::Reference(id) => doc.get_dictionary(*id).ok().cloned(),
                _ => None,
            };
        }
        match dict.get(b"Parent") {
            Ok(Object::Reference(id)) => current = *id,
            _ => return None,
        }
    }
}

/// Resolve page /Contents to a flat list of stream ObjectIds.
fn resolve_content_streams(doc: &lopdf::Document, page_id: ObjectId) -> Vec<ObjectId> {
    let page_obj = match doc.get_object(page_id) {
        Ok(obj) => obj,
        Err(_) => return Vec::new(),
    };
    let page_dict = match page_obj {
        Object::Dictionary(ref d) => d,
        _ => return Vec::new(),
    };
    match page_dict.get(b"Contents").ok() {
        Some(c) => flatten_content_refs(doc, c),
        None => Vec::new(),
    }
}

fn flatten_content_refs(doc: &lopdf::Document, obj: &Object) -> Vec<ObjectId> {
    match obj {
        Object::Reference(id) => {
            if let Ok(Object::Array(arr)) = doc.get_object(*id) {
                return arr
                    .iter()
                    .flat_map(|o| flatten_content_refs(doc, o))
                    .collect();
            }
            vec![*id]
        }
        Object::Array(arr) => arr
            .iter()
            .flat_map(|o| flatten_content_refs(doc, o))
            .collect(),
        _ => Vec::new(),
    }
}

/// Wrap existing page content in q/Q to isolate graphics state.
fn wrap_existing_content_in_save_restore(doc: &mut lopdf::Document, page_id: ObjectId) {
    let existing = resolve_content_streams(doc, page_id);
    if existing.is_empty() {
        return;
    }

    let q_stream = Stream::new(dictionary! {}, b"q\n".to_vec());
    let q_id = doc.add_object(Object::Stream(q_stream));

    let big_q_stream = Stream::new(dictionary! {}, b"\nQ\n".to_vec());
    let big_q_id = doc.add_object(Object::Stream(big_q_stream));

    // Build flat array: [q, ...existing streams..., Q]
    let mut new_arr = Vec::with_capacity(existing.len() + 2);
    new_arr.push(Object::Reference(q_id));
    for id in existing {
        new_arr.push(Object::Reference(id));
    }
    new_arr.push(Object::Reference(big_q_id));
    let wrapped = Object::Array(new_arr);

    if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
        d.set("Contents", wrapped);
    }
}

/// Append a content stream reference to a page's Contents.
fn append_content_to_page(doc: &mut lopdf::Document, page_id: ObjectId, content_id: ObjectId) {
    let existing = resolve_content_streams(doc, page_id);
    let new_contents = if existing.is_empty() {
        Object::Reference(content_id)
    } else {
        let mut arr: Vec<Object> = existing.into_iter().map(Object::Reference).collect();
        arr.push(Object::Reference(content_id));
        Object::Array(arr)
    };

    if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
        d.set("Contents", new_contents);
    }
}

/// Compute the coordinate transformation mapping appearance BBox to Rect.
fn get_matrix_and_bbox(n_stream_dict: &lopdf::Dictionary, rect: &[f64; 4]) -> Option<[f64; 6]> {
    let bbox = n_stream_dict.get(b"BBox").ok()?;
    let bbox_arr = match bbox {
        Object::Array(ref arr) if arr.len() >= 4 => arr,
        _ => return None,
    };
    let bx0 = as_number(&bbox_arr[0])?;
    let by0 = as_number(&bbox_arr[1])?;
    let bx1 = as_number(&bbox_arr[2])?;
    let by1 = as_number(&bbox_arr[3])?;

    let bw = bx1 - bx0;
    let bh = by1 - by0;
    if bw == 0.0 || bh == 0.0 {
        return None;
    }

    let rx0 = rect[0];
    let ry0 = rect[1];
    let rx1 = rect[2];
    let ry1 = rect[3];
    let rw = rx1 - rx0;
    let rh = ry1 - ry0;

    // Optional Matrix in appearance stream
    let matrix_arr = n_stream_dict.get(b"Matrix").ok().and_then(|m| match m {
        Object::Array(ref arr) if arr.len() >= 6 => Some(arr),
        _ => None,
    });

    if let Some(m) = matrix_arr {
        let ma = as_number(&m[0]).unwrap_or(1.0);
        let mb = as_number(&m[1]).unwrap_or(0.0);
        let mc = as_number(&m[2]).unwrap_or(0.0);
        let md = as_number(&m[3]).unwrap_or(1.0);
        let me = as_number(&m[4]).unwrap_or(0.0);
        let mf = as_number(&m[5]).unwrap_or(0.0);

        let corners = [(bx0, by0), (bx1, by0), (bx0, by1), (bx1, by1)];
        let mut t_corners = [(0.0, 0.0); 4];
        for (i, &(x, y)) in corners.iter().enumerate() {
            t_corners[i] = (ma * x + mc * y + me, mb * x + md * y + mf);
        }
        let t_bx0 = t_corners.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
        let t_by0 = t_corners.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
        let t_bx1 = t_corners
            .iter()
            .map(|p| p.0)
            .fold(f64::NEG_INFINITY, f64::max);
        let t_by1 = t_corners
            .iter()
            .map(|p| p.1)
            .fold(f64::NEG_INFINITY, f64::max);

        let t_bw = t_bx1 - t_bx0;
        let t_bh = t_by1 - t_by0;
        if t_bw == 0.0 || t_bh == 0.0 {
            return None;
        }

        let sx = rw / t_bw;
        let sy = rh / t_bh;
        let tx = rx0 - t_bx0 * sx;
        let ty = ry0 - t_by0 * sy;

        Some([
            ma * sx,
            mb * sy,
            mc * sx,
            md * sy,
            me * sx + tx,
            mf * sy + ty,
        ])
    } else {
        let sx = rw / bw;
        let sy = rh / bh;
        let tx = rx0 - bx0 * sx;
        let ty = ry0 - by0 * sy;
        Some([sx, 0.0, 0.0, sy, tx, ty])
    }
}

/// Flatten all non-widget, non-link annotations on all pages.
///
/// Reference integrity: when a markup annotation is flattened, its associated
/// `/Popup` companion annotation is removed as well, so no `/Parent` reference
/// is left dangling. Cross-annotation `/IRT` (reply threads) and structure-tree
/// `OBJR` references that point at a flattened annotation are NOT rewritten;
/// per ISO 32000-1 §7.3.10 a reference to a removed object resolves to the null
/// object, which conforming readers tolerate. Rewriting those references is
/// intentionally out of scope (tracked as a follow-up).
pub fn flatten_annotations(doc: &mut lopdf::Document) -> Result<(), crate::error::AnnotBuildError> {
    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let mut deleted_objects = HashSet::new();
    // Popup companions of flattened markup annotations. Removed alongside their
    // parent so the popup's /Parent back-reference does not dangle.
    let mut popups_to_remove: HashSet<ObjectId> = HashSet::new();

    for page_id in page_ids {
        // Retrieve /Annots array
        let page_dict = match doc.get_object(page_id) {
            Ok(Object::Dictionary(ref d)) => d.clone(),
            _ => continue,
        };

        let annots_arr = match page_dict.get(b"Annots") {
            Ok(Object::Array(ref arr)) => arr.clone(),
            Ok(Object::Reference(id)) => match doc.get_object(*id) {
                Ok(Object::Array(ref arr)) => arr.clone(),
                _ => continue,
            },
            _ => continue,
        };

        if annots_arr.is_empty() {
            continue;
        }

        let mut remaining_annots = Vec::new();
        let mut draw_ops = Vec::new();

        let mut resources = if let Ok(res_obj) = page_dict.get(b"Resources") {
            match res_obj {
                Object::Dictionary(ref d) => d.clone(),
                Object::Reference(id) => match doc.get_object(*id) {
                    Ok(Object::Dictionary(ref d)) => d.clone(),
                    _ => lopdf::Dictionary::new(),
                },
                _ => lopdf::Dictionary::new(),
            }
        } else {
            resolve_inherited_resources_clone(doc, page_id).unwrap_or_default()
        };

        let mut xobjects = match resources.get(b"XObject") {
            Ok(Object::Dictionary(ref d)) => d.clone(),
            Ok(Object::Reference(id)) => match doc.get_object(*id) {
                Ok(Object::Dictionary(ref d)) => d.clone(),
                _ => lopdf::Dictionary::new(),
            },
            _ => lopdf::Dictionary::new(),
        };

        let mut flat_annot_count = 0;

        for annot_obj in &annots_arr {
            let annot_id = match annot_obj {
                Object::Reference(id) => *id,
                _ => {
                    remaining_annots.push(annot_obj.clone());
                    continue;
                }
            };

            let annot_dict = match doc.get_object(annot_id) {
                Ok(Object::Dictionary(ref d)) => d.clone(),
                _ => {
                    remaining_annots.push(annot_obj.clone());
                    continue;
                }
            };

            let subtype = match annot_dict.get(b"Subtype") {
                Ok(Object::Name(ref name)) => name.as_slice(),
                _ => {
                    remaining_annots.push(annot_obj.clone());
                    continue;
                }
            };

            // Skip Widget and Link annotations
            if subtype == b"Widget" || subtype == b"Link" {
                remaining_annots.push(annot_obj.clone());
                continue;
            }

            // Must have appearance dictionary /AP with normal appearance /N
            let ap = match annot_dict.get(b"AP") {
                Ok(Object::Dictionary(ref d)) => d.clone(),
                Ok(Object::Reference(id)) => match doc.get_object(*id) {
                    Ok(Object::Dictionary(ref d)) => d.clone(),
                    _ => {
                        remaining_annots.push(annot_obj.clone());
                        continue;
                    }
                },
                _ => {
                    remaining_annots.push(annot_obj.clone());
                    continue;
                }
            };

            let n_obj = match ap.get(b"N") {
                Ok(obj) => obj.clone(),
                _ => {
                    remaining_annots.push(annot_obj.clone());
                    continue;
                }
            };

            let (n_stream_id, n_stream) = match n_obj {
                Object::Reference(id) => match doc.get_object(id) {
                    Ok(Object::Stream(ref s)) => (Some(id), s.clone()),
                    _ => {
                        remaining_annots.push(annot_obj.clone());
                        continue;
                    }
                },
                Object::Stream(ref s) => (None, s.clone()),
                _ => {
                    remaining_annots.push(annot_obj.clone());
                    continue;
                }
            };

            // Get Rect
            let rect_obj = match annot_dict.get(b"Rect") {
                Ok(Object::Array(ref arr)) if arr.len() >= 4 => arr,
                _ => {
                    remaining_annots.push(annot_obj.clone());
                    continue;
                }
            };
            let rx0 = match as_number(&rect_obj[0]) {
                Some(v) => v,
                None => {
                    remaining_annots.push(annot_obj.clone());
                    continue;
                }
            };
            let ry0 = match as_number(&rect_obj[1]) {
                Some(v) => v,
                None => {
                    remaining_annots.push(annot_obj.clone());
                    continue;
                }
            };
            let rx1 = match as_number(&rect_obj[2]) {
                Some(v) => v,
                None => {
                    remaining_annots.push(annot_obj.clone());
                    continue;
                }
            };
            let ry1 = match as_number(&rect_obj[3]) {
                Some(v) => v,
                None => {
                    remaining_annots.push(annot_obj.clone());
                    continue;
                }
            };
            let rect = [rx0, ry0, rx1, ry1];

            // Compute matrix
            let matrix = match get_matrix_and_bbox(&n_stream.dict, &rect) {
                Some(m) => m,
                None => {
                    remaining_annots.push(annot_obj.clone());
                    continue;
                }
            };

            // Set Form XObject type/subtype
            let stream_id = match n_stream_id {
                Some(id) => {
                    if let Ok(Object::Stream(ref mut s)) = doc.get_object_mut(id) {
                        s.dict.set("Type", Object::Name(b"XObject".to_vec()));
                        s.dict.set("Subtype", Object::Name(b"Form".to_vec()));
                    }
                    id
                }
                None => {
                    let mut s = n_stream.clone();
                    s.dict.set("Type", Object::Name(b"XObject".to_vec()));
                    s.dict.set("Subtype", Object::Name(b"Form".to_vec()));
                    doc.add_object(Object::Stream(s))
                }
            };

            // Insert into Resources /XObject
            let ap_name = format!("FlatAnnot{}", flat_annot_count);
            flat_annot_count += 1;
            xobjects.set(ap_name.as_bytes().to_vec(), Object::Reference(stream_id));

            // Append Do operator
            let draw_op = format!(
                "q\n{} {} {} {} {} {} cm\n/{} Do\nQ\n",
                matrix[0], matrix[1], matrix[2], matrix[3], matrix[4], matrix[5], ap_name
            );
            draw_ops.extend_from_slice(draw_op.as_bytes());

            deleted_objects.insert(annot_id);
            // Mark the markup annotation's popup companion for removal so its
            // /Parent back-reference does not dangle after flattening.
            if let Ok(Object::Reference(popup_id)) = annot_dict.get(b"Popup") {
                popups_to_remove.insert(*popup_id);
            }
        }

        if !draw_ops.is_empty() {
            // Write /XObject resources back to resources dict
            resources.set("XObject", Object::Dictionary(xobjects));

            if let Ok(Object::Dictionary(ref mut pd)) = doc.get_object_mut(page_id) {
                pd.set("Resources", Object::Dictionary(resources));
            }

            // Wrap existing page content in q/Q
            wrap_existing_content_in_save_restore(doc, page_id);

            // Create new content stream and append
            let new_content_stream = Stream::new(dictionary! {}, draw_ops);
            let new_content_id = doc.add_object(Object::Stream(new_content_stream));
            append_content_to_page(doc, page_id, new_content_id);

            // Drop popup companions of flattened annots so the array does not
            // keep an interactive popup whose /Parent now points at nothing.
            remaining_annots.retain(|o| match o {
                Object::Reference(id) => !popups_to_remove.contains(id),
                _ => true,
            });

            // Write modified /Annots array or remove it if empty
            if let Ok(Object::Dictionary(ref mut pd)) = doc.get_object_mut(page_id) {
                if remaining_annots.is_empty() {
                    pd.remove(b"Annots");
                } else {
                    pd.set("Annots", Object::Array(remaining_annots));
                }
            }
        }
    }

    // Purge the deleted annotation dictionaries and their popup companions.
    for id in deleted_objects.into_iter().chain(popups_to_remove) {
        doc.objects.remove(&id);
    }

    Ok(())
}
