//! AcroForm dictionary parser (B.1).

use crate::flags::FieldFlags;
use crate::tree::*;
use pdf_syntax::object::dict::keys;
use pdf_syntax::object::{Array, Dict, Name, Object, Rect};
use pdf_syntax::Pdf;

/// Parse the AcroForm dictionary from a PDF document and build a field tree.
pub fn parse_acroform(pdf: &Pdf) -> Option<FieldTree> {
    let xref = pdf.xref();
    let catalog: Dict<'_> = xref.get(xref.root_id())?;
    let acroform: Dict<'_> = catalog.get(keys::ACRO_FORM)?;
    let mut tree = FieldTree::new();

    if let Some(da) = get_string_value(&acroform, keys::DA) {
        tree.document_da = Some(da);
    }
    if let Some(q) = acroform.get::<u32>(keys::Q) {
        tree.document_quadding = Some(parse_quadding(q));
    }
    if let Some(na) = acroform.get::<bool>(keys::NEED_APPEARANCES) {
        tree.need_appearances = na;
    }
    if let Some(sf) = acroform.get::<u32>(keys::SIG_FLAGS) {
        tree.sig_flags = sf;
    }

    if let Some(fields_arr) = acroform.get::<Array<'_>>(keys::FIELDS) {
        for field_dict in fields_arr.iter::<Dict<'_>>() {
            parse_field_recursive(&field_dict, &mut tree, None);
        }
    }

    if let Some(co_arr) = acroform.get::<Array<'_>>(keys::CO) {
        for co_obj in co_arr.iter::<Object<'_>>() {
            if let Object::Dict(co_dict) = co_obj {
                if let Some(obj_id) = co_dict.obj_id() {
                    let target = (obj_id.obj_number, obj_id.gen_number);
                    if let Some(id) = find_by_object_id(&tree, target) {
                        tree.calculation_order.push(id);
                    }
                }
            }
        }
    }

    assign_page_indices(pdf, &mut tree);
    Some(tree)
}

fn parse_field_recursive(dict: &Dict<'_>, tree: &mut FieldTree, parent: Option<FieldId>) {
    let partial_name = get_string_value(dict, keys::T).unwrap_or_default();
    let field_type = dict.get::<Name>(keys::FT).and_then(|n| match n.as_ref() {
        b"Tx" => Some(FieldType::Text),
        b"Btn" => Some(FieldType::Button),
        b"Ch" => Some(FieldType::Choice),
        b"Sig" => Some(FieldType::Signature),
        _ => None,
    });
    let flags = dict
        .get::<u32>(keys::FF)
        .map(FieldFlags::from_bits)
        .unwrap_or_default();
    let rect = dict
        .get::<Rect>(keys::RECT)
        .map(|r| [r.x0 as f32, r.y0 as f32, r.x1 as f32, r.y1 as f32]);
    let appearance_state = dict
        .get::<Name>(keys::AS)
        .map(|n| String::from_utf8_lossy(n.as_ref()).into_owned());
    let object_id = dict.obj_id().map(|oid| (oid.obj_number, oid.gen_number));

    let node = FieldNode {
        partial_name,
        alternate_name: get_string_value(dict, keys::TU),
        mapping_name: get_string_value(dict, keys::TM),
        field_type,
        flags,
        value: parse_field_value(dict, keys::V),
        default_value: parse_field_value(dict, keys::DV),
        default_appearance: get_string_value(dict, keys::DA),
        quadding: dict.get::<u32>(keys::Q).map(parse_quadding),
        max_len: dict.get::<u32>(keys::MAX_LEN),
        options: parse_options(dict),
        top_index: dict.get::<u32>(keys::TI),
        rect,
        appearance_state,
        page_index: None,
        parent,
        children: vec![],
        object_id,
        has_actions: dict.contains_key(keys::AA),
        mk: parse_mk(dict),
        border_style: parse_border_style(dict),
    };
    let id = tree.alloc(node);
    if let Some(pid) = parent {
        tree.get_mut(pid).children.push(id);
    }
    if let Some(kids_arr) = dict.get::<Array<'_>>(keys::KIDS) {
        for kid_dict in kids_arr.iter::<Dict<'_>>() {
            parse_field_recursive(&kid_dict, tree, Some(id));
        }
    }
}

fn parse_field_value(dict: &Dict<'_>, key: &[u8]) -> Option<FieldValue> {
    let obj: Object<'_> = dict.get(key)?;
    match obj {
        Object::String(s) => Some(FieldValue::Text(
            String::from_utf8_lossy(s.as_bytes()).into_owned(),
        )),
        Object::Name(n) => Some(FieldValue::Text(
            String::from_utf8_lossy(n.as_ref()).into_owned(),
        )),
        Object::Array(arr) => {
            let vals: Vec<String> = arr
                .iter::<Object<'_>>()
                .filter_map(|o| match o {
                    Object::String(s) => Some(String::from_utf8_lossy(s.as_bytes()).into_owned()),
                    Object::Name(n) => Some(String::from_utf8_lossy(n.as_ref()).into_owned()),
                    _ => None,
                })
                .collect();
            Some(FieldValue::StringArray(vals))
        }
        _ => None,
    }
}

fn parse_options(dict: &Dict<'_>) -> Vec<ChoiceOption> {
    let Some(arr) = dict.get::<Array<'_>>(keys::OPT) else {
        return vec![];
    };
    arr.iter::<Object<'_>>()
        .filter_map(|obj| match obj {
            Object::String(s) => {
                let text = String::from_utf8_lossy(s.as_bytes()).into_owned();
                Some(ChoiceOption {
                    export: text.clone(),
                    display: text,
                })
            }
            Object::Array(pair) => {
                let items: Vec<Object<'_>> = pair.iter::<Object<'_>>().collect();
                if items.len() >= 2 {
                    Some(ChoiceOption {
                        export: obj_to_string(&items[0]).unwrap_or_default(),
                        display: obj_to_string(&items[1]).unwrap_or_default(),
                    })
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect()
}

fn parse_mk(dict: &Dict<'_>) -> Option<MkDict> {
    let mk_dict: Dict<'_> = dict.get(keys::MK)?;
    Some(MkDict {
        border_color: parse_color_array(&mk_dict, keys::BC),
        background_color: parse_color_array(&mk_dict, keys::BG),
        caption: get_string_value(&mk_dict, keys::CA),
        rollover_caption: get_string_value(&mk_dict, &b"RC"[..]),
        alternate_caption: get_string_value(&mk_dict, keys::AC),
        text_position: mk_dict.get::<u32>(&b"TP"[..]),
        rotation: mk_dict.get::<u32>(&b"R"[..]),
    })
}

fn parse_color_array(dict: &Dict<'_>, key: &[u8]) -> Option<Vec<f32>> {
    let arr: Array<'_> = dict.get(key)?;
    let vals: Vec<f32> = arr.iter::<f32>().collect();
    if vals.is_empty() {
        None
    } else {
        Some(vals)
    }
}

fn parse_border_style(dict: &Dict<'_>) -> Option<BorderStyle> {
    let bs_dict: Dict<'_> = dict.get(keys::BS)?;
    Some(BorderStyle {
        width: bs_dict.get::<f32>(&b"W"[..]).unwrap_or(1.0),
        style: bs_dict
            .get::<Name>(&b"S"[..])
            .and_then(|n| n.as_ref().first().copied())
            .unwrap_or(b'S'),
    })
}

fn parse_quadding(q: u32) -> Quadding {
    match q {
        1 => Quadding::Center,
        2 => Quadding::Right,
        _ => Quadding::Left,
    }
}

fn get_string_value(dict: &Dict<'_>, key: &[u8]) -> Option<String> {
    obj_to_string(&dict.get::<Object<'_>>(key)?)
}

fn obj_to_string(obj: &Object<'_>) -> Option<String> {
    match obj {
        Object::String(s) => Some(String::from_utf8_lossy(s.as_bytes()).into_owned()),
        Object::Name(n) => Some(String::from_utf8_lossy(n.as_ref()).into_owned()),
        _ => None,
    }
}

fn find_by_object_id(tree: &FieldTree, target: (i32, i32)) -> Option<FieldId> {
    tree.all_ids()
        .find(|&id| tree.get(id).object_id == Some(target))
}

fn assign_page_indices(pdf: &Pdf, tree: &mut FieldTree) {
    let pages = pdf.pages();
    for (page_idx, page) in pages.iter().enumerate() {
        let raw = page.raw();
        let Some(annots_arr) = raw.get::<Array<'_>>(keys::ANNOTS) else {
            continue;
        };
        for annot_obj in annots_arr.iter::<Object<'_>>() {
            if let Object::Dict(annot_dict) = annot_obj {
                if let Some(annot_oid) = annot_dict.obj_id() {
                    let target = (annot_oid.obj_number, annot_oid.gen_number);
                    if let Some(fid) = find_by_object_id(tree, target) {
                        tree.get_mut(fid).page_index = Some(page_idx);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quadding_values() {
        assert_eq!(parse_quadding(0), Quadding::Left);
        assert_eq!(parse_quadding(1), Quadding::Center);
        assert_eq!(parse_quadding(2), Quadding::Right);
    }
}
