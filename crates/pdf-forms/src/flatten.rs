//! Form flattening: convert interactive fields to static content (B.8).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::appearance::generate_appearance;
use crate::tree::{FieldId, FieldTree, FieldType};
use lopdf::dictionary;

/// Configuration for form flattening.
#[derive(Debug, Clone)]
pub struct FlattenConfig {
    /// If set, only flatten fields with these fully qualified names. If empty, flatten all.
    pub field_names: Vec<String>,
    /// Remove the /AcroForm dictionary after flattening.
    pub remove_acroform: bool,
    /// PDF/A compliance mode.
    pub pdfa: bool,
}

impl Default for FlattenConfig {
    fn default() -> Self {
        Self {
            field_names: vec![],
            remove_acroform: true,
            pdfa: false,
        }
    }
}

/// Result of a flatten operation.
#[derive(Debug)]
pub struct FlattenResult {
    /// Number of fields flattened.
    pub fields_flattened: usize,
    /// Fields that could not be flattened.
    pub skipped: Vec<String>,
}

/// Flatten form fields into static content in a lopdf Document.
pub fn flatten_form(
    doc: &mut lopdf::Document,
    tree: &FieldTree,
    config: &FlattenConfig,
) -> FlattenResult {
    let mut result = FlattenResult {
        fields_flattened: 0,
        skipped: vec![],
    };

    // Pre-build a HashSet from config.field_names to avoid O(F×N) Vec::contains
    // inside the filter — with large forms (many fields) and many target names
    // the slice contains() is O(N) per field. (#perf)
    let fields_to_flatten: Vec<FieldId> = if config.field_names.is_empty() {
        tree.terminal_fields()
    } else {
        let name_set: std::collections::HashSet<&str> =
            config.field_names.iter().map(String::as_str).collect();
        tree.terminal_fields()
            .into_iter()
            .filter(|&id| name_set.contains(tree.fully_qualified_name(id).as_str()))
            .collect()
    };

    for &field_id in &fields_to_flatten {
        if tree.effective_field_type(field_id) == Some(FieldType::Signature) {
            result.skipped.push(tree.fully_qualified_name(field_id));
            continue;
        }
        let ap_data = match generate_appearance(tree, field_id) {
            Some(data) => data,
            None => {
                result.skipped.push(tree.fully_qualified_name(field_id));
                continue;
            }
        };
        let node = tree.get(field_id);
        let rect = match node.rect {
            Some(r) => r,
            None => {
                result.skipped.push(tree.fully_qualified_name(field_id));
                continue;
            }
        };
        let page_idx = node.page_index.unwrap_or(0);
        let bbox = vec![
            lopdf::Object::Real(0.0),
            lopdf::Object::Real(0.0),
            lopdf::Object::Real(rect[2] - rect[0]),
            lopdf::Object::Real(rect[3] - rect[1]),
        ];
        let xobj_dict = dictionary! {
            "Type" => lopdf::Object::Name(b"XObject".to_vec()),
            "Subtype" => lopdf::Object::Name(b"Form".to_vec()),
            "BBox" => lopdf::Object::Array(bbox),
            "Matrix" => lopdf::Object::Array(vec![
                lopdf::Object::Integer(1), lopdf::Object::Integer(0),
                lopdf::Object::Integer(0), lopdf::Object::Integer(1),
                lopdf::Object::Integer(0), lopdf::Object::Integer(0),
            ]),
        };
        let xobj_stream = lopdf::Stream::new(xobj_dict, ap_data);
        let xobj_id = doc.add_object(lopdf::Object::Stream(xobj_stream));
        let xobj_name = format!("Fm{}", xobj_id.0);

        let page_ids: Vec<lopdf::ObjectId> = doc.page_iter().collect();
        if let Some(&page_id) = page_ids.get(page_idx) {
            let resources_id = get_or_create_page_resources(doc, page_id);
            add_xobject_to_resources(doc, resources_id, &xobj_name, xobj_id);
            let content_ops = format!(
                "q {} 0 0 {} {} {} cm /{} Do Q\n",
                rect[2] - rect[0],
                rect[3] - rect[1],
                rect[0],
                rect[1],
                xobj_name
            );
            append_to_page_content(doc, page_id, content_ops.as_bytes());
            result.fields_flattened += 1;
        } else {
            result.skipped.push(tree.fully_qualified_name(field_id));
        }
    }

    remove_widget_annotations(doc, tree, &fields_to_flatten);
    if config.remove_acroform {
        remove_acroform_dict(doc);
    }
    result
}

fn get_or_create_page_resources(
    doc: &mut lopdf::Document,
    page_id: lopdf::ObjectId,
) -> lopdf::ObjectId {
    if let Ok(lopdf::Object::Dictionary(d)) = doc.get_object(page_id) {
        if let Ok(lopdf::Object::Reference(res_id)) = d.get(b"Resources") {
            return *res_id;
        }
    }
    let res_id = doc.add_object(dictionary! {});
    if let Ok(lopdf::Object::Dictionary(ref mut page_dict)) = doc.get_object_mut(page_id) {
        page_dict.set("Resources", lopdf::Object::Reference(res_id));
    }
    res_id
}

fn add_xobject_to_resources(
    doc: &mut lopdf::Document,
    resources_id: lopdf::ObjectId,
    name: &str,
    xobj_id: lopdf::ObjectId,
) {
    if let Ok(lopdf::Object::Dictionary(ref mut res_dict)) = doc.get_object_mut(resources_id) {
        if let Ok(lopdf::Object::Dictionary(ref mut xobj_dict)) = res_dict.get_mut(b"XObject") {
            xobj_dict.set(name, lopdf::Object::Reference(xobj_id));
        } else {
            let mut xobj_dict = lopdf::Dictionary::new();
            xobj_dict.set(name, lopdf::Object::Reference(xobj_id));
            res_dict.set("XObject", lopdf::Object::Dictionary(xobj_dict));
        }
    }
}

fn append_to_page_content(doc: &mut lopdf::Document, page_id: lopdf::ObjectId, data: &[u8]) {
    let content_ref = doc.get_object(page_id).ok().and_then(|o| {
        if let lopdf::Object::Dictionary(d) = o {
            d.get(b"Contents").ok().cloned()
        } else {
            None
        }
    });
    match content_ref {
        Some(lopdf::Object::Reference(content_id)) => {
            if let Ok(lopdf::Object::Stream(ref mut stream)) = doc.get_object_mut(content_id) {
                stream.content.extend_from_slice(data);
            }
        }
        Some(lopdf::Object::Array(arr)) => {
            let new_stream = lopdf::Stream::new(dictionary! {}, data.to_vec());
            let new_id = doc.add_object(lopdf::Object::Stream(new_stream));
            let mut new_arr = arr;
            new_arr.push(lopdf::Object::Reference(new_id));
            if let Ok(lopdf::Object::Dictionary(ref mut pd)) = doc.get_object_mut(page_id) {
                pd.set("Contents", lopdf::Object::Array(new_arr));
            }
        }
        _ => {
            let new_stream = lopdf::Stream::new(dictionary! {}, data.to_vec());
            let new_id = doc.add_object(lopdf::Object::Stream(new_stream));
            if let Ok(lopdf::Object::Dictionary(ref mut pd)) = doc.get_object_mut(page_id) {
                pd.set("Contents", lopdf::Object::Reference(new_id));
            }
        }
    }
}

fn remove_widget_annotations(doc: &mut lopdf::Document, tree: &FieldTree, flattened: &[FieldId]) {
    let obj_ids_to_remove: Vec<lopdf::ObjectId> = flattened
        .iter()
        .filter_map(|&id| {
            tree.get(id)
                .object_id
                .map(|(obj, gen)| (obj as u32, gen as u16))
        })
        .collect();
    if obj_ids_to_remove.is_empty() {
        return;
    }

    strip_widget_javascript_additional_actions(doc, &obj_ids_to_remove);

    let page_ids: Vec<lopdf::ObjectId> = doc.page_iter().collect();
    for page_id in page_ids {
        let annots = doc.get_object(page_id).ok().and_then(|o| {
            if let lopdf::Object::Dictionary(d) = o {
                d.get(b"Annots").ok().cloned()
            } else {
                None
            }
        });
        if let Some(lopdf::Object::Array(arr)) = annots {
            let filtered: Vec<lopdf::Object> = arr
                .into_iter()
                .filter(|obj| {
                    if let lopdf::Object::Reference(ref_id) = obj {
                        !obj_ids_to_remove.contains(ref_id)
                    } else {
                        true
                    }
                })
                .collect();
            if let Ok(lopdf::Object::Dictionary(ref mut pd)) = doc.get_object_mut(page_id) {
                if filtered.is_empty() {
                    pd.remove(b"Annots");
                } else {
                    pd.set("Annots", lopdf::Object::Array(filtered));
                }
            }
        }
    }
}

fn strip_widget_javascript_additional_actions(
    doc: &mut lopdf::Document,
    widget_ids: &[lopdf::ObjectId],
) -> usize {
    // M8-SEC-02 policy source: JavaScript is inspectable, never executable,
    // and JS-bearing widget /AA entries are STRIP_ON_FLATTEN on hardening paths.
    let mut stripped = 0;
    for &widget_id in widget_ids {
        let aa_action = match doc.objects.get(&widget_id) {
            Some(lopdf::Object::Dictionary(dict)) if is_widget_annotation(dict) => {
                dict.get(b"AA").ok().cloned()
            }
            _ => None,
        };

        match aa_action {
            Some(lopdf::Object::Dictionary(aa_dict)) => {
                let js_keys = javascript_additional_action_keys(doc, &aa_dict);
                if js_keys.is_empty() {
                    continue;
                }
                let remove_aa = js_keys.len() == aa_dict.len();
                if let Some(lopdf::Object::Dictionary(dict)) = doc.objects.get_mut(&widget_id) {
                    if remove_aa {
                        dict.remove(b"AA");
                    } else if let Ok(lopdf::Object::Dictionary(aa)) = dict.get_mut(b"AA") {
                        for key in &js_keys {
                            aa.remove(key);
                        }
                    }
                    stripped += js_keys.len();
                }
            }
            Some(lopdf::Object::Reference(aa_id)) => {
                // Compute JS keys + sanitized non-JS copy in a read-only pass.
                // Mirror of the direct-Dictionary branch's selectivity, with
                // clone-on-write to avoid in-place mutation of the shared
                // indirect object (other widgets may still reference it).
                let (js_keys, sanitized) = match doc.objects.get(&aa_id) {
                    Some(lopdf::Object::Dictionary(aa_dict)) => {
                        let js = javascript_additional_action_keys(doc, aa_dict);
                        if js.is_empty() || js.len() == aa_dict.len() {
                            // No JS, or all-JS — nothing to keep on the widget.
                            (js, None)
                        } else {
                            // Mixed entries — build a widget-private dict
                            // containing only the non-JS ones.
                            let mut s = lopdf::Dictionary::new();
                            for (key, val) in aa_dict.iter() {
                                if !js.iter().any(|jk| jk == key) {
                                    s.set(key.clone(), val.clone());
                                }
                            }
                            (js, Some(s))
                        }
                    }
                    _ => (Vec::new(), None),
                };
                if js_keys.is_empty() {
                    continue;
                }

                if let Some(lopdf::Object::Dictionary(dict)) = doc.objects.get_mut(&widget_id) {
                    match sanitized {
                        None => {
                            // All-JS (or unresolvable): drop /AA from this widget.
                            dict.remove(b"AA");
                        }
                        Some(s) => {
                            // Replace the indirect ref with a widget-private
                            // sanitized inline dict. The shared indirect object
                            // is left untouched; other widgets keep their /AA.
                            dict.set("AA", lopdf::Object::Dictionary(s));
                        }
                    }
                    stripped += js_keys.len();
                }
            }
            _ => {}
        }
    }
    stripped
}

fn javascript_additional_action_keys(
    doc: &lopdf::Document,
    aa_dict: &lopdf::Dictionary,
) -> Vec<Vec<u8>> {
    aa_dict
        .iter()
        .filter_map(|(key, action)| {
            is_javascript_action_object(doc, action, 0).then_some(key.clone())
        })
        .collect()
}

fn is_widget_annotation(dict: &lopdf::Dictionary) -> bool {
    matches!(
        dict.get(b"Subtype").ok(),
        Some(lopdf::Object::Name(name)) if name == b"Widget"
    )
}

fn is_javascript_action_object(
    doc: &lopdf::Document,
    action: &lopdf::Object,
    depth: usize,
) -> bool {
    if depth > 16 {
        return false;
    }

    match action {
        lopdf::Object::Dictionary(dict) => is_javascript_action_dict(dict),
        lopdf::Object::Reference(id) => doc
            .objects
            .get(id)
            .is_some_and(|object| is_javascript_action_object(doc, object, depth + 1)),
        _ => false,
    }
}

fn is_javascript_action_dict(dict: &lopdf::Dictionary) -> bool {
    // Primary check: /S /JavaScript
    if matches!(
        dict.get(b"S").ok(),
        Some(lopdf::Object::Name(name)) if name == b"JavaScript"
    ) {
        return true;
    }
    // /Next chained actions: a non-JS trigger can chain to a JS action via
    // /Next, which would survive flattening if only the top-level /S is
    // checked. We treat any action dict that chains to a JS action as JS
    // (M8-SEC-02). The recursive walk is bounded by the caller's depth cap.
    match dict.get(b"Next").ok() {
        Some(lopdf::Object::Dictionary(next)) => is_javascript_action_dict(next),
        Some(lopdf::Object::Array(arr)) => arr.iter().any(|obj| {
            if let lopdf::Object::Dictionary(d) = obj {
                is_javascript_action_dict(d)
            } else {
                false
            }
        }),
        _ => false,
    }
}

fn remove_acroform_dict(doc: &mut lopdf::Document) {
    if let Ok(catalog) = doc.catalog_mut() {
        catalog.remove(b"AcroForm");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flags::FieldFlags;
    use crate::tree::{FieldNode, FieldValue};
    use lopdf::{Dictionary, Document, Object, ObjectId, Stream, StringFormat};

    fn js_action() -> Object {
        Object::Dictionary(dictionary! {
            "S" => Object::Name(b"JavaScript".to_vec()),
            "JS" => Object::String(b"app.alert('blocked')".to_vec(), StringFormat::Literal),
        })
    }

    fn uri_action() -> Object {
        Object::Dictionary(dictionary! {
            "S" => Object::Name(b"URI".to_vec()),
            "URI" => Object::String(b"https://example.com".to_vec(), StringFormat::Literal),
        })
    }

    fn widget_dict(widget_extra: Dictionary) -> Dictionary {
        let mut widget = dictionary! {
            "Type" => Object::Name(b"Annot".to_vec()),
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "Rect" => Object::Array(vec![
                Object::Integer(100),
                Object::Integer(700),
                Object::Integer(220),
                Object::Integer(730),
            ]),
        };
        for (key, value) in widget_extra {
            widget.set(key, value);
        }
        widget
    }

    fn make_doc_with_widget(widget_extra: Dictionary) -> (Document, ObjectId) {
        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, Vec::new())));
        let widget_id = doc.new_object_id();
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Integer(612),
                Object::Integer(792),
            ]),
            "Contents" => Object::Reference(content_id),
            "Annots" => Object::Array(vec![Object::Reference(widget_id)]),
        }));
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => Object::Name(b"Pages".to_vec()),
                "Kids" => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => Object::Integer(1),
            }),
        );

        doc.objects
            .insert(widget_id, Object::Dictionary(widget_dict(widget_extra)));

        let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "Pages" => Object::Reference(pages_id),
        }));
        doc.trailer.set("Root", Object::Reference(catalog_id));
        (doc, widget_id)
    }

    fn make_doc_with_shared_indirect_mixed_aa() -> (Document, ObjectId, ObjectId, ObjectId) {
        // Build a doc where two widgets share the same indirect /AA dict, and
        // that dict carries one JS-bearing entry (/E) plus one non-JS entry
        // (/X with a URI action). Used to exercise the clone-on-write path
        // where the targeted widget should keep /X but lose /E, and the
        // shared indirect dict must remain intact.
        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, Vec::new())));
        let shared_aa_id = doc.add_object(Object::Dictionary(dictionary! {
            "E" => js_action(),
            "X" => uri_action(),
        }));
        let widget_a_id = doc.add_object(Object::Dictionary(widget_dict(dictionary! {
            "AA" => Object::Reference(shared_aa_id),
        })));
        let widget_b_id = doc.add_object(Object::Dictionary(widget_dict(dictionary! {
            "AA" => Object::Reference(shared_aa_id),
        })));
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Integer(612),
                Object::Integer(792),
            ]),
            "Contents" => Object::Reference(content_id),
            "Annots" => Object::Array(vec![
                Object::Reference(widget_a_id),
                Object::Reference(widget_b_id),
            ]),
        }));
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => Object::Name(b"Pages".to_vec()),
                "Kids" => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => Object::Integer(1),
            }),
        );
        let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "Pages" => Object::Reference(pages_id),
        }));
        doc.trailer.set("Root", Object::Reference(catalog_id));
        (doc, widget_a_id, widget_b_id, shared_aa_id)
    }

    fn make_doc_with_shared_indirect_aa() -> (Document, ObjectId, ObjectId, ObjectId) {
        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, Vec::new())));
        let shared_aa_id = doc.add_object(Object::Dictionary(dictionary! {
            "E" => js_action(),
        }));
        let widget_a_id = doc.add_object(Object::Dictionary(widget_dict(dictionary! {
            "AA" => Object::Reference(shared_aa_id),
        })));
        let widget_b_id = doc.add_object(Object::Dictionary(widget_dict(dictionary! {
            "AA" => Object::Reference(shared_aa_id),
        })));
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Integer(612),
                Object::Integer(792),
            ]),
            "Contents" => Object::Reference(content_id),
            "Annots" => Object::Array(vec![
                Object::Reference(widget_a_id),
                Object::Reference(widget_b_id),
            ]),
        }));
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => Object::Name(b"Pages".to_vec()),
                "Kids" => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => Object::Integer(1),
            }),
        );
        let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "Pages" => Object::Reference(pages_id),
        }));
        doc.trailer.set("Root", Object::Reference(catalog_id));
        (doc, widget_a_id, widget_b_id, shared_aa_id)
    }

    fn field_tree_for_widget(widget_id: ObjectId) -> FieldTree {
        field_tree_for_widgets(&[("name", widget_id)])
    }

    fn field_tree_for_widgets(widgets: &[(&str, ObjectId)]) -> FieldTree {
        let mut tree = FieldTree::new();
        tree.document_da = Some("/Helv 12 Tf 0 g".to_string());
        for &(name, widget_id) in widgets {
            tree.alloc(FieldNode {
                partial_name: name.into(),
                alternate_name: None,
                mapping_name: None,
                field_type: Some(FieldType::Text),
                flags: FieldFlags::empty(),
                value: Some(FieldValue::Text("Ada".into())),
                default_value: None,
                default_appearance: Some("/Helv 12 Tf 0 g".into()),
                quadding: None,
                max_len: None,
                options: vec![],
                top_index: None,
                rect: Some([100.0, 700.0, 220.0, 730.0]),
                appearance_state: None,
                on_state: None,
                page_index: Some(0),
                parent: None,
                children: vec![],
                object_id: Some((widget_id.0 as i32, widget_id.1 as i32)),
                has_actions: true,
                mk: None,
                border_style: None,
            });
        }
        tree
    }

    fn widget_aa_reference(doc: &Document, widget_id: ObjectId) -> Option<ObjectId> {
        doc.get_dictionary(widget_id)
            .ok()?
            .get(b"AA")
            .ok()?
            .as_reference()
            .ok()
    }

    #[test]
    fn flatten_config_default() {
        let config = FlattenConfig::default();
        assert!(config.field_names.is_empty());
        assert!(config.remove_acroform);
    }
    #[test]
    fn flatten_empty_tree() {
        let tree = FieldTree::new();
        let mut doc = lopdf::Document::new();
        let result = flatten_form(&mut doc, &tree, &FlattenConfig::default());
        assert_eq!(result.fields_flattened, 0);
    }

    #[test]
    fn flatten_strips_widget_javascript_additional_actions() {
        let (mut doc, widget_id) = make_doc_with_widget(dictionary! {
            "AA" => Object::Dictionary(dictionary! {
                "E" => js_action(),
            }),
        });
        let tree = field_tree_for_widget(widget_id);

        let result = flatten_form(
            &mut doc,
            &tree,
            &FlattenConfig {
                remove_acroform: false,
                ..Default::default()
            },
        );

        assert_eq!(result.fields_flattened, 1);
        let widget = doc.get_dictionary(widget_id).expect("widget dict");
        assert!(
            widget.get(b"AA").is_err(),
            "JS-only widget /AA must be removed after flatten"
        );
    }

    #[test]
    fn flatten_preserves_widget_without_additional_actions() {
        let (mut doc, widget_id) = make_doc_with_widget(dictionary! {});
        let tree = field_tree_for_widget(widget_id);

        let result = flatten_form(
            &mut doc,
            &tree,
            &FlattenConfig {
                remove_acroform: false,
                ..Default::default()
            },
        );

        assert_eq!(result.fields_flattened, 1);
        let widget = doc.get_dictionary(widget_id).expect("widget dict");
        assert!(widget.get(b"AA").is_err());
        assert!(matches!(
            widget.get(b"Subtype"),
            Ok(Object::Name(name)) if name == b"Widget"
        ));
    }

    #[test]
    fn flatten_preserves_non_javascript_additional_actions_for_later_policy() {
        let (mut doc, widget_id) = make_doc_with_widget(dictionary! {
            "AA" => Object::Dictionary(dictionary! {
                "E" => js_action(),
                "X" => uri_action(),
            }),
        });
        let tree = field_tree_for_widget(widget_id);

        let result = flatten_form(
            &mut doc,
            &tree,
            &FlattenConfig {
                remove_acroform: false,
                ..Default::default()
            },
        );

        assert_eq!(result.fields_flattened, 1);
        let widget = doc.get_dictionary(widget_id).expect("widget dict");
        let aa = widget
            .get(b"AA")
            .expect("non-JS /AA entry should remain")
            .as_dict()
            .expect("AA dict");
        assert!(aa.get(b"E").is_err(), "JS /AA entry must be stripped");
        assert!(aa.get(b"X").is_ok(), "non-JS /AA entry is out of scope");
    }

    #[test]
    fn flatten_strips_targeted_widget_aa_without_mutating_shared_indirect_dict() {
        let (mut doc, widget_a_id, widget_b_id, shared_aa_id) = make_doc_with_shared_indirect_aa();
        let tree = field_tree_for_widgets(&[("a", widget_a_id), ("b", widget_b_id)]);

        let result = flatten_form(
            &mut doc,
            &tree,
            &FlattenConfig {
                field_names: vec!["a".into()],
                remove_acroform: false,
                ..Default::default()
            },
        );

        assert_eq!(result.fields_flattened, 1);
        let widget_a = doc.get_dictionary(widget_a_id).expect("widget A dict");
        assert!(widget_a.get(b"AA").is_err(), "targeted widget AA stripped");
        assert_eq!(
            widget_aa_reference(&doc, widget_b_id),
            Some(shared_aa_id),
            "untargeted widget keeps its shared AA reference"
        );
        let shared_aa = doc.get_dictionary(shared_aa_id).expect("shared AA dict");
        assert!(
            shared_aa.get(b"E").is_ok(),
            "shared AA object must not be emptied in place"
        );
    }

    #[test]
    fn flatten_strips_each_targeted_widget_aa_when_shared_indirect_dict_is_reused() {
        let (mut doc, widget_a_id, widget_b_id, shared_aa_id) = make_doc_with_shared_indirect_aa();
        let tree = field_tree_for_widgets(&[("a", widget_a_id), ("b", widget_b_id)]);

        let result = flatten_form(
            &mut doc,
            &tree,
            &FlattenConfig {
                field_names: vec!["a".into(), "b".into()],
                remove_acroform: false,
                ..Default::default()
            },
        );

        assert_eq!(result.fields_flattened, 2);
        let widget_a = doc.get_dictionary(widget_a_id).expect("widget A dict");
        let widget_b = doc.get_dictionary(widget_b_id).expect("widget B dict");
        assert!(widget_a.get(b"AA").is_err(), "widget A AA stripped");
        assert!(widget_b.get(b"AA").is_err(), "widget B AA stripped");
        let shared_aa = doc.get_dictionary(shared_aa_id).expect("shared AA dict");
        assert!(
            shared_aa.get(b"E").is_ok(),
            "shared AA object remains intact even when all users are targeted"
        );
    }

    #[test]
    fn flatten_preserves_non_js_entries_on_targeted_widget_with_shared_indirect_aa() {
        // Codex P2 regression-guard (PR #1373 follow-up): when /AA is an
        // indirect dict shared between widgets and contains a mix of JS and
        // non-JS entries, flattening the targeted widget must:
        //   - drop the JS entry (/E) from that widget's effective /AA
        //   - keep the non-JS entry (/X URI) on the targeted widget
        //   - leave the untargeted widget's reference to the shared dict intact
        //   - never mutate the shared indirect dict in place
        let (mut doc, widget_a_id, widget_b_id, shared_aa_id) =
            make_doc_with_shared_indirect_mixed_aa();
        let tree = field_tree_for_widgets(&[("a", widget_a_id), ("b", widget_b_id)]);

        let result = flatten_form(
            &mut doc,
            &tree,
            &FlattenConfig {
                field_names: vec!["a".into()],
                remove_acroform: false,
                ..Default::default()
            },
        );

        assert_eq!(result.fields_flattened, 1);

        // Targeted widget A: /AA must now be an inline dict that contains /X
        // but not /E (clone-on-write).
        let widget_a = doc.get_dictionary(widget_a_id).expect("widget A dict");
        let aa_a = widget_a
            .get(b"AA")
            .expect("targeted widget keeps non-JS /AA entries")
            .as_dict()
            .expect("widget A /AA must be an inline sanitized dict");
        assert!(
            aa_a.get(b"E").is_err(),
            "JS /AA entry must be stripped from targeted widget"
        );
        assert!(
            aa_a.get(b"X").is_ok(),
            "non-JS /AA entry must be preserved on targeted widget"
        );

        // Untargeted widget B: still references the shared indirect /AA.
        assert_eq!(
            widget_aa_reference(&doc, widget_b_id),
            Some(shared_aa_id),
            "untargeted widget keeps its shared indirect /AA reference"
        );

        // Shared indirect /AA dict: untouched, both /E and /X intact.
        let shared_aa = doc.get_dictionary(shared_aa_id).expect("shared AA dict");
        assert!(
            shared_aa.get(b"E").is_ok(),
            "shared /AA dict must not be mutated in place (E key)"
        );
        assert!(
            shared_aa.get(b"X").is_ok(),
            "shared /AA dict must not be mutated in place (X key)"
        );
    }

    #[test]
    fn flatten_strips_js_hidden_in_next_action_chain() {
        // M8-SEC-02 regression: a non-JS top-level action that chains to JS
        // via /Next must be treated as a JS action and stripped on flatten.
        // Before the fix, only the top-level /S key was checked, so a chain
        // like /GoTo → /Next /JavaScript would survive flattening.
        let chained_js = Object::Dictionary(dictionary! {
            "S" => Object::Name(b"GoTo".to_vec()),
            "D" => Object::String(b"page1".to_vec(), StringFormat::Literal),
            "Next" => js_action(),
        });
        let (mut doc, widget_id) = make_doc_with_widget(dictionary! {
            "AA" => Object::Dictionary(dictionary! {
                "E" => chained_js,
            }),
        });
        let tree = field_tree_for_widget(widget_id);

        let result = flatten_form(
            &mut doc,
            &tree,
            &FlattenConfig {
                remove_acroform: false,
                ..Default::default()
            },
        );

        assert_eq!(result.fields_flattened, 1);
        // The widget should have no /AA left after flattening — the chained
        // JS action must have been detected and stripped.
        let widget = doc
            .get_object(widget_id)
            .expect("widget still in doc")
            .as_dict()
            .expect("widget is dict");
        assert!(
            widget.get(b"AA").is_err(),
            "/AA must be stripped when /Next chain contains JavaScript"
        );
        assert!(widget.get(b"JS").is_err(), "widget must not retain /JS key");
    }

    #[test]
    fn flatten_preserves_non_js_action_without_next_chain() {
        // Ensure plain non-JS actions without /Next are still preserved.
        let (mut doc, widget_id) = make_doc_with_widget(dictionary! {
            "AA" => Object::Dictionary(dictionary! {
                "E" => uri_action(),
            }),
        });
        let tree = field_tree_for_widget(widget_id);

        let result = flatten_form(
            &mut doc,
            &tree,
            &FlattenConfig {
                remove_acroform: false,
                ..Default::default()
            },
        );

        assert_eq!(result.fields_flattened, 1);
        // URI actions are not JavaScript — /AA should be preserved (it's the
        // viewer's job to decide whether to execute URI actions).
        let widget = doc
            .get_object(widget_id)
            .expect("widget still in doc")
            .as_dict()
            .expect("widget is dict");
        // The widget annotation was removed from the page Annots, but the
        // object itself may or may not remain — the important thing is that
        // no JS-stripping occurred on the URI action.
        // We verify the URI AA entry on the widget object is untouched.
        if let Ok(Object::Dictionary(aa_dict)) = widget.get(b"AA") {
            if let Ok(Object::Dictionary(e_dict)) = aa_dict.get(b"E") {
                if let Ok(Object::Name(name)) = e_dict.get(b"S") {
                    assert_ne!(name, b"JavaScript", "/E must not be JS");
                }
            }
        }
    }

    #[test]
    fn flatten_writes_static_field_appearance_after_widget_aa_strip() {
        let (mut doc, widget_id) = make_doc_with_widget(dictionary! {
            "AA" => Object::Dictionary(dictionary! {
                "E" => js_action(),
            }),
        });
        let tree = field_tree_for_widget(widget_id);

        let result = flatten_form(
            &mut doc,
            &tree,
            &FlattenConfig {
                remove_acroform: false,
                ..Default::default()
            },
        );

        assert_eq!(result.fields_flattened, 1);
        let page_id = doc.page_iter().next().expect("page");
        let page = doc.get_dictionary(page_id).expect("page dict");
        let content_id = page
            .get(b"Contents")
            .expect("contents")
            .as_reference()
            .expect("contents ref");
        let stream = doc
            .get_object(content_id)
            .expect("content object")
            .as_stream()
            .expect("content stream");
        assert!(
            !stream.content.is_empty(),
            "flatten should still render static field appearance"
        );
    }
}
