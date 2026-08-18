// Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
//
// This software is proprietary. The PDFluent application is free to use,
// including for commercial purposes. Redistribution, or extraction or reuse
// of its components (including the embedded PDF engine), requires a licence.
// See https://pdfluent.com/license for terms.

//! Embedding a caller-supplied Unicode font as a Type0 composite
//! font, so a replacement can contain characters the original font has no
//! glyph for.
//!
//! # Why this exists
//!
//! The Phase 1B replacement path re-encodes through the *original* font's
//! reverse map, and its fallback ([`FontFallback::InjectStandard`]) injects
//! Helvetica/WinAnsi. Both are bounded by what the document already had:
//! WinAnsi tops out at U+00FF. That is fine for replacing a word in an
//! English document and useless for writing Polish, Greek, Cyrillic, CJK or
//! any other script the source never contained.
//!
//! A Type0 font with `Identity-H` encoding removes that ceiling: character
//! codes are 2-byte glyph indices into an embedded font program, so the
//! reachable set is whatever the supplied font covers.
//!
//! # Why the caller supplies the font
//!
//! Embedding glyphs redistributes the font. Rather than bundle one and
//! inherit its licence terms (and its size — a full CJK face is tens of
//! megabytes), the caller passes the bytes of a font they are licensed to
//! embed. That also lets them pick a face per language, which a bundled
//! default could not do. A bundled default can be layered on later without
//! changing this API.
//!
//! # What is written
//!
//! ```text
//! Type0 (/Encoding /Identity-H)
//!  └── DescendantFonts[0] → CIDFontType2
//!        ├── /CIDToGIDMap /Identity     CID == GID, no extra indirection
//!        ├── /W                          per-glyph advances
//!        └── /FontDescriptor → /FontFile2 (subsetted font program)
//!  └── /ToUnicode                        CMap so the text stays extractable
//! ```
//!
//! The `/ToUnicode` map is not optional here. Without it a translated
//! document renders correctly and yields nothing on copy, search or
//! screen-reader — which for a translation workflow is a silent, total loss
//! of the actual product.
//!
//! # Subsetting and glyph identity
//!
//! Glyphs are remapped to a compact range via [`subsetter::GlyphRemapper`],
//! so the embedded program carries only what is used. Because `Identity-H`
//! ties the character code to the glyph index, the codes written into the
//! content stream are the **new** (post-remap) indices, never the original
//! ones. Getting that pairing wrong produces a file that opens cleanly and
//! renders the wrong glyphs, so the two always travel together in
//! [`UnicodeEncoder`].

use crate::error::{ManipError, Result};

/// Which outline format the font program carries.
///
/// This decides the whole descendant-font shape in the PDF, so it is read once
/// from the program and carried along rather than re-derived at write time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outlines {
    /// `glyf` outlines — a `CIDFontType2` descendant with `/FontFile2`.
    TrueType,
    /// CFF outlines — a `CIDFontType0` descendant with `/FontFile3`.
    Cff,
}
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Resource name for the injected Unicode font.
pub(crate) const UNICODE_FONT_RESOURCE: &str = "F__Uni";

/// A caller-supplied font, validated as embeddable.
///
/// Cheap to clone (the program bytes sit behind an [`Arc`]).
#[derive(Clone)]
pub struct UnicodeFont {
    data: Arc<Vec<u8>>,
    units_per_em: u16,
    /// Decides the descendant-font shape; see [`Outlines`].
    outlines: Outlines,
    /// Short stable identity, used for equality and diagnostics rather than
    /// comparing whole font programs.
    fingerprint: [u8; 8],
    postscript_name: String,
    /// Variation axis settings to instance to before subsetting. Empty for a
    /// static font.
    ///
    /// A variable font embedded as-is renders at its *default* instance, and
    /// that default is not always the weight a reader expects: Noto Sans SC
    /// defaults to wght 100 (Thin), so body text embedded from it comes out
    /// hairline. Defaulting this to Regular absorbs that trap instead of
    /// leaving every caller to discover it from a printed page.
    variations: Vec<([u8; 4], f32)>,
}

/// The `wght` variation axis.
const WGHT: [u8; 4] = *b"wght";
/// Conventional "Regular" weight.
const REGULAR_WEIGHT: f32 = 400.0;

impl std::fmt::Debug for UnicodeFont {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never dump the program bytes: a font is megabytes and this type
        // ends up inside error and option structs that get logged.
        f.debug_struct("UnicodeFont")
            .field("name", &self.postscript_name)
            .field("bytes", &self.data.len())
            .field("fingerprint", &hex8(&self.fingerprint))
            .finish()
    }
}

impl PartialEq for UnicodeFont {
    fn eq(&self, other: &Self) -> bool {
        self.fingerprint == other.fingerprint
    }
}
impl Eq for UnicodeFont {}

#[cfg(feature = "serde")]
impl serde::Serialize for UnicodeFont {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        // A font program is not useful in a report; identity is.
        let mut st = s.serialize_struct("UnicodeFont", 2)?;
        st.serialize_field("name", &self.postscript_name)?;
        st.serialize_field("fingerprint", &hex8(&self.fingerprint))?;
        st.end()
    }
}

fn hex8(bytes: &[u8; 8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

impl UnicodeFont {
    /// Validate and take ownership of a font program.
    ///
    /// Rejects fonts this writer cannot embed correctly rather than
    /// producing a file that opens but renders wrongly:
    ///
    /// - the program must parse as OpenType/TrueType;
    /// - it must carry outlines in a format we can embed: TrueType (`glyf`)
    ///   becomes a `CIDFontType2`/`FontFile2` descendant, CFF becomes
    ///   `CIDFontType0`/`FontFile3`. A program with neither has nothing to
    ///   embed and is refused.
    pub fn from_bytes(data: Vec<u8>) -> Result<Self> {
        let face = ttf_parser::Face::parse(&data, 0)
            .map_err(|e| ManipError::Other(format!("not a usable font program: {e}")))?;

        // Both flavours are embeddable, but they take different shapes in the
        // PDF, so decide here and carry the answer rather than guessing later.
        let outlines = if face.tables().glyf.is_some() {
            Outlines::TrueType
        } else if face.tables().cff.is_some() {
            Outlines::Cff
        } else {
            return Err(ManipError::Other(
                "font carries neither TrueType (glyf) nor CFF outlines, so there is \
                 nothing to embed"
                    .to_string(),
            ));
        };

        let units_per_em = face.units_per_em();
        if units_per_em == 0 {
            return Err(ManipError::Other(
                "font declares unitsPerEm = 0, cannot scale widths".to_string(),
            ));
        }

        let postscript_name = face
            .names()
            .into_iter()
            .find(|n| n.name_id == ttf_parser::name_id::POST_SCRIPT_NAME)
            .and_then(|n| n.to_string())
            .unwrap_or_else(|| "EmbeddedFont".to_string());

        let fingerprint = {
            use sha2::{Digest, Sha256};
            let digest = Sha256::digest(&data);
            let mut fp = [0u8; 8];
            fp.copy_from_slice(&digest[..8]);
            fp
        };

        // Aim a variable font at Regular unless its own default already is.
        // Clamped to the axis range so a family that stops below 400 gets its
        // heaviest available weight rather than an out-of-range request.
        let variations = face
            .variation_axes()
            .into_iter()
            .find(|a| a.tag.to_bytes() == WGHT)
            .filter(|a| (a.def_value - REGULAR_WEIGHT).abs() > f32::EPSILON)
            .map(|a| vec![(WGHT, REGULAR_WEIGHT.clamp(a.min_value, a.max_value))])
            .unwrap_or_default();

        Ok(Self {
            data: Arc::new(data),
            units_per_em,
            outlines,
            fingerprint,
            postscript_name,
            variations,
        })
    }

    /// Pin a variation axis, e.g. `wght` 700 for bold.
    ///
    /// Overrides the automatic Regular targeting. No-op on a static font.
    #[must_use]
    pub fn with_variation(mut self, axis: &[u8; 4], value: f32) -> Self {
        self.variations.retain(|(t, _)| t != axis);
        self.variations.push((*axis, value));
        self
    }

    /// Whether this font is variable and will be instanced before embedding.
    #[must_use]
    pub fn is_variable(&self) -> bool {
        ttf_parser::Face::parse(&self.data, 0)
            .map(|f| !f.variation_axes().is_empty())
            .unwrap_or(false)
    }

    /// Characters in `text` this font has no glyph for, in first-seen order
    /// and without duplicates.
    ///
    /// Callers use this to choose a font *before* staging an edit; the
    /// encoder reports the same condition as an error at encode time.
    #[must_use]
    pub fn missing_chars(&self, text: &str) -> Vec<char> {
        let Ok(face) = ttf_parser::Face::parse(&self.data, 0) else {
            return text.chars().collect();
        };
        let mut missing = Vec::new();
        for ch in text.chars() {
            if face.glyph_index(ch).is_none() && !missing.contains(&ch) {
                missing.push(ch);
            }
        }
        missing
    }

    /// Whether this font covers every character in `text`.
    #[must_use]
    pub fn covers(&self, text: &str) -> bool {
        self.missing_chars(text).is_empty()
    }

    /// PostScript name as declared by the font.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.postscript_name
    }
}

/// Accumulates glyph usage for one page, then writes the font.
///
/// Encoding and embedding are deliberately one object: the character codes
/// written into the content stream are post-subset glyph indices, so they are
/// only meaningful next to the font program produced by the same remapper.
pub struct UnicodeEncoder {
    font: UnicodeFont,
    remapper: subsetter::GlyphRemapper,
    /// new GID → the text it came from, for `/ToUnicode`.
    to_unicode: BTreeMap<u16, String>,
    /// new GID → advance in font units.
    advances: BTreeMap<u16, u16>,
}

impl UnicodeEncoder {
    /// Start accumulating for `font`.
    #[must_use]
    pub fn new(font: UnicodeFont) -> Self {
        Self {
            font,
            // `GlyphRemapper::new` always seeds .notdef at new GID 0, which is
            // what a CID font needs at CID 0 anyway.
            remapper: subsetter::GlyphRemapper::new(),
            to_unicode: BTreeMap::new(),
            advances: BTreeMap::new(),
        }
    }

    /// Encode `text` as `Identity-H` character codes (2 bytes per glyph,
    /// big-endian), registering every glyph it uses.
    ///
    /// Fails if the font has no glyph for a character, naming the characters
    /// rather than silently substituting `.notdef` — a row of blank boxes in
    /// a translated document is worse than a refusal the caller can act on.
    pub fn encode(&mut self, text: &str) -> Result<Vec<u8>> {
        let face = ttf_parser::Face::parse(&self.font.data, 0)
            .map_err(|e| ManipError::Other(format!("font became unparseable: {e}")))?;

        let missing: Vec<char> = text
            .chars()
            .filter(|c| face.glyph_index(*c).is_none())
            .collect();
        if !missing.is_empty() {
            let shown: String = missing
                .iter()
                .take(8)
                .map(|c| format!("'{c}' (U+{:04X})", *c as u32))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(ManipError::Other(format!(
                "font '{}' has no glyph for {shown}{}",
                self.font.postscript_name,
                if missing.len() > 8 {
                    format!(" and {} more", missing.len() - 8)
                } else {
                    String::new()
                }
            )));
        }

        let mut bytes = Vec::with_capacity(text.len() * 2);
        for ch in text.chars() {
            // Unwrap-free: the missing-glyph check above already ran.
            let Some(old_gid) = face.glyph_index(ch) else {
                continue;
            };
            let new_gid = self.remapper.remap(old_gid.0);

            // Insert once per glyph, never append: a character that occurs
            // twice maps to the same glyph id both times, and appending would
            // make its /ToUnicode entry "aa". The page still renders, but
            // every copy, search and screen-reader pass yields doubled text.
            self.to_unicode
                .entry(new_gid)
                .or_insert_with(|| ch.to_string());
            if let Some(adv) = face.glyph_hor_advance(old_gid) {
                self.advances.insert(new_gid, adv);
            }

            bytes.extend_from_slice(&new_gid.to_be_bytes());
        }
        Ok(bytes)
    }

    /// Advance width of `text` in em units (1.0 = the font size), using this
    /// font's own metrics.
    ///
    /// Used to decide whether a replacement fits where the original sat. The
    /// substitute font is never exactly as wide as the one it replaces, so a
    /// translated block needs measuring rather than assuming.
    #[must_use]
    pub fn width_em(&self, text: &str) -> f64 {
        let Ok(face) = ttf_parser::Face::parse(&self.font.data, 0) else {
            return 0.0;
        };
        let upem = f64::from(self.font.units_per_em);
        text.chars()
            .filter_map(|c| face.glyph_index(c))
            .filter_map(|g| face.glyph_hor_advance(g))
            .map(|a| f64::from(a) / upem)
            .sum()
    }

    /// Whether anything was encoded, i.e. whether a font needs writing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.to_unicode.is_empty()
    }

    /// Subset the program, write the Type0 font, and register it in the
    /// page's resources. Returns the resource name to use in `Tf`.
    pub fn embed(self, doc: &mut Document, page_num: u32) -> Result<String> {
        if self.is_empty() {
            return Err(ManipError::Other(
                "nothing encoded; no font to embed".to_string(),
            ));
        }

        let subset = if self.font.variations.is_empty() {
            subsetter::subset(&self.font.data, 0, &self.remapper)
        } else {
            // Instancing collapses the variable font to one weight; without
            // this the embedded program renders at the family's default,
            // which for several Noto CJK builds is Thin.
            let coords: Vec<(subsetter::Tag, f32)> = self
                .font
                .variations
                .iter()
                .map(|(tag, value)| (subsetter::Tag::new(tag), *value))
                .collect();
            subsetter::subset_with_variations(&self.font.data, 0, &coords, &self.remapper)
        }
        .map_err(|e| ManipError::Other(format!("font subsetting failed: {e}")))?;

        // Metrics come from the ORIGINAL face: subsetting rewrites glyph ids,
        // and reading head/hhea from the subset would be equivalent but adds a
        // parse that can fail for no benefit.
        let face = ttf_parser::Face::parse(&self.font.data, 0)
            .map_err(|e| ManipError::Other(format!("font became unparseable: {e}")))?;

        let upem = f64::from(self.font.units_per_em);
        let to_pdf = |v: f64| (v * 1000.0 / upem).round() as i64;

        let descriptor_id = self.write_descriptor(doc, &subset, &face, to_pdf)?;
        let cid_font_id = self.write_cid_font(doc, descriptor_id, to_pdf);
        let to_unicode_id = self.write_to_unicode(doc)?;

        let base_font = self.subset_base_font_name();
        let type0 = dictionary! {
            "Type" => Object::Name(b"Font".to_vec()),
            "Subtype" => Object::Name(b"Type0".to_vec()),
            "BaseFont" => Object::Name(base_font.into_bytes()),
            "Encoding" => Object::Name(b"Identity-H".to_vec()),
            "DescendantFonts" => Object::Array(vec![Object::Reference(cid_font_id)]),
            "ToUnicode" => Object::Reference(to_unicode_id),
        };
        let type0_id = doc.add_object(Object::Dictionary(type0));

        register_page_font(doc, page_num, UNICODE_FONT_RESOURCE, type0_id).ok_or_else(|| {
            ManipError::Other(format!(
                "could not register the Unicode font in the resources of page {page_num}"
            ))
        })?;

        Ok(UNICODE_FONT_RESOURCE.to_string())
    }

    /// `ABCDEF+Name` — the spec wants a 6-uppercase-letter tag on a subsetted
    /// font. Derived from the fingerprint so the same font subsets to the same
    /// tag across runs (byte-identical output for byte-identical input).
    fn subset_base_font_name(&self) -> String {
        let tag: String = self.font.fingerprint[..6]
            .iter()
            .map(|b| char::from(b'A' + (b % 26)))
            .collect();
        let clean: String = self
            .font
            .postscript_name
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        format!("{tag}+{clean}")
    }

    fn write_descriptor(
        &self,
        doc: &mut Document,
        subset: &[u8],
        face: &ttf_parser::Face,
        to_pdf: impl Fn(f64) -> i64,
    ) -> Result<ObjectId> {
        // The stream dictionary differs per flavour, and PDF/A checks it.
        //
        // /Length1 is the uncompressed length of a TrueType program and belongs
        // only on /FontFile2. A /FontFile3 stream instead needs its own
        // /Subtype naming the format it carries: the subsetter emits an OTTO
        // (CFF-flavoured OpenType) container, so that is /OpenType — bare CFF
        // would be /CIDFontType0C.
        //
        // Leaving the /Subtype off is not cosmetic. veraPDF rejects the file
        // under PDF/A-2 clause 6.2.11.4.1 with "Invalid subtype of the embedded
        // font stream", while every reader still opens and draws it — so
        // without the conformance test this would have shipped looking fine.
        let stream_dict = match self.font.outlines {
            Outlines::TrueType => {
                dictionary! { "Length1" => Object::Integer(subset.len() as i64) }
            }
            Outlines::Cff => {
                dictionary! { "Subtype" => Object::Name(b"OpenType".to_vec()) }
            }
        };
        let mut stream = Stream::new(stream_dict, subset.to_vec());
        // Ignore compression failure: an uncompressed program is valid, just
        // larger, and is strictly better than failing the whole edit.
        let _ = stream.compress();
        let file_id = doc.add_object(Object::Stream(stream));

        let bbox = face.global_bounding_box();
        let mut descriptor = dictionary! {
            "Type" => Object::Name(b"FontDescriptor".to_vec()),
            "FontName" => Object::Name(self.subset_base_font_name().into_bytes()),
            // Symbolic: Identity-H codes are glyph indices, not a Latin
            // character set, so the nonsymbolic flag would be a false claim.
            "Flags" => Object::Integer(4),
            "FontBBox" => Object::Array(vec![
                Object::Integer(to_pdf(f64::from(bbox.x_min))),
                Object::Integer(to_pdf(f64::from(bbox.y_min))),
                Object::Integer(to_pdf(f64::from(bbox.x_max))),
                Object::Integer(to_pdf(f64::from(bbox.y_max))),
            ]),
            "ItalicAngle" => Object::Real(face.italic_angle()),
            "Ascent" => Object::Integer(to_pdf(f64::from(face.ascender()))),
            "Descent" => Object::Integer(to_pdf(f64::from(face.descender()))),
            "CapHeight" => Object::Integer(
                face.capital_height()
                    .map_or_else(|| to_pdf(f64::from(face.ascender())), |h| to_pdf(f64::from(h)))
            ),
            // No StemV in an OpenType file. It is required by the spec and
            // used only as a rendering hint, so a conventional mid value is
            // the honest choice; deriving a fake precise number would not be.
            "StemV" => Object::Integer(80),
        };
        // Where the program hangs depends on its outline format. A CFF program
        // under /FontFile2 produces a file that opens and shows nothing.
        match self.font.outlines {
            Outlines::TrueType => {
                descriptor.set("FontFile2", Object::Reference(file_id));
            }
            Outlines::Cff => {
                descriptor.set("FontFile3", Object::Reference(file_id));
            }
        }
        Ok(doc.add_object(Object::Dictionary(descriptor)))
    }

    fn write_cid_font(
        &self,
        doc: &mut Document,
        descriptor_id: ObjectId,
        to_pdf: impl Fn(f64) -> i64,
    ) -> ObjectId {
        // CIDFontType2 wraps TrueType outlines, CIDFontType0 wraps CFF. Naming
        // the wrong one is the classic way to produce a PDF that every reader
        // accepts and no reader draws.
        let subtype: &[u8] = match self.font.outlines {
            Outlines::TrueType => b"CIDFontType2",
            Outlines::Cff => b"CIDFontType0",
        };
        let mut cid_font = dictionary! {
            "Type" => Object::Name(b"Font".to_vec()),
            "Subtype" => Object::Name(subtype.to_vec()),
            "BaseFont" => Object::Name(self.subset_base_font_name().into_bytes()),
            "CIDSystemInfo" => dictionary! {
                "Registry" => Object::string_literal("Adobe"),
                "Ordering" => Object::string_literal("Identity"),
                "Supplement" => Object::Integer(0),
            },
            "FontDescriptor" => Object::Reference(descriptor_id),

            "DW" => Object::Integer(1000),
            "W" => Object::Array(self.build_w_array(to_pdf)),
        };

        // /CIDToGIDMap is a CIDFontType2 key. On a CIDFontType0 the CFF charset
        // already carries the CID-to-glyph relation, so the key does not belong
        // there — but leaving it out of the TrueType case breaks that font, as
        // the existing Identity-H test caught the moment it went missing.
        if self.font.outlines == Outlines::TrueType {
            cid_font.set("CIDToGIDMap", Object::Name(b"Identity".to_vec()));
        }
        doc.add_object(Object::Dictionary(cid_font))
    }

    /// `/W` in the `c [w1 w2 …]` form, coalescing consecutive CIDs into one
    /// run so the array stays small on documents with many glyphs.
    fn build_w_array(&self, to_pdf: impl Fn(f64) -> i64) -> Vec<Object> {
        let mut out = Vec::new();
        let mut run_start: Option<u16> = None;
        let mut run: Vec<Object> = Vec::new();
        let mut prev: Option<u16> = None;

        for (&gid, &adv) in &self.advances {
            let contiguous = prev.is_some_and(|p| gid == p + 1);
            if !contiguous {
                if let Some(start) = run_start.take() {
                    out.push(Object::Integer(i64::from(start)));
                    out.push(Object::Array(std::mem::take(&mut run)));
                }
                run_start = Some(gid);
            }
            run.push(Object::Integer(to_pdf(f64::from(adv))));
            prev = Some(gid);
        }
        if let Some(start) = run_start {
            out.push(Object::Integer(i64::from(start)));
            out.push(Object::Array(run));
        }
        out
    }

    /// A `/ToUnicode` CMap mapping each CID back to the text it renders, so
    /// the result stays selectable, searchable and machine-readable.
    fn write_to_unicode(&self, doc: &mut Document) -> Result<ObjectId> {
        let mut cmap = String::from(
            "/CIDInit /ProcSet findresource begin\n\
             12 dict begin\n\
             begincmap\n\
             /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
             /CMapName /Adobe-Identity-UCS def\n\
             /CMapType 2 def\n\
             1 begincodespacerange\n<0000> <FFFF>\n endcodespacerange\n",
        );

        // bfchar takes at most 100 entries per block.
        let entries: Vec<(u16, &String)> = self.to_unicode.iter().map(|(g, s)| (*g, s)).collect();
        for chunk in entries.chunks(100) {
            cmap.push_str(&format!("{} beginbfchar\n", chunk.len()));
            for (gid, text) in chunk {
                let utf16: String = text
                    .encode_utf16()
                    .map(|u| format!("{u:04X}"))
                    .collect::<Vec<_>>()
                    .join("");
                cmap.push_str(&format!("<{gid:04X}> <{utf16}>\n"));
            }
            cmap.push_str("endbfchar\n");
        }

        cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");

        let mut stream = Stream::new(Dictionary::new(), cmap.into_bytes());
        let _ = stream.compress();
        Ok(doc.add_object(Object::Stream(stream)))
    }
}

/// Put `font_id` in the page's `/Resources /Font` under `name`.
///
/// Returns `None` when the resources could not be reached or written, so the
/// caller can refuse to emit a `Tf` for a font that is not actually there —
/// the failure mode that silently breaks text extraction.
fn register_page_font(
    doc: &mut Document,
    page_num: u32,
    name: &str,
    font_id: ObjectId,
) -> Option<()> {
    let pages = doc.get_pages();
    let &page_id = pages.get(&page_num)?;

    let resources_entry = doc
        .get_object(page_id)
        .ok()
        .and_then(|o| o.as_dict().ok())
        .and_then(|d| d.get(b"Resources").ok())
        .cloned();

    match resources_entry {
        // Resources live in their own object: reach through the reference.
        Some(Object::Reference(res_id)) => {
            let font_entry = doc
                .get_object(res_id)
                .ok()
                .and_then(|o| o.as_dict().ok())
                .and_then(|d| d.get(b"Font").ok())
                .cloned();

            match font_entry {
                Some(Object::Reference(font_dict_id)) => {
                    let dict = doc.get_object_mut(font_dict_id).ok()?.as_dict_mut().ok()?;
                    dict.set(name, Object::Reference(font_id));
                }
                Some(Object::Dictionary(mut d)) => {
                    d.set(name, Object::Reference(font_id));
                    let res = doc.get_object_mut(res_id).ok()?.as_dict_mut().ok()?;
                    res.set("Font", Object::Dictionary(d));
                }
                _ => {
                    let mut d = Dictionary::new();
                    d.set(name, Object::Reference(font_id));
                    let res = doc.get_object_mut(res_id).ok()?.as_dict_mut().ok()?;
                    res.set("Font", Object::Dictionary(d));
                }
            }
        }
        // Resources inline on the page dictionary.
        Some(Object::Dictionary(mut res)) => match res.get(b"Font").ok().cloned() {
            Some(Object::Reference(font_dict_id)) => {
                let dict = doc.get_object_mut(font_dict_id).ok()?.as_dict_mut().ok()?;
                dict.set(name, Object::Reference(font_id));
            }
            Some(Object::Dictionary(mut d)) => {
                d.set(name, Object::Reference(font_id));
                res.set("Font", Object::Dictionary(d));
                let page = doc.get_object_mut(page_id).ok()?.as_dict_mut().ok()?;
                page.set("Resources", Object::Dictionary(res));
            }
            _ => {
                let mut d = Dictionary::new();
                d.set(name, Object::Reference(font_id));
                res.set("Font", Object::Dictionary(d));
                let page = doc.get_object_mut(page_id).ok()?.as_dict_mut().ok()?;
                page.set("Resources", Object::Dictionary(res));
            }
        },
        // No resources at all: create the whole chain.
        _ => {
            let mut fonts = Dictionary::new();
            fonts.set(name, Object::Reference(font_id));
            let mut res = Dictionary::new();
            res.set("Font", Object::Dictionary(fonts));
            let page = doc.get_object_mut(page_id).ok()?.as_dict_mut().ok()?;
            page.set("Resources", Object::Dictionary(res));
        }
    }

    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real font with wide coverage, from the host. Skips rather than fails
    /// when absent so the suite stays green on machines without it.
    fn host_font() -> Option<Vec<u8>> {
        for path in [
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
            "/System/Library/Fonts/Supplemental/Arial.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        ] {
            if let Ok(data) = std::fs::read(path) {
                return Some(data);
            }
        }
        None
    }

    #[test]
    fn rejects_a_non_font() {
        let err = UnicodeFont::from_bytes(b"this is not a font".to_vec()).unwrap_err();
        assert!(err.to_string().contains("not a usable font program"));
    }

    #[test]
    fn reports_the_characters_a_font_cannot_write() {
        let Some(data) = host_font() else { return };
        let font = UnicodeFont::from_bytes(data).unwrap();
        // Latin is present in every candidate face above.
        assert!(font.covers("Hello"));
        // A private-use codepoint is in none of them.
        let missing = font.missing_chars("A\u{E000}B");
        assert_eq!(missing, vec!['\u{E000}']);
    }

    #[test]
    fn encodes_two_bytes_per_glyph_and_registers_them() {
        let Some(data) = host_font() else { return };
        let font = UnicodeFont::from_bytes(data).unwrap();
        let mut enc = UnicodeEncoder::new(font);

        let bytes = enc.encode("AB").unwrap();
        assert_eq!(bytes.len(), 4, "Identity-H is 2 bytes per glyph");
        assert!(!enc.is_empty());

        // Same character twice must reuse one glyph id, or the subset grows
        // with every repetition.
        let again = enc.encode("A").unwrap();
        assert_eq!(&bytes[0..2], &again[..]);
    }

    #[test]
    fn refuses_characters_the_font_lacks_instead_of_writing_blanks() {
        let Some(data) = host_font() else { return };
        let font = UnicodeFont::from_bytes(data).unwrap();
        let mut enc = UnicodeEncoder::new(font);
        let err = enc.encode("A\u{E000}").unwrap_err().to_string();
        assert!(
            err.contains("U+E000"),
            "error should name the character: {err}"
        );
    }

    #[test]
    fn a_repeated_character_maps_back_to_one_character() {
        // Regression: the first implementation appended to the /ToUnicode
        // entry on every occurrence, so "aa" round-tripped as "aaaa".
        // Rendering looked perfect; only extraction was wrong, which is the
        // failure mode a structural assertion cannot see.
        let Some(data) = host_font() else { return };
        let font = UnicodeFont::from_bytes(data).unwrap();
        let mut enc = UnicodeEncoder::new(font);
        enc.encode("aaa").unwrap();
        enc.encode("a").unwrap();

        let mapped = enc
            .to_unicode
            .values()
            .find(|s| s.starts_with('a'))
            .expect("the glyph for 'a' should be registered");
        assert_eq!(
            mapped, "a",
            "one glyph must map back to exactly one character, got {mapped:?}"
        );
    }

    #[test]
    fn a_variable_font_is_aimed_at_regular_not_its_own_default() {
        // Noto Sans SC's variable build defaults to wght 100 (Thin), so
        // embedding it as-is gives hairline body text that looks like a
        // rendering fault rather than a font choice. Measured on the real
        // file; the axis default is data, not an assumption.
        let Some(data) = host_font() else { return };
        let font = UnicodeFont::from_bytes(data).unwrap();

        if !font.is_variable() {
            // Static face: nothing to instance, and nothing to assert.
            assert!(font.variations.is_empty());
            return;
        }
        for (axis, value) in &font.variations {
            if axis == &WGHT {
                assert!(
                    (*value - REGULAR_WEIGHT).abs() < f32::EPSILON,
                    "a variable font should be pinned to Regular, got {value}"
                );
            }
        }
    }

    #[test]
    fn an_explicit_variation_overrides_the_automatic_one() {
        let Some(data) = host_font() else { return };
        let font = UnicodeFont::from_bytes(data)
            .unwrap()
            .with_variation(b"wght", 700.0);
        let weight = font
            .variations
            .iter()
            .find(|(a, _)| a == &WGHT)
            .map(|(_, v)| *v);
        assert_eq!(weight, Some(700.0), "caller's weight must win");
    }

    #[test]
    fn w_array_coalesces_consecutive_glyphs() {
        let Some(data) = host_font() else { return };
        let font = UnicodeFont::from_bytes(data).unwrap();
        let mut enc = UnicodeEncoder::new(font);
        enc.encode("Hello world").unwrap();
        let w = enc.build_w_array(|v| v.round() as i64);
        // Alternating "start [widths…]" pairs.
        assert_eq!(w.len() % 2, 0);
        assert!(matches!(w.first(), Some(Object::Integer(_))));
        assert!(matches!(w.get(1), Some(Object::Array(_))));
    }

    #[test]
    fn subset_tag_is_six_uppercase_letters_and_stable() {
        let Some(data) = host_font() else { return };
        let font = UnicodeFont::from_bytes(data).unwrap();
        let a = UnicodeEncoder::new(font.clone()).subset_base_font_name();
        let b = UnicodeEncoder::new(font).subset_base_font_name();
        assert_eq!(a, b, "same font must give the same tag across runs");
        let tag = a.split('+').next().unwrap();
        assert_eq!(tag.len(), 6);
        assert!(tag.chars().all(|c| c.is_ascii_uppercase()));
    }
}
