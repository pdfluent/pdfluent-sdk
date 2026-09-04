//! Text replacement in PDF content streams (legacy convenience API).
//!
//! Since Phase 1C these functions are thin wrappers over the
//! [`crate::text_edit`] engine (see `docs/TEXT_REPLACE_ENGINE_DESIGN.md` §9).
//! They keep the historical count-only contract: per-occurrence failures are
//! not errors, they simply do not count. Callers that need per-edit
//! diagnostics, single-occurrence selection, signature protection or stream
//! preservation guarantees should use [`crate::text_edit`] directly.
//!
//! Behaviour changes relative to the pre-1C implementation (deliberate,
//! each covered by a flipped characterization test):
//! - multiple `/Contents` streams are preserved instead of collapsed;
//! - text covered by `/ActualText` is left untouched instead of desynced;
//! - cross-run matches whose replacement needs a fallback font now succeed;
//! - TJ kerning outside the edited region is preserved;
//! - encrypted documents whose permissions forbid modification are refused.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::error::{ManipError, Result};
use crate::text_edit::{
    self, CommitPolicy, DocumentRevision, FontFallback, ReplaceOptions, SignaturePolicy,
    TextEditError, TextQuery,
};
use crate::text_run::FontMap;
use lopdf::{Document, Object};

/// Replace all occurrences of `search` with `replacement` in a page's content.
///
/// Returns the number of occurrences replaced. Occurrences that cannot be
/// replaced (unencodable characters, unsupported containers such as Form
/// XObjects, `/ActualText`-covered spans) do not count and are not errors.
/// The replacement is encoded in the original font when possible; otherwise a
/// Helvetica/WinAnsiEncoding fallback font is injected and reported through
/// the [`crate::text_edit`] API (this wrapper only returns the count).
///
/// The `fonts` parameter is retained for signature compatibility; the engine
/// builds its own font map.
pub fn replace_text(
    doc: &mut Document,
    page_num: u32,
    search: &str,
    replacement: &str,
    _fonts: &FontMap,
) -> Result<usize> {
    let total = doc.get_pages().len();
    if page_num == 0 || page_num as usize > total {
        return Err(ManipError::PageOutOfRange(page_num as usize, total));
    }
    legacy_replace(
        doc,
        TextQuery::exact(search).pages(page_num..=page_num),
        replacement,
    )
}

/// Replace text across all pages in a document.
///
/// Same contract as [`replace_text`]: returns the total count; occurrences
/// that cannot be replaced simply do not count.
pub fn replace_text_all_pages(
    doc: &mut Document,
    search: &str,
    replacement: &str,
) -> Result<usize> {
    legacy_replace(doc, TextQuery::exact(search), replacement)
}

fn legacy_replace(doc: &mut Document, query: TextQuery, replacement: &str) -> Result<usize> {
    // Match tokens never leave this call, so an internal-only revision
    // suffices; persistence workflows go through `text_edit` directly.
    let revision = DocumentRevision::from_source_bytes(&[]);
    let options = ReplaceOptions::default()
        .font_fallback(FontFallback::InjectStandard)
        .signature_policy(SignaturePolicy::AllowPostSignatureChange)
        .commit_policy(CommitPolicy::BestEffort);
    match text_edit::replace_text(doc, revision, query, replacement, options) {
        Ok(report) => Ok(report.replacements_applied),
        Err(e) => match e.error {
            TextEditError::Document(m) => Err(m),
            other => Err(ManipError::Other(format!(
                "text replacement failed: {other}"
            ))),
        },
    }
}

// ---------------------------------------------------------------------------
// Text encoding
// ---------------------------------------------------------------------------

/// Encode Unicode text back to PDF string bytes for the given font.
///
/// Priority:
/// 1. ToUnicode CMap reverse map (authoritative).
/// 2. Differences/Encoding-based reverse map (for fonts without ToUnicode
///    but with explicit Encoding in the PDF dict, e.g. WinAnsiEncoding).
/// 3. Latin-1 fallback for full (non-subset) fonts only.  Subset fonts
///    (BaseFont prefix like "ABCDEF+") have unknown glyph inventories,
///    so we return an error to avoid silently writing unrenderable bytes.
pub(crate) fn encode_text_for_font(
    font_name: &str,
    text: &str,
    fonts: &FontMap,
) -> Result<Vec<u8>> {
    if fonts.is_cid_font(font_name) {
        return encode_cid_text(font_name, text, fonts);
    }

    // Reverse map covers both ToUnicode CMap entries and Differences-based
    // encoding entries (built in FontMap::build_reverse_map).
    let reverse = fonts.build_reverse_map(font_name);
    if !reverse.is_empty() {
        let mut bytes = Vec::with_capacity(text.len());
        for ch in text.chars() {
            if let Some(&code) = reverse.get(&ch) {
                if code <= 0xFF {
                    bytes.push(code as u8);
                } else {
                    return Err(ManipError::Other(format!(
                        "character '{}' (U+{:04X}) maps to code {} which exceeds single-byte range in font '{}'",
                        ch, ch as u32, code, font_name
                    )));
                }
            } else {
                return Err(ManipError::Other(format!(
                    "character '{}' (U+{:04X}) not available in font '{}' (subset or custom encoding)",
                    ch, ch as u32, font_name
                )));
            }
        }
        // Round-trip verification: decode back and confirm we get the original text.
        // Catches ToUnicode CMaps where the reverse map maps a char to a code that
        // the CMap decodes to a different character (e.g. partial CMap with overlap).
        // Fixes #468.
        let decoded_back = fonts.decode_string(font_name, &bytes);
        if decoded_back != text {
            return Err(ManipError::Other(format!(
                "encoding round-trip mismatch for font '{}': '{}' re-decodes as '{}'",
                font_name, text, decoded_back
            )));
        }
        return Ok(bytes);
    }

    // No reverse map available (font has neither ToUnicode nor an explicit
    // Encoding dict).  Refuse for two classes of fonts where Latin-1 is unsafe:
    //
    // 1. Subset fonts ("ABCDEF+" prefix): only the glyphs used in the source
    //    document are embedded; writing arbitrary bytes may produce unrenderable
    //    glyphs and a FAIL on round-trip verification.
    //
    // 2. Symbolic fonts (FontDescriptor.Flags bit 3): these use the font's
    //    built-in encoding, which may differ arbitrarily from StandardEncoding
    //    (the PDF default for fonts without an Encoding entry).  A Latin-1
    //    encoding of, e.g., '_' as 0x5F would fail round-trip verification if
    //    the font maps 0x5F to a different glyph. Fixes #455.
    if fonts.is_subset_font(font_name) {
        return Err(ManipError::Other(format!(
            "font '{}' is a subset with no known encoding — cannot safely encode replacement text",
            font_name
        )));
    }
    if fonts.is_symbolic_font(font_name) {
        return Err(ManipError::Other(format!(
            "font '{}' is symbolic with no known encoding — Latin-1 fallback unsafe",
            font_name
        )));
    }

    // Latin-1 fallback for full fonts without explicit Encoding.
    let mut bytes = Vec::with_capacity(text.len());
    for ch in text.chars() {
        let code = ch as u32;
        if code <= 0xFF {
            bytes.push(code as u8);
        } else {
            return Err(ManipError::Other(format!(
                "character '{}' (U+{:04X}) cannot be encoded in font '{}'",
                ch, code, font_name
            )));
        }
    }
    Ok(bytes)
}

fn encode_cid_text(font_name: &str, text: &str, fonts: &FontMap) -> Result<Vec<u8>> {
    // For CID fonts with Identity-H encoding, try to build reverse map
    // from the ToUnicode CMap. If no reverse map is available, attempt
    // direct Unicode → CID mapping.
    let reverse = fonts.build_reverse_map(font_name);
    let mut bytes = Vec::with_capacity(text.len() * 2);

    for ch in text.chars() {
        if let Some(code) = reverse.get(&ch) {
            bytes.push((*code >> 8) as u8);
            bytes.push((*code & 0xFF) as u8);
        } else {
            return Err(ManipError::Other(format!(
                "character '{}' (U+{:04X}) not in font '{}' CMap",
                ch, ch as u32, font_name
            )));
        }
    }

    Ok(bytes)
}

// ---------------------------------------------------------------------------
// Font fallback for replacement encoding (Fixes #466 bugs 1–4)
// ---------------------------------------------------------------------------

/// Encode text as Latin-1 (ISO 8859-1) bytes.
pub(crate) fn encode_latin1(text: &str) -> Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(text.len());
    for ch in text.chars() {
        let code = ch as u32;
        if code <= 0xFF {
            bytes.push(code as u8);
        } else {
            return Err(ManipError::Other(format!(
                "character '{}' (U+{:04X}) cannot be encoded as Latin-1",
                ch, code
            )));
        }
    }
    Ok(bytes)
}

/// Inject a Helvetica/WinAnsiEncoding font resource named `"F__Helv"` into
/// the page's Resources/Font dictionary, creating sub-dictionaries as needed.
///
/// Returns `None` if the injection could not be confirmed (e.g. Resources dict
/// is not writable), so callers can avoid referencing an unknown font.
pub(crate) fn inject_fallback_font(doc: &mut Document, page_num: u32) -> Option<String> {
    const FALLBACK: &str = "F__Helv";

    let pages = doc.get_pages();
    let &page_id = pages.get(&page_num)?;

    // Add the Helvetica font object first (no borrow of doc held after this).
    let mut helv_dict = lopdf::Dictionary::new();
    helv_dict.set("Type", Object::Name(b"Font".to_vec()));
    helv_dict.set("Subtype", Object::Name(b"Type1".to_vec()));
    helv_dict.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
    helv_dict.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
    let helv_id = doc.add_object(Object::Dictionary(helv_dict));

    // Read the current Resources entry before any mutation.
    let resources_entry = doc.get_object(page_id).ok().and_then(|obj| {
        if let Object::Dictionary(ref d) = obj {
            d.get(b"Resources").ok().cloned()
        } else {
            None
        }
    });

    // Track whether F__Helv was actually inserted into the Resources/Font dict.
    // If insertion fails (e.g. because the target object isn't a mutable dict),
    // we return None so callers don't emit Tf operators for an unknown font.
    // Fixes: fallback injection was silently failing on some PDFs, leaving the
    // content stream referencing F__Helv that wasn't in Resources — causing
    // PDFium to fail text extraction ("cannot extract text after roundtrip").
    let inserted = match resources_entry {
        Some(Object::Reference(res_id)) => {
            let font_entry = doc.get_object(res_id).ok().and_then(|obj| {
                if let Object::Dictionary(ref d) = obj {
                    d.get(b"Font").ok().cloned()
                } else {
                    None
                }
            });
            match font_entry {
                Some(Object::Reference(fd_id)) => {
                    if let Ok(Object::Dictionary(ref mut fd)) = doc.get_object_mut(fd_id) {
                        fd.set(FALLBACK, Object::Reference(helv_id));
                        true
                    } else {
                        false
                    }
                }
                font_val => {
                    let mut new_font = match font_val {
                        Some(Object::Dictionary(fd)) => fd,
                        _ => lopdf::Dictionary::new(),
                    };
                    new_font.set(FALLBACK, Object::Reference(helv_id));
                    if let Ok(Object::Dictionary(ref mut rd)) = doc.get_object_mut(res_id) {
                        rd.set("Font", Object::Dictionary(new_font));
                        true
                    } else {
                        false
                    }
                }
            }
        }
        Some(Object::Dictionary(res_dict)) => {
            let font_entry = res_dict.get(b"Font").ok().cloned();
            match font_entry {
                Some(Object::Reference(fd_id)) => {
                    if let Ok(Object::Dictionary(ref mut fd)) = doc.get_object_mut(fd_id) {
                        fd.set(FALLBACK, Object::Reference(helv_id));
                        true
                    } else {
                        false
                    }
                }
                font_val => {
                    let mut new_font = match font_val {
                        Some(Object::Dictionary(fd)) => fd,
                        _ => lopdf::Dictionary::new(),
                    };
                    new_font.set(FALLBACK, Object::Reference(helv_id));
                    let mut new_res = res_dict;
                    new_res.set("Font", Object::Dictionary(new_font));
                    if let Ok(Object::Dictionary(ref mut pd)) = doc.get_object_mut(page_id) {
                        pd.set("Resources", Object::Dictionary(new_res));
                        true
                    } else {
                        false
                    }
                }
            }
        }
        _ => {
            // Page has no direct Resources — create a minimal dict with only
            // F__Helv. Note: any fonts from an inherited parent Resources are
            // not copied here, but since get_page_font_dict also doesn't walk
            // the parent chain, the FontMap would be empty for this page and
            // no replacement would have been attempted in the first place.
            let mut new_font = lopdf::Dictionary::new();
            new_font.set(FALLBACK, Object::Reference(helv_id));
            let mut new_res = lopdf::Dictionary::new();
            new_res.set("Font", Object::Dictionary(new_font));
            if let Ok(Object::Dictionary(ref mut pd)) = doc.get_object_mut(page_id) {
                pd.set("Resources", Object::Dictionary(new_res));
                true
            } else {
                false
            }
        }
    };

    if inserted {
        Some(FALLBACK.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_editor::editor_for_page;
    use crate::text_run::extract_text_runs;
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Document, Object, Stream};

    /// Build a minimal document with a Type0 (CID/Identity-H) font whose
    /// ToUnicode CMap covers the printable ASCII range (0x0020–0x007E).
    /// The character codes equal the Unicode codepoints, so e.g. 'H' is
    /// encoded as the 2-byte CID code [0x00, 0x48].
    fn make_doc_with_cid_font(content_bytes: Vec<u8>) -> Document {
        let mut doc = Document::with_version("1.7");

        // ToUnicode CMap: maps CID = Unicode for printable ASCII.
        let to_unicode_data = b"/CIDInit /ProcSet findresource begin\n\
            12 dict begin\n\
            begincmap\n\
            /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
            /CMapName /Adobe-Identity-UCS def\n\
            /CMapType 2 def\n\
            1 begincodespacerange\n\
            <0000> <FFFF>\n\
            endcodespacerange\n\
            1 beginbfrange\n\
            <0020> <007E> <0020>\n\
            endbfrange\n\
            endcmap\n\
            CMapName currentdict /CMap defineresource pop\n\
            end\n\
            end\n";
        let to_unicode_stream = Stream::new(dictionary! {}, to_unicode_data.to_vec());
        let to_unicode_id = doc.add_object(Object::Stream(to_unicode_stream));

        let cid_font = dictionary! {
            "Type" => Object::Name(b"Font".to_vec()),
            "Subtype" => Object::Name(b"CIDFontType2".to_vec()),
            "BaseFont" => Object::Name(b"TestCIDFont".to_vec()),
            "CIDSystemInfo" => Object::Dictionary(dictionary! {
                "Registry" => Object::String(b"Adobe".to_vec(), lopdf::StringFormat::Literal),
                "Ordering" => Object::String(b"Identity".to_vec(), lopdf::StringFormat::Literal),
                "Supplement" => Object::Integer(0),
            }),
            "DW" => Object::Integer(1000),
        };
        let cid_font_id = doc.add_object(Object::Dictionary(cid_font));

        let type0_font = dictionary! {
            "Type" => Object::Name(b"Font".to_vec()),
            "Subtype" => Object::Name(b"Type0".to_vec()),
            "BaseFont" => Object::Name(b"TestCIDFont".to_vec()),
            "Encoding" => Object::Name(b"Identity-H".to_vec()),
            "DescendantFonts" => Object::Array(vec![Object::Reference(cid_font_id)]),
            "ToUnicode" => Object::Reference(to_unicode_id),
        };
        let type0_font_id = doc.add_object(Object::Dictionary(type0_font));

        let font_resources = dictionary! {
            "F1" => Object::Reference(type0_font_id),
        };
        let resources = dictionary! {
            "Font" => Object::Dictionary(font_resources),
        };

        let content_stream = Stream::new(dictionary! {}, content_bytes);
        let content_id = doc.add_object(Object::Stream(content_stream));

        let page_dict = dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(612), Object::Integer(792),
            ]),
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(resources),
        };
        let page_id = doc.add_object(Object::Dictionary(page_dict));

        let pages_dict = dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => Object::Integer(1),
        };
        let pages_id = doc.add_object(Object::Dictionary(pages_dict));

        if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
            d.set("Parent", Object::Reference(pages_id));
        }

        let catalog = dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc
    }

    /// Encode "Hello" as Identity-H CID bytes: each char's code = Unicode codepoint.
    fn hello_cid_content() -> Vec<u8> {
        // CID codes: H=0x0048, e=0x0065, l=0x006C, l=0x006C, o=0x006F
        let cid_bytes = vec![0x00u8, 0x48, 0x00, 0x65, 0x00, 0x6C, 0x00, 0x6C, 0x00, 0x6F];
        Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), Object::Real(12.0)]),
                Operation::new("Td", vec![Object::Real(100.0), Object::Real(700.0)]),
                Operation::new(
                    "Tj",
                    vec![Object::String(cid_bytes, lopdf::StringFormat::Hexadecimal)],
                ),
                Operation::new("ET", vec![]),
            ],
        }
        .encode()
        .unwrap()
    }

    fn make_doc_with_text(content: &[u8]) -> Document {
        let mut doc = Document::with_version("1.7");

        let font = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        };
        let font_id = doc.add_object(Object::Dictionary(font));

        let font_resources = dictionary! {
            "F1" => Object::Reference(font_id),
        };
        let resources = dictionary! {
            "Font" => Object::Dictionary(font_resources),
        };

        let content_stream = Stream::new(dictionary! {}, content.to_vec());
        let content_id = doc.add_object(Object::Stream(content_stream));

        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(resources),
        };
        let page_id = doc.add_object(Object::Dictionary(page_dict));

        let pages_dict = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        };
        let pages_id = doc.add_object(Object::Dictionary(pages_dict));

        if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
            d.set("Parent", Object::Reference(pages_id));
        }

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc
    }

    #[test]
    fn replace_simple_same_length() {
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (Hello World) Tj ET");
        let fonts = FontMap::from_page(&doc, 1).unwrap();
        let count = replace_text(&mut doc, 1, "Hello", "Hallo", &fonts).unwrap();
        assert_eq!(count, 1);

        // Verify the replacement.
        let editor = editor_for_page(&doc, 1).unwrap();
        let runs = extract_text_runs(&editor, &fonts);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "Hallo World");
    }

    #[test]
    fn replace_different_length() {
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (Hello World) Tj ET");
        let fonts = FontMap::from_page(&doc, 1).unwrap();
        let count = replace_text(&mut doc, 1, "World", "Earth!", &fonts).unwrap();
        assert_eq!(count, 1);

        let editor = editor_for_page(&doc, 1).unwrap();
        let runs = extract_text_runs(&editor, &fonts);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "Hello Earth!");
    }

    #[test]
    fn replace_no_match() {
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET");
        let fonts = FontMap::from_page(&doc, 1).unwrap();
        let count = replace_text(&mut doc, 1, "Missing", "Replacement", &fonts).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn replace_in_tj_array() {
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 100 700 Td [(Hel) -100 (lo)] TJ ET");
        let fonts = FontMap::from_page(&doc, 1).unwrap();
        let count = replace_text(&mut doc, 1, "Hello", "Hallo", &fonts).unwrap();
        assert_eq!(count, 1);

        let editor = editor_for_page(&doc, 1).unwrap();
        let runs = extract_text_runs(&editor, &fonts);
        assert_eq!(runs[0].text, "Hallo");
    }

    #[test]
    fn replace_error_on_unencodable_char() {
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET");
        let fonts = FontMap::from_page(&doc, 1).unwrap();
        // Characters outside Latin-1 cannot be encoded by the original font or
        // the Latin-1 fallback, so the replacement is silently skipped (Ok(0)).
        let result = replace_text(&mut doc, 1, "Hello", "\u{4e16}\u{754c}", &fonts);
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn replace_all_pages() {
        let mut doc = Document::with_version("1.7");

        let font = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        };
        let font_id = doc.add_object(Object::Dictionary(font));

        let font_resources = dictionary! {
            "F1" => Object::Reference(font_id),
        };
        let resources = dictionary! {
            "Font" => Object::Dictionary(font_resources),
        };

        // Two pages with same text.
        let mut page_ids = Vec::new();
        for _ in 0..2 {
            let content = b"BT /F1 12 Tf (Hello) Tj ET";
            let content_stream = Stream::new(dictionary! {}, content.to_vec());
            let content_id = doc.add_object(Object::Stream(content_stream));

            let page_dict = dictionary! {
                "Type" => "Page",
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                "Contents" => Object::Reference(content_id),
                "Resources" => Object::Dictionary(resources.clone()),
            };
            let page_id = doc.add_object(Object::Dictionary(page_dict));
            page_ids.push(page_id);
        }

        let pages_dict = dictionary! {
            "Type" => "Pages",
            "Kids" => page_ids.iter().map(|id| Object::Reference(*id)).collect::<Vec<_>>(),
            "Count" => page_ids.len() as i64,
        };
        let pages_id = doc.add_object(Object::Dictionary(pages_dict));

        for &page_id in &page_ids {
            if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
                d.set("Parent", Object::Reference(pages_id));
            }
        }

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let count = replace_text_all_pages(&mut doc, "Hello", "Hallo").unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn replace_cross_run_split_tj() {
        // Text "January" split across three Tj operators.
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (Jan) Tj (u) Tj (ary) Tj ET");
        let fonts = FontMap::from_page(&doc, 1).unwrap();
        let count = replace_text(&mut doc, 1, "January", "Februar", &fonts).unwrap();
        assert_eq!(count, 1);

        // Verify: the combined text should now contain "Februar".
        let editor = editor_for_page(&doc, 1).unwrap();
        let runs = extract_text_runs(&editor, &fonts);
        let combined: String = runs.iter().map(|r| r.text.as_str()).collect();
        assert!(
            combined.contains("Februar"),
            "expected 'Februar' in '{combined}'"
        );
    }

    #[test]
    fn replace_cross_run_with_positioning() {
        // Text "Hello" split across two Tj ops with a Td positioning op in between.
        let mut doc =
            make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (Hel) Tj 0.5 0 Td (lo World) Tj ET");
        let fonts = FontMap::from_page(&doc, 1).unwrap();
        let count = replace_text(&mut doc, 1, "Hello", "Hallo", &fonts).unwrap();
        assert_eq!(count, 1);

        let editor = editor_for_page(&doc, 1).unwrap();
        let runs = extract_text_runs(&editor, &fonts);
        let combined: String = runs.iter().map(|r| r.text.as_str()).collect();
        assert!(
            combined.contains("Hallo"),
            "expected 'Hallo' in '{combined}'"
        );
    }

    #[test]
    fn replace_cross_run_single_run_takes_priority() {
        // When the match is within a single run, use the fast path.
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (Hello World) Tj ET");
        let fonts = FontMap::from_page(&doc, 1).unwrap();
        let count = replace_text(&mut doc, 1, "Hello", "Hallo", &fonts).unwrap();
        assert_eq!(count, 1);

        let editor = editor_for_page(&doc, 1).unwrap();
        let runs = extract_text_runs(&editor, &fonts);
        assert_eq!(runs[0].text, "Hallo World");
    }

    #[test]
    fn replace_cross_run_multiline_preserves_other_lines() {
        // Regression (PDFluent multiline collapse): a text block of three lines,
        // each positioned with its own Td, all in the same font. The middle line
        // is split across two Tj operators, so editing it must go through the
        // cross-run path. Runs of the OTHER lines must keep their own text —
        // the rewrite must not concatenate the whole same-font group into the
        // first run (which draws the entire block on line 1, off-screen).
        let mut doc = make_doc_with_text(
            b"BT /F1 12 Tf 72 700 Td (First line here.) Tj 0 -14 Td (Some words to ) Tj (edit now.) Tj 0 -14 Td (Third line stays.) Tj ET",
        );
        let fonts = FontMap::from_page(&doc, 1).unwrap();
        let count = replace_text(
            &mut doc,
            1,
            "Some words to edit now.",
            "Some words to edit.",
            &fonts,
        )
        .unwrap();
        assert_eq!(count, 1);

        let editor = editor_for_page(&doc, 1).unwrap();
        let runs = extract_text_runs(&editor, &fonts);

        // Untouched lines keep their own operators and text.
        assert_eq!(
            runs.first().map(|r| r.text.as_str()),
            Some("First line here."),
            "first line must keep only its own text"
        );
        assert_eq!(
            runs.last().map(|r| r.text.as_str()),
            Some("Third line stays."),
            "third line must not be emptied"
        );

        // The edited line carries the replacement; nothing else changed.
        let combined: String = runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(
            combined,
            "First line here.Some words to edit.Third line stays."
        );

        // Lines stay on distinct baselines (positioning ops untouched).
        let first_y = runs.first().unwrap().y;
        let last_y = runs.last().unwrap().y;
        assert!(
            (first_y - last_y).abs() > 20.0,
            "lines collapsed onto one baseline: first_y={first_y} last_y={last_y}"
        );
    }

    #[test]
    fn replace_cross_run_keeps_prefix_and_suffix_in_their_runs() {
        // Match starts mid-run-0 and ends mid-run-1: the prefix stays in run 0,
        // the suffix stays in run 1, and the replacement lands in run 0.
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 72 700 Td (AAA xx) Tj (yy BBB) Tj ET");
        let fonts = FontMap::from_page(&doc, 1).unwrap();
        let count = replace_text(&mut doc, 1, "xxyy", "Q", &fonts).unwrap();
        assert_eq!(count, 1);

        let editor = editor_for_page(&doc, 1).unwrap();
        let runs = extract_text_runs(&editor, &fonts);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].text, "AAA Q");
        assert_eq!(runs[1].text, " BBB");
    }

    #[test]
    fn replace_cross_run_multiple_occurrences_in_group() {
        // Two occurrences inside one same-font group, each straddling a run
        // boundary. Both are replaced; each replacement lands in the run where
        // its match starts.
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 72 700 Td (ab) Tj (cab) Tj (c) Tj ET");
        let fonts = FontMap::from_page(&doc, 1).unwrap();
        let count = replace_text(&mut doc, 1, "abc", "X", &fonts).unwrap();
        assert_eq!(count, 2);

        let editor = editor_for_page(&doc, 1).unwrap();
        let runs = extract_text_runs(&editor, &fonts);
        let combined: String = runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(combined, "XX");
    }

    // ── CID font tests ────────────────────────────────────────────────────────

    #[test]
    fn replace_cid_font_chars_in_cmap() {
        // CID/Identity-H font whose ToUnicode CMap covers printable ASCII.
        // "Hallo" uses the same chars as "Hello" — all in the CMap — so
        // encode_cid_text should succeed without Helvetica injection.
        let mut doc = make_doc_with_cid_font(hello_cid_content());
        let fonts = FontMap::from_page(&doc, 1).unwrap();

        assert!(fonts.is_cid_font("F1"), "F1 must be recognized as CID font");

        let count = replace_text(&mut doc, 1, "Hello", "Hallo", &fonts).unwrap();
        assert_eq!(count, 1);

        // Decode the result and verify the replacement.
        let fonts_after = FontMap::from_page(&doc, 1).unwrap();
        let editor = editor_for_page(&doc, 1).unwrap();
        let runs = extract_text_runs(&editor, &fonts_after);
        let combined: String = runs.iter().map(|r| r.text.as_str()).collect();
        assert!(
            combined.contains("Hallo"),
            "expected 'Hallo' in '{combined}'"
        );
        assert!(
            !combined.contains("Hello"),
            "old text 'Hello' should be gone"
        );
    }

    #[test]
    fn replace_cid_font_no_match() {
        let mut doc = make_doc_with_cid_font(hello_cid_content());
        let fonts = FontMap::from_page(&doc, 1).unwrap();
        let count = replace_text(&mut doc, 1, "World", "Earth", &fonts).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn replace_cid_font_fallback_chars_not_in_cmap() {
        // Replacement text contains chars outside the CID font's CMap range
        // (e.g. Japanese kana).  encode_cid_text will fail and the Helvetica
        // fallback path is used instead.  Verify that a replacement still
        // happens (count == 1) — the exact font used is secondary.
        let mut doc = make_doc_with_cid_font(hello_cid_content());
        let fonts = FontMap::from_page(&doc, 1).unwrap();
        // '\u{3053}' = 'こ', not in the ASCII-only CMap → fallback needed.
        let count = replace_text(&mut doc, 1, "Hello", "Hal\u{3053}", &fonts).unwrap();
        // Non-Latin replacement cannot be encoded in either CID font or Helvetica
        // (Helvetica/WinAnsi only covers Latin-1).  Expect 0 replacements.
        assert_eq!(count, 0);
    }

    #[test]
    fn replace_cid_font_latin_fallback() {
        // Replacement is ASCII but NOT in the CID font's CMap (the CMap only
        // covers 0x0020-0x007E and 'ê' is U+00EA = outside that range).
        // encode_cid_text fails, but the Helvetica fallback should handle
        // pure-Latin-1 chars.  Verify count == 1.
        let mut doc = make_doc_with_cid_font(hello_cid_content());
        let fonts = FontMap::from_page(&doc, 1).unwrap();
        // Replace prefix + out-of-CMap char + suffix so fallback is exercised.
        // "Hêllo" → 'ê' (U+00EA) is Latin-1 but outside the bfrange 0x0020-0x007E.
        let count = replace_text(&mut doc, 1, "Hello", "Hêllo", &fonts).unwrap();
        assert_eq!(count, 1);
    }
}
