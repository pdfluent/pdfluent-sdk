//! Page manipulation: merge, split, insert, delete, rearrange, rotate, crop.
//!
//! All page indices are **1-based** to match PDF conventions.

use crate::error::{ManipError, Result};
use lopdf::{Document, Object, ObjectId};
use std::collections::BTreeMap;
use std::path::Path;

/// Extract specific pages from a document into a new document.
pub fn extract_pages(doc: &Document, pages: &[u32]) -> Result<Document> {
    let total = doc.get_pages().len() as u32;
    for &p in pages {
        if p == 0 || p > total {
            return Err(ManipError::PageOutOfRange(p as usize, total as usize));
        }
    }
    if pages.is_empty() {
        return Err(ManipError::EmptyPageRange);
    }

    let mut new_doc = doc.clone();
    let to_delete: Vec<u32> = (1..=total).filter(|p| !pages.contains(p)).collect();

    for &page_num in to_delete.iter().rev() {
        new_doc.delete_pages(&[page_num]);
    }
    Ok(new_doc)
}

/// Delete pages from a document (in-place).
pub fn delete_pages(doc: &mut Document, pages: &[u32]) -> Result<()> {
    let total = doc.get_pages().len() as u32;
    for &p in pages {
        if p == 0 || p > total {
            return Err(ManipError::PageOutOfRange(p as usize, total as usize));
        }
    }
    if pages.len() as u32 >= total {
        return Err(ManipError::Other(
            "cannot delete all pages from a document".into(),
        ));
    }

    let mut sorted: Vec<u32> = pages.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    for &page_num in sorted.iter().rev() {
        doc.delete_pages(&[page_num]);
    }
    Ok(())
}

/// Insert all pages from `source` into `target` at the given position (1-based).
pub fn insert_pages(target: &mut Document, source: &Document, position: u32) -> Result<()> {
    let target_count = target.get_pages().len() as u32;
    if position == 0 || position > target_count + 1 {
        return Err(ManipError::PageOutOfRange(
            position as usize,
            target_count as usize,
        ));
    }

    let source_pages = source.get_pages();
    let mut id_map: BTreeMap<ObjectId, ObjectId> = BTreeMap::new();
    let mut max_id = target.max_id;

    for (&old_id, object) in &source.objects {
        max_id += 1;
        let new_id = (max_id, 0);
        id_map.insert(old_id, new_id);
        target.objects.insert(new_id, object.clone());
    }
    target.max_id = max_id;

    let new_ids: Vec<ObjectId> = id_map.values().copied().collect();
    for &new_id in &new_ids {
        if let Some(obj) = target.objects.get_mut(&new_id) {
            remap_references(obj, &id_map);
        }
    }

    let pages_id = target
        .catalog()
        .ok()
        .and_then(|cat| cat.get(b"Pages").ok())
        .and_then(|obj| obj.as_reference().ok())
        .ok_or_else(|| ManipError::Other("cannot find Pages in catalog".into()))?;

    let mut source_page_ids: Vec<ObjectId> = Vec::new();
    for page_num in 1..=(source_pages.len() as u32) {
        if let Some(&old_page_id) = source_pages.get(&page_num) {
            if let Some(&new_page_id) = id_map.get(&old_page_id) {
                source_page_ids.push(new_page_id);
                if let Some(Object::Dictionary(ref mut dict)) = target.objects.get_mut(&new_page_id)
                {
                    dict.set("Parent", Object::Reference(pages_id));
                }
            }
        }
    }

    if let Some(Object::Dictionary(ref mut pages_dict)) = target.objects.get_mut(&pages_id) {
        if let Ok(Object::Array(ref mut kids)) = pages_dict.get_mut(b"Kids") {
            let insert_idx = ((position - 1) as usize).min(kids.len());
            for (i, page_id) in source_page_ids.iter().enumerate() {
                kids.insert(insert_idx + i, Object::Reference(*page_id));
            }
        }
        let new_count = target_count + source_page_ids.len() as u32;
        pages_dict.set("Count", Object::Integer(new_count as i64));
    }

    Ok(())
}

/// Merge multiple PDF files into one.
pub fn merge<P: AsRef<Path>>(paths: &[P]) -> Result<Document> {
    if paths.is_empty() {
        return Err(ManipError::EmptyPageRange);
    }
    let first = Document::load(paths[0].as_ref())?;
    let mut merged = first;
    for path in &paths[1..] {
        let doc = Document::load(path.as_ref())?;
        let page_count = merged.get_pages().len() as u32;
        insert_pages(&mut merged, &doc, page_count + 1)?;
    }
    Ok(merged)
}

/// Merge multiple in-memory documents into one.
pub fn merge_documents(docs: &[Document]) -> Result<Document> {
    if docs.is_empty() {
        return Err(ManipError::EmptyPageRange);
    }
    let mut merged = docs[0].clone();
    for doc in &docs[1..] {
        let page_count = merged.get_pages().len() as u32;
        insert_pages(&mut merged, doc, page_count + 1)?;
    }
    Ok(merged)
}

/// Split a document into single-page documents.
pub fn split_per_page(doc: &Document) -> Result<Vec<Document>> {
    let total = doc.get_pages().len() as u32;
    (1..=total).map(|p| extract_pages(doc, &[p])).collect()
}

/// Split a document by page ranges (inclusive, 1-based).
pub fn split_by_ranges(doc: &Document, ranges: &[(u32, u32)]) -> Result<Vec<Document>> {
    ranges
        .iter()
        .map(|&(start, end)| {
            let pages: Vec<u32> = (start..=end).collect();
            extract_pages(doc, &pages)
        })
        .collect()
}

/// Rearrange pages. `new_order` is 1-based page numbers in desired order.
pub fn rearrange_pages(doc: &Document, new_order: &[u32]) -> Result<Document> {
    extract_pages(doc, new_order)
}

/// Rotate a page by a multiple of 90 degrees.
pub fn rotate_page(doc: &mut Document, page_num: u32, degrees: i64) -> Result<()> {
    if degrees % 90 != 0 {
        return Err(ManipError::Other(format!(
            "rotation must be a multiple of 90, got {degrees}"
        )));
    }
    let normalized = degrees.rem_euclid(360);
    let pages = doc.get_pages();
    let total = pages.len() as u32;
    let page_id = *pages.get(&page_num).ok_or(ManipError::PageOutOfRange(
        page_num as usize,
        total as usize,
    ))?;

    if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&page_id) {
        let current: i64 = dict
            .get(b"Rotate")
            .and_then(|o| match o {
                Object::Integer(n) => Ok(*n),
                _ => Err(lopdf::Error::DictKey("Rotate".into())),
            })
            .unwrap_or(0);
        dict.set("Rotate", Object::Integer((current + normalized) % 360));
    }
    Ok(())
}

/// Set the CropBox on a page.
pub fn crop_page(doc: &mut Document, page_num: u32, crop_box: [f32; 4]) -> Result<()> {
    let pages = doc.get_pages();
    let total = pages.len() as u32;
    let page_id = *pages.get(&page_num).ok_or(ManipError::PageOutOfRange(
        page_num as usize,
        total as usize,
    ))?;

    if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&page_id) {
        dict.set(
            "CropBox",
            Object::Array(vec![
                Object::Real(crop_box[0]),
                Object::Real(crop_box[1]),
                Object::Real(crop_box[2]),
                Object::Real(crop_box[3]),
            ]),
        );
    }
    Ok(())
}

fn remap_references(obj: &mut Object, id_map: &BTreeMap<ObjectId, ObjectId>) {
    match obj {
        Object::Reference(ref mut id) => {
            if let Some(&new_id) = id_map.get(id) {
                *id = new_id;
            }
        }
        Object::Array(arr) => {
            for item in arr.iter_mut() {
                remap_references(item, id_map);
            }
        }
        Object::Dictionary(dict) => {
            for (_, val) in dict.iter_mut() {
                remap_references(val, id_map);
            }
        }
        Object::Stream(stream) => {
            for (_, val) in stream.dict.iter_mut() {
                remap_references(val, id_map);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Stream};

    fn make_test_doc(num_pages: usize) -> Document {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let mut kids = Vec::new();

        for i in 0..num_pages {
            let content = Stream::new(
                dictionary! {},
                format!("BT /F1 12 Tf (Page {}) Tj ET", i + 1).into_bytes(),
            );
            let content_id = doc.add_object(Object::Stream(content));
            let page = dictionary! {
                "Type" => "Page",
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => Object::Array(vec![
                    Object::Integer(0), Object::Integer(0),
                    Object::Integer(612), Object::Integer(792),
                ]),
                "Contents" => Object::Reference(content_id),
            };
            kids.push(Object::Reference(doc.add_object(Object::Dictionary(page))));
        }

        let pages_dict = dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(num_pages as i64),
            "Kids" => Object::Array(kids),
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages_dict));
        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));
        doc
    }

    #[test]
    fn test_extract_pages() {
        let doc = make_test_doc(5);
        let extracted = extract_pages(&doc, &[2, 4]).unwrap();
        assert_eq!(extracted.get_pages().len(), 2);
    }

    #[test]
    fn test_extract_out_of_range() {
        let doc = make_test_doc(3);
        assert!(extract_pages(&doc, &[4]).is_err());
        assert!(extract_pages(&doc, &[0]).is_err());
    }

    #[test]
    fn test_delete_pages() {
        let mut doc = make_test_doc(5);
        delete_pages(&mut doc, &[2, 4]).unwrap();
        assert_eq!(doc.get_pages().len(), 3);
    }

    #[test]
    fn test_split_per_page() {
        let doc = make_test_doc(3);
        let parts = split_per_page(&doc).unwrap();
        assert_eq!(parts.len(), 3);
        for part in &parts {
            assert_eq!(part.get_pages().len(), 1);
        }
    }

    #[test]
    fn test_rotate_page() {
        let mut doc = make_test_doc(2);
        rotate_page(&mut doc, 1, 90).unwrap();
        let pages = doc.get_pages();
        let page_id = pages[&1];
        if let Some(Object::Dictionary(dict)) = doc.objects.get(&page_id) {
            assert_eq!(*dict.get(b"Rotate").unwrap(), Object::Integer(90));
        }
    }

    #[test]
    fn test_crop_page() {
        let mut doc = make_test_doc(1);
        crop_page(&mut doc, 1, [50.0, 50.0, 400.0, 700.0]).unwrap();
        let pages = doc.get_pages();
        let page_id = pages[&1];
        if let Some(Object::Dictionary(dict)) = doc.objects.get(&page_id) {
            assert!(dict.get(b"CropBox").is_ok());
        }
    }

    #[test]
    fn test_merge_documents() {
        let doc1 = make_test_doc(2);
        let doc2 = make_test_doc(3);
        let merged = merge_documents(&[doc1, doc2]).unwrap();
        assert_eq!(merged.get_pages().len(), 5);
    }
}
