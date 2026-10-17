//! An incremental update inherits the older revisions' trailer keys (#643).
//!
//! Engine-level proof for the lopdf trailer merge (ISO 32000-1, section 7.5.6):
//! when an update's trailer (or cross-reference stream dictionary) omits a key
//! the base revision carried, the merged document keeps the older value. Here
//! the update repeats only the catalog entry and omits `/Info`, so the
//! creation date must still read through the public [`PdfDocument::metadata`]
//! path. Both chain directions are covered: classic base with a stream update,
//! and the reverse.
//!
//! Fixtures are built byte-for-byte below; no external file is read.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::prelude::*;
use std::collections::BTreeMap;

const ID_HEX: &str = "00112233445566778899AABBCCDDEEFF";
const CREATION_DATE: &str = "D:20200101000000Z";

fn catalog() -> String {
    "<< /Type /Catalog /Pages 2 0 R >>".to_string()
}

fn pages() -> String {
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()
}

fn page() -> String {
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources << >> >>".to_string()
}

fn info() -> String {
    format!("<< /Title (E7 engine) /Creator (E7 builder) /CreationDate ({CREATION_DATE}) >>")
}

fn indirect(num: u32, inner: &str) -> Vec<u8> {
    format!("{num} 0 obj\n{inner}\nendobj\n").into_bytes()
}

fn runs(nums: &[u32]) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for &n in nums {
        let extend = match out.last() {
            Some((start, count)) => *start + *count == n,
            None => false,
        };
        if extend {
            if let Some(slot) = out.last_mut() {
                slot.1 += 1;
            }
        } else {
            out.push((n, 1));
        }
    }
    out
}

fn xref_table(entries: &BTreeMap<u32, usize>, free_head: bool) -> Vec<u8> {
    let nums: Vec<u32> = entries.keys().copied().collect();
    let mut out = b"xref\n".to_vec();
    if free_head && !entries.contains_key(&0) {
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
    offsets: BTreeMap<u32, usize>,
    size: u32,
}

fn base_revision(stream_xref: bool) -> Revision {
    let mut buf = b"%PDF-1.7\n%\xBB\xAD\xC0\xDE\n".to_vec();
    let mut offsets = BTreeMap::new();
    for (num, inner) in [(1, catalog()), (2, pages()), (3, page()), (4, info())] {
        offsets.insert(num, buf.len());
        buf.extend_from_slice(&indirect(num, &inner));
    }
    let keys = format!("/Root 1 0 R /ID [<{ID_HEX}> <{ID_HEX}>] /Info 4 0 R");
    finish(buf, offsets, stream_xref, &keys, None, true, None)
}

/// Update that carries no new objects: it repeats the catalog entry and omits
/// `/Info`, so only trailer inheritance keeps the creation date.
fn update_without_info(base: &Revision, stream_xref: bool) -> Vec<u8> {
    let mut only = BTreeMap::new();
    only.insert(1u32, base.offsets[&1]);
    let keys = format!("/Root 1 0 R /ID [<{ID_HEX}> <{ID_HEX}>]");
    let first_free = base.size;
    finish(
        base.bytes.clone(),
        only,
        stream_xref,
        &keys,
        Some(base.xref_offset),
        false,
        Some((base.size, first_free)),
    )
    .bytes
}

fn finish(
    mut buf: Vec<u8>,
    entries: BTreeMap<u32, usize>,
    stream_xref: bool,
    trailer_keys: &str,
    prev: Option<usize>,
    free_head: bool,
    size_hint: Option<(u32, u32)>,
) -> Revision {
    let prev_entry = prev.map(|p| format!(" /Prev {p}")).unwrap_or_default();
    let xref_offset = buf.len();
    let size = if stream_xref {
        let mut listed: Vec<(u32, usize)> = entries.iter().map(|(n, o)| (*n, *o)).collect();
        let stream_num = size_hint
            .map(|(_, first_free)| first_free)
            .unwrap_or_else(|| listed.iter().map(|(n, _)| *n).max().unwrap_or(0) + 1);
        listed.push((stream_num, xref_offset));
        listed.sort_by_key(|(n, _)| *n);
        let nums: Vec<u32> = listed.iter().map(|(n, _)| *n).collect();
        let mut size = nums.iter().max().map(|m| m + 1).unwrap_or(0);
        if let Some((base_size, _)) = size_hint {
            size = size.max(base_size + 1);
        }
        size
    } else {
        let mut size = entries.keys().max().map(|m| m + 1).unwrap_or(0);
        if let Some((base_size, _)) = size_hint {
            size = size.max(base_size);
        }
        size
    };
    if stream_xref {
        let mut listed: Vec<(u32, usize)> = entries.iter().map(|(n, o)| (*n, *o)).collect();
        let stream_num = size_hint
            .map(|(_, first_free)| first_free)
            .unwrap_or_else(|| listed.iter().map(|(n, _)| *n).max().unwrap_or(0) + 1);
        listed.push((stream_num, xref_offset));
        listed.sort_by_key(|(n, _)| *n);
        let nums: Vec<u32> = listed.iter().map(|(n, _)| *n).collect();
        let index = runs(&nums)
            .iter()
            .flat_map(|(s, c)| [s.to_string(), c.to_string()])
            .collect::<Vec<_>>()
            .join(" ");
        let content = xref_stream_content(&listed);
        let dict = format!(
            "<< /Type /XRef /Size {size} /W [1 4 2] /Index [{index}] \
             {trailer_keys}{prev_entry} /Length {} >>",
            content.len()
        );
        buf.extend_from_slice(format!("{stream_num} 0 obj\n{dict}\nstream\n").as_bytes());
        buf.extend_from_slice(&content);
        buf.extend_from_slice(b"\nendstream\nendobj\n");
        buf.extend_from_slice(format!("startxref\n{xref_offset}\n%%EOF\n").as_bytes());
    } else {
        buf.extend_from_slice(&xref_table(&entries, free_head));
        buf.extend_from_slice(
            format!("trailer\n<< /Size {size} {trailer_keys}{prev_entry} >>\n").as_bytes(),
        );
        buf.extend_from_slice(format!("startxref\n{xref_offset}\n%%EOF\n").as_bytes());
    }
    Revision {
        bytes: buf,
        xref_offset,
        offsets: entries,
        size,
    }
}

fn creation_date_of(bytes: &[u8]) -> Option<String> {
    let doc = PdfDocument::from_bytes(bytes).expect("incremental file opens");
    assert_eq!(doc.page_count(), 1);
    doc.metadata().creation_date
}

#[test]
fn classic_base_with_stream_update_keeps_info() {
    let base = base_revision(false);
    let bytes = update_without_info(&base, true);
    assert_eq!(
        creation_date_of(&bytes).as_deref(),
        Some(CREATION_DATE),
        "the stream update omits /Info; the base value must persist"
    );
}

#[test]
fn stream_base_with_classic_update_keeps_info() {
    let base = base_revision(true);
    let bytes = update_without_info(&base, false);
    assert_eq!(
        creation_date_of(&bytes).as_deref(),
        Some(CREATION_DATE),
        "the classic update omits /Info; the base value must persist"
    );
}
