// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Attribute existing PDFs; conversion stays in `pdfa_convert_real`.
//! Usage: pdfa_size_report INPUT_DIR OUTPUT_DIR LIST [--json REPORT.json]
//! Payload columns count encoded stream bytes, exclusively. Everything else
//! (dictionaries, object delimiters, xref, trailer, historical revisions) is the
//! exact on-disk remainder. Unreachable payload is a separate exclusive class.
//! A subset prefix is a naming observation, not proof of actual subsetting.
//! Pages list resource reachability, not whether a font/image is drawn. The
//! JSON includes encoded and decoded hashes for before/after image comparison;
//! unknown/unsupported decoding is null, never interpreted as equal pixels.

use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

type Error = Box<dyn std::error::Error>;

#[derive(Default, Serialize)]
struct Report {
    bytes: usize,
    sha256: String,
    repaired_for_loading: bool,
    classes: BTreeMap<String, usize>,
    uncompressed: BTreeMap<String, usize>,
    flate_payload_savings: BTreeMap<String, usize>,
    duplicate_payload: BTreeMap<String, usize>,
    unreachable_objects: usize,
    streams: Vec<StreamRow>,
}

#[derive(Serialize)]
struct StreamRow {
    id: ObjectId,
    class: String,
    bytes: usize,
    filter: String,
    sha256: String,
    decoded_sha256: Option<String>,
    names: Vec<String>,
    subset_prefix: Vec<bool>,
    descriptor_references: usize,
    pages: Vec<u32>,
    duplicate_of: Option<ObjectId>,
    width: Option<i64>,
    height: Option<i64>,
    colorspace: Option<String>,
}

#[derive(Serialize)]
struct Pair {
    name: String,
    input: Report,
    output: Report,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn refs(obj: &Object, out: &mut Vec<ObjectId>) {
    match obj {
        Object::Reference(id) => out.push(*id),
        Object::Array(a) => a.iter().for_each(|o| refs(o, out)),
        Object::Dictionary(d) => dict_refs(d, out),
        Object::Stream(s) => dict_refs(&s.dict, out),
        _ => {}
    }
}

fn dict_refs(dict: &Dictionary, out: &mut Vec<ObjectId>) {
    for (_, obj) in dict.iter() {
        refs(obj, out);
    }
}

fn reachable(doc: &Document, mut queue: Vec<ObjectId>) -> BTreeSet<ObjectId> {
    let mut seen = BTreeSet::new();
    while let Some(id) = queue.pop() {
        if seen.insert(id) {
            if let Some(obj) = doc.objects.get(&id) {
                refs(obj, &mut queue);
            }
        }
    }
    seen
}

fn name(dict: &Dictionary, key: &[u8]) -> String {
    dict.get(key)
        .and_then(Object::as_name)
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .unwrap_or_default()
}

fn mark_refs(obj: &Object, class: &str, classes: &mut BTreeMap<ObjectId, String>) {
    let mut ids = Vec::new();
    refs(obj, &mut ids);
    for id in ids {
        classes.insert(id, class.to_owned());
    }
}

fn inspect(path: &Path) -> Result<Report, Error> {
    let bytes = std::fs::read(path)?;
    let (doc, repaired_for_loading) = match Document::load_mem(&bytes) {
        Ok(doc) => (doc, false),
        Err(_) => (
            pdf_manip::pdfa::load_for_conversion(&bytes, &Default::default())?,
            true,
        ),
    };
    let mut roots = Vec::new();
    dict_refs(&doc.trailer, &mut roots);
    let live = reachable(&doc, roots);
    let mut classes = BTreeMap::new();
    let mut font_names: BTreeMap<ObjectId, Vec<String>> = BTreeMap::new();
    let mut page_usage: BTreeMap<ObjectId, Vec<u32>> = BTreeMap::new();
    for (page, id) in doc.get_pages() {
        let mut roots = Vec::new();
        let (resources, inherited) = doc.get_page_resources(id)?;
        if let Some(d) = resources {
            dict_refs(d, &mut roots);
        }
        roots.extend(inherited);
        for id in reachable(&doc, roots) {
            page_usage.entry(id).or_default().push(page);
        }
    }
    // Walk direct dictionaries too: FontDescriptors and ICCBased arrays need
    // not be indirect objects. Do not dereference here (cycles are legal).
    fn classify(
        obj: &Object,
        classes: &mut BTreeMap<ObjectId, String>,
        fonts: &mut BTreeMap<ObjectId, Vec<String>>,
    ) {
        let dict = match obj {
            Object::Dictionary(d) => Some(d),
            Object::Stream(s) => Some(&s.dict),
            Object::Array(a) => {
                if a.first().and_then(|o| o.as_name().ok()) == Some(b"ICCBased") {
                    if let Some(o) = a.get(1) {
                        mark_refs(o, "icc", classes);
                    }
                }
                for o in a {
                    classify(o, classes, fonts);
                }
                None
            }
            _ => None,
        };
        if let Some(d) = dict {
            for (key, o) in d.iter() {
                let class = match key.as_slice() {
                    b"FontFile" | b"FontFile2" | b"FontFile3" => "fonts",
                    b"Contents" => "content",
                    b"ToUnicode" => "cmap",
                    b"Metadata" => "metadata",
                    b"DestOutputProfile" => "icc",
                    _ => "",
                };
                if !class.is_empty() {
                    mark_refs(o, class, classes);
                    if class == "fonts" {
                        if let Ok(id) = o.as_reference() {
                            fonts.entry(id).or_default().push(name(d, b"FontName"));
                        }
                    }
                }
                classify(o, classes, fonts);
            }
        }
    }
    for obj in doc.objects.values() {
        classify(obj, &mut classes, &mut font_names);
    }
    let mut report = Report {
        bytes: bytes.len(),
        sha256: digest(&bytes),
        repaired_for_loading,
        unreachable_objects: doc.objects.keys().filter(|id| !live.contains(id)).count(),
        ..Default::default()
    };
    let mut duplicates: BTreeMap<String, Vec<ObjectId>> = BTreeMap::new();
    let mut payload = 0;
    for (&id, obj) in &doc.objects {
        let Object::Stream(s) = obj else { continue };
        let typ = name(&s.dict, b"Type");
        let subtype = name(&s.dict, b"Subtype");
        let class = if typ == "XRef" || typ == "ObjStm" {
            "xref_object_streams".to_owned()
        } else if !live.contains(&id) {
            "unreachable".to_owned()
        } else if subtype == "Image" {
            "images".to_owned()
        } else if subtype == "Form" {
            "content".to_owned()
        } else if typ == "Metadata" {
            "metadata".to_owned()
        } else {
            classes
                .get(&id)
                .cloned()
                .unwrap_or_else(|| "other_streams".to_owned())
        };
        payload += s.content.len();
        *report.classes.entry(class.clone()).or_default() += s.content.len();
        if !s.dict.has(b"Filter") {
            *report.uncompressed.entry(class.clone()).or_default() += s.content.len();
            let mut compressed = s.clone();
            compressed.compress()?;
            *report
                .flate_payload_savings
                .entry(class.clone())
                .or_default() += s.content.len() - compressed.content.len();
        }
        let hash = digest(&s.content);
        let candidates = duplicates.entry(hash.clone()).or_default();
        // Equal payloads with different dictionaries are NOT duplicates:
        // widths, filters, masks, Decode arrays and colour spaces matter.
        let duplicate_of = candidates.iter().copied().find(|candidate| {
            doc.get_object(*candidate)
                .ok()
                .and_then(|o| o.as_stream().ok())
                .is_some_and(|other| other.dict == s.dict && other.content == s.content)
        });
        if duplicate_of.is_some() {
            *report.duplicate_payload.entry(class.clone()).or_default() += s.content.len();
        }
        candidates.push(id);
        let names = font_names.get(&id).cloned().unwrap_or_default();
        report.streams.push(StreamRow {
            id,
            class,
            bytes: s.content.len(),
            filter: s
                .dict
                .get(b"Filter")
                .map(|o| format!("{o:?}"))
                .unwrap_or_else(|_| "none".into()),
            sha256: hash,
            decoded_sha256: if s.dict.has(b"Filter") {
                s.decompressed_content_with_limit(256 * 1024 * 1024)
                    .ok()
                    .map(|b| digest(&b))
            } else {
                Some(digest(&s.content))
            },
            subset_prefix: names
                .iter()
                .map(|n| n.as_bytes().get(6) == Some(&b'+'))
                .collect(),
            descriptor_references: names.len(),
            names,
            pages: page_usage.get(&id).cloned().unwrap_or_default(),
            duplicate_of,
            width: s.dict.get(b"Width").ok().and_then(|o| o.as_i64().ok()),
            height: s.dict.get(b"Height").ok().and_then(|o| o.as_i64().ok()),
            colorspace: s.dict.get(b"ColorSpace").ok().map(|o| format!("{o:?}")),
        });
    }
    let overhead = bytes
        .len()
        .checked_sub(payload)
        .ok_or("decoded stream payload exceeds file size; cannot attribute faithfully")?;
    report
        .classes
        .insert("object_xref_overhead".into(), overhead);
    Ok(report)
}

fn table(label: &str, before: &BTreeMap<String, usize>, after: &BTreeMap<String, usize>) {
    println!("\n{label}\n\n| class | input bytes | output bytes | delta |\n|---|---:|---:|---:|");
    let keys: BTreeSet<_> = before.keys().chain(after.keys()).collect();
    for key in keys {
        let a = *before.get(key).unwrap_or(&0);
        let b = *after.get(key).unwrap_or(&0);
        println!("| {key} | {a} | {b} | {} |", b as i128 - a as i128);
    }
}

fn main() -> Result<(), Error> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 && !(args.len() == 5 && args[3] == "--json") {
        return Err(
            "usage: pdfa_size_report INPUT_DIR OUTPUT_DIR LIST [--json REPORT.json]".into(),
        );
    }
    let names = std::fs::read_to_string(&args[2])?;
    let mut pairs = Vec::new();
    let mut total_in = BTreeMap::new();
    let mut total_out = BTreeMap::new();
    for name in names.lines().map(str::trim).filter(|n| !n.is_empty()) {
        let input = inspect(&Path::new(&args[0]).join(name))?;
        let output = inspect(&Path::new(&args[1]).join(name))?;
        println!(
            "\n{name}: {} -> {} ({:.2}%)",
            input.bytes,
            output.bytes,
            100.0 * output.bytes as f64 / input.bytes as f64
        );
        table(name, &input.classes, &output.classes);
        for (a, b) in [
            (&mut total_in, &input.classes),
            (&mut total_out, &output.classes),
        ] {
            for (key, value) in b {
                *a.entry(key.clone()).or_insert(0) += value;
            }
        }
        for stream in &output.streams {
            if !stream.names.is_empty() || stream.class == "images" {
                println!("object {:?}: {} {} bytes, filter {}, names {:?}, subset-prefix {:?}, descriptors {}, pages {:?}, duplicate {:?}, dimensions {:?}x{:?}, colorspace {:?}", stream.id, stream.class, stream.bytes, stream.filter, stream.names, stream.subset_prefix, stream.descriptor_references, stream.pages, stream.duplicate_of, stream.width, stream.height, stream.colorspace);
            }
        }
        pairs.push(Pair {
            name: name.to_owned(),
            input,
            output,
        });
    }
    table("TOTAL", &total_in, &total_out);
    println!(
        "{} documents; {} -> {} bytes",
        pairs.len(),
        total_in.values().sum::<usize>(),
        total_out.values().sum::<usize>()
    );
    if args.len() == 5 {
        std::fs::write(&args[4], serde_json::to_vec_pretty(&pairs)?)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Stream};

    #[test]
    fn attribution_accounts_for_every_byte_and_separates_unreachable_payload() {
        let mut doc = Document::with_version("1.4");
        let orphan = doc.add_object(Stream::new(dictionary! {}, vec![42; 1234]));
        let image = doc.add_object(Stream::new(
            dictionary! { "Subtype" => "Image", "Width" => 2, "Height" => 2 },
            vec![128; 4],
        ));
        let pages = doc.add_object(
            dictionary! { "Type" => "Pages", "Kids" => Vec::<Object>::new(), "Count" => 0 },
        );
        let root = doc.add_object(
            dictionary! { "Type" => "Catalog", "Pages" => pages, "TestImage" => image },
        );
        doc.trailer.set("Root", root);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("generated.pdf");
        doc.save(&path).unwrap();
        let report = inspect(&path).unwrap();
        assert_eq!(report.classes.values().sum::<usize>(), report.bytes);
        assert_eq!(report.classes["unreachable"], 1234);
        assert_eq!(report.classes["images"], 4);
        assert_eq!(
            report
                .streams
                .iter()
                .find(|s| s.id == orphan)
                .unwrap()
                .class,
            "unreachable"
        );
    }

    #[test]
    fn duplicate_diagnostics_compare_dictionary_as_well_as_payload() {
        let mut doc = Document::with_version("1.4");
        let stream = Stream::new(dictionary! {}, vec![0; 1024]);
        let a = doc.add_object(stream.clone());
        let b = doc.add_object(stream);
        let c = doc.add_object(Stream::new(
            dictionary! { "Length1" => 1024 },
            vec![0; 1024],
        ));
        let descriptors: Vec<Object> = [a, b, c].into_iter().map(|id| {
            Object::Dictionary(dictionary! { "Type" => "FontDescriptor", "FontName" => "ABCDEF+Generated", "FontFile2" => id })
        }).collect();
        let pages = doc.add_object(
            dictionary! { "Type" => "Pages", "Kids" => Vec::<Object>::new(), "Count" => 0 },
        );
        let root = doc.add_object(
            dictionary! { "Type" => "Catalog", "Pages" => pages, "TestFonts" => descriptors },
        );
        doc.trailer.set("Root", root);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("generated.pdf");
        doc.save(&path).unwrap();
        let report = inspect(&path).unwrap();
        assert_eq!(report.duplicate_payload["fonts"], 1024);
        assert_eq!(
            report
                .streams
                .iter()
                .find(|s| s.id == b)
                .unwrap()
                .duplicate_of,
            Some(a)
        );
        assert_eq!(
            report
                .streams
                .iter()
                .find(|s| s.id == c)
                .unwrap()
                .duplicate_of,
            None
        );
    }
}
