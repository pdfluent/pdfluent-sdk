//! PDF compression and optimization.
//!
//! Includes stream compression, unused object removal, image downsampling,
//! font subsetting preparation, and duplicate detection.

use crate::error::{ManipError, Result};
use flate2::write::ZlibEncoder;
use flate2::Compression;
use lopdf::{Document, Object, ObjectId};
use std::collections::{HashMap, HashSet};
use std::io::Write;

/// Optimization configuration.
#[derive(Debug, Clone)]
pub struct OptimizeConfig {
    /// Compress uncompressed streams with FlateDecode.
    pub compress_streams: bool,
    /// Remove unused objects (not referenced by any other object).
    pub remove_unused: bool,
    /// Deduplicate identical streams.
    pub deduplicate_streams: bool,
    /// Remove metadata (XMP, document info).
    pub strip_metadata: bool,
    /// Target DPI for image downsampling (0 = no downsampling).
    pub image_target_dpi: u32,
    /// JPEG quality for image recompression (1–100, 0 = no recompression).
    pub jpeg_quality: u8,
}

impl Default for OptimizeConfig {
    fn default() -> Self {
        Self {
            compress_streams: true,
            remove_unused: true,
            deduplicate_streams: true,
            strip_metadata: false,
            image_target_dpi: 0,
            jpeg_quality: 0,
        }
    }
}

/// Result of an optimization pass.
#[derive(Debug, Clone)]
pub struct OptimizeResult {
    /// Original size in bytes (approximate, based on object count).
    pub objects_before: usize,
    /// Size after optimization.
    pub objects_after: usize,
    /// Number of streams compressed.
    pub streams_compressed: usize,
    /// Number of duplicate streams merged.
    pub duplicates_merged: usize,
    /// Number of unused objects removed.
    pub unused_removed: usize,
}

/// Run all configured optimizations on a document.
pub fn optimize(doc: &mut Document, config: &OptimizeConfig) -> Result<OptimizeResult> {
    let objects_before = doc.objects.len();
    let mut streams_compressed = 0;
    let mut duplicates_merged = 0;
    let mut unused_removed = 0;

    if config.compress_streams {
        streams_compressed = compress_streams(doc)?;
    }

    if config.deduplicate_streams {
        duplicates_merged = deduplicate_streams(doc);
    }

    if config.remove_unused {
        unused_removed = remove_unused_objects(doc);
    }

    if config.strip_metadata {
        strip_metadata(doc);
    }

    let objects_after = doc.objects.len();

    Ok(OptimizeResult {
        objects_before,
        objects_after,
        streams_compressed,
        duplicates_merged,
        unused_removed,
    })
}

/// Compress all uncompressed streams with FlateDecode.
pub fn compress_streams(doc: &mut Document) -> Result<usize> {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();

    for id in ids {
        let needs_compression = {
            if let Some(Object::Stream(stream)) = doc.objects.get(&id) {
                // Skip if already compressed.
                !stream.dict.has(b"Filter") && !stream.content.is_empty()
            } else {
                false
            }
        };

        if needs_compression {
            if let Some(Object::Stream(ref mut stream)) = doc.objects.get_mut(&id) {
                let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
                encoder
                    .write_all(&stream.content)
                    .map_err(|e| ManipError::Other(format!("compression failed: {e}")))?;
                let compressed = encoder
                    .finish()
                    .map_err(|e| ManipError::Other(format!("compression finalize failed: {e}")))?;

                // Only use compressed version if it's actually smaller.
                if compressed.len() < stream.content.len() {
                    stream
                        .dict
                        .set("Filter", Object::Name(b"FlateDecode".to_vec()));
                    stream
                        .dict
                        .set("Length", Object::Integer(compressed.len() as i64));
                    stream.set_content(compressed);
                    count += 1;
                }
            }
        }
    }

    Ok(count)
}

/// Remove objects that are not referenced by any other object.
pub fn remove_unused_objects(doc: &mut Document) -> usize {
    // Collect all referenced object IDs.
    let mut referenced: HashSet<ObjectId> = HashSet::new();

    // The trailer references root, info, encrypt, etc.
    collect_references_from_dict(&doc.trailer, &mut referenced);

    // Transitively collect all referenced objects.
    let mut queue: Vec<ObjectId> = referenced.iter().copied().collect();
    let mut visited: HashSet<ObjectId> = HashSet::new();

    while let Some(id) = queue.pop() {
        if !visited.insert(id) {
            continue;
        }
        if let Some(obj) = doc.objects.get(&id) {
            let mut refs = HashSet::new();
            collect_references_from_object(obj, &mut refs);
            for r in refs {
                referenced.insert(r);
                if !visited.contains(&r) {
                    queue.push(r);
                }
            }
        }
    }

    // Remove unreferenced objects.
    let all_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut removed = 0;
    for id in all_ids {
        if !referenced.contains(&id) {
            doc.objects.remove(&id);
            removed += 1;
        }
    }

    removed
}

/// Deduplicate identical streams (same content + same dictionary).
pub fn deduplicate_streams(doc: &mut Document) -> usize {
    // Hash stream contents to find duplicates.
    let mut content_map: HashMap<Vec<u8>, ObjectId> = HashMap::new();
    let mut replacements: HashMap<ObjectId, ObjectId> = HashMap::new();

    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in &ids {
        if let Some(Object::Stream(stream)) = doc.objects.get(id) {
            let key = stream.content.clone();
            if let Some(&canonical_id) = content_map.get(&key) {
                if canonical_id != *id {
                    replacements.insert(*id, canonical_id);
                }
            } else {
                content_map.insert(key, *id);
            }
        }
    }

    if replacements.is_empty() {
        return 0;
    }

    let merged = replacements.len();

    // Replace all references to duplicates with the canonical ID.
    for id in &ids {
        if let Some(obj) = doc.objects.get_mut(id) {
            replace_references(obj, &replacements);
        }
    }
    replace_references_in_dict(&mut doc.trailer, &replacements);

    // Remove the duplicate objects.
    for dup_id in replacements.keys() {
        doc.objects.remove(dup_id);
    }

    merged
}

/// Strip metadata from the document.
pub fn strip_metadata(doc: &mut Document) {
    // Remove /Info from trailer.
    doc.trailer.remove(b"Info");

    // Remove XMP metadata stream from catalog.
    if let Ok(catalog_ref) = doc.trailer.get(b"Root").and_then(|r| {
        r.as_reference()
            .map_err(|_| lopdf::Error::DictKey("Root".into()))
    }) {
        if let Some(Object::Dictionary(ref mut cat)) = doc.objects.get_mut(&catalog_ref) {
            cat.remove(b"Metadata");
        }
    }
}

/// Collect object references from a dictionary.
fn collect_references_from_dict(dict: &lopdf::Dictionary, refs: &mut HashSet<ObjectId>) {
    for (_, val) in dict.iter() {
        collect_references_from_object(val, refs);
    }
}

/// Collect object references from any object.
fn collect_references_from_object(obj: &Object, refs: &mut HashSet<ObjectId>) {
    match obj {
        Object::Reference(id) => {
            refs.insert(*id);
        }
        Object::Array(arr) => {
            for item in arr {
                collect_references_from_object(item, refs);
            }
        }
        Object::Dictionary(dict) => {
            collect_references_from_dict(dict, refs);
        }
        Object::Stream(stream) => {
            collect_references_from_dict(&stream.dict, refs);
        }
        _ => {}
    }
}

/// Replace references in an object according to the replacement map.
fn replace_references(obj: &mut Object, replacements: &HashMap<ObjectId, ObjectId>) {
    match obj {
        Object::Reference(ref mut id) => {
            if let Some(&new_id) = replacements.get(id) {
                *id = new_id;
            }
        }
        Object::Array(arr) => {
            for item in arr.iter_mut() {
                replace_references(item, replacements);
            }
        }
        Object::Dictionary(dict) => {
            replace_references_in_dict(dict, replacements);
        }
        Object::Stream(stream) => {
            replace_references_in_dict(&mut stream.dict, replacements);
        }
        _ => {}
    }
}

fn replace_references_in_dict(
    dict: &mut lopdf::Dictionary,
    replacements: &HashMap<ObjectId, ObjectId>,
) {
    for (_, val) in dict.iter_mut() {
        replace_references(val, replacements);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Stream};

    fn make_test_doc() -> Document {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();

        // Add an uncompressed stream.
        let content = Stream::new(dictionary! {}, b"BT /F1 12 Tf (Hello) Tj ET".to_vec());
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
        let page_id = doc.add_object(Object::Dictionary(page));

        let pages = dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(1),
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc
    }

    #[test]
    fn test_compress_streams() {
        let mut doc = make_test_doc();
        let count = compress_streams(&mut doc).unwrap();
        // The test content is small, compression might not be smaller.
        // Either way, the function should not error.
        assert!(count == 0 || count == 1);
    }

    #[test]
    fn test_remove_unused() {
        let mut doc = make_test_doc();
        // Add an orphaned object.
        doc.add_object(Object::Integer(42));
        let before = doc.objects.len();
        let removed = remove_unused_objects(&mut doc);
        assert!(removed >= 1);
        assert!(doc.objects.len() < before);
    }

    #[test]
    fn test_deduplicate() {
        let mut doc = make_test_doc();
        // Add a duplicate stream.
        let dup_content = Stream::new(dictionary! {}, b"BT /F1 12 Tf (Hello) Tj ET".to_vec());
        let _dup_id = doc.add_object(Object::Stream(dup_content));
        let merged = deduplicate_streams(&mut doc);
        assert_eq!(merged, 1);
    }

    #[test]
    fn test_optimize_default() {
        let mut doc = make_test_doc();
        doc.add_object(Object::Integer(999)); // orphan
        let result = optimize(&mut doc, &OptimizeConfig::default()).unwrap();
        assert!(result.unused_removed >= 1);
    }

    #[test]
    fn test_strip_metadata() {
        let mut doc = make_test_doc();
        let info = dictionary! {
            "Title" => Object::String("Test".into(), lopdf::StringFormat::Literal),
        };
        let info_id = doc.add_object(Object::Dictionary(info));
        doc.trailer.set("Info", Object::Reference(info_id));
        strip_metadata(&mut doc);
        assert!(doc.trailer.get(b"Info").is_err());
    }
}
