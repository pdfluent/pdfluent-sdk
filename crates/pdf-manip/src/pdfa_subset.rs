//! Conservative TrueType subsetting for existing PDF fonts.
//!
//! Keep glyph IDs, all metrics, cmaps and hint instructions unchanged. Simple
//! PDF fonts can address only 256 character codes. Retain every glyph any of
//! those codes can select, including alternate cmap/name lookup paths and
//! composite components. This deliberately retains more than the text shown
//! on sampled pages: inherited resources, appearances and future form values
//! using the same simple font remain safe. CID fonts and unsupported programs
//! are left intact and reported rather than guessed from their width arrays.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{Dictionary, Document, Object, ObjectId};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// Storage changes and conservative fallback reasons from the font pass.
#[derive(Debug, Clone, Default)]
pub struct SubsetReport {
    /// Programs with safely reduced serialized font streams.
    pub programs_subsetted: usize,
    /// Net encoded payload bytes recovered.
    pub bytes_saved: usize,
    /// Programs kept unchanged, with the reason (object number, generation).
    pub skipped: Vec<(ObjectId, String)>,
}

fn name<'a>(d: &'a Dictionary, key: &[u8]) -> Option<&'a [u8]> {
    d.get(key).and_then(Object::as_name).ok()
}

fn resolved<'a>(doc: &'a Document, o: &'a Object) -> Option<&'a Object> {
    doc.dereference(o).ok().map(|(_, o)| o)
}

/// Subset the addressable repertoire of standalone simple TrueType fonts.
pub(crate) fn subset_fonts(doc: &mut Document) -> SubsetReport {
    let mut report = SubsetReport::default();
    let mut users: BTreeMap<ObjectId, Vec<(ObjectId, ObjectId)>> = BTreeMap::new();
    let mut blocked = BTreeSet::new();
    // Track all descriptor users, not names: unrelated fonts can share a name,
    // while one program can be shared by differently encoded font dictionaries.
    for (font_id, obj) in &doc.objects {
        let Ok(font) = obj.as_dict() else { continue };
        let Some(fd_obj) = font.get(b"FontDescriptor").ok() else {
            continue;
        };
        let Some(fd) = resolved(doc, fd_obj).and_then(|o| o.as_dict().ok()) else {
            continue;
        };
        for key in [b"FontFile".as_slice(), b"FontFile2", b"FontFile3"] {
            let Some(program) = fd.get(key).ok().and_then(|o| o.as_reference().ok()) else {
                continue;
            };
            if name(font, b"Subtype") != Some(b"TrueType") || key != b"FontFile2" {
                blocked.insert(program);
                continue;
            }
            if let Ok(fd_id) = fd_obj.as_reference() {
                users.entry(program).or_default().push((*font_id, fd_id));
            } else {
                blocked.insert(program);
            }
        }
    }
    // Direct font/descriptor dictionaries are legal. Do not modify a program
    // when a direct descriptor user could escape the standalone-user census.
    fn direct_users(doc: &Document, o: &Object, top: bool, blocked: &mut BTreeSet<ObjectId>) {
        let dict = match o {
            Object::Dictionary(d) => Some(d),
            Object::Stream(s) => Some(&s.dict),
            Object::Array(a) => {
                for o in a {
                    direct_users(doc, o, false, blocked);
                }
                None
            }
            _ => None,
        };
        if let Some(d) = dict {
            if !top {
                // An inline font may point to an indirect descriptor shared
                // with a standalone font. Its encoding was not in the census.
                if let Some(fd) = d
                    .get(b"FontDescriptor")
                    .ok()
                    .and_then(|o| resolved(doc, o))
                    .and_then(|o| o.as_dict().ok())
                {
                    for key in [b"FontFile2".as_slice(), b"FontFile3", b"FontFile"] {
                        if let Ok(id) = fd.get(key).and_then(Object::as_reference) {
                            blocked.insert(id);
                        }
                    }
                }
                for key in [b"FontFile2".as_slice(), b"FontFile3", b"FontFile"] {
                    if let Ok(id) = d.get(key).and_then(Object::as_reference) {
                        blocked.insert(id);
                    }
                }
            }
            for (_, v) in d.iter() {
                direct_users(doc, v, false, blocked);
            }
        }
    }
    for o in doc.objects.values() {
        direct_users(doc, o, true, &mut blocked);
    }

    for (program, fonts) in users {
        let result = (|| -> Result<(lopdf::Stream, usize, String), &'static str> {
            if blocked.contains(&program) {
                return Err("shared with an unsupported or direct font");
            }
            let stream = doc
                .get_object(program)
                .ok()
                .and_then(|o| o.as_stream().ok())
                .ok_or("missing program")?;
            if !stream.allows_compression {
                return Err("caller disabled font stream compression");
            }
            if [b"F".as_slice(), b"FFilter", b"FDecodeParms"]
                .iter()
                .any(|k| stream.dict.has(k))
            {
                return Err("external font program");
            }
            let data = if stream.dict.has(b"Filter") {
                stream
                    .decompressed_content()
                    .map_err(|_| "undecodable font program")?
            } else {
                stream.content.clone()
            };
            let face = ttf_parser::Face::parse(&data, 0).map_err(|_| "unparseable font")?;
            let mut keep = BTreeSet::from([0]);
            for (font, _) in &fonts {
                let font = doc
                    .get_dictionary(*font)
                    .map_err(|_| "missing font dictionary")?;
                let (encoding, differences) =
                    crate::pdfa_fonts::get_simple_encoding_info(doc, font);
                if !matches!(
                    encoding.as_str(),
                    "" | "WinAnsiEncoding" | "MacRomanEncoding" | "StandardEncoding"
                ) {
                    return Err("unsupported simple encoding");
                }
                for code in 0..=255u32 {
                    // Direct glyph-index fallbacks are used by some symbolic
                    // subset readers; retaining them costs at most 256 glyphs.
                    if code < u32::from(face.number_of_glyphs()) {
                        keep.insert(code as u16);
                    }
                    let mut points = vec![
                        code,
                        0xf000 + code,
                        0xf100 + code,
                        0xf200 + code,
                        crate::pdfa_fonts::encoding_to_char(code, &encoding) as u32,
                    ];
                    if let Some(glyph_name) = differences.get(&code) {
                        if let Some(g) = face.glyph_index_by_name(glyph_name) {
                            keep.insert(g.0);
                        }
                        if let Some(c) = crate::pdfa_fonts::glyph_name_to_unicode(glyph_name) {
                            points.push(c as u32);
                        }
                    }
                    if let Some(cmap) = face.tables().cmap {
                        for sub in cmap.subtables {
                            for cp in &points {
                                if let Some(g) = sub.glyph_index(*cp) {
                                    keep.insert(g.0);
                                }
                            }
                        }
                    }
                }
            }
            let subset = retain_glyphs(&data, &keep)?;
            let digest = Sha256::digest(&subset);
            let tag: String = digest[..6]
                .iter()
                .map(|b| char::from(b'A' + b % 26))
                .collect();
            let mut replacement = stream.clone();
            replacement.set_plain_content(subset.clone());
            replacement.dict.remove(b"DecodeParms");
            replacement.dict.set("Length1", subset.len() as i64);
            replacement
                .compress()
                .map_err(|_| "font compression failed")?;
            // Include the added subset names/dictionaries in a conservative
            // margin; report encoded bytes, never uncompressed hypothetical savings.
            if replacement.content.len() + 128 * fonts.len() >= stream.content.len() {
                return Err("no net storage saving");
            }
            let saving = stream.content.len() - replacement.content.len();
            Ok((replacement, saving, tag))
        })();
        match result {
            Ok((stream, saving, tag)) => {
                doc.objects.insert(program, Object::Stream(stream));
                for (font, descriptor) in fonts {
                    for (id, key) in [(font, b"BaseFont".as_slice()), (descriptor, b"FontName")] {
                        if let Ok(d) = doc.get_dictionary_mut(id) {
                            if let Some(old) = name(d, key) {
                                let base = if old.len() > 7 && old[6] == b'+' {
                                    &old[7..]
                                } else {
                                    old
                                };
                                let mut new = tag.as_bytes().to_vec();
                                new.push(b'+');
                                new.extend_from_slice(base);
                                d.set(key, Object::Name(new));
                            }
                        }
                    }
                }
                report.programs_subsetted += 1;
                report.bytes_saved += saving;
            }
            Err(reason) => report.skipped.push((program, reason.into())),
        }
    }
    for id in blocked {
        if !report.skipped.iter().any(|(p, _)| *p == id) {
            report.skipped.push((
                id,
                "CID, CFF, Type1 or direct/shared font requires a different subset strategy".into(),
            ));
        }
    }
    report
}

fn u16_at(data: &[u8], offset: usize) -> Result<u16, &'static str> {
    Ok(u16::from_be_bytes(
        data.get(offset..offset + 2)
            .ok_or("truncated font")?
            .try_into()
            .unwrap(),
    ))
}
fn u32_at(data: &[u8], offset: usize) -> Result<u32, &'static str> {
    Ok(u32::from_be_bytes(
        data.get(offset..offset + 4)
            .ok_or("truncated font")?
            .try_into()
            .unwrap(),
    ))
}
fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |sum, chunk| {
        let mut padded = [0; 4];
        padded[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(padded))
    })
}

/// Drop only unaddressable outlines, retaining IDs and every other SFNT table.
fn retain_glyphs(data: &[u8], requested: &BTreeSet<u16>) -> Result<Vec<u8>, &'static str> {
    if !matches!(data.get(..4), Some(b"\x00\x01\x00\x00" | b"true")) {
        return Err("not a standalone TrueType font");
    }
    let count = u16_at(data, 4)? as usize;
    if count == 0 || count > 256 {
        return Err("invalid table count");
    }
    let mut tables = BTreeMap::new();
    for i in 0..count {
        let at = 12 + 16 * i;
        let tag: [u8; 4] = data
            .get(at..at + 4)
            .ok_or("truncated table directory")?
            .try_into()
            .unwrap();
        let start = u32_at(data, at + 8)? as usize;
        let end = start
            .checked_add(u32_at(data, at + 12)? as usize)
            .ok_or("table overflow")?;
        if tables
            .insert(
                tag,
                data.get(start..end).ok_or("invalid table extent")?.to_vec(),
            )
            .is_some()
        {
            return Err("duplicate SFNT table");
        }
    }
    for tag in [
        b"DSIG", b"fvar", b"gvar", b"COLR", b"SVG ", b"sbix", b"CBDT", b"EBDT",
    ] {
        if tables.contains_key(tag) {
            return Err("signed, variable or color font");
        }
    }
    let head = tables.get(b"head").ok_or("missing head")?;
    let long = match u16_at(head, 50)? {
        0 => false,
        1 => true,
        _ => return Err("invalid loca format"),
    };
    let maxp = tables.get(b"maxp").ok_or("missing maxp")?;
    let glyph_count = u16_at(maxp, 4)? as usize;
    let loca = tables.get(b"loca").ok_or("missing loca")?;
    let glyf = tables.get(b"glyf").ok_or("missing glyf")?;
    if let Some(os2) = tables.get(b"OS/2") {
        if u16_at(os2, 8)? & 0x0300 != 0 {
            return Err("font disallows subsetting or outline embedding");
        }
    }
    let offsets: Vec<usize> = (0..=glyph_count)
        .map(|i| {
            if long {
                u32_at(loca, i * 4).map(|o| o as usize)
            } else {
                u16_at(loca, i * 2).map(|o| o as usize * 2)
            }
        })
        .collect::<Result<_, _>>()?;
    if offsets.windows(2).any(|w| w[0] > w[1]) || offsets[glyph_count] > glyf.len() {
        return Err("invalid glyph offsets");
    }
    let mut keep = requested.clone();
    keep.insert(0);
    let mut pending: Vec<_> = keep.iter().copied().collect();
    while let Some(gid) = pending.pop() {
        let i = gid as usize;
        if i >= glyph_count {
            return Err("glyph index out of range");
        }
        let bytes = &glyf[offsets[i]..offsets[i + 1]];
        if bytes.is_empty() {
            continue;
        }
        if bytes.len() < 10 {
            return Err("truncated glyph");
        }
        if (u16_at(bytes, 0)? as i16) >= 0 {
            continue;
        }
        let mut at = 10;
        loop {
            let flags = u16_at(bytes, at)?;
            let component = u16_at(bytes, at + 2)?;
            if keep.insert(component) {
                pending.push(component);
            }
            at += 4 + if flags & 1 != 0 { 4 } else { 2 };
            at += if flags & 8 != 0 {
                2
            } else if flags & 64 != 0 {
                4
            } else if flags & 128 != 0 {
                8
            } else {
                0
            };
            if at > bytes.len() {
                return Err("truncated composite");
            }
            if flags & 32 == 0 {
                break;
            }
        }
    }
    let mut new_glyf = Vec::new();
    let mut new_loca = Vec::new();
    for i in 0..=glyph_count {
        if long {
            new_loca.extend_from_slice(&(new_glyf.len() as u32).to_be_bytes());
        } else {
            new_loca.extend_from_slice(&((new_glyf.len() / 2) as u16).to_be_bytes());
        }
        if i < glyph_count && keep.contains(&(i as u16)) {
            new_glyf.extend_from_slice(&glyf[offsets[i]..offsets[i + 1]]);
            if new_glyf.len() % 2 != 0 {
                new_glyf.push(0);
            }
        }
    }
    if new_glyf.len() >= glyf.len() {
        return Err("no unused outline payload");
    }
    tables.insert(*b"glyf", new_glyf);
    tables.insert(*b"loca", new_loca);
    let head = tables.get_mut(b"head").unwrap();
    head[8..12].fill(0);
    let mut out = data[..12].to_vec();
    out.resize(12 + 16 * count, 0);
    let mut head_start = 0;
    for (i, (tag, bytes)) in tables.into_iter().enumerate() {
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
        let at = 12 + 16 * i;
        out[at..at + 4].copy_from_slice(&tag);
        out[at + 4..at + 8].copy_from_slice(&checksum(&bytes).to_be_bytes());
        let start = out.len();
        out[at + 8..at + 12].copy_from_slice(&(start as u32).to_be_bytes());
        out[at + 12..at + 16].copy_from_slice(&(bytes.len() as u32).to_be_bytes());
        if tag == *b"head" {
            head_start = start;
        }
        out.extend_from_slice(&bytes);
    }
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
    let adjustment = 0xB1B0_AFBAu32.wrapping_sub(checksum(&out));
    out[head_start + 8..head_start + 12].copy_from_slice(&adjustment.to_be_bytes());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Generated SFNT with 512 glyph slots. Glyph 1 references glyph 300;
    // the cmap, names, metrics and interpreter tables must remain unchanged.
    fn font(long: bool) -> Vec<u8> {
        let mut tables: BTreeMap<[u8; 4], Vec<u8>> = BTreeMap::new();
        let mut head = vec![0; 54];
        head[..4].copy_from_slice(&0x10000u32.to_be_bytes());
        head[18..20].copy_from_slice(&1000u16.to_be_bytes());
        head[50..52].copy_from_slice(&u16::from(long).to_be_bytes());
        tables.insert(*b"head", head);
        let mut maxp = vec![0; 32];
        maxp[..4].copy_from_slice(&0x10000u32.to_be_bytes());
        maxp[4..6].copy_from_slice(&512u16.to_be_bytes());
        tables.insert(*b"maxp", maxp);
        let mut hhea = vec![0; 36];
        hhea[..4].copy_from_slice(&0x10000u32.to_be_bytes());
        hhea[34..36].copy_from_slice(&512u16.to_be_bytes());
        tables.insert(*b"hhea", hhea);
        tables.insert(*b"hmtx", [1, 244, 0, 0].repeat(512));
        let mut cmap = vec![0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 12, 0, 0, 1, 6, 0, 0];
        cmap.resize(274, 0);
        cmap[18 + 65] = 1;
        tables.insert(*b"cmap", cmap);
        tables.insert(*b"prep", vec![0xb0, 0, 0x21]);
        let mut glyf = Vec::new();
        let mut loca = Vec::new();
        for gid in 0..=512 {
            if long {
                loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());
            } else {
                loca.extend_from_slice(&((glyf.len() / 2) as u16).to_be_bytes());
            }
            if gid == 512 {
                break;
            }
            if gid == 1 {
                glyf.extend_from_slice(&[
                    255, 255, 0, 0, 0, 0, 0, 10, 0, 10, 0, 3, 1, 44, 0, 0, 0, 0,
                ]);
            } else {
                // Empty outline with deterministic valid push/pop hints.
                glyf.extend_from_slice(&[0; 10]);
                glyf.extend_from_slice(&48u16.to_be_bytes());
                for j in 0..16 {
                    glyf.extend_from_slice(&[0xb0, ((gid * 31 + j * 17) % 256) as u8, 0x21]);
                }
            }
        }
        tables.insert(*b"glyf", glyf);
        tables.insert(*b"loca", loca);
        let mut out = vec![0, 1, 0, 0, 0, tables.len() as u8, 0, 128, 0, 3, 0, 0];
        out.resize(12 + tables.len() * 16, 0);
        for (i, (tag, bytes)) in tables.iter().enumerate() {
            while !out.len().is_multiple_of(4) {
                out.push(0);
            }
            let at = 12 + i * 16;
            let offset = out.len() as u32;
            out[at..at + 4].copy_from_slice(tag);
            out[at + 4..at + 8].copy_from_slice(&checksum(bytes).to_be_bytes());
            out[at + 8..at + 12].copy_from_slice(&offset.to_be_bytes());
            out[at + 12..at + 16].copy_from_slice(&(bytes.len() as u32).to_be_bytes());
            out.extend_from_slice(bytes);
        }
        out
    }

    fn table<'a>(data: &'a [u8], tag: &[u8; 4]) -> &'a [u8] {
        for i in 0..u16_at(data, 4).unwrap() as usize {
            let at = 12 + i * 16;
            if &data[at..at + 4] == tag {
                let offset = u32_at(data, at + 8).unwrap() as usize;
                return &data[offset..offset + u32_at(data, at + 12).unwrap() as usize];
            }
        }
        panic!("missing table");
    }

    // The shipping pipeline must actually call this pass. Its own unit tests
    // hold `retain_glyphs` to its contract, but they stay green when
    // `convert_document` stops invoking it, which is the way the saving is
    // lost in practice.
    #[test]
    fn conversion_subsets_an_embedded_simple_truetype_program() {
        use lopdf::dictionary;
        let source = font(false);
        let mut doc = Document::with_version("1.7");
        let program = doc.add_object(lopdf::Stream::new(
            dictionary! { "Length1" => source.len() as i64 },
            source.clone(),
        ));
        let descriptor = doc.add_object(dictionary! {
            "Type" => "FontDescriptor", "FontName" => "Owned", "Flags" => 32,
            "FontFile2" => program,
        });
        let font = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "TrueType", "BaseFont" => "Owned",
            "FirstChar" => 65, "LastChar" => 65, "Widths" => vec![500.into()],
            "FontDescriptor" => descriptor,
        });
        let contents = doc.add_object(lopdf::Stream::new(
            dictionary! {},
            b"BT /F1 12 Tf 10 10 Td (A) Tj ET".to_vec(),
        ));
        let pages_id = doc.new_object_id();
        let page = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Contents" => contents,
            "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        });
        doc.objects.insert(
            pages_id,
            dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 }.into(),
        );
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog);

        let report = crate::pdfa::convert_document(&mut doc, &Default::default()).unwrap();
        assert_eq!(report.subsets.programs_subsetted, 1);
        let stored = doc
            .get_object(program)
            .unwrap()
            .as_stream()
            .unwrap()
            .decompressed_content()
            .unwrap();
        assert!(
            stored.len() * 4 < source.len() * 3,
            "program not subsetted: {} of {} bytes",
            stored.len(),
            source.len()
        );
        // Subsetting empties outlines; it never renumbers. Glyph ids and the
        // metrics addressed through them have to survive the pipeline.
        assert_eq!(u16_at(table(&stored, b"maxp"), 4).unwrap(), 512);
        assert_eq!(table(&stored, b"hmtx"), table(&source, b"hmtx"));
    }

    #[test]
    fn retains_composite_components_ids_metrics_cmaps_and_hints() {
        for long in [false, true] {
            let source = font(long);
            let subset = retain_glyphs(&source, &BTreeSet::from([1])).unwrap();
            assert!(subset.len() < source.len() / 2);
            assert_eq!(checksum(&subset), 0xB1B0_AFBA);
            for tag in [b"cmap", b"hmtx", b"hhea", b"maxp", b"prep"] {
                assert_eq!(table(&source, tag), table(&subset, tag));
            }
            let offsets = |data: &[u8], gid: usize| {
                let loca = table(data, b"loca");
                if long {
                    u32_at(loca, gid * 4).unwrap() as usize
                } else {
                    2 * u16_at(loca, gid * 2).unwrap() as usize
                }
            };
            for gid in [0, 1, 300] {
                assert_eq!(
                    &table(&source, b"glyf")[offsets(&source, gid)..offsets(&source, gid + 1)],
                    &table(&subset, b"glyf")[offsets(&subset, gid)..offsets(&subset, gid + 1)]
                );
            }
            assert_eq!(offsets(&subset, 2), offsets(&subset, 3));
            assert!(retain_glyphs(&subset, &BTreeSet::from([1])).is_err());
        }
    }

    #[test]
    fn shared_simple_repertoires_subset_once_and_cid_users_prevent_mutation() {
        use lopdf::{dictionary, Stream};
        let mut doc = Document::with_version("1.7");
        let original = font(true);
        assert!(ttf_parser::Face::parse(&original, 0).is_ok());
        let mut stream = Stream::new(dictionary! {"Length1" => original.len() as i64}, original);
        stream.compress().unwrap();
        let program = doc.add_object(stream);
        let descriptor = doc.add_object(
            dictionary! {"Type" => "FontDescriptor", "FontName" => "Owned", "FontFile2" => program},
        );
        let simple = doc.add_object(dictionary! {"Type" => "Font", "Subtype" => "TrueType", "BaseFont" => "Owned", "FontDescriptor" => descriptor, "Encoding" => "WinAnsiEncoding"});
        let cid = doc.add_object(dictionary! {"Type" => "Font", "Subtype" => "CIDFontType2", "FontDescriptor" => descriptor});
        let before = doc.objects[&program].clone();
        assert_eq!(subset_fonts(&mut doc).programs_subsetted, 0);
        assert_eq!(doc.objects[&program], before);
        doc.objects.remove(&cid);
        let direct = doc.add_object(dictionary! {"Font" => dictionary! {"Inline" => dictionary! {"Subtype" => "TrueType", "FontDescriptor" => descriptor}}});
        assert_eq!(subset_fonts(&mut doc).programs_subsetted, 0);
        assert_eq!(doc.objects[&program], before);
        doc.objects.remove(&direct);
        doc.get_object_mut(program)
            .unwrap()
            .as_stream_mut()
            .unwrap()
            .allows_compression = false;
        let disabled = doc.objects[&program].clone();
        let report = subset_fonts(&mut doc);
        assert_eq!(report.programs_subsetted, 0);
        assert!(report
            .skipped
            .iter()
            .any(|(_, reason)| reason.contains("caller disabled")));
        assert_eq!(doc.objects[&program], disabled);
        doc.get_object_mut(program)
            .unwrap()
            .as_stream_mut()
            .unwrap()
            .allows_compression = true;
        let r = subset_fonts(&mut doc);
        assert_eq!(r.programs_subsetted, 1, "{r:?}");
        assert!(r.bytes_saved > 0);
        assert_eq!(
            name(doc.get_dictionary(simple).unwrap(), b"BaseFont").unwrap()[6],
            b'+'
        );
        assert_eq!(subset_fonts(&mut doc).programs_subsetted, 0);
    }

    #[test]
    fn rejects_truncated_and_out_of_range_programs_without_panicking() {
        let source = font(true);
        for end in [0, 4, 12, 30, source.len() - 1] {
            assert!(retain_glyphs(&source[..end], &BTreeSet::from([1])).is_err());
        }
        assert!(retain_glyphs(&source, &BTreeSet::from([65535])).is_err());
    }
}
