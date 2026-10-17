//! Conservative glyph-preserving font storage compaction.
//!
//! Keep selected glyph IDs, outlines, metrics, cmaps and hint instructions.
//! A strict document-wide text census includes page contents, forms, patterns,
//! appearances and Type3 charprocs. Ambiguous content retains the full
//! simple-font repertoire, and so does a program an interactive form could
//! still draw new text with -- the ones its default resources name, not every
//! program in the file. A COMPOSITE program a form could draw with keeps the
//! repertoire its parent's encoding can address, which is the whole face under
//! `Identity-H` and the CMap's own CIDs under a table this build can read.
//! Identity CID fonts require a complete census and unambiguous parent
//! mappings; unsupported CMaps remain intact.
//! TrueType outlines are repacked; unused CFF charstrings are replaced in place
//! only when every retained outline and advance still agrees with the source.

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
    /// Programs an interactive form could still draw new text with, which are
    /// reduced only to the full simple-font repertoire rather than to the codes
    /// the document draws.
    pub form_protected: usize,
}

fn name<'a>(d: &'a Dictionary, key: &[u8]) -> Option<&'a [u8]> {
    d.get(key).and_then(Object::as_name).ok()
}

fn resolved<'a>(doc: &'a Document, o: &'a Object) -> Option<&'a Object> {
    doc.dereference(o).ok().map(|(_, o)| o)
}

/// A document-wide union deliberately over-retains codes from other fonts. It
/// avoids guessing font/resource inheritance while still removing unused
/// repertoire. Editable forms retain all codes for future appearance values.
#[derive(Default)]
struct TextCodes {
    bytes: BTreeSet<u8>,
    cids: BTreeSet<u16>,
    /// The drawn strings themselves, deduplicated.
    ///
    /// `cids` above reads every two-byte alignment, which is the whole answer
    /// for `Identity-H` and nothing like it for a CMap with a mixed code
    /// space: `90ms-RKSJ-H` writes one byte for ASCII and two for kana, and a
    /// code boundary cannot be recovered from a set of pairs. Keeping the
    /// strings lets a font that names a readable CMap decode them the way its
    /// reader will (`crate::cmap_codes`).
    strings: BTreeSet<Vec<u8>>,
}
impl TextCodes {
    fn insert(&mut self, bytes: &[u8]) {
        self.bytes.extend(bytes);
        // Include both alignments: codes from other fonts only over-retain.
        self.cids
            .extend(bytes.windows(2).map(|p| u16::from_be_bytes([p[0], p[1]])));
        if !bytes.is_empty() {
            self.strings.insert(bytes.to_vec());
        }
    }
}

/// Programs a viewer may still draw text with that is not in the file yet.
///
/// A variable-text field's appearance is regenerated from its `/DA` string, and
/// `/DA` resolves its font in the form's default resources `/DR`, not in the
/// page's (PDF 32000-1 12.7.3.3). An interactive form therefore does not make
/// every program in the document unpredictable -- only the ones a field could
/// be drawn with. Everything else is drawn by content that is already here,
/// which is what the census reads.
///
/// What naming them buys is not "leave them alone". A program a field could be
/// drawn with keeps the FULL SIMPLE-FONT REPERTOIRE -- every code 0..255 the
/// encoding can address -- which is exactly what every font in an interactive
/// form used to get, and is what protects a character nobody has typed yet.
/// What it does not keep is the rest of a face: a substitute for Arial Unicode
/// carries fifty thousand glyphs and a byte-addressed simple font can reach two
/// hundred and fifty-six of them. Blocking those programs outright instead was
/// measured and was worse -- seven gate-corpus documents grew, one from 28.9%
/// of its input to 62.0%, because leaving a 700 kB face alone costs more than
/// the whole census saves.
///
/// A COMPOSITE program named here is not left alone either, and until #513 it
/// was: `subset_fonts` required `!form_programs.contains(&program)`, so a `/DR`
/// font with a descendant went out whole whatever its parent's encoding was.
/// The repertoire of a composite font is what its encoding addresses --
/// every CID under `Identity-H`, and only the CIDs the table produces under a
/// CMap this build can read (`form_repertoire_cids`).
///
/// `Err` means the document is unpredictable as a whole, and every program in
/// it falls back to the same full repertoire. That is a form whose fonts cannot
/// be named: no `/DR`, no `/Font` in it, an `/AcroForm` that will not resolve --
/// and an XFA form, whose appearances are built by a different engine from a
/// template this module does not read.
fn form_regenerable_programs(doc: &Document) -> Result<BTreeSet<ObjectId>, &'static str> {
    let Ok(catalog) = doc.catalog() else {
        return Err("no catalog");
    };
    let Ok(form) = catalog.get(b"AcroForm") else {
        return Ok(BTreeSet::new());
    };
    let Some(form) = resolved(doc, form).and_then(|o| o.as_dict().ok()) else {
        return Err("an AcroForm entry that does not resolve to a dictionary");
    };
    if form.has(b"XFA") {
        return Err("an XFA form generates its appearances from its own template");
    }
    // An /AcroForm with no fields is a leftover dictionary: there is nothing to
    // fill in, so there is nothing a viewer would regenerate.
    let has_fields = form
        .get(b"Fields")
        .ok()
        .and_then(|o| resolved(doc, o))
        .and_then(|o| o.as_array().ok())
        .is_some_and(|fields| !fields.is_empty());
    if !has_fields {
        return Ok(BTreeSet::new());
    }
    let Some(fonts) = form
        .get(b"DR")
        .ok()
        .and_then(|o| resolved(doc, o))
        .and_then(|o| o.as_dict().ok())
        .and_then(|dr| dr.get(b"Font").ok())
        .and_then(|o| resolved(doc, o))
        .and_then(|o| o.as_dict().ok())
    else {
        return Err("a form whose default resources name no font");
    };
    let mut programs = BTreeSet::new();
    for (_, font) in fonts.iter() {
        let Some(font) = resolved(doc, font).and_then(|o| o.as_dict().ok()) else {
            return Err("a default resource font that does not resolve");
        };
        // A composite default-resource font reaches its program through its
        // descendant, so the descendants are walked with the parent.
        let descendants = font
            .get(b"DescendantFonts")
            .ok()
            .and_then(|o| resolved(doc, o))
            .and_then(|o| o.as_array().ok())
            .map(|a| a.to_vec())
            .unwrap_or_default();
        for font in std::iter::once(Object::Dictionary(font.clone())).chain(descendants) {
            let Some(font) = resolved(doc, &font).and_then(|o| o.as_dict().ok()) else {
                return Err("a default resource descendant that does not resolve");
            };
            let Some(fd) = font
                .get(b"FontDescriptor")
                .ok()
                .and_then(|o| resolved(doc, o))
                .and_then(|o| o.as_dict().ok())
            else {
                continue;
            };
            for key in [b"FontFile".as_slice(), b"FontFile2", b"FontFile3"] {
                if let Ok(id) = fd.get(key).and_then(Object::as_reference) {
                    programs.insert(id);
                }
            }
        }
    }
    Ok(programs)
}

fn document_text_codes(doc: &Document) -> Option<TextCodes> {
    fn walk(
        doc: &Document,
        obj: &Object,
        seen: &mut BTreeSet<ObjectId>,
        streams: &mut BTreeSet<ObjectId>,
        depth: usize,
    ) -> Option<()> {
        if depth > 64 {
            return None;
        }
        match obj {
            Object::Reference(id) => {
                if seen.insert(*id) {
                    let obj = doc.get_object(*id).ok()?;
                    if matches!(obj, Object::Stream(_)) {
                        streams.insert(*id);
                    } else {
                        walk(doc, obj, seen, streams, depth + 1)?;
                    }
                }
            }
            Object::Array(a) => {
                for o in a {
                    walk(doc, o, seen, streams, depth + 1)?;
                }
            }
            Object::Dictionary(d) => {
                for (_, o) in d.iter() {
                    walk(doc, o, seen, streams, depth + 1)?;
                }
            }
            Object::Null => {}
            _ => return None,
        }
        Some(())
    }
    let mut streams = BTreeSet::new();
    let mut seen = BTreeSet::new();
    for (&id, obj) in &doc.objects {
        if let Object::Stream(s) = obj {
            if name(&s.dict, b"Subtype") == Some(b"Form")
                || s.dict.has(b"PatternType")
                || s.dict.has(b"Resources")
            {
                streams.insert(id);
            }
        }
        if let Ok(d) = obj.as_dict() {
            for key in [b"Contents".as_slice(), b"AP", b"CharProcs"] {
                if key == b"Contents" && name(d, b"Type") != Some(b"Page") {
                    continue;
                }
                if let Ok(o) = d.get(key) {
                    walk(doc, o, &mut seen, &mut streams, 0)?;
                }
            }
        }
    }
    let mut codes = TextCodes::default();
    for id in streams {
        let s = doc.get_object(id).ok()?.as_stream().ok()?;
        let bytes = s.get_plain_content().ok()?;
        let editor = crate::content_editor::ContentEditor::from_stream_strict(&bytes).ok()?;
        for op in editor.operations() {
            match op.operator.as_str() {
                "Tj" | "'" | "\"" => {
                    let i = if op.operator == "\"" { 2 } else { 0 };
                    codes.insert(op.operands.get(i)?.as_str().ok()?);
                }
                "TJ" => {
                    for o in op.operands.first()?.as_array().ok()? {
                        match o {
                            Object::String(bytes, _) => codes.insert(bytes),
                            Object::Integer(_) | Object::Real(_) => {}
                            _ => return None,
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Some(codes)
}

/// The Type0 fonts that name `descendant`, or nothing when the shape is not
/// the one-parent-one-descendant chain this module reasons about.
///
/// The encoding is not judged here. What a parent's CMap means for the census
/// is [`composite_reach`]; what it means for the subset tag is nothing, and
/// the tag has to reach the parent's `/BaseFont` whatever the CMap is, because
/// ISO 32000-1 9.7.6.1 has the two names agree.
fn composite_parents(doc: &Document, descendant: ObjectId) -> Option<Vec<ObjectId>> {
    let mut parents = Vec::new();
    for (&id, obj) in &doc.objects {
        let Ok(d) = obj.as_dict() else { continue };
        let Some(a) = d
            .get(b"DescendantFonts")
            .ok()
            .and_then(|o| resolved(doc, o))
            .and_then(|o| o.as_array().ok())
        else {
            continue;
        };
        if !a.iter().any(|o| o.as_reference().ok() == Some(descendant)) {
            continue;
        }
        if a.len() != 1 {
            return None;
        }
        parents.push(id);
    }
    (!parents.is_empty()).then_some(parents)
}

/// How the bytes a document draws reach the glyphs of one composite font.
///
/// A CIDFontType2 is addressed twice over: the parent's CMap turns bytes into
/// CIDs, and `/CIDToGIDMap` turns CIDs into glyph ids. This module used to
/// require the first step to be `Identity-H`/`Identity-V`, where the two-byte
/// code *is* the CID, and refused every other composite font -- correctly,
/// because reading a `GBKp-EUC-H` code as a CID is the defect `pdfa_fonts`
/// documents at length.
///
/// Refusing is not free. An unembedded CID font is given a substitute face,
/// and since `83066e4f6` that face is a whole SFNT rather than a bare CFF:
/// four of them, 410 kB each, is what `pdfjs__test_pdfs_issue3521.pdf` grew by
/// (4,486 bytes in, 876,199 out) while the subsetter declined to open them
/// (#503). The two ways out below are both upper bounds a reader cannot
/// exceed, so nothing the document can draw is dropped.
enum CompositeReach {
    /// Every parent is `Identity-H`/`Identity-V`: the code is the CID.
    Identity,
    /// Every parent's CMap can be read: one it embeds, one of the predefined
    /// tables in `crate::cmap_codes`, or `Identity` itself. The drawn strings
    /// decode to CIDs exactly as the reader will decode them.
    ///
    /// `identity_parent` is not a detail. One descendant may be named by two
    /// Type0 fonts with different encodings, and a CID that only the Identity
    /// parent reaches appears in no table's answer: reading the tables alone
    /// would delete a glyph that parent draws.
    Tables {
        tables: Vec<crate::cmap_codes::CodeCidMap>,
        identity_parent: bool,
    },
    /// No table for the CMap, but the descendant carries a `/CIDToGIDMap`
    /// stream. Whatever the CMap says a code means, the glyph it reaches is
    /// one this stream names, so the stream's own values are a complete and
    /// safe upper bound -- and on a Latin substitute under a CJK CMap they are
    /// a small one.
    MappedGlyphs,
}

/// A `/CIDToGIDMap` stream's bytes, or `None` for the `/Identity` default.
///
/// `Err` is a map this module will not guess at: a name other than
/// `/Identity`, a stream that will not decode, an odd length.
fn cid_to_gid_map(doc: &Document, font: &Dictionary) -> Result<Option<Vec<u8>>, &'static str> {
    match font.get(b"CIDToGIDMap") {
        Err(_) => Ok(None),
        Ok(Object::Name(n)) if n.as_slice() == b"Identity" => Ok(None),
        Ok(Object::Name(_)) => Err("unsupported CIDToGIDMap"),
        Ok(obj) => {
            let s = resolved(doc, obj)
                .and_then(|o| o.as_stream().ok())
                .ok_or("unsupported CIDToGIDMap")?;
            let data = s
                .get_plain_content()
                .map_err(|_| "undecodable CIDToGIDMap")?;
            if data.len() % 2 != 0 {
                return Err("odd CIDToGIDMap");
            }
            Ok(Some(data))
        }
    }
}

/// The CMap program of a Type0 font's `/Encoding`, when it is a stream.
fn embedded_cmap(doc: &Document, parent: &Dictionary) -> Option<Vec<u8>> {
    let enc = parent.get(b"Encoding").ok()?;
    resolved(doc, enc)
        .and_then(|o| o.as_stream().ok())
        .and_then(|s| s.get_plain_content().ok())
}

/// Which of the three routes above this descendant offers, or nothing.
///
/// `cache` keeps a named table parsed once. The predefined files run to two
/// hundred kilobytes and a document can name the same one from every font it
/// has.
fn composite_reach(
    doc: &Document,
    descendant: ObjectId,
    cache: &mut BTreeMap<String, Option<crate::cmap_codes::CodeCidMap>>,
) -> Option<CompositeReach> {
    let parents = composite_parents(doc, descendant)?;
    let mut tables = Vec::new();
    let mut identity_parent = false;
    let mut all_identity = true;
    for parent in &parents {
        let Ok(d) = doc.get_dictionary(*parent) else {
            return None;
        };
        if matches!(name(d, b"Encoding"), Some(b"Identity-H" | b"Identity-V")) {
            identity_parent = true;
            continue;
        }
        all_identity = false;
        let table = match name(d, b"Encoding") {
            Some(named) => {
                let named = String::from_utf8_lossy(named).into_owned();
                cache
                    .entry(named.clone())
                    .or_insert_with(|| crate::cmap_codes::predefined(&named))
                    .clone()
            }
            None => embedded_cmap(doc, d)
                .and_then(|data| crate::cmap_codes::CodeCidMap::parse(&data, &|_| None)),
        };
        match table {
            Some(table) => tables.push(table),
            // One unreadable parent makes the whole program unreadable: it is
            // shared, and the glyphs it can reach through this parent are not
            // covered by the tables the others gave.
            None => {
                let descendant = doc.get_dictionary(descendant).ok()?;
                return matches!(cid_to_gid_map(doc, descendant), Ok(Some(_)))
                    .then_some(CompositeReach::MappedGlyphs);
            }
        }
    }
    Some(if all_identity {
        CompositeReach::Identity
    } else {
        CompositeReach::Tables {
            tables,
            identity_parent,
        }
    })
}

/// Every CID the document can ask this composite font for.
///
/// Over-retention is deliberate throughout: the census is document-wide, so a
/// string drawn with another font is decoded here too. An extra outline costs
/// bytes; a missing one costs a character.
fn composite_cids(
    reach: &CompositeReach,
    codes: &TextCodes,
) -> Result<BTreeSet<u32>, &'static str> {
    match reach {
        CompositeReach::Identity => Ok(codes.cids.iter().map(|cid| u32::from(*cid)).collect()),
        CompositeReach::Tables {
            tables,
            identity_parent,
        } => {
            let mut cids = BTreeSet::new();
            // An Identity parent reads the two-byte code as the CID, and no
            // table says so on its behalf.
            if *identity_parent {
                cids.extend(codes.cids.iter().map(|cid| u32::from(*cid)));
            }
            // The document's codes are gathered per table and resolved once.
            // Resolving per string instead re-scans the table for every
            // occurrence of a code, and a predefined Unicode table is more
            // than ten thousand entries long.
            for table in tables {
                let mut drawn = BTreeSet::new();
                for string in &codes.strings {
                    table.codes_or_superset(string, &mut drawn);
                }
                table.cids_of(&drawn, &mut cids);
            }
            Ok(cids)
        }
        // The caller reads the map itself; there is no CID step to take.
        CompositeReach::MappedGlyphs => Err("CID usage unresolved"),
    }
}

/// Every CID an interactive form field could ask this composite font for.
///
/// A program a field could be drawn with keeps a repertoire rather than a
/// census: nobody has typed the text yet, so no count of the content that
/// exists bounds it. For a simple font that repertoire is every code 0..255
/// the encoding can address (#379 round 3). This is the same answer for a
/// composite font, and the same reasoning decides which encodings have one:
///
/// * an `Identity` parent reads the two-byte code as the CID, so a field can
///   reach EVERY glyph in the face and there is no repertoire short of the
///   whole program. That is why such a program is still kept whole.
/// * a parent whose CMap this build can read addresses only the CIDs that CMap
///   produces. `UniJIS-UTF16-H` over a Latin substitute is the measured case
///   (#513): 2,268 of the face's 2,620 glyphs, and the other 352 are ones no
///   text a viewer regenerates can select.
/// * `MappedGlyphs` never reaches here -- the caller reads `/CIDToGIDMap`
///   itself, and the gids that stream names are a bound whatever is typed.
///
/// This is deliberately NOT the census: `codes` is not consulted, because the
/// bytes the document draws are the one thing this bound may not depend on.
fn form_repertoire_cids(reach: &CompositeReach) -> Result<BTreeSet<u32>, &'static str> {
    match reach {
        CompositeReach::Identity => Err("an Identity parent reaches every CID of a form font"),
        CompositeReach::Tables {
            identity_parent: true,
            ..
        } => Err("an Identity parent reaches every CID of a form font"),
        CompositeReach::Tables { tables, .. } => {
            let mut cids = BTreeSet::new();
            for table in tables {
                table.produced_cids(&mut cids);
            }
            Ok(cids)
        }
        CompositeReach::MappedGlyphs => Err("CID usage unresolved"),
    }
}

/// Compact safely addressable TrueType and CFF programs without remapping glyph IDs.
pub(crate) fn subset_fonts(doc: &mut Document) -> SubsetReport {
    let mut report = SubsetReport::default();
    let (used_codes, form_programs) = match form_regenerable_programs(doc) {
        Ok(programs) => (document_text_codes(doc), programs),
        // A form whose fonts this module cannot name leaves the whole document
        // unpredictable, exactly as every form used to.
        Err(_) => (None, BTreeSet::new()),
    };
    report.form_protected = form_programs.len();
    // Read each composite font's route to its glyphs once, before anything is
    // judged: a named CMap table is parsed here and nowhere else.
    let mut cmap_cache = BTreeMap::new();
    let mut reach: BTreeMap<ObjectId, CompositeReach> = BTreeMap::new();
    for (&font_id, obj) in &doc.objects {
        let Ok(font) = obj.as_dict() else { continue };
        if !matches!(
            name(font, b"Subtype"),
            Some(b"CIDFontType0" | b"CIDFontType2")
        ) {
            continue;
        }
        if let Some(found) = composite_reach(doc, font_id, &mut cmap_cache) {
            reach.insert(font_id, found);
        }
    }
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
            let supported = (name(font, b"Subtype") == Some(b"TrueType") && key == b"FontFile2")
                || (name(font, b"Subtype") == Some(b"Type1")
                    && key == b"FontFile3"
                    && doc
                        .get_object(program)
                        .ok()
                        .and_then(|o| o.as_stream().ok())
                        .is_some_and(|s| name(&s.dict, b"Subtype") == Some(b"Type1C")));
            // A composite font needs a resolved census, and a program a form
            // field could draw with does not have one: nobody has typed the
            // text yet. What replaces the census is the repertoire the parent's
            // ENCODING can address, which is the simple-font answer of #379
            // round 3 read for a composite font -- so the rule that used to
            // refuse every such program now refuses only the ones whose
            // encoding reaches the whole face (#513).
            let bounded_without_the_census = reach.get(font_id).is_some_and(|reach| {
                matches!(reach, CompositeReach::MappedGlyphs) || form_repertoire_cids(reach).is_ok()
            });
            let supported_cid = used_codes.is_some()
                && (!form_programs.contains(&program) || bounded_without_the_census)
                && reach.contains_key(font_id)
                && ((name(font, b"Subtype") == Some(b"CIDFontType2") && key == b"FontFile2")
                    || (name(font, b"Subtype") == Some(b"CIDFontType0")
                        && key == b"FontFile3"
                        && doc
                            .get_object(program)
                            .ok()
                            .and_then(|o| o.as_stream().ok())
                            .is_some_and(|s| name(&s.dict, b"Subtype") == Some(b"CIDFontType0C"))));
            if !supported && !supported_cid {
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
        let a_field_could_draw_with_it = form_programs.contains(&program);
        // The census is per program from here on: a program a form field could
        // be drawn with is read as though the document had no census at all,
        // which retains every code the encoding can address.
        let used_codes = if a_field_could_draw_with_it {
            None
        } else {
            used_codes.as_ref()
        };
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
            let subset = if matches!(
                name(&stream.dict, b"Subtype"),
                Some(b"Type1C" | b"CIDFontType0C")
            ) {
                let cff = cff_parser::Table::parse(&data).ok_or("unparseable CFF")?;
                let mut keep = BTreeSet::from([0]);
                for (font_id, _) in &fonts {
                    let font = doc.get_dictionary(*font_id).map_err(|_| "missing font")?;
                    if name(font, b"Subtype") == Some(b"CIDFontType0") {
                        let reach = reach.get(font_id).ok_or("CID usage unresolved")?;
                        // A form program has no census; it has a repertoire.
                        let cids = match used_codes.as_ref() {
                            Some(codes) => composite_cids(reach, codes)?,
                            None => form_repertoire_cids(reach)?,
                        };
                        for gid in 0..cff.number_of_glyphs() {
                            // A CID-keyed CFF says which CID each glyph is.
                            // A CIDFontType0 may also carry a CFF that is NOT
                            // CID-keyed, and ISO 32000-1 9.7.4.2 then reads the
                            // CID as the glyph index -- `issue9534_reduced`
                            // does exactly that, 494 glyphs and not one of them
                            // with a CID. Asking only the charset there keeps
                            // `.notdef` and nothing else, and the page comes
                            // out blank.
                            let reached = match cff.glyph_cid(cff_parser::GlyphId(gid)) {
                                Some(cid) => cids.contains(&u32::from(cid)),
                                None => cids.contains(&u32::from(gid)),
                            };
                            if reached {
                                keep.insert(gid);
                            }
                        }
                        continue;
                    }
                    let (encoding, differences) =
                        crate::pdfa_fonts::get_simple_encoding_info(doc, font);
                    if !matches!(
                        encoding.as_str(),
                        "" | "WinAnsiEncoding" | "MacRomanEncoding" | "StandardEncoding"
                    ) {
                        return Err("unsupported simple CFF encoding");
                    }
                    for code in 0..=255u32 {
                        if used_codes
                            .as_ref()
                            .is_some_and(|codes| !codes.bytes.contains(&(code as u8)))
                        {
                            continue;
                        }
                        // A lookup that answers `.notdef` has not mapped the
                        // code: the font is saying it has no glyph for it, and
                        // a reader with nothing else to try still reads the
                        // code as an index.
                        let mut mapped = false;
                        if let Some(g) = cff.glyph_index(code as u8) {
                            keep.insert(g.0);
                            mapped |= g.0 != 0;
                        }
                        if let Some(n) = differences.get(&code) {
                            if let Some(g) = cff.glyph_index_by_name(n) {
                                keep.insert(g.0);
                                mapped |= g.0 != 0;
                            }
                        }
                        let c = crate::pdfa_fonts::encoding_to_char(code, &encoding);
                        let alternate = differences
                            .get(&code)
                            .and_then(|n| crate::pdfa_fonts::glyph_name_to_unicode(n));
                        if differences.get(&code).is_some_and(|n| {
                            n != ".notdef"
                                && cff.glyph_index_by_name(n).is_none()
                                && alternate.is_none()
                        }) {
                            return Err("unresolved CFF glyph name");
                        }
                        for gid in 0..cff.number_of_glyphs() {
                            if cff
                                .glyph_name(cff_parser::GlyphId(gid))
                                .and_then(crate::pdfa_fonts::glyph_name_to_unicode)
                                .is_some_and(|u| u == c || Some(u) == alternate)
                            {
                                keep.insert(gid);
                                mapped |= gid != 0;
                            }
                        }
                        // Same rule as the TrueType path above: the code is a
                        // glyph index only where nothing else mapped it.
                        if !mapped && code < u32::from(cff.number_of_glyphs()) {
                            keep.insert(code as u16);
                        }
                    }
                }
                crate::cff_append::retain_charstrings(&data, &keep)?
            } else {
                let face = ttf_parser::Face::parse(&data, 0).map_err(|_| "unparseable font")?;
                let mut keep = BTreeSet::from([0]);
                for (font_id, _) in &fonts {
                    let font = doc
                        .get_dictionary(*font_id)
                        .map_err(|_| "missing font dictionary")?;
                    if name(font, b"Subtype") == Some(b"CIDFontType2") {
                        let reach = reach.get(font_id).ok_or("CID usage unresolved")?;
                        let mapping = cid_to_gid_map(doc, font)?;
                        if matches!(reach, CompositeReach::MappedGlyphs) {
                            // Nothing decodes the codes here. Every glyph the
                            // map names stays, and no other glyph is reachable.
                            let map = mapping.as_ref().ok_or("CID usage unresolved")?;
                            for pair in map.chunks_exact(2) {
                                let gid = u16::from_be_bytes([pair[0], pair[1]]);
                                if gid < face.number_of_glyphs() {
                                    keep.insert(gid);
                                }
                            }
                            continue;
                        }
                        // A form program has no census; it has a repertoire.
                        let cids = match used_codes.as_ref() {
                            Some(codes) => composite_cids(reach, codes)?,
                            None => form_repertoire_cids(reach)?,
                        };
                        for cid in cids {
                            // ISO 32000-1 9.7.4.2: a CID is 0..65535. Anything
                            // else came from a malformed table and indexes
                            // nothing -- and `cid as usize * 2` on a 32-bit
                            // target would overflow rather than miss (Codex, on
                            // the review of #503).
                            let Ok(cid) = u16::try_from(cid) else {
                                continue;
                            };
                            let gid = if let Some(map) = &mapping {
                                let i = usize::from(cid) * 2;
                                map.get(i..i + 2)
                                    .map(|p| u16::from_be_bytes([p[0], p[1]]))
                                    .unwrap_or(0)
                            } else {
                                cid
                            };
                            if gid < face.number_of_glyphs() {
                                keep.insert(gid);
                            }
                        }
                        continue;
                    }
                    let (encoding, differences) =
                        crate::pdfa_fonts::get_simple_encoding_info(doc, font);
                    if !matches!(
                        encoding.as_str(),
                        "" | "WinAnsiEncoding" | "MacRomanEncoding" | "StandardEncoding"
                    ) {
                        return Err("unsupported simple encoding");
                    }
                    for code in 0..=255u32 {
                        if used_codes
                            .as_ref()
                            .is_some_and(|codes| !codes.bytes.contains(&(code as u8)))
                        {
                            continue;
                        }
                        let mut mapped = false;
                        // A lookup that answers `.notdef` has not mapped the
                        // code; see the CFF path above.
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
                                mapped |= g.0 != 0;
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
                                        mapped |= g.0 != 0;
                                    }
                                }
                            }
                        }
                        // THE DIRECT GLYPH-INDEX FALLBACK, WHERE IT IS THE ONLY
                        // MAPPING LEFT.
                        //
                        // A reader that cannot map a code through the font's
                        // character map or the encoding's glyph names has one
                        // thing left to try: read the code as a glyph index.
                        // That is a real path -- symbolic TrueType subsets are
                        // written with no usable cmap and it is how they draw --
                        // so the glyph stays for a code nothing else resolved.
                        //
                        // It used to stay for EVERY code in use, and that is a
                        // different claim: it keeps up to 256 outlines chosen by
                        // their position in the file rather than by anything the
                        // document draws. On a substitute face embedded for a
                        // Standard 14 name, whose cmap maps every code the page
                        // shows, those 256 are pure storage -- 1.8 MB over the
                        // 300-document tuning set (#379 round 3).
                        if !mapped && code < u32::from(face.number_of_glyphs()) {
                            keep.insert(code as u16);
                        }
                    }
                }
                retain_glyphs(&data, &keep)?
            };
            let digest = Sha256::digest(&subset);
            let tag: String = digest[..6]
                .iter()
                .map(|b| char::from(b'A' + b % 26))
                .collect();
            let mut replacement = stream.clone();
            replacement.set_plain_content(subset.clone());
            replacement.dict.remove(b"DecodeParms");
            if replacement.dict.has(b"Length1") {
                replacement.dict.set("Length1", subset.len() as i64);
            }
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
                    let mut names = vec![(font, b"BaseFont".as_slice()), (descriptor, b"FontName")];
                    if let Some(parents) = composite_parents(doc, font) {
                        names.extend(parents.into_iter().map(|id| (id, b"BaseFont".as_slice())));
                    }
                    for (id, key) in names {
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
                "unsupported program, CMap or direct/shared font requires a different subset strategy".into(),
            ));
        }
    }
    report
}

fn compact_post_names(post: &[u8], keep: &BTreeSet<u16>) -> Option<Vec<u8>> {
    if u32_at(post, 0).ok()? != 0x0002_0000 {
        return None;
    }
    let count = usize::from(u16_at(post, 32).ok()?);
    let indices = (0..count)
        .map(|i| u16_at(post, 34 + i * 2))
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    let custom_count = usize::from(indices.iter().copied().max()?.saturating_sub(257));
    let mut names = Vec::new();
    let mut cursor = 34 + count * 2;
    for _ in 0..custom_count {
        let len = usize::from(*post.get(cursor)?);
        names.push(post.get(cursor..cursor + len + 1)?);
        cursor += len + 1;
    }
    let mut out = post.get(..34)?.to_vec();
    let mut selected = BTreeMap::new();
    let mut strings = Vec::new();
    for (gid, &index) in indices.iter().enumerate() {
        let index = if !keep.contains(&(gid as u16)) {
            0
        } else if index < 258 {
            index
        } else {
            *selected.entry(index).or_insert_with(|| {
                let mapped = 258 + strings.len() as u16;
                strings.push(names[usize::from(index - 258)]);
                mapped
            })
        };
        out.extend_from_slice(&index.to_be_bytes());
    }
    for name in strings {
        out.extend_from_slice(name);
    }
    Some(out)
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
        b"fvar", b"gvar", b"COLR", b"SVG ", b"sbix", b"CBDT", b"EBDT",
    ] {
        if tables.contains_key(tag) {
            return Err("variable or color font");
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
    // Per-glyph auxiliary data for discarded outlines need not keep random
    // bytes alive in the compressed stream. Retained glyph metrics and names
    // remain unchanged, including names used by simple-font fallback lookup.
    if let Some(hdmx) = tables.get_mut(b"hdmx") {
        if let (Ok(records), Ok(size)) = (u16_at(hdmx, 2), u32_at(hdmx, 4)) {
            let size = size as usize;
            if size >= glyph_count + 2
                && 8usize
                    .checked_add(size.saturating_mul(records as usize))
                    .is_some_and(|n| n <= hdmx.len())
            {
                for record in 0..records as usize {
                    for gid in 0..glyph_count {
                        if !keep.contains(&(gid as u16)) {
                            hdmx[8 + record * size + 2 + gid] = 0;
                        }
                    }
                    let start = 8 + record * size;
                    hdmx[start + 1] = hdmx[start + 2..start + 2 + glyph_count]
                        .iter()
                        .copied()
                        .max()
                        .unwrap_or(0);
                }
            }
        }
    }
    if let Some(post) = tables.get(b"post") {
        if let Some(compact) = compact_post_names(post, &keep) {
            tables.insert(*b"post", compact);
        }
    }
    tables.insert(*b"glyf", new_glyf);
    tables.insert(*b"loca", new_loca);
    let head = tables.get_mut(b"head").unwrap();
    head[8..12].fill(0);
    // An original font signature cannot authenticate a subset. Drop DSIG only
    // after all embedding/subsetting restrictions and outline checks passed.
    // Retained glyph rendering and metrics survive; this is not a PDF signature.
    tables.remove(b"DSIG");
    // OPENTYPE LAYOUT IS NOT A THING A PDF READER CAN REACH.
    //
    // Which glyph is drawn and where it sits are both fixed by the content
    // stream -- the character codes, the font's `/Widths` or `/W`, and the
    // offsets inside `TJ`. No PDF reader runs `GSUB` to pick a ligature or
    // `GPOS`/`kern` to move a glyph; a viewer that did would disagree with the
    // widths the file declares, which is exactly what 6.2.11.5 measures.
    // These tables are therefore payload with no reader, and on the substitute
    // faces they are the payload: `GPOS` alone is 76,664 of LiberationSans's
    // 410,712 bytes, so an all-glyph subset of it still shipped 120 kB per
    // style on `issue3521` (#503).
    //
    // What stays: `cmap` (a simple TrueType font is addressed through it),
    // `post` (glyph names answer a `/Differences` a reader cannot otherwise
    // resolve, and it is compacted above), `hmtx`/`hhea` (the metrics veraPDF
    // compares against `/W`), and the hinting tables `fpgm`/`prep`/`cvt `.
    for tag in [
        b"GPOS", b"GSUB", b"GDEF", b"BASE", b"JSTF", b"MATH", b"kern", b"LTSH", b"VDMX", b"PCLT",
        b"FFTM",
    ] {
        tables.remove(tag);
    }
    let count = tables.len();
    let mut out = data[..12].to_vec();
    let power = (count as u16).ilog2() as u16;
    let search_range = 16u16 << power;
    out[4..6].copy_from_slice(&(count as u16).to_be_bytes());
    out[6..8].copy_from_slice(&search_range.to_be_bytes());
    out[8..10].copy_from_slice(&power.to_be_bytes());
    out[10..12].copy_from_slice(&(16 * count as u16 - search_range).to_be_bytes());
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
    use lopdf::dictionary;

    /// A catalogue carrying the given `/AcroForm` dictionary, and nothing else
    /// this function reads.
    fn with_form(form: Dictionary) -> Document {
        let mut doc = Document::with_version("1.7");
        let form_id = doc.add_object(Object::Dictionary(form));
        let pages_id = doc.add_object(dictionary! {
            "Type" => "Pages",
            "Kids" => Vec::<Object>::new(),
            "Count" => 0_i64,
        });
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
            "AcroForm" => Object::Reference(form_id),
        });
        doc.trailer.set("Root", Object::Reference(catalog_id));
        doc
    }

    fn a_field() -> Vec<Object> {
        vec![Object::Dictionary(dictionary! { "FT" => "Tx" })]
    }

    /// A form dictionary with one field and one font in its default resources.
    fn form_with_default_font(doc_font: ObjectId) -> Dictionary {
        dictionary! {
            "Fields" => a_field(),
            "DR" => dictionary! { "Font" => dictionary! { "D0" => Object::Reference(doc_font) } },
        }
    }

    /// PDF/A removes an `/XFA` entry (ISO 19005-2 6.4.2:1) before this pass
    /// runs, so a conversion never reaches this branch -- which is exactly why
    /// it is tested here. A caller running the pass on an untouched document
    /// gets the refusal, because what an XFA template draws with is decided by
    /// a template this module does not read and not by `/DR`.
    #[test]
    fn an_xfa_form_leaves_nothing_reducible() {
        let mut doc = with_form(dictionary! {});
        let font = doc.add_object(dictionary! { "Type" => "Font" });
        let mut form = form_with_default_font(font);
        form.set("XFA", Object::string_literal("<xdp/>"));
        let doc = {
            let mut doc = with_form(form);
            doc.add_object(dictionary! { "Type" => "Font" });
            doc
        };
        assert!(
            form_regenerable_programs(&doc).is_err(),
            "an XFA form must stop every reduction, not hand back a list from /DR"
        );
    }

    #[test]
    fn a_form_that_names_a_font_names_its_program() {
        let mut doc = Document::with_version("1.7");
        let program = doc.add_object(Object::Stream(lopdf::Stream::new(
            dictionary! { "Length1" => 4_i64 },
            b"font".to_vec(),
        )));
        let descriptor = doc.add_object(dictionary! {
            "Type" => "FontDescriptor",
            "FontFile2" => Object::Reference(program),
        });
        let font = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "TrueType",
            "FontDescriptor" => Object::Reference(descriptor),
        });
        let form_id = doc.add_object(Object::Dictionary(form_with_default_font(font)));
        let pages_id = doc.add_object(dictionary! {
            "Type" => "Pages",
            "Kids" => Vec::<Object>::new(),
            "Count" => 0_i64,
        });
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
            "AcroForm" => Object::Reference(form_id),
        });
        doc.trailer.set("Root", Object::Reference(catalog_id));

        assert_eq!(
            form_regenerable_programs(&doc).expect("an ordinary form is readable"),
            BTreeSet::from([program]),
            "the program behind the default resource font is the one a field \
             could be drawn with"
        );
    }

    #[test]
    fn a_document_without_a_form_exempts_nothing() {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.add_object(dictionary! {
            "Type" => "Pages",
            "Kids" => Vec::<Object>::new(),
            "Count" => 0_i64,
        });
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        });
        doc.trailer.set("Root", Object::Reference(catalog_id));
        assert_eq!(
            form_regenerable_programs(&doc).expect("no form is not a refusal"),
            BTreeSet::new(),
        );
    }

    // Generated SFNT with 512 glyph slots. Glyph 1 references glyph 300;
    // the cmap, names, metrics and interpreter tables must remain unchanged.
    fn font(long: bool) -> Vec<u8> {
        font_with_tables(long, &[])
    }

    fn font_with_tables(long: bool, extra: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
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
        tables.extend(extra.iter().cloned());
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
        let source = font_with_tables(false, &[(*b"DSIG", vec![0; 8])]);
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
    fn shipping_conversion_compacts_cff_while_retaining_a_drawn_outline() {
        use lopdf::{dictionary, Stream};
        let original = pdf_standard_fonts::StandardFont::Helvetica.data();
        let mut doc = Document::with_version("1.7");
        let program = doc.add_object(Stream::new(
            dictionary! {"Subtype"=>"Type1C"},
            original.to_vec(),
        ));
        let descriptor = doc.add_object(dictionary! {
            "Type"=>"FontDescriptor", "FontName"=>"Helvetica", "Flags"=>32,
            "FontFile3"=>program, "FontBBox"=>vec![(-200).into(),(-300).into(),1200.into(),1000.into()],
            "Ascent"=>800, "Descent"=>-200, "CapHeight"=>700, "ItalicAngle"=>0, "StemV"=>80,
        });
        let font = doc.add_object(dictionary! {
            "Type"=>"Font", "Subtype"=>"Type1", "BaseFont"=>"Helvetica", "Encoding"=>"WinAnsiEncoding",
            "FontDescriptor"=>descriptor, "FirstChar"=>65, "LastChar"=>65, "Widths"=>vec![667.into()],
        });
        let contents = doc.add_object(Stream::new(
            dictionary! {},
            b"BT /F 12 Tf 20 20 Td (A) Tj ET".to_vec(),
        ));
        let pages = doc.new_object_id();
        let page = doc.add_object(
            dictionary! {"Type"=>"Page", "Parent"=>pages, "Contents"=>contents,
            "MediaBox"=>vec![0.into(),0.into(),200.into(),200.into()],
            "Resources"=>dictionary! {"Font"=>dictionary! {"F"=>font}}},
        );
        doc.objects.insert(
            pages,
            dictionary! {"Type"=>"Pages", "Kids"=>vec![page.into()], "Count"=>1}.into(),
        );
        let root = doc.add_object(dictionary! {"Type"=>"Catalog", "Pages"=>pages});
        doc.trailer.set("Root", root);
        let result = crate::pdfa::convert_document(&mut doc, &Default::default()).unwrap();
        assert_eq!(result.subsets.programs_subsetted, 1, "{result:?}");
        let stream = doc.get_object(program).unwrap().as_stream().unwrap();
        let data = stream.get_plain_content().unwrap();
        let before = cff_parser::Table::parse(original).unwrap();
        let after = cff_parser::Table::parse(&data).unwrap();
        let a = before.glyph_index_by_name("A").unwrap();
        assert_eq!(after.glyph_index_by_name("A"), Some(a));
        assert_eq!(before.glyph_width(a), after.glyph_width(a));
        assert_eq!(before.number_of_glyphs(), after.number_of_glyphs());
        assert!(stream.content.len() < original.len() / 2);
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
    fn signed_font_subset_drops_stale_signature_but_keeps_restrictions() {
        let source = font_with_tables(true, &[(*b"DSIG", vec![0; 8])]);
        let subset = retain_glyphs(&source, &BTreeSet::from([1])).unwrap();
        assert_eq!(checksum(&subset), 0xB1B0_AFBA);
        let face = ttf_parser::Face::parse(&subset, 0).unwrap();
        assert!(face
            .raw_face()
            .table(ttf_parser::Tag::from_bytes(b"DSIG"))
            .is_none());
        for tag in [b"cmap", b"hmtx", b"hhea", b"maxp", b"prep"] {
            assert_eq!(table(&source, tag), table(&subset, tag));
        }
        let count = u16_at(&subset, 4).unwrap();
        let range = 16u16 << count.ilog2();
        assert_eq!(u16_at(&subset, 6).unwrap(), range);
        assert_eq!(u16_at(&subset, 10).unwrap(), 16 * count - range);
        for restriction in [0x0100u16, 0x0200] {
            let mut os2 = vec![0; 96];
            os2[8..10].copy_from_slice(&restriction.to_be_bytes());
            let restricted = font_with_tables(true, &[(*b"DSIG", vec![0; 8]), (*b"OS/2", os2)]);
            assert_eq!(
                retain_glyphs(&restricted, &BTreeSet::from([1])),
                Err("font disallows subsetting or outline embedding")
            );
        }
    }

    #[test]
    fn used_codes_include_forms_patterns_and_appearances_and_decline_uncertainty() {
        use lopdf::{dictionary, Stream};
        let mut doc = Document::with_version("1.7");
        let root = doc.add_object(dictionary! {"Type"=>"Catalog"});
        doc.trailer.set("Root", root);
        doc.add_object(Stream::new(
            dictionary! {"Subtype"=>"Form"},
            b"BT (AB) Tj ET".to_vec(),
        ));
        doc.add_object(Stream::new(
            dictionary! {"PatternType"=>1},
            b"[(CD)] TJ".to_vec(),
        ));
        let ap = doc.add_object(Stream::new(dictionary! {}, b"(EF) Tj".to_vec()));
        doc.add_object(dictionary! {"AP"=>dictionary! {"N"=>ap}, "Contents"=>Object::string_literal("annotation description")});
        let codes = document_text_codes(&doc).unwrap();
        assert_eq!(codes.bytes, BTreeSet::from(*b"ABCDEF"));
        assert!(codes.cids.contains(&u16::from_be_bytes(*b"EF")));
        // An interactive form used to stop the census here. It does not any
        // more: what a form makes unpredictable is the fonts a field could be
        // drawn with, which `form_regenerable_programs` names from /DR
        // (#379 round 3). The content census is unaffected by it.
        doc.catalog_mut().unwrap().set(
            "AcroForm",
            dictionary! {"Fields"=>vec![Object::Dictionary(dictionary! {"FT"=>"Tx"})]},
        );
        assert_eq!(
            document_text_codes(&doc).unwrap().bytes,
            BTreeSet::from(*b"ABCDEF"),
        );
        assert!(
            form_regenerable_programs(&doc).is_err(),
            "a form with fields and no default resources names no font, so \
             nothing in the document may be reduced"
        );
        doc.catalog_mut().unwrap().remove(b"AcroForm");
        doc.add_object(Stream::new(
            dictionary! {"Subtype"=>"Form"},
            b"[(unterminated".to_vec(),
        ));
        assert!(document_text_codes(&doc).is_none());
    }

    #[test]
    fn post_names_of_retained_glyphs_survive_without_unused_strings() {
        let mut post = vec![0; 32];
        post[..4].copy_from_slice(&0x20000u32.to_be_bytes());
        post.extend_from_slice(&3u16.to_be_bytes());
        for i in [0u16, 258, 259] {
            post.extend_from_slice(&i.to_be_bytes());
        }
        post.extend_from_slice(b"\x04kept\x06unused");
        let out = compact_post_names(&post, &BTreeSet::from([0, 1])).unwrap();
        assert_eq!(u16_at(&out, 36).unwrap(), 258);
        assert_eq!(u16_at(&out, 38).unwrap(), 0);
        assert_eq!(&out[40..], b"\x04kept");
        assert!(compact_post_names(&post[..39], &BTreeSet::from([0, 1])).is_none());
    }

    #[test]
    fn identity_cid_uses_cid_to_gid_map_and_keeps_parent_font_name_in_sync() {
        use lopdf::{dictionary, Stream};
        let original = font(true);
        let mut doc = Document::with_version("1.7");
        let program = doc.add_object(Stream::new(
            dictionary! {"Length1"=>original.len() as i64},
            original.clone(),
        ));
        let descriptor = doc.add_object(
            dictionary! {"Type"=>"FontDescriptor","FontName"=>"OwnedCID","FontFile2"=>program},
        );
        let map = doc.add_object(Stream::new(dictionary! {}, vec![0, 0, 1, 44])); // CID 1 -> GID 300
        let cid = doc.add_object(dictionary! {"Type"=>"Font","Subtype"=>"CIDFontType2","BaseFont"=>"OwnedCID","FontDescriptor"=>descriptor,"CIDToGIDMap"=>map});
        let descendants = doc.add_object(Object::Array(vec![cid.into()]));
        let parent = doc.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type0","BaseFont"=>"OwnedCID","Encoding"=>"Identity-H","DescendantFonts"=>descendants});
        let contents = doc.add_object(Stream::new(
            dictionary! {},
            b"BT /F 12 Tf <0001> Tj ET".to_vec(),
        ));
        let pages = doc.new_object_id();
        let page = doc.add_object(dictionary! {"Type"=>"Page","Parent"=>pages,"Contents"=>contents,"Resources"=>dictionary! {"Font"=>dictionary! {"F"=>parent}}});
        doc.objects.insert(
            pages,
            dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}.into(),
        );
        let root = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
        doc.trailer.set("Root", root);
        // A CMap with no table anywhere, over a descendant whose /CIDToGIDMap
        // is a STREAM: the codes cannot be resolved, but every glyph they can
        // possibly reach is one that stream names, so the program is still
        // reducible to those (#503). The map itself is not touched.
        let mut mapped = doc.clone();
        mapped
            .get_dictionary_mut(parent)
            .unwrap()
            .set("Encoding", "Unknown-CMap");
        assert_eq!(subset_fonts(&mut mapped).programs_subsetted, 1);
        let reduced = mapped
            .get_object(program)
            .unwrap()
            .as_stream()
            .unwrap()
            .get_plain_content()
            .unwrap();
        let extent_of = |l: &[u8], gid: usize| {
            (
                u32_at(l, gid * 4).unwrap() as usize,
                u32_at(l, (gid + 1) * 4).unwrap() as usize,
            )
        };
        let (from, to) = extent_of(table(&reduced, b"loca"), 300);
        assert!(
            to > from,
            "glyph 300 is the one the CIDToGIDMap stream names"
        );

        // The same CMap over /CIDToGIDMap /Identity bounds nothing: the CID is
        // the glyph index and the CID is what cannot be read.
        let mut unknown = doc.clone();
        unknown
            .get_dictionary_mut(parent)
            .unwrap()
            .set("Encoding", "Unknown-CMap");
        unknown
            .get_dictionary_mut(cid)
            .unwrap()
            .set("CIDToGIDMap", Object::Name(b"Identity".to_vec()));
        assert_eq!(subset_fonts(&mut unknown).programs_subsetted, 0);
        assert_eq!(unknown.objects[&program], doc.objects[&program]);
        let report = subset_fonts(&mut doc);
        assert_eq!(report.programs_subsetted, 1, "{report:?}");
        let data = doc
            .get_object(program)
            .unwrap()
            .as_stream()
            .unwrap()
            .get_plain_content()
            .unwrap();
        let a = table(&original, b"loca");
        let b = table(&data, b"loca");
        let extent = |l: &[u8], gid: usize| {
            (
                u32_at(l, gid * 4).unwrap() as usize,
                u32_at(l, (gid + 1) * 4).unwrap() as usize,
            )
        };
        let (aa, az) = extent(a, 300);
        let (ba, bz) = extent(b, 300);
        assert_eq!(
            &table(&original, b"glyf")[aa..az],
            &table(&data, b"glyf")[ba..bz]
        );
        let (ba, bz) = extent(b, 301);
        assert_eq!(ba, bz);
        assert_eq!(
            name(doc.get_dictionary(parent).unwrap(), b"BaseFont"),
            name(doc.get_dictionary(cid).unwrap(), b"BaseFont")
        );
        assert_eq!(
            doc.get_object(map)
                .unwrap()
                .as_stream()
                .unwrap()
                .get_plain_content()
                .unwrap(),
            [0, 0, 1, 44]
        );
    }

    /// One descendant, two parents: an `Identity-H` one and one over a CMap
    /// this build can read. The identity parent's CIDs are in no table's
    /// answer, so reading the tables alone deletes a glyph it draws (Codex, on
    /// the review of #503).
    #[test]
    fn a_descendant_with_both_identity_and_cmap_parents_keeps_both_repertoires() {
        use lopdf::{dictionary, Stream};
        let original = font(true);
        let mut doc = Document::with_version("1.7");
        let program = doc.add_object(Stream::new(
            dictionary! {"Length1"=>original.len() as i64},
            original.clone(),
        ));
        let descriptor = doc.add_object(
            dictionary! {"Type"=>"FontDescriptor","FontName"=>"Shared","FontFile2"=>program},
        );
        let cid = doc.add_object(dictionary! {"Type"=>"Font","Subtype"=>"CIDFontType2","BaseFont"=>"Shared","FontDescriptor"=>descriptor,"CIDToGIDMap"=>"Identity"});
        // Two byte codes, and <0041> is CID 300 -- so the table parent reaches
        // glyph 300 and only the identity parent reaches glyph 301.
        let cmap = doc.add_object(Stream::new(
            dictionary! {"Type"=>"CMap"},
            b"1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
              1 begincidrange\n<0041> <0041> 300\nendcidrange\n"
                .to_vec(),
        ));
        let mut parents = Vec::new();
        for encoding in [
            Object::Name(b"Identity-H".to_vec()),
            Object::Reference(cmap),
        ] {
            let descendants = doc.add_object(Object::Array(vec![cid.into()]));
            parents.push(doc.add_object(dictionary! {
                "Type"=>"Font","Subtype"=>"Type0","BaseFont"=>"Shared",
                "Encoding"=>encoding,"DescendantFonts"=>descendants
            }));
        }
        // <0041> through the CMap parent, <012D> (CID 301) through the identity one.
        let contents = doc.add_object(Stream::new(
            dictionary! {},
            b"BT /A 12 Tf <0041> Tj /B 12 Tf <012D> Tj ET".to_vec(),
        ));
        let pages = doc.new_object_id();
        let page = doc.add_object(dictionary! {"Type"=>"Page","Parent"=>pages,"Contents"=>contents,"Resources"=>dictionary! {"Font"=>dictionary! {"A"=>parents[1],"B"=>parents[0]}}});
        doc.objects.insert(
            pages,
            dictionary! {"Type"=>"Pages","Kids"=>vec![page.into()],"Count"=>1}.into(),
        );
        let root = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
        doc.trailer.set("Root", root);

        let report = subset_fonts(&mut doc);
        assert_eq!(report.programs_subsetted, 1, "{report:?}");
        let data = doc
            .get_object(program)
            .unwrap()
            .as_stream()
            .unwrap()
            .get_plain_content()
            .unwrap();
        let loca = table(&data, b"loca");
        let extent = |gid: usize| {
            (
                u32_at(loca, gid * 4).unwrap(),
                u32_at(loca, (gid + 1) * 4).unwrap(),
            )
        };
        for (gid, parent) in [(300usize, "the CMap parent"), (301, "the identity parent")] {
            let (from, to) = extent(gid);
            assert!(to > from, "glyph {gid} is drawn through {parent}");
        }
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
