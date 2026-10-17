#![cfg(feature = "write")]
//! Annotation flattening implementation.
//!
//! Flattens non-widget, non-link annotations containing appearance streams (/AP /N)
//! directly into the page content stream.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use std::collections::HashSet;

use crate::builder::{AnnotRect, AnnotSubtype, AnnotationBuilder};

/// Pick a `/XObject` resource name of the form `FlatAnnotN` that is not already
/// present in `xobjects`. `next` is advanced past the chosen index so repeated
/// calls stay monotonic. This makes flattening collision-proof: it never
/// overwrites a pre-existing XObject, and a second flatten pass on an
/// already-flattened page cannot clobber the names from the first.
fn unique_xobject_name(xobjects: &lopdf::Dictionary, next: &mut usize) -> String {
    loop {
        let candidate = format!("FlatAnnot{}", *next);
        *next += 1;
        if !xobjects.has(candidate.as_bytes()) {
            return candidate;
        }
    }
}

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

/// Whether an annotation dictionary already carries a normal appearance
/// that [`flatten_annotations`] can paint: `/AP` with a `/N` stream.
fn has_usable_appearance(doc: &Document, annot_dict: &lopdf::Dictionary) -> bool {
    let ap = match annot_dict.get(b"AP") {
        Ok(Object::Dictionary(d)) => d.clone(),
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => return false,
        },
        _ => return false,
    };
    match ap.get(b"N") {
        Ok(Object::Stream(_)) => true,
        Ok(Object::Reference(id)) => {
            matches!(doc.get_object(*id), Ok(Object::Stream(_)))
        }
        _ => false,
    }
}

/// Read `/C` or `/IC` as an RGB triple: grey repeats, CMYK converts naively.
fn annot_rgb(value: &Object) -> Option<(f64, f64, f64)> {
    let arr = value.as_array().ok()?;
    let nums: Vec<f64> = arr.iter().filter_map(as_number).collect();
    match nums.as_slice() {
        [g] => Some((*g, *g, *g)),
        [r, g, b] => Some((*r, *g, *b)),
        [c, m, y, k] => Some((
            (1.0 - (c + k).min(1.0)).max(0.0),
            (1.0 - (m + k).min(1.0)).max(0.0),
            (1.0 - (y + k).min(1.0)).max(0.0),
        )),
        _ => None,
    }
}

/// Border width from `/BS /W`, else the `/Border` array's third number.
fn annot_border_width(doc: &Document, annot_dict: &lopdf::Dictionary) -> Option<f64> {
    if let Ok(bs) = annot_dict.get(b"BS") {
        let dict = match bs {
            Object::Dictionary(d) => Some(d.clone()),
            Object::Reference(id) => doc.get_dictionary(*id).ok().cloned(),
            _ => None,
        };
        if let Some(d) = dict {
            if let Ok(w) = d.get(b"W") {
                if let Some(w) = as_number(w) {
                    return Some(w);
                }
            }
        }
    }
    if let Ok(Object::Array(border)) = annot_dict.get(b"Border") {
        if let Some(w) = border.get(2).and_then(as_number) {
            return Some(w);
        }
    }
    None
}

/// Dash array from `/BS /D`.
fn annot_dash(doc: &Document, annot_dict: &lopdf::Dictionary) -> Option<Vec<f64>> {
    let bs = annot_dict.get(b"BS").ok()?;
    let dict = match bs {
        Object::Dictionary(d) => d.clone(),
        Object::Reference(id) => doc.get_dictionary(*id).ok().cloned()?,
        _ => return None,
    };
    let dash = dict.get(b"D").ok()?.as_array().ok()?.clone();
    let nums: Vec<f64> = dash.iter().filter_map(as_number).collect();
    if nums.is_empty() {
        None
    } else {
        Some(nums)
    }
}

/// Text of a direct `/Contents` string.
fn annot_text(value: &Object) -> Option<String> {
    match value {
        Object::String(bytes, _) => Some(String::from_utf8_lossy(bytes).into_owned()),
        _ => None,
    }
}

/// Give every flattenable annotation without one a default appearance.
///
/// [`flatten_annotations`] only paints annotations that carry `/AP /N`; this
/// pass runs first so annotations without an appearance flatten too instead
/// of silently staying live. Widget and link annotations are left alone, as
/// are subtypes the builder cannot draw. Returns how many appearances were
/// added.
pub fn ensure_appearances(doc: &mut Document) -> Result<usize, crate::error::AnnotBuildError> {
    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let mut added = 0usize;
    for page_id in page_ids {
        let page_dict = match doc.get_object(page_id) {
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => continue,
        };
        let annots_arr = match page_dict.get(b"Annots") {
            Ok(Object::Array(arr)) => arr.clone(),
            Ok(Object::Reference(id)) => match doc.get_object(*id) {
                Ok(Object::Array(arr)) => arr.clone(),
                _ => continue,
            },
            _ => continue,
        };
        for annot_obj in &annots_arr {
            let annot_id = match annot_obj {
                Object::Reference(id) => *id,
                _ => continue,
            };
            let annot_dict = match doc.get_object(annot_id) {
                Ok(Object::Dictionary(d)) => d.clone(),
                _ => continue,
            };
            let subtype = match annot_dict.get(b"Subtype") {
                Ok(Object::Name(name)) => String::from_utf8_lossy(name).into_owned(),
                _ => continue,
            };
            if subtype == "Widget" || subtype == "Link" {
                continue;
            }
            let parsed = match AnnotSubtype::from_name(&subtype) {
                Some(found) => found,
                None => continue,
            };
            if has_usable_appearance(doc, &annot_dict) {
                continue;
            }
            let rect = match annot_dict.get(b"Rect") {
                Ok(Object::Array(arr)) if arr.len() >= 4 => [
                    as_number(&arr[0]),
                    as_number(&arr[1]),
                    as_number(&arr[2]),
                    as_number(&arr[3]),
                ],
                _ => continue,
            };
            let [Some(x0), Some(y0), Some(x1), Some(y1)] = rect else {
                continue;
            };
            if (x1 - x0).abs() < f64::EPSILON || (y1 - y0).abs() < f64::EPSILON {
                continue;
            }
            let rect = AnnotRect::new(x0, y0, x1, y1);
            let mut builder = match parsed {
                AnnotSubtype::Ink => {
                    let strokes = match annot_dict.get(b"InkList") {
                        Ok(Object::Array(paths)) => paths
                            .iter()
                            .filter_map(|p| p.as_array().ok())
                            .map(|a| a.iter().filter_map(as_number).collect::<Vec<_>>())
                            .filter(|p| p.len() >= 2)
                            .collect(),
                        _ => Vec::new(),
                    };
                    AnnotationBuilder::ink(rect, strokes)
                }
                AnnotSubtype::Polygon => {
                    let verts = match annot_dict.get(b"Vertices") {
                        Ok(Object::Array(arr)) => {
                            arr.iter().filter_map(as_number).collect::<Vec<_>>()
                        }
                        _ => Vec::new(),
                    };
                    AnnotationBuilder::polygon(rect, verts)
                }
                AnnotSubtype::PolyLine => {
                    let verts = match annot_dict.get(b"Vertices") {
                        Ok(Object::Array(arr)) => {
                            arr.iter().filter_map(as_number).collect::<Vec<_>>()
                        }
                        _ => Vec::new(),
                    };
                    AnnotationBuilder::polyline(rect, verts)
                }
                other => AnnotationBuilder::new(other, rect),
            };
            if parsed == AnnotSubtype::Line {
                if let Ok(Object::Array(line)) = annot_dict.get(b"L") {
                    if line.len() >= 4 {
                        let ends = [
                            as_number(&line[0]),
                            as_number(&line[1]),
                            as_number(&line[2]),
                            as_number(&line[3]),
                        ];
                        if let [Some(x1), Some(y1), Some(x2), Some(y2)] = ends {
                            builder.set_line_endpoints([x1, y1, x2, y2]);
                        }
                    }
                }
            }
            if let Ok(color) = annot_dict.get(b"C") {
                if let Some((r, g, b)) = annot_rgb(color) {
                    builder = builder.color(r, g, b);
                }
            }
            if let Ok(ic) = annot_dict.get(b"IC") {
                if let Some((r, g, b)) = annot_rgb(ic) {
                    builder = builder.interior_color(r, g, b);
                }
            }
            if let Ok(ca) = annot_dict.get(b"CA") {
                if let Some(alpha) = as_number(ca) {
                    builder = builder.opacity(alpha);
                }
            }
            if let Some(w) = annot_border_width(doc, &annot_dict) {
                builder = builder.border_width(w);
            }
            if let Some(dash) = annot_dash(doc, &annot_dict) {
                builder = builder.dash(dash);
            }
            if let Ok(contents) = annot_dict.get(b"Contents") {
                if let Some(text) = annot_text(contents) {
                    builder = builder.contents(text);
                }
            }
            if let Ok(Object::Integer(q)) = annot_dict.get(b"Q") {
                builder = builder.alignment(*q);
            }
            if matches!(parsed, AnnotSubtype::Stamp | AnnotSubtype::Text) {
                if let Ok(Object::Name(name)) = annot_dict.get(b"Name") {
                    builder.set_icon_name(String::from_utf8_lossy(name).into_owned());
                }
            }
            if let Ok(Object::Array(qp)) = annot_dict.get(b"QuadPoints") {
                let nums: Vec<f64> = qp.iter().filter_map(as_number).collect();
                if !nums.is_empty() {
                    builder = builder.quad_points(nums);
                }
            }
            let ap_id = match builder.appearance_stream(doc) {
                Ok(id) => id,
                Err(crate::error::AnnotBuildError::InvalidRect) => continue,
                Err(other) => return Err(other),
            };
            // Keep any existing /R or /D entries beside the new /N.
            let ap_ref = Object::Reference(ap_id);
            match annot_dict.get(b"AP").cloned() {
                Ok(Object::Dictionary(mut ap)) => {
                    ap.set("N", ap_ref);
                    if let Ok(Object::Dictionary(d)) = doc.get_object_mut(annot_id) {
                        d.set("AP", Object::Dictionary(ap));
                        added += 1;
                    }
                }
                Ok(Object::Reference(id)) => {
                    if let Ok(Object::Dictionary(ap)) = doc.get_object_mut(id) {
                        ap.set("N", ap_ref);
                        added += 1;
                    } else if let Ok(Object::Dictionary(d)) = doc.get_object_mut(annot_id) {
                        let ap = dictionary! { "N" => ap_ref };
                        d.set("AP", Object::Dictionary(ap));
                        added += 1;
                    }
                }
                _ => {
                    if let Ok(Object::Dictionary(d)) = doc.get_object_mut(annot_id) {
                        let ap = dictionary! { "N" => ap_ref };
                        d.set("AP", Object::Dictionary(ap));
                        added += 1;
                    }
                }
            }
        }
    }
    Ok(added)
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

            // Insert into Resources /XObject under a name that does not collide
            // with any existing entry (including names left by a previous flatten
            // pass), so flattening never overwrites an unrelated XObject.
            let ap_name = unique_xobject_name(&xobjects, &mut flat_annot_count);
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
    // `delete_object` also strips every remaining reference to each id across the
    // whole document — page `/Annots`, `/IRT` reply links, structure-tree `OBJR`
    // `/Obj` entries, and `/Popup` back-references — so no dangling reference to a
    // flattened annotation survives. (Cost is O(deleted × objects); flattening is
    // not a hot path.)
    for id in deleted_objects.into_iter().chain(popups_to_remove) {
        doc.delete_object(id);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ensure_appearances, flatten_annotations, unique_xobject_name};
    use lopdf::Dictionary;
    use lopdf::{dictionary, Document, Object, ObjectId, Stream};

    #[test]
    fn unique_xobject_name_skips_existing_entries() {
        let mut xobjects = Dictionary::new();
        xobjects.set("FlatAnnot0", Object::Null);
        xobjects.set("FlatAnnot1", Object::Null);

        // FlatAnnot0/1 already exist, so the first free name is FlatAnnot2.
        let mut next = 0usize;
        assert_eq!(unique_xobject_name(&xobjects, &mut next), "FlatAnnot2");

        // After registering it, the next call stays monotonic and skips it.
        xobjects.set("FlatAnnot2", Object::Null);
        assert_eq!(unique_xobject_name(&xobjects, &mut next), "FlatAnnot3");
    }

    #[test]
    fn unique_xobject_name_empty_dict_starts_at_zero() {
        let xobjects = Dictionary::new();
        let mut next = 0usize;
        assert_eq!(unique_xobject_name(&xobjects, &mut next), "FlatAnnot0");
        assert_eq!(unique_xobject_name(&xobjects, &mut next), "FlatAnnot1");
    }

    fn one_page_doc() -> (Document, ObjectId) {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let content_id = doc.add_object(Stream::new(dictionary! {}, b"".to_vec()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! {},
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page_id)],
                "Count" => 1_i64,
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        (doc, page_id)
    }

    fn add_annot(doc: &mut Document, page_id: ObjectId, subtype: &str, with_ap: bool) -> ObjectId {
        let mut dict = dictionary! {
            "Type" => "Annot",
            "Subtype" => Object::Name(subtype.as_bytes().to_vec()),
            "Rect" => vec![10.into(), 10.into(), 110.into(), 60.into()],
        };
        if with_ap {
            let ap = Stream::new(
                dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Form",
                    "BBox" => vec![0.into(), 0.into(), 100.into(), 50.into()],
                },
                b"0 0 1 RG 0 0 100 50 re S".to_vec(),
            );
            let ap_id = doc.add_object(Object::Stream(ap));
            dict.set(
                "AP",
                Object::Dictionary(dictionary! { "N" => Object::Reference(ap_id) }),
            );
        }
        let id = doc.add_object(Object::Dictionary(dict));
        let page = doc.get_dictionary_mut(page_id).unwrap();
        match page.get(b"Annots").cloned() {
            Ok(Object::Array(mut arr)) => {
                arr.push(Object::Reference(id));
                page.set("Annots", Object::Array(arr));
            }
            _ => {
                page.set("Annots", Object::Array(vec![Object::Reference(id)]));
            }
        }
        id
    }

    fn ap_normal(doc: &Document, annot_id: ObjectId) -> Option<ObjectId> {
        let dict = doc.get_dictionary(annot_id).ok()?;
        let ap = dict.get(b"AP").ok()?.clone();
        let ap_dict = match ap {
            Object::Dictionary(d) => d,
            Object::Reference(id) => doc.get_dictionary(id).ok()?.clone(),
            _ => return None,
        };
        match ap_dict.get(b"N").ok()?.clone() {
            Object::Reference(id) => Some(id),
            _ => None,
        }
    }

    #[test]
    fn ensure_appearances_backfills_only_what_flatten_paints() {
        let (mut doc, page_id) = one_page_doc();
        let square = add_annot(&mut doc, page_id, "Square", false);
        let highlight = add_annot(&mut doc, page_id, "Highlight", false);
        let kept = add_annot(&mut doc, page_id, "Square", true);
        let kept_ap = ap_normal(&doc, kept).expect("fixture has an appearance");
        let link = add_annot(&mut doc, page_id, "Link", false);
        let widget = add_annot(&mut doc, page_id, "Widget", false);
        let sound = add_annot(&mut doc, page_id, "Sound", false);

        assert_eq!(ensure_appearances(&mut doc).unwrap(), 2);
        assert!(ap_normal(&doc, square).is_some());
        assert!(ap_normal(&doc, highlight).is_some());
        // Untouched: existing appearance, link, widget, undrawable subtype.
        assert_eq!(ap_normal(&doc, kept), Some(kept_ap));
        assert_eq!(ap_normal(&doc, link), None);
        assert_eq!(ap_normal(&doc, widget), None);
        assert_eq!(ap_normal(&doc, sound), None);
    }

    #[test]
    fn backfilled_appearances_flatten_away() {
        let (mut doc, page_id) = one_page_doc();
        add_annot(&mut doc, page_id, "Circle", false);
        add_annot(&mut doc, page_id, "Link", false);

        assert_eq!(ensure_appearances(&mut doc).unwrap(), 1);
        flatten_annotations(&mut doc).unwrap();

        let page = doc.get_dictionary(page_id).unwrap();
        let annots = page.get(b"Annots").unwrap().as_array().unwrap().clone();
        assert_eq!(annots.len(), 1);
        let survivor = doc
            .get_dictionary(annots[0].as_reference().unwrap())
            .unwrap();
        assert_eq!(
            survivor.get(b"Subtype").unwrap().as_name().unwrap(),
            b"Link"
        );
    }
}
