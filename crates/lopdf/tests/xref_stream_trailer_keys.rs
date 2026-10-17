//! Trailer keys from cross-reference streams, and across `/Prev` revisions (#643).
//!
//! Since PDF 1.5 the trailer keys (`Root`, `Encrypt`, `Info`, `ID`, `Size`,
//! `Prev`) may live in a cross-reference stream's dictionary instead of a
//! `trailer` dict (ISO 32000-1, section 7.5.8, table 15). The document trailer
//! must carry exactly those keys either way: no stream machinery (`Type`, `W`,
//! `Index`, `Length`, `Filter`, `DecodeParms`) may leak into it.
//!
//! Across an incremental update chain the earlier revisions' trailer keys
//! persist unless a newer revision overrides them (section 7.5.6): the merge
//! walks from the newest revision backwards and only fills in missing keys.
//! `Prev` itself is the chain link, not document state, and is never inherited.
//!
//! Every fixture here is built byte-for-byte by the helpers below; no external
//! file is read. The bogus `/Encrypt` dicts use passwords nobody knows, so the
//! loader keeps the trailer intact instead of decrypting.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// Licensed under the same terms as this crate: MIT.

use pdfluent_lopdf::{Document, Object};
use std::collections::BTreeMap;
use std::io::Write as _;

const ID_HEX: &str = "00112233445566778899AABBCCDDEEFF";
const O_HEX: &str = "BABE00112233445566778899AABBCCDDEEFF00112233445566778899AABBCCDD";
const U_HEX: &str = "F00D00112233445566778899AABBCCDDEEFF00112233445566778899AABBCCDD";

fn catalog() -> String {
    "<< /Type /Catalog /Pages 2 0 R >>".to_string()
}

fn pages() -> String {
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()
}

fn page() -> String {
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources << >> >>".to_string()
}

fn info(title: &str) -> String {
    format!("<< /Title ({title}) /Creator (E7 builder) >>")
}

fn encrypt_dict() -> String {
    format!("<< /Filter /Standard /V 1 /R 2 /O <{O_HEX}> /U <{U_HEX}> /P -4 >>")
}

fn indirect(num: u32, inner: &str) -> Vec<u8> {
    format!("{num} 0 obj\n{inner}\nendobj\n").into_bytes()
}

fn id_entry() -> String {
    format!("/ID [<{ID_HEX}> <{ID_HEX}>]")
}

fn trailer_keys(root: u32, info: Option<u32>, encrypt: Option<u32>) -> String {
    let mut keys = format!("/Root {root} 0 R {}", id_entry());
    if let Some(i) = info {
        keys.push_str(&format!(" /Info {i} 0 R"));
    }
    if let Some(e) = encrypt {
        keys.push_str(&format!(" /Encrypt {e} 0 R"));
    }
    keys
}

/// Group consecutive object numbers into `(start, count)` runs.
fn runs(nums: &[u32]) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for &n in nums {
        if let Some((start, count)) = out.last_mut()
            && *start + *count == n
        {
            *count += 1;
        } else {
            out.push((n, 1));
        }
    }
    out
}

/// A classic cross-reference table with one section per run of `entries`.
/// First revisions open with the conventional free head for object 0.
fn xref_table(entries: &BTreeMap<u32, usize>, free_head: bool) -> Vec<u8> {
    let nums: Vec<u32> = entries.keys().copied().collect();
    let mut out = b"xref\n".to_vec();
    if free_head && !entries.contains_key(&0) {
        // Exactly 20 bytes: a stray space before the EOL breaks section parsing.
        out.extend_from_slice(b"0 1\n0000000000 65535 f\r\n");
    }
    for (start, count) in runs(&nums) {
        out.extend_from_slice(format!("{start} {count}\n").as_bytes());
        for n in start..start + count {
            let offset = entries[&n];
            out.extend_from_slice(format!("{offset:010} 00000 n\r\n").as_bytes());
        }
    }
    out
}

/// Field layout `W[1 4 2]`: one type byte, four offset bytes, two generation bytes.
fn xref_stream_content(entries: &[(u32, usize)]) -> Vec<u8> {
    let mut out = Vec::with_capacity(entries.len() * 7);
    for (_, offset) in entries {
        out.push(1u8);
        out.extend_from_slice(&(*offset as u32).to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes());
    }
    out
}

struct Revision {
    bytes: Vec<u8>,
    xref_offset: usize,
}

/// First revision: `objects` plus a classic table or a stream carrying
/// `trailer_keys` (`/Root 1 0 R ...`).
fn first_revision(objects: &[(u32, String)], stream_xref: bool, compressed: bool, trailer_keys: &str) -> Revision {
    let mut buf = b"%PDF-1.7\n%\xBB\xAD\xC0\xDE\n".to_vec();
    let mut offsets = BTreeMap::new();
    for (num, inner) in objects {
        offsets.insert(*num, buf.len());
        buf.extend_from_slice(&indirect(*num, inner));
    }
    finish_revision(buf, offsets, stream_xref, compressed, trailer_keys, None, true)
}

/// Incremental update on top of `base`: only `objects` are new, so the new
/// cross-reference section lists just them (plus the stream object itself).
fn append_revision(base: &Revision, objects: &[(u32, String)], stream_xref: bool, trailer_keys: &str) -> Revision {
    let mut buf = base.bytes.clone();
    let mut new_offsets = BTreeMap::new();
    for (num, inner) in objects {
        new_offsets.insert(*num, buf.len());
        buf.extend_from_slice(&indirect(*num, inner));
    }
    finish_revision(
        buf,
        new_offsets,
        stream_xref,
        false,
        trailer_keys,
        Some(base.xref_offset),
        false,
    )
}

fn finish_revision(
    mut buf: Vec<u8>, new_offsets: BTreeMap<u32, usize>, stream_xref: bool, compressed: bool, trailer_keys: &str,
    prev: Option<usize>, free_head: bool,
) -> Revision {
    let prev_entry = prev.map(|p| format!(" /Prev {p}")).unwrap_or_default();
    let xref_offset = buf.len();
    if stream_xref {
        let mut entries: Vec<(u32, usize)> = new_offsets.iter().map(|(n, o)| (*n, *o)).collect();
        // The stream lists itself, the way real writers do.
        let stream_num = new_offsets.keys().max().map(|m| m + 1).unwrap_or(0);
        entries.push((stream_num, xref_offset));
        entries.sort_by_key(|(n, _)| *n);
        let nums: Vec<u32> = entries.iter().map(|(n, _)| *n).collect();
        let size = nums.iter().max().map(|m| m + 1).unwrap_or(0);
        let index = runs(&nums)
            .iter()
            .flat_map(|(s, c)| [s.to_string(), c.to_string()])
            .collect::<Vec<_>>()
            .join(" ");
        let content = xref_stream_content(&entries);
        let (content, filter) = if compressed {
            let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(&content).unwrap();
            (encoder.finish().unwrap(), " /Filter /FlateDecode")
        } else {
            (content, "")
        };
        let dict = format!(
            "<< /Type /XRef /Size {size} /W [1 4 2] /Index [{index}]{filter} \
             {trailer_keys}{prev_entry} /Length {} >>",
            content.len()
        );
        buf.extend_from_slice(format!("{stream_num} 0 obj\n{dict}\nstream\n").as_bytes());
        buf.extend_from_slice(&content);
        buf.extend_from_slice(b"\nendstream\nendobj\n");
        buf.extend_from_slice(format!("startxref\n{xref_offset}\n%%EOF\n").as_bytes());
        Revision {
            bytes: buf,
            xref_offset,
        }
    } else {
        let size = new_offsets.keys().max().map(|m| m + 1).unwrap_or(0);
        buf.extend_from_slice(&xref_table(&new_offsets, free_head));
        buf.extend_from_slice(format!("trailer\n<< /Size {size} {trailer_keys}{prev_entry} >>\n").as_bytes());
        buf.extend_from_slice(format!("startxref\n{xref_offset}\n%%EOF\n").as_bytes());
        Revision {
            bytes: buf,
            xref_offset,
        }
    }
}

fn as_reference(doc: &Document, key: &[u8]) -> (u32, u16) {
    doc.trailer.get(key).and_then(|o| o.as_reference()).unwrap_or((0, 0))
}

fn base_objects() -> Vec<(u32, String)> {
    vec![
        (1, catalog()),
        (2, pages()),
        (3, page()),
        (4, info("v1")),
        (5, encrypt_dict()),
    ]
}

/// A single-revision xref-stream file with `/Encrypt` reads as encrypted, and
/// its trailer carries `Root`/`Info`/`ID` — with no stream-machinery leakage.
#[test]
fn xref_stream_surfaces_encrypt_root_info_id() {
    let rev = first_revision(&base_objects(), true, false, &trailer_keys(1, Some(4), Some(5)));
    let doc = Document::load_mem(&rev.bytes).expect("stream revision loads");
    assert_eq!(doc.xref_start, rev.xref_offset, "must use the stream, not the rebuild");
    assert!(doc.is_encrypted(), "xref-stream /Encrypt must read as encrypted");
    assert_eq!(as_reference(&doc, b"Root"), (1, 0));
    assert_eq!(as_reference(&doc, b"Info"), (4, 0));
    assert_eq!(as_reference(&doc, b"Encrypt"), (5, 0));
    let id = doc
        .trailer
        .get(b"ID")
        .and_then(Object::as_array)
        .expect("trailer keeps /ID");
    assert_eq!(id.len(), 2);
    for key in [b"Type".as_slice(), b"W", b"Index", b"Length", b"Filter", b"DecodeParms"] {
        assert!(
            doc.trailer.get(key).is_err(),
            "trailer must not leak /{}",
            String::from_utf8_lossy(key)
        );
    }
}

/// Same file with a classic table: the reference behaviour stream revisions
/// must match, key for key.
#[test]
fn classic_trailer_behaviour_is_unchanged() {
    let rev = first_revision(&base_objects(), false, false, &trailer_keys(1, Some(4), Some(5)));
    let doc = Document::load_mem(&rev.bytes).expect("classic revision loads");
    assert_eq!(doc.xref_start, rev.xref_offset, "must use the table, not the rebuild");
    assert!(doc.is_encrypted());
    assert_eq!(as_reference(&doc, b"Root"), (1, 0));
    assert_eq!(as_reference(&doc, b"Info"), (4, 0));
    assert_eq!(as_reference(&doc, b"Encrypt"), (5, 0));
    let id = doc
        .trailer
        .get(b"ID")
        .and_then(Object::as_array)
        .expect("trailer keeps /ID");
    assert_eq!(id.len(), 2);
    let mut keys: Vec<&[u8]> = doc.trailer.iter().map(|(k, _)| k.as_slice()).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec![b"Encrypt".as_slice(), b"ID", b"Info", b"Root", b"Size"]);
}

/// A Flate-compressed xref stream reads the same trailer keys, without
/// `Filter` leaking into the trailer.
#[test]
fn compressed_xref_stream_hides_filter_keys() {
    let objects = vec![(1, catalog()), (2, pages()), (3, page()), (4, info("zipped"))];
    let rev = first_revision(&objects, true, true, &trailer_keys(1, Some(4), None));
    let doc = Document::load_mem(&rev.bytes).expect("compressed stream revision loads");
    assert_eq!(doc.xref_start, rev.xref_offset, "must use the stream, not the rebuild");
    assert!(!doc.is_encrypted());
    assert_eq!(as_reference(&doc, b"Root"), (1, 0));
    assert_eq!(as_reference(&doc, b"Info"), (4, 0));
    assert!(doc.trailer.get(b"Filter").is_err(), "trailer must not leak /Filter");
    assert!(doc.trailer.get(b"Type").is_err(), "trailer must not leak /Type");
    assert_eq!(doc.get_pages().len(), 1);
}

/// A plain (unencrypted) xref-stream file still loads its objects; the trailer
/// fix must not disturb the object path.
#[test]
fn plain_xref_stream_still_loads_objects() {
    let objects = vec![(1, catalog()), (2, pages()), (3, page()), (4, info("plain"))];
    let rev = first_revision(&objects, true, false, &trailer_keys(1, Some(4), None));
    let doc = Document::load_mem(&rev.bytes).expect("plain stream revision loads");
    assert_eq!(doc.xref_start, rev.xref_offset, "must use the stream, not the rebuild");
    assert!(!doc.is_encrypted());
    assert_eq!(doc.get_pages().len(), 1);
    let info = doc.get_dictionary((4, 0)).expect("info object loads");
    assert_eq!(info.get(b"Title").and_then(|o| o.as_str()).unwrap(), b"plain");
}

/// Classic base, stream update: the newest `Info` wins, while `Encrypt` and
/// the other older keys persist; `Prev` is consumed, never inherited.
#[test]
fn incremental_classic_then_stream_merges_trailer() {
    let base = first_revision(&base_objects(), false, false, &trailer_keys(1, Some(4), Some(5)));
    let update = append_revision(&base, &[(6, info("v2"))], true, &trailer_keys(1, Some(6), None));
    let doc = Document::load_mem(&update.bytes).expect("incremental file loads");
    assert_eq!(
        doc.xref_start, update.xref_offset,
        "must use the chain, not the rebuild"
    );
    assert!(doc.is_encrypted(), "Encrypt from the base revision must persist");
    assert_eq!(as_reference(&doc, b"Encrypt"), (5, 0));
    assert_eq!(as_reference(&doc, b"Info"), (6, 0), "newest Info wins");
    assert_eq!(as_reference(&doc, b"Root"), (1, 0));
    assert!(doc.trailer.get(b"ID").is_ok());
    assert!(doc.trailer.get(b"Prev").is_err(), "Prev is consumed, not inherited");
}

/// Stream base, classic update: the same merge in the reverse direction.
#[test]
fn incremental_stream_then_classic_merges_trailer() {
    let base = first_revision(&base_objects(), true, false, &trailer_keys(1, Some(4), Some(5)));
    let update = append_revision(&base, &[(7, info("v2"))], false, &trailer_keys(1, Some(7), None));
    let doc = Document::load_mem(&update.bytes).expect("incremental file loads");
    assert_eq!(
        doc.xref_start, update.xref_offset,
        "must use the chain, not the rebuild"
    );
    assert!(doc.is_encrypted(), "Encrypt from the base revision must persist");
    assert_eq!(as_reference(&doc, b"Encrypt"), (5, 0));
    assert_eq!(as_reference(&doc, b"Info"), (7, 0), "newest Info wins");
    assert_eq!(as_reference(&doc, b"Root"), (1, 0));
    assert!(doc.trailer.get(b"ID").is_ok());
    assert!(doc.trailer.get(b"Prev").is_err(), "Prev is consumed, not inherited");
}
