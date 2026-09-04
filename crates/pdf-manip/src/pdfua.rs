//! PDF/UA remediation helpers.
//!
//! Applies conservative, best-effort fixes for common PDF/UA failures:
//! document language, MarkInfo/Marked, page tab order, basic paragraph/image
//! tagging, missing figure alt text, and simple heading hierarchy gaps.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::content_editor::{editor_for_page, write_editor_to_page};
use crate::error::{ManipError, Result};
use crate::text_run::extract_page_text_runs;
use lopdf::content::Operation;
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, StringFormat};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Summary of a PDF/UA remediation pass.
#[derive(Debug, Clone, Default)]
pub struct RemediationReport {
    /// Number of issues detected by the remediation pass.
    pub issues_found: usize,
    /// Number of issues successfully fixed.
    pub issues_fixed: usize,
    /// Human-readable descriptions of issues that were detected but not fixed.
    pub issues_unfixable: Vec<String>,
}

#[derive(Debug, Clone)]
enum TagKind {
    Paragraph,
    Figure { alt_text: String },
}

impl TagKind {
    fn struct_type(&self) -> &'static str {
        match self {
            Self::Paragraph => "P",
            Self::Figure { .. } => "Figure",
        }
    }

    fn alt_text(&self) -> Option<&str> {
        match self {
            Self::Figure { alt_text } => Some(alt_text.as_str()),
            Self::Paragraph => None,
        }
    }
}

#[derive(Debug, Clone)]
struct TagCandidate {
    page_num: u32,
    page_id: ObjectId,
    start: usize,
    end: usize,
    mcid: i32,
    kind: TagKind,
}

#[derive(Debug, Default)]
struct PageTagging {
    candidates: Vec<TagCandidate>,
    unfixable: Vec<String>,
}

#[derive(Debug, Default)]
struct TreeRepairReport {
    issues_found: usize,
    issues_fixed: usize,
    issues_unfixable: Vec<String>,
}

/// Apply a conservative PDF/UA remediation pass to a document.
pub fn remediate_pdfua(doc: &mut Document) -> Result<RemediationReport> {
    let mut report = RemediationReport::default();
    let catalog_id = get_catalog_id(doc)?;

    if !catalog_has_lang(doc, catalog_id) {
        report.issues_found += 1;
        match detect_document_language(doc) {
            Some(lang) => {
                set_catalog_lang(doc, catalog_id, &lang)?;
                report.issues_fixed += 1;
            }
            None => report.issues_unfixable.push(
                "Missing catalog /Lang; could not infer a language from extractable text".into(),
            ),
        }
    }

    if ensure_mark_info(doc, catalog_id)? {
        report.issues_found += 1;
        report.issues_fixed += 1;
    }

    let tabs_fixed = ensure_page_tabs(doc)?;
    report.issues_found += tabs_fixed;
    report.issues_fixed += tabs_fixed;

    if !has_usable_struct_tree(doc, catalog_id) {
        report.issues_found += 1;
        let tagging = auto_tag_document(doc, catalog_id)?;
        if tagging.candidates.is_empty() {
            report.issues_unfixable.push(
                "Missing usable structure tree; no taggable text or image content was found".into(),
            );
        } else {
            report.issues_found += tagging.candidates.len();
            report.issues_fixed += 1 + tagging.candidates.len();
        }
        report.issues_unfixable.extend(tagging.unfixable);
    }

    if has_usable_struct_tree(doc, catalog_id) {
        let figure_repairs = ensure_figure_alt_text(doc, catalog_id)?;
        report.issues_found += figure_repairs.issues_found;
        report.issues_fixed += figure_repairs.issues_fixed;
        report
            .issues_unfixable
            .extend(figure_repairs.issues_unfixable);

        let heading_repairs = repair_heading_hierarchy(doc, catalog_id)?;
        report.issues_found += heading_repairs.issues_found;
        report.issues_fixed += heading_repairs.issues_fixed;
        report
            .issues_unfixable
            .extend(heading_repairs.issues_unfixable);
    }

    Ok(report)
}

fn get_catalog_id(doc: &Document) -> Result<ObjectId> {
    doc.trailer
        .get(b"Root")
        .ok()
        .and_then(|obj| obj.as_reference().ok())
        .ok_or_else(|| ManipError::Other("document catalog missing trailer /Root".into()))
}

fn catalog_has_lang(doc: &Document, catalog_id: ObjectId) -> bool {
    matches!(
        doc.get_object(catalog_id)
            .ok()
            .and_then(|obj| obj.as_dict().ok())
            .and_then(|cat| cat.get(b"Lang").ok()),
        Some(Object::String(bytes, _)) if !bytes.is_empty()
    )
}

fn set_catalog_lang(doc: &mut Document, catalog_id: ObjectId, lang: &str) -> Result<()> {
    match doc.get_object_mut(catalog_id) {
        Ok(Object::Dictionary(ref mut cat)) => {
            cat.set(
                "Lang",
                Object::String(lang.as_bytes().to_vec(), StringFormat::Literal),
            );
            Ok(())
        }
        _ => Err(ManipError::Other(
            "document catalog is not a dictionary".into(),
        )),
    }
}

fn ensure_mark_info(doc: &mut Document, catalog_id: ObjectId) -> Result<bool> {
    let mark_info_ref = match doc.get_object(catalog_id) {
        Ok(Object::Dictionary(cat)) => cat
            .get(b"MarkInfo")
            .ok()
            .and_then(|obj| obj.as_reference().ok()),
        _ => None,
    };

    let already_marked = match mark_info_ref {
        Some(mark_info_id) => matches!(
            doc.get_object(mark_info_id)
                .ok()
                .and_then(|obj| obj.as_dict().ok())
                .and_then(|dict| dict.get(b"Marked").ok()),
            Some(Object::Boolean(true))
        ),
        None => matches!(
            doc.get_object(catalog_id)
                .ok()
                .and_then(|obj| obj.as_dict().ok())
                .and_then(|cat| cat.get(b"MarkInfo").ok()),
            Some(Object::Dictionary(dict))
                if matches!(dict.get(b"Marked").ok(), Some(Object::Boolean(true)))
        ),
    };

    if already_marked {
        return Ok(false);
    }

    if let Some(mark_info_id) = mark_info_ref {
        if let Ok(Object::Dictionary(ref mut dict)) = doc.get_object_mut(mark_info_id) {
            dict.set("Marked", Object::Boolean(true));
            return Ok(true);
        }
    }

    match doc.get_object_mut(catalog_id) {
        Ok(Object::Dictionary(ref mut cat)) => {
            cat.set(
                "MarkInfo",
                Object::Dictionary(dictionary! {
                    "Marked" => Object::Boolean(true),
                }),
            );
            Ok(true)
        }
        _ => Err(ManipError::Other(
            "document catalog is not a dictionary".into(),
        )),
    }
}

fn ensure_page_tabs(doc: &mut Document) -> Result<usize> {
    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let mut fixed = 0;

    for page_id in page_ids {
        let needs_fix = !matches!(
            doc.get_object(page_id)
                .ok()
                .and_then(|obj| obj.as_dict().ok())
                .and_then(|page| page.get(b"Tabs").ok()),
            Some(Object::Name(name)) if name == b"S"
        );

        if needs_fix {
            match doc.get_object_mut(page_id) {
                Ok(Object::Dictionary(ref mut page)) => {
                    page.set("Tabs", Object::Name(b"S".to_vec()));
                    fixed += 1;
                }
                _ => {
                    return Err(ManipError::Other(format!(
                        "page object {page_id:?} is not a dictionary"
                    )));
                }
            }
        }
    }

    Ok(fixed)
}

fn has_usable_struct_tree(doc: &Document, catalog_id: ObjectId) -> bool {
    let Some(tree_root_obj) = doc
        .get_object(catalog_id)
        .ok()
        .and_then(|obj| obj.as_dict().ok())
        .and_then(|cat| cat.get(b"StructTreeRoot").ok())
    else {
        return false;
    };

    let Some(tree_dict) = resolve_dict(doc, tree_root_obj) else {
        return false;
    };

    match tree_dict.get(b"K").ok() {
        Some(Object::Array(children)) => !children.is_empty(),
        Some(Object::Dictionary(_))
        | Some(Object::Reference(_))
        | Some(Object::Integer(_))
        | Some(Object::Stream(_)) => true,
        _ => false,
    }
}

fn auto_tag_document(doc: &mut Document, catalog_id: ObjectId) -> Result<PageTagging> {
    let pages: BTreeMap<u32, ObjectId> = doc.get_pages();
    let mut all_candidates = Vec::new();
    let mut unfixable = Vec::new();

    for (&page_num, &page_id) in &pages {
        let mut editor = match editor_for_page(doc, page_num) {
            Ok(editor) => editor,
            Err(err) => {
                unfixable.push(format!(
                    "Page {page_num}: content stream could not be parsed for auto-tagging ({err})"
                ));
                continue;
            }
        };

        let image_names = image_xobject_names_for_page(doc, page_id);
        let candidates =
            detect_tag_candidates(page_num, page_id, editor.operations(), &image_names);

        if candidates.is_empty() {
            continue;
        }

        apply_tag_candidates(&mut editor, &candidates);
        write_editor_to_page(doc, page_num, &editor)?;
        all_candidates.extend(candidates);
    }

    if all_candidates.is_empty() {
        return Ok(PageTagging {
            candidates: Vec::new(),
            unfixable,
        });
    }

    build_structure_tree(doc, catalog_id, &all_candidates)?;

    Ok(PageTagging {
        candidates: all_candidates,
        unfixable,
    })
}

fn detect_tag_candidates(
    page_num: u32,
    page_id: ObjectId,
    operations: &[Operation],
    image_names: &HashSet<Vec<u8>>,
) -> Vec<TagCandidate> {
    let mut candidates = Vec::new();
    let mut next_mcid = 0_i32;
    let mut text_block_start: Option<usize> = None;
    let mut text_block_has_text = false;

    for (idx, op) in operations.iter().enumerate() {
        match op.operator.as_str() {
            "BT" => {
                text_block_start = Some(idx);
                text_block_has_text = false;
            }
            "Tj" | "TJ" | "'" | "\"" => {
                if text_block_start.is_some() {
                    text_block_has_text = true;
                } else {
                    candidates.push(TagCandidate {
                        page_num,
                        page_id,
                        start: idx,
                        end: idx,
                        mcid: next_mcid,
                        kind: TagKind::Paragraph,
                    });
                    next_mcid += 1;
                }
            }
            "ET" => {
                if let Some(start) = text_block_start.take() {
                    if text_block_has_text {
                        candidates.push(TagCandidate {
                            page_num,
                            page_id,
                            start,
                            end: idx,
                            mcid: next_mcid,
                            kind: TagKind::Paragraph,
                        });
                        next_mcid += 1;
                    }
                }
                text_block_has_text = false;
            }
            "Do" => {
                if let Some(Object::Name(name)) = op.operands.first() {
                    if image_names.contains(name.as_slice())
                        && !range_overlaps(&candidates, idx, idx)
                    {
                        candidates.push(TagCandidate {
                            page_num,
                            page_id,
                            start: idx,
                            end: idx,
                            mcid: next_mcid,
                            kind: TagKind::Figure {
                                alt_text: String::new(),
                            },
                        });
                        next_mcid += 1;
                    }
                }
            }
            _ => {}
        }
    }

    if let Some(start) = text_block_start {
        if text_block_has_text {
            let end = operations.len().saturating_sub(1);
            candidates.push(TagCandidate {
                page_num,
                page_id,
                start,
                end,
                mcid: next_mcid,
                kind: TagKind::Paragraph,
            });
        }
    }

    candidates
}

fn range_overlaps(existing: &[TagCandidate], start: usize, end: usize) -> bool {
    existing
        .iter()
        .any(|candidate| start <= candidate.end && end >= candidate.start)
}

fn apply_tag_candidates(
    editor: &mut crate::content_editor::ContentEditor,
    candidates: &[TagCandidate],
) {
    if candidates.is_empty() {
        return;
    }

    let mut starts: HashMap<usize, Vec<&TagCandidate>> = HashMap::new();
    let mut ends: HashMap<usize, Vec<&TagCandidate>> = HashMap::new();

    for candidate in candidates {
        starts.entry(candidate.start).or_default().push(candidate);
        ends.entry(candidate.end).or_default().push(candidate);
    }

    let original = editor.operations().to_vec();
    let mut rewritten = Vec::with_capacity(original.len() + candidates.len() * 2);

    for (idx, op) in original.into_iter().enumerate() {
        if let Some(candidates_at_start) = starts.get(&idx) {
            for candidate in candidates_at_start {
                rewritten.push(Operation::new(
                    "BDC",
                    vec![
                        Object::Name(candidate.kind.struct_type().as_bytes().to_vec()),
                        Object::Dictionary(dictionary! {
                            "MCID" => Object::Integer(candidate.mcid as i64),
                        }),
                    ],
                ));
            }
        }

        rewritten.push(op);

        if let Some(candidates_at_end) = ends.get(&idx) {
            for _ in candidates_at_end {
                rewritten.push(Operation::new("EMC", vec![]));
            }
        }
    }

    *editor.operations_mut() = rewritten;
}

fn build_structure_tree(
    doc: &mut Document,
    catalog_id: ObjectId,
    candidates: &[TagCandidate],
) -> Result<()> {
    let struct_tree_root_id = doc.add_object(Object::Dictionary(dictionary! {}));
    let document_elem_id = doc.add_object(Object::Dictionary(dictionary! {}));
    let page_key_map = build_page_key_map(candidates);

    let mut child_refs = Vec::with_capacity(candidates.len());
    let mut parent_tree_entries: HashMap<i64, Vec<Option<ObjectId>>> = HashMap::new();

    for candidate in candidates {
        let elem_id = build_struct_elem(doc, candidate, document_elem_id)?;
        child_refs.push(Object::Reference(elem_id));

        let page_parent_key = page_key_map.get(&candidate.page_num).copied().unwrap_or(0);
        let slots = parent_tree_entries.entry(page_parent_key).or_default();
        let slot_idx = candidate.mcid.max(0) as usize;
        if slots.len() <= slot_idx {
            slots.resize(slot_idx + 1, None);
        }
        slots[slot_idx] = Some(elem_id);
    }

    let parent_tree_id = build_parent_tree(doc, &parent_tree_entries);

    if let Ok(Object::Dictionary(ref mut doc_elem)) = doc.get_object_mut(document_elem_id) {
        *doc_elem = dictionary! {
            "Type" => "StructElem",
            "S" => Object::Name(b"Document".to_vec()),
            "P" => Object::Reference(struct_tree_root_id),
            "K" => Object::Array(child_refs),
        };
    }

    if let Ok(Object::Dictionary(ref mut tree_root)) = doc.get_object_mut(struct_tree_root_id) {
        *tree_root = dictionary! {
            "Type" => "StructTreeRoot",
            "K" => Object::Reference(document_elem_id),
            "ParentTree" => Object::Reference(parent_tree_id),
            "ParentTreeNextKey" => Object::Integer(parent_tree_entries.len() as i64),
        };
    }

    if let Ok(Object::Dictionary(ref mut catalog)) = doc.get_object_mut(catalog_id) {
        catalog.set("StructTreeRoot", Object::Reference(struct_tree_root_id));
    }

    for (page_num, page_key) in page_key_map {
        if let Some(page_id) = doc.get_pages().get(&page_num).copied() {
            if let Ok(Object::Dictionary(ref mut page)) = doc.get_object_mut(page_id) {
                page.set("StructParents", Object::Integer(page_key));
            }
        }
    }

    Ok(())
}

fn build_struct_elem(
    doc: &mut Document,
    candidate: &TagCandidate,
    parent_id: ObjectId,
) -> Result<ObjectId> {
    let mut elem = dictionary! {
        "Type" => "StructElem",
        "S" => Object::Name(candidate.kind.struct_type().as_bytes().to_vec()),
        "P" => Object::Reference(parent_id),
        "K" => Object::Integer(candidate.mcid as i64),
        "Pg" => Object::Reference(candidate.page_id),
    };

    if let Some(alt_text) = candidate.kind.alt_text() {
        elem.set(
            "Alt",
            Object::String(alt_text.as_bytes().to_vec(), StringFormat::Literal),
        );
    }

    Ok(doc.add_object(Object::Dictionary(elem)))
}

fn build_parent_tree(
    doc: &mut Document,
    parent_tree_entries: &HashMap<i64, Vec<Option<ObjectId>>>,
) -> ObjectId {
    let mut keys: Vec<i64> = parent_tree_entries.keys().copied().collect();
    keys.sort_unstable();

    let mut nums = Vec::with_capacity(keys.len() * 2);
    for key in keys {
        nums.push(Object::Integer(key));
        let arr = parent_tree_entries[&key]
            .iter()
            .map(|entry| match entry {
                Some(id) => Object::Reference(*id),
                None => Object::Null,
            })
            .collect();
        nums.push(Object::Array(arr));
    }

    doc.add_object(Object::Dictionary(dictionary! {
        "Nums" => Object::Array(nums),
    }))
}

fn build_page_key_map(candidates: &[TagCandidate]) -> HashMap<u32, i64> {
    let mut page_nums: Vec<u32> = candidates
        .iter()
        .map(|candidate| candidate.page_num)
        .collect();
    page_nums.sort_unstable();
    page_nums.dedup();

    page_nums
        .into_iter()
        .enumerate()
        .map(|(idx, page_num)| (page_num, idx as i64))
        .collect()
}

fn image_xobject_names_for_page(doc: &Document, page_id: ObjectId) -> HashSet<Vec<u8>> {
    let mut image_names = HashSet::new();
    let Some(resources) = resolve_page_resources(doc, page_id) else {
        return image_names;
    };

    let xobjects = match resources.get(b"XObject") {
        Ok(Object::Dictionary(dict)) => dict.clone(),
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Dictionary(dict)) => dict.clone(),
            _ => return image_names,
        },
        _ => return image_names,
    };

    for (name, value) in xobjects.iter() {
        let Some(object_id) = value.as_reference().ok() else {
            continue;
        };
        let Ok(Object::Stream(stream)) = doc.get_object(object_id) else {
            continue;
        };
        let subtype = stream
            .dict
            .get(b"Subtype")
            .ok()
            .and_then(|obj| obj.as_name().ok());
        if matches!(subtype, Some(subtype_name) if subtype_name == b"Image") {
            image_names.insert(name.to_vec());
        }
    }

    image_names
}

fn resolve_page_resources(doc: &Document, page_id: ObjectId) -> Option<Dictionary> {
    let mut current_id = Some(page_id);

    while let Some(id) = current_id {
        let page = doc.get_object(id).ok()?.as_dict().ok()?;
        match page.get(b"Resources").ok() {
            Some(Object::Dictionary(dict)) => return Some(dict.clone()),
            Some(Object::Reference(resource_id)) => match doc.get_object(*resource_id).ok() {
                Some(Object::Dictionary(dict)) => return Some(dict.clone()),
                _ => return None,
            },
            _ => {
                current_id = page
                    .get(b"Parent")
                    .ok()
                    .and_then(|obj| obj.as_reference().ok());
            }
        }
    }

    None
}

fn detect_document_language(doc: &Document) -> Option<String> {
    let text = collect_document_text(doc);
    infer_language_from_text(&text)
}

fn collect_document_text(doc: &Document) -> String {
    let page_numbers: Vec<u32> = doc.get_pages().keys().copied().collect();

    if let Ok(text) = doc.extract_text(&page_numbers) {
        if !text.trim().is_empty() {
            return text;
        }
    }

    let mut combined = String::new();
    for page_num in page_numbers {
        if let Ok(runs) = extract_page_text_runs(doc, page_num) {
            for run in runs {
                if !combined.is_empty() {
                    combined.push(' ');
                }
                combined.push_str(&run.text);
            }
        }
    }
    combined
}

fn infer_language_from_text(text: &str) -> Option<String> {
    let alphabetic: Vec<char> = text.chars().filter(|ch| ch.is_alphabetic()).collect();
    if alphabetic.is_empty() {
        return None;
    }

    let latin_count = alphabetic.iter().filter(|&&ch| is_latin_char(ch)).count();
    let latin_ratio = latin_count as f64 / alphabetic.len() as f64;

    let mut scores = HashMap::new();
    for token in text
        .split(|ch: char| !ch.is_alphabetic())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_lowercase())
    {
        match token.as_str() {
            "the" | "and" | "for" | "with" => *scores.entry("en").or_insert(0usize) += 1,
            "de" | "het" | "een" | "van" => *scores.entry("nl").or_insert(0usize) += 1,
            "le" | "la" | "les" | "des" | "et" => *scores.entry("fr").or_insert(0usize) += 1,
            "der" | "die" | "das" | "und" | "ein" => *scores.entry("de").or_insert(0usize) += 1,
            _ => {}
        }
    }

    if let Some((lang, score)) = scores.into_iter().max_by_key(|(_, score)| *score) {
        if score >= 2 {
            return Some(lang.to_string());
        }
    }

    if latin_ratio > 0.8 {
        Some("en".to_string())
    } else {
        None
    }
}

fn is_latin_char(ch: char) -> bool {
    let code = ch as u32;
    ch.is_alphabetic() && matches!(code, 0x0041..=0x024F | 0x1E00..=0x1EFF)
}

fn ensure_figure_alt_text(doc: &mut Document, catalog_id: ObjectId) -> Result<TreeRepairReport> {
    let Some(tree_root_id) = get_struct_tree_root_id(doc, catalog_id) else {
        return Ok(TreeRepairReport::default());
    };

    let role_map = role_map_for_tree_root(doc, tree_root_id);
    let mut elem_ids = Vec::new();
    collect_struct_elem_refs(
        doc,
        &Object::Reference(tree_root_id),
        &mut elem_ids,
        &mut HashSet::new(),
    );

    let mut report = TreeRepairReport::default();

    for elem_id in elem_ids {
        let needs_alt = match doc.get_object(elem_id) {
            Ok(Object::Dictionary(dict)) => {
                if resolved_struct_type(dict, &role_map).as_deref() != Some(b"Figure".as_slice()) {
                    false
                } else {
                    let missing_alt = dict.get(b"Alt").is_err() && dict.get(b"ActualText").is_err();
                    if missing_alt {
                        report.issues_found += 1;
                    }
                    missing_alt
                }
            }
            _ => false,
        };

        if needs_alt {
            if let Ok(Object::Dictionary(ref mut dict)) = doc.get_object_mut(elem_id) {
                dict.set("Alt", Object::String(Vec::new(), StringFormat::Literal));
                report.issues_fixed += 1;
            } else {
                report.issues_unfixable.push(format!(
                    "Figure structure element {elem_id:?} could not be updated"
                ));
            }
        }
    }

    Ok(report)
}

fn repair_heading_hierarchy(doc: &mut Document, catalog_id: ObjectId) -> Result<TreeRepairReport> {
    let Some(tree_root_id) = get_struct_tree_root_id(doc, catalog_id) else {
        return Ok(TreeRepairReport::default());
    };

    let role_map = role_map_for_tree_root(doc, tree_root_id);
    let mut elem_ids = Vec::new();
    collect_struct_elem_refs(
        doc,
        &Object::Reference(tree_root_id),
        &mut elem_ids,
        &mut HashSet::new(),
    );

    let mut report = TreeRepairReport::default();
    let mut previous_level: Option<u8> = None;

    for elem_id in elem_ids {
        let Some(level) = doc
            .get_object(elem_id)
            .ok()
            .and_then(|obj| obj.as_dict().ok())
            .and_then(|dict| resolved_struct_type(dict, &role_map))
            .as_deref()
            .and_then(heading_level)
        else {
            continue;
        };

        if let Some(prev) = previous_level {
            if level > prev + 1 {
                report.issues_found += 1;
                let repaired = prev + 1;
                if let Ok(Object::Dictionary(ref mut dict)) = doc.get_object_mut(elem_id) {
                    dict.set("S", Object::Name(format!("H{repaired}").into_bytes()));
                    report.issues_fixed += 1;
                    previous_level = Some(repaired);
                    continue;
                }
                report.issues_unfixable.push(format!(
                    "Heading structure element {elem_id:?} could not be rewritten"
                ));
            }
        }

        previous_level = Some(level);
    }

    Ok(report)
}

fn get_struct_tree_root_id(doc: &Document, catalog_id: ObjectId) -> Option<ObjectId> {
    doc.get_object(catalog_id)
        .ok()
        .and_then(|obj| obj.as_dict().ok())
        .and_then(|cat| cat.get(b"StructTreeRoot").ok())
        .and_then(|obj| obj.as_reference().ok())
}

fn role_map_for_tree_root(doc: &Document, tree_root_id: ObjectId) -> HashMap<Vec<u8>, Vec<u8>> {
    let mut role_map = HashMap::new();
    let Ok(Object::Dictionary(tree_root)) = doc.get_object(tree_root_id) else {
        return role_map;
    };

    let role_map_dict = match tree_root.get(b"RoleMap").ok() {
        Some(Object::Dictionary(dict)) => dict.clone(),
        Some(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Dictionary(dict)) => dict.clone(),
            _ => return role_map,
        },
        _ => return role_map,
    };

    for (name, value) in role_map_dict.iter() {
        if let Ok(mapped) = value.as_name() {
            role_map.insert(name.to_vec(), mapped.to_vec());
        }
    }

    role_map
}

fn collect_struct_elem_refs(
    doc: &Document,
    obj: &Object,
    out: &mut Vec<ObjectId>,
    visited: &mut HashSet<ObjectId>,
) {
    match obj {
        Object::Reference(id) => {
            if !visited.insert(*id) {
                return;
            }
            let Ok(Object::Dictionary(dict)) = doc.get_object(*id) else {
                return;
            };
            if is_struct_elem_dict(dict) {
                out.push(*id);
            }
            if let Ok(kids) = dict.get(b"K") {
                collect_struct_elem_refs(doc, kids, out, visited);
            }
        }
        Object::Array(items) => {
            for item in items {
                collect_struct_elem_refs(doc, item, out, visited);
            }
        }
        Object::Dictionary(dict) => {
            if let Ok(kids) = dict.get(b"K") {
                collect_struct_elem_refs(doc, kids, out, visited);
            }
        }
        _ => {}
    }
}

fn is_struct_elem_dict(dict: &Dictionary) -> bool {
    matches!(dict.get(b"Type").ok().and_then(|obj| obj.as_name().ok()), Some(name) if name == b"StructElem")
}

fn resolved_struct_type(
    dict: &Dictionary,
    role_map: &HashMap<Vec<u8>, Vec<u8>>,
) -> Option<Vec<u8>> {
    let name = dict.get(b"S").ok()?.as_name().ok()?.to_vec();
    Some(role_map.get(&name).cloned().unwrap_or(name))
}

fn heading_level(name: &[u8]) -> Option<u8> {
    match name {
        b"H1" => Some(1),
        b"H2" => Some(2),
        b"H3" => Some(3),
        b"H4" => Some(4),
        b"H5" => Some(5),
        b"H6" => Some(6),
        _ => None,
    }
}

fn resolve_dict(doc: &Document, obj: &Object) -> Option<Dictionary> {
    match obj {
        Object::Dictionary(dict) => Some(dict.clone()),
        Object::Reference(id) => match doc.get_object(*id).ok()? {
            Object::Dictionary(dict) => Some(dict.clone()),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::content::{Content, Operation};
    use lopdf::{Object, Stream};
    use pdf_compliance::validate_pdfua;

    fn save_bytes(doc: &mut Document) -> Vec<u8> {
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    fn build_pdfua_metadata() -> Object {
        let xmp = br#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description xmlns:pdfuaid="http://www.aiim.org/pdfua/ns/id/" pdfuaid:part="1"/>
  </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#;
        Object::Stream(Stream::new(
            dictionary! {
                "Type" => "Metadata",
                "Subtype" => "XML",
            },
            xmp.to_vec(),
        ))
    }

    fn make_non_accessible_doc() -> Document {
        let mut doc = Document::with_version("1.7");

        let font_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        }));

        let resources = dictionary! {
            "Font" => Object::Dictionary(dictionary! {
                "F1" => Object::Reference(font_id),
            }),
        };

        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new(
                    "Tf",
                    vec![Object::Name(b"F1".to_vec()), Object::Integer(12)],
                ),
                Operation::new("Td", vec![Object::Integer(72), Object::Integer(720)]),
                Operation::new(
                    "Tj",
                    vec![Object::String(
                        b"Hello world".to_vec(),
                        StringFormat::Literal,
                    )],
                ),
                Operation::new("ET", vec![]),
            ],
        }
        .encode()
        .unwrap();

        let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(resources),
            "Annots" => Object::Array(vec![]),
        }));
        let pages_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        }));

        if let Ok(Object::Dictionary(ref mut page)) = doc.get_object_mut(page_id) {
            page.set("Parent", Object::Reference(pages_id));
        }

        let metadata_id = doc.add_object(build_pdfua_metadata());
        let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
            "Metadata" => Object::Reference(metadata_id),
            "ViewerPreferences" => Object::Dictionary(dictionary! {
                "DisplayDocTitle" => Object::Boolean(true),
            }),
        }));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc
    }

    #[test]
    fn remediate_pdfua_fixes_basic_structure_issues() {
        let mut source = make_non_accessible_doc();
        let source_bytes = save_bytes(&mut source);
        let before_pdf = pdf_syntax::Pdf::new(source_bytes.clone()).unwrap();
        let before = validate_pdfua(&before_pdf);
        assert!(before.error_count() >= 4);

        let mut doc = Document::load_mem(&source_bytes).unwrap();
        let report = remediate_pdfua(&mut doc).unwrap();
        assert!(report.issues_fixed > 0);

        let remediated_bytes = save_bytes(&mut doc);
        let remediated_pdf = pdf_syntax::Pdf::new(remediated_bytes).unwrap();
        let after = validate_pdfua(&remediated_pdf);

        assert!(after.error_count() < before.error_count());
        assert!(after.is_compliant(), "issues: {:?}", after.issues);
    }

    #[test]
    fn remediation_repairs_figure_alt_and_heading_gaps() {
        let mut doc = make_non_accessible_doc();
        let page_id = doc.get_pages()[&1];
        let catalog_id = get_catalog_id(&doc).unwrap();

        let struct_tree_root_id = doc.add_object(Object::Dictionary(dictionary! {}));
        let document_elem_id = doc.add_object(Object::Dictionary(dictionary! {}));
        let heading_1_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "StructElem",
            "S" => "H1",
            "P" => Object::Reference(document_elem_id),
            "K" => Object::Integer(0),
            "Pg" => Object::Reference(page_id),
        }));
        let heading_3_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "StructElem",
            "S" => "H3",
            "P" => Object::Reference(document_elem_id),
            "K" => Object::Integer(1),
            "Pg" => Object::Reference(page_id),
        }));
        let figure_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "StructElem",
            "S" => "Figure",
            "P" => Object::Reference(document_elem_id),
            "K" => Object::Integer(2),
            "Pg" => Object::Reference(page_id),
        }));

        if let Ok(Object::Dictionary(ref mut document_elem)) = doc.get_object_mut(document_elem_id)
        {
            *document_elem = dictionary! {
                "Type" => "StructElem",
                "S" => "Document",
                "P" => Object::Reference(struct_tree_root_id),
                "K" => Object::Array(vec![
                    Object::Reference(heading_1_id),
                    Object::Reference(heading_3_id),
                    Object::Reference(figure_id),
                ]),
            };
        }
        if let Ok(Object::Dictionary(ref mut tree_root)) = doc.get_object_mut(struct_tree_root_id) {
            *tree_root = dictionary! {
                "Type" => "StructTreeRoot",
                "K" => Object::Reference(document_elem_id),
            };
        }
        if let Ok(Object::Dictionary(ref mut catalog)) = doc.get_object_mut(catalog_id) {
            catalog.set("StructTreeRoot", Object::Reference(struct_tree_root_id));
        }

        let report = remediate_pdfua(&mut doc).unwrap();
        assert!(report.issues_fixed >= 2);

        let heading = doc.get_object(heading_3_id).unwrap().as_dict().unwrap();
        assert_eq!(heading.get(b"S").unwrap().as_name().unwrap(), b"H2");

        let figure = doc.get_object(figure_id).unwrap().as_dict().unwrap();
        assert!(figure.get(b"Alt").is_ok());
    }
}
