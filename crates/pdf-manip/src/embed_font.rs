// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Embed a TrueType font and make it usable from page content.
//!
//! # Why this is not just `lopdf::Document::add_font`
//!
//! `add_font` writes the font stream, a `/FontDescriptor` and a simple
//! `/Subtype /TrueType` dictionary, and stops there. Two things are missing,
//! and each of them on its own makes the result unusable:
//!
//! * **No `/FirstChar`, `/LastChar` or `/Widths`.** PDF 32000-1 §9.6.3 requires
//!   all three for any simple font that is not one of the standard 14. Without
//!   them a viewer has no advance widths and the text either overlaps or is
//!   rejected outright.
//! * **The font is registered on no page.** `add_font` returns an object id and
//!   touches no `/Resources`, so content referring to the font by name finds
//!   nothing.
//!
//! Wiring `PdfDocument::embed_font` straight to `add_font` would therefore have
//! turned an honest `Err(MissingDependency)` into an `Ok(())` that leaves a font
//! object no page can reach -- a failure the caller cannot see. This module is
//! the other half.
//!
//! # What is deliberately not here
//!
//! Composite (Type0/CID) fonts. A simple font addresses at most 256 glyphs
//! through a single-byte encoding, which covers WinAnsi text and does not cover
//! CJK, or any script needing more glyphs than that. Doing it properly means a
//! `/Type0` dictionary, a descendant `CIDFontType2`, a `/CIDToGIDMap` and a
//! `/W` array, plus a `ToUnicode` CMap for extraction -- a different shape, not
//! a bigger version of this one. Callers that need it get
//! [`EmbedFontError::TooManyGlyphsForSimpleFont`] rather than a document that
//! renders the wrong glyphs.

use lopdf::{Dictionary, Document, FontData, Object, ObjectId};

/// The first character a simple font's `/Widths` array describes.
///
/// 32 rather than 0: codes below the space are control characters with no
/// glyph, and every byte from `FirstChar` to `LastChar` costs an entry in the
/// array whether it is used or not.
const FIRST_CHAR: u8 = 32;
/// The last character. WinAnsiEncoding is single-byte, so this is its ceiling.
const LAST_CHAR: u8 = 255;

/// Glyph space is 1000 units per em in a PDF simple font, whatever the font
/// itself uses.
const PDF_GLYPH_UNITS: f64 = 1000.0;

/// What can go wrong that is the caller's problem rather than ours.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbedFontError {
    /// The bytes are not a TrueType/OpenType font this can read.
    ///
    /// `FontData::new` calls `.expect()` on exactly this, so the parse is done
    /// here first and the panic is never reached.
    NotAFont,
    /// The font declares `unitsPerEm` of zero, which would divide by zero when
    /// scaling advances into glyph space.
    ZeroUnitsPerEm,
    /// The font carries CFF outlines, which need `/FontFile3` rather than the
    /// `/FontFile2` this path writes.
    CffOutlinesNotSupported,
    /// The font needs a composite (Type0/CID) encoding, which this does not write.
    TooManyGlyphsForSimpleFont {
        /// How many glyphs the font actually has.
        glyphs: u16,
    },
    /// The document could not be modified -- a broken page tree, usually.
    Document(String),
}

impl core::fmt::Display for EmbedFontError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotAFont => write!(f, "the data is not a readable TrueType or OpenType font"),
            Self::ZeroUnitsPerEm => write!(f, "the font declares unitsPerEm of zero"),
            Self::CffOutlinesNotSupported => write!(
                f,
                "the font has CFF outlines, which need /FontFile3; this path writes \
                 /FontFile2 and would produce a font viewers cannot load"
            ),
            Self::TooManyGlyphsForSimpleFont { glyphs } => write!(
                f,
                "the font has {glyphs} glyphs; a simple font addresses at most 256, \
                 and composite (Type0/CID) fonts are not written by this path"
            ),
            Self::Document(why) => write!(f, "the document could not be modified: {why}"),
        }
    }
}

impl std::error::Error for EmbedFontError {}

/// What was embedded, and how page content refers to it.
#[derive(Debug, Clone)]
pub struct EmbeddedFont {
    /// The font dictionary.
    pub font_id: ObjectId,
    /// The name to use in a `Tf` operator, without the leading slash.
    pub resource_name: String,
    /// The pages whose `/Resources/Font` now name it.
    pub pages_registered: usize,
}

/// Embed `font_data` under `name` and register it on every page.
///
/// Returns the font's object id and the resource name that page content should
/// use. The name is `name` itself when nothing else in the document claims it,
/// and `name` with a numeric suffix otherwise -- silently overwriting another
/// font's resource entry would change how existing content renders.
pub fn embed_font(
    doc: &mut Document,
    font_data: &[u8],
    name: &str,
) -> Result<EmbeddedFont, EmbedFontError> {
    let face = ttf_parser::Face::parse(font_data, 0).map_err(|_| EmbedFontError::NotAFont)?;

    let units_per_em = face.units_per_em();
    if units_per_em == 0 {
        return Err(EmbedFontError::ZeroUnitsPerEm);
    }

    // The check is on what a simple font can address, not on what the font
    // holds: a 3000-glyph font used for Latin text is fine, it is the encoding
    // that is single-byte. But without a cmap there is no way to map a byte to
    // a glyph at all, so that case is rejected too.
    if face.tables().cmap.is_none() {
        return Err(EmbedFontError::TooManyGlyphsForSimpleFont {
            glyphs: face.number_of_glyphs(),
        });
    }

    // `add_font` always writes /Subtype /TrueType and puts the programme in
    // /FontFile2. A CFF-flavoured OpenType (.otf) parses perfectly well here and
    // would be embedded under a key that promises glyf outlines, which viewers
    // cannot reliably load -- a document that opens and renders nothing, from a
    // call that returned Ok. CFF belongs in /FontFile3 with a matching subtype,
    // which this path does not write. (Codex, #1614.)
    if face.tables().cff.is_some() || face.tables().cff2.is_some() {
        return Err(EmbedFontError::CffOutlinesNotSupported);
    }

    let widths = widths_for_winansi(&face, units_per_em);

    let font_id = doc.add_font(FontData::new(font_data, name.to_string()));
    let font_id = font_id.map_err(|e| EmbedFontError::Document(format!("{e:?}")))?;

    {
        let dict = doc
            .get_object_mut(font_id)
            .and_then(Object::as_dict_mut)
            .map_err(|e| EmbedFontError::Document(format!("{e:?}")))?;
        dict.set("FirstChar", Object::Integer(i64::from(FIRST_CHAR)));
        dict.set("LastChar", Object::Integer(i64::from(LAST_CHAR)));
        dict.set(
            "Widths",
            Object::Array(widths.into_iter().map(Object::Integer).collect()),
        );
    }

    let resource_name = unused_resource_name(doc, name);
    let pages_registered = register_on_every_page(doc, &resource_name, font_id)?;

    Ok(EmbeddedFont {
        font_id,
        resource_name,
        pages_registered,
    })
}

/// One advance per code point from [`FIRST_CHAR`] to [`LAST_CHAR`], in glyph space.
///
/// A code with no glyph gets zero rather than being skipped: `/Widths` is
/// positional, so a missing entry shifts every width after it onto the wrong
/// character.
fn widths_for_winansi(face: &ttf_parser::Face<'_>, units_per_em: u16) -> Vec<i64> {
    let scale = PDF_GLYPH_UNITS / f64::from(units_per_em);

    (FIRST_CHAR..=LAST_CHAR)
        .map(|code| {
            let advance = face
                .glyph_index(winansi_to_char(code))
                .and_then(|gid| face.glyph_hor_advance(gid))
                .unwrap_or(0);
            (f64::from(advance) * scale).round() as i64
        })
        .collect()
}

/// The character a WinAnsi byte means, which is not always the byte itself.
///
/// WinAnsiEncoding agrees with Latin-1 everywhere except `0x80..=0x9F`. There
/// Latin-1 has C1 control codes and WinAnsi has typography -- the euro sign,
/// the curly quotes, the dashes, the bullet. Asking the cmap for `char::from(
/// 0x92)` looks up U+0092, a control character no font has a glyph for, so the
/// width came out zero while the font does carry a right single quote at
/// U+2019. Text using any of these sixteen was then laid out with the wrong
/// spacing. (Codex, #1614.)
///
/// The two `None`s are the two codes WinAnsiEncoding leaves undefined.
fn winansi_to_char(code: u8) -> char {
    const C1: [Option<char>; 32] = [
        Some('\u{20AC}'),
        None,
        Some('\u{201A}'),
        Some('\u{0192}'),
        Some('\u{201E}'),
        Some('\u{2026}'),
        Some('\u{2020}'),
        Some('\u{2021}'),
        Some('\u{02C6}'),
        Some('\u{2030}'),
        Some('\u{0160}'),
        Some('\u{2039}'),
        Some('\u{0152}'),
        None,
        Some('\u{017D}'),
        None,
        None,
        Some('\u{2018}'),
        Some('\u{2019}'),
        Some('\u{201C}'),
        Some('\u{201D}'),
        Some('\u{2022}'),
        Some('\u{2013}'),
        Some('\u{2014}'),
        Some('\u{02DC}'),
        Some('\u{2122}'),
        Some('\u{0161}'),
        Some('\u{203A}'),
        Some('\u{0153}'),
        None,
        Some('\u{017E}'),
        Some('\u{0178}'),
    ];
    if (0x80..=0x9F).contains(&code) {
        // An undefined slot falls back to the raw code point, which has no glyph
        // either -- so the width is zero, which is the honest answer for a byte
        // the encoding does not define.
        return C1[usize::from(code - 0x80)].unwrap_or(char::from(code));
    }
    char::from(code)
}

/// A `/Font` resource name no page already uses.
fn unused_resource_name(doc: &Document, wanted: &str) -> String {
    let taken = existing_font_resource_names(doc);
    if !taken.contains(&wanted.to_string()) {
        return wanted.to_string();
    }
    (1..)
        .map(|n| format!("{wanted}{n}"))
        .find(|candidate| !taken.contains(candidate))
        .expect("the range is unbounded, so some name is free")
}

fn existing_font_resource_names(doc: &Document) -> Vec<String> {
    let mut names = Vec::new();
    for (_, page_id) in doc.get_pages() {
        let Ok((resources, _)) = doc.get_page_resources(page_id) else {
            continue;
        };
        let Some(font_entry) = resources.and_then(|r| r.get(b"Font").ok()) else {
            continue;
        };
        let Ok((_, resolved)) = doc.dereference(font_entry) else {
            continue;
        };
        let Ok(fonts) = resolved.as_dict() else {
            continue;
        };
        for (key, _) in fonts.iter() {
            names.push(String::from_utf8_lossy(key).into_owned());
        }
    }
    names
}

/// Put the font in every page's `/Resources/Font`, and say how many that was.
///
/// Every page rather than one: `embed_font` takes no page argument, and a font
/// that only some pages can name is the same class of half-done as one no page
/// can name.
fn register_on_every_page(
    doc: &mut Document,
    resource_name: &str,
    font_id: ObjectId,
) -> Result<usize, EmbedFontError> {
    let page_ids: Vec<ObjectId> = doc.get_pages().into_values().collect();
    let mut registered = 0;
    // A /Resources dictionary inherited from a /Pages ancestor is shared by all
    // the pages under it, so writing the font into it once serves them all --
    // and writing it twice would be harmless but pointless.
    let mut done: Vec<ObjectId> = Vec::new();

    for page_id in page_ids {
        let holder = resources_holder(doc, page_id)?;
        if holder.shared_id.is_some_and(|id| done.contains(&id)) {
            registered += 1;
            continue;
        }

        let resources = match holder.shared_id {
            Some(id) => doc
                .get_object_mut(id)
                .and_then(Object::as_dict_mut)
                .map_err(|e| EmbedFontError::Document(format!("{e:?}")))?,
            None => doc
                .get_or_create_resources(page_id)
                .and_then(Object::as_dict_mut)
                .map_err(|e| EmbedFontError::Document(format!("{e:?}")))?,
        };

        if !resources.has(b"Font") {
            resources.set("Font", Dictionary::new());
        }

        // /Font may be an indirect reference, and a chain of them. Following it
        // is what add_xobject does for /XObject; setting the key on the holder
        // dictionary instead would write into a copy nothing reads.
        let mut fonts = resources
            .get_mut(b"Font")
            .map_err(|e| EmbedFontError::Document(format!("{e:?}")))?;
        if let Object::Reference(first) = fonts {
            let mut fonts_id = *first;
            // Bounded and cycle-aware, as a second line rather than the first.
            // Measured: on a /Font pointing into an A -> B -> A cycle, lopdf's
            // own reference limit fires first and get_object returns
            // ReferenceLimit, so the loop ends there. This bound covers the case
            // where a chain is long but under that limit, and costs a Vec of at
            // most 32 ids. Raised by Codex on #1614; the hang it described is
            // not reachable through this call today, and the guard is cheap
            // enough to keep for when the layer below changes.
            let mut seen = vec![fonts_id];
            while let Ok(Object::Reference(next)) = doc.get_object(fonts_id) {
                if seen.contains(next) || seen.len() > MAX_REFERENCE_CHAIN {
                    return Err(EmbedFontError::Document(format!(
                        "/Resources/Font on page {page_id:?} is a reference cycle"
                    )));
                }
                fonts_id = *next;
                seen.push(fonts_id);
            }
            fonts = doc
                .get_object_mut(fonts_id)
                .map_err(|e| EmbedFontError::Document(format!("{e:?}")))?;
        }

        Object::as_dict_mut(fonts)
            .map_err(|e| EmbedFontError::Document(format!("{e:?}")))?
            .set(resource_name.to_string(), Object::Reference(font_id));
        if let Some(id) = holder.shared_id {
            done.push(id);
        }
        registered += 1;
    }

    Ok(registered)
}

/// How deep a chain of indirect references to follow before calling it broken.
const MAX_REFERENCE_CHAIN: usize = 32;

struct ResourcesHolder {
    /// The object that actually carries the resources, when they are indirect --
    /// which is also how an inherited dictionary reaches us.
    shared_id: Option<ObjectId>,
}

/// Find the dictionary a page's resources really live in.
///
/// `get_or_create_resources` makes a fresh, empty page-level `/Resources` when
/// the page has none of its own. For a page that inherits from a `/Pages`
/// ancestor that is destructive: `/Resources` inheritance picks the nearest
/// dictionary rather than merging, so a new empty one hides every inherited
/// font, XObject, colour space and graphics state, and content that used them
/// stops rendering. (Codex, #1614.)
fn resources_holder(doc: &Document, page_id: ObjectId) -> Result<ResourcesHolder, EmbedFontError> {
    let (own, inherited_ids) = doc
        .get_page_resources(page_id)
        .map_err(|e| EmbedFontError::Document(format!("{e:?}")))?;

    // An own, direct dictionary: get_or_create_resources will hand back exactly
    // that, so there is nothing to preserve.
    if own.is_some() {
        return Ok(ResourcesHolder { shared_id: None });
    }
    // Otherwise the nearest indirect one wins -- the page's own if it has one,
    // else the closest ancestor's. Both are correct to write into: we are
    // registering on every page anyway.
    Ok(ResourcesHolder {
        shared_id: inherited_ids.first().copied(),
    })
}
