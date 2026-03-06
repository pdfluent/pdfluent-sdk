//! Form flattening: convert interactive fields to static content (B.8).

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

    let fields_to_flatten: Vec<FieldId> = if config.field_names.is_empty() {
        tree.terminal_fields()
    } else {
        tree.terminal_fields()
            .into_iter()
            .filter(|&id| config.field_names.contains(&tree.fully_qualified_name(id)))
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

fn remove_acroform_dict(doc: &mut lopdf::Document) {
    if let Ok(catalog) = doc.catalog_mut() {
        catalog.remove(b"AcroForm");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
