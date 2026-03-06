//! Core annotation wrapper providing access to common annotation properties.

extern crate alloc;

use crate::appearance::AppearanceDict;
use crate::types::*;
use pdf_syntax::object::dict::keys::*;
use pdf_syntax::object::{Array, Dict, Name, Rect, Stream};
use pdf_syntax::page::Page;

/// A PDF annotation, wrapping the raw dictionary.
#[derive(Debug, Clone)]
pub struct Annotation<'a> {
    dict: Dict<'a>,
}

impl<'a> Annotation<'a> {
    /// Parse all annotations from a page.
    pub fn from_page(page: &Page<'a>) -> Vec<Self> {
        page.annots()
            .into_iter()
            .map(|dict| Self { dict })
            .collect()
    }

    /// Wrap an existing annotation dictionary.
    pub fn from_dict(dict: Dict<'a>) -> Self {
        Self { dict }
    }

    /// Return the raw annotation dictionary.
    pub fn dict(&self) -> &Dict<'a> {
        &self.dict
    }

    /// Return the annotation subtype.
    pub fn annotation_type(&self) -> AnnotationType {
        self.dict
            .get::<Name>(SUBTYPE)
            .map(|n| AnnotationType::from_name(n.as_ref()))
            .unwrap_or(AnnotationType::Unknown)
    }

    /// Return the annotation rectangle.
    pub fn rect(&self) -> Option<Rect> {
        self.dict.get::<Rect>(RECT)
    }

    /// Return the text contents of the annotation.
    pub fn contents(&self) -> Option<alloc::string::String> {
        self.dict
            .get::<pdf_syntax::object::String>(CONTENTS)
            .map(|s| pdf_string_to_string(&s))
    }

    /// Return the annotation flags.
    pub fn flags(&self) -> AnnotationFlags {
        AnnotationFlags(self.dict.get::<u32>(F).unwrap_or(0))
    }

    /// Whether the annotation is hidden.
    pub fn is_hidden(&self) -> bool {
        self.flags().hidden()
    }

    /// Whether the annotation should be printed.
    pub fn is_printable(&self) -> bool {
        self.flags().print()
    }

    /// Return the unique annotation name (`/NM`).
    pub fn name(&self) -> Option<alloc::string::String> {
        self.dict
            .get::<pdf_syntax::object::String>(NM)
            .map(|s| pdf_string_to_string(&s))
    }

    /// Return the modification date (`/M`).
    pub fn modified(&self) -> Option<alloc::string::String> {
        self.dict
            .get::<pdf_syntax::object::String>(M)
            .map(|s| pdf_string_to_string(&s))
    }

    /// Return the author (`/T`).
    pub fn author(&self) -> Option<alloc::string::String> {
        self.dict
            .get::<pdf_syntax::object::String>(T)
            .map(|s| pdf_string_to_string(&s))
    }

    /// Return the subject (`/Subj`).
    pub fn subject(&self) -> Option<alloc::string::String> {
        self.dict
            .get::<pdf_syntax::object::String>(SUBJ)
            .map(|s| pdf_string_to_string(&s))
    }

    /// Return the annotation color (`/C`).
    pub fn color(&self) -> Option<Color> {
        self.dict
            .get::<Array<'_>>(C)
            .map(|arr| Color::from_array(&arr))
    }

    /// Return the border style (`/BS`).
    pub fn border_style(&self) -> Option<BorderStyle> {
        self.dict
            .get::<Dict<'_>>(BS)
            .map(|d| BorderStyle::from_dict(&d))
    }

    /// Return the legacy border array (`/Border`).
    pub fn border_array(&self) -> Option<[f32; 3]> {
        let arr = self.dict.get::<Array<'_>>(BORDER)?;
        let mut iter = arr.iter::<f32>();
        let h_radius = iter.next()?;
        let v_radius = iter.next()?;
        let width = iter.next()?;
        Some([h_radius, v_radius, width])
    }

    /// Return the border effect (`/BE`).
    pub fn border_effect(&self) -> Option<BorderEffect> {
        self.dict
            .get::<Dict<'_>>(BE)
            .map(|d| BorderEffect::from_dict(&d))
    }

    /// Return the interior color (`/IC`).
    pub fn interior_color(&self) -> Option<Color> {
        self.dict
            .get::<Array<'_>>(IC)
            .map(|arr| Color::from_array(&arr))
    }

    /// Return the appearance dictionary (`/AP`).
    pub fn appearance(&self) -> Option<AppearanceDict<'a>> {
        AppearanceDict::from_annot(&self.dict)
    }

    /// Return the normal appearance stream directly.
    pub fn normal_appearance(&self) -> Option<Stream<'a>> {
        self.appearance()?.normal(&self.dict)
    }

    /// Return the creation date (`/CreationDate`).
    pub fn creation_date(&self) -> Option<alloc::string::String> {
        self.dict
            .get::<pdf_syntax::object::String>(CREATION_DATE)
            .map(|s| pdf_string_to_string(&s))
    }

    /// Return the opacity (`/CA`).
    pub fn opacity(&self) -> Option<f32> {
        self.dict.get::<f32>(CA)
    }

    /// Return the in-reply-to annotation dictionary (`/IRT`).
    pub fn irt(&self) -> Option<Annotation<'a>> {
        self.dict
            .get::<Dict<'_>>(IRT)
            .map(|d| Annotation { dict: d })
    }

    /// Return the reply type (`/RT`).
    pub fn reply_type(&self) -> Option<alloc::string::String> {
        self.dict
            .get::<Name>(RT)
            .map(|n| alloc::string::String::from(n.as_str()))
    }

    /// Return the annotation state (`/State`).
    pub fn state(&self) -> Option<alloc::string::String> {
        self.dict
            .get::<Name>(STATE)
            .map(|n| alloc::string::String::from(n.as_str()))
    }

    /// Return the state model (`/StateModel`).
    pub fn state_model(&self) -> Option<alloc::string::String> {
        self.dict
            .get::<Name>(STATE_MODEL)
            .map(|n| alloc::string::String::from(n.as_str()))
    }

    /// Return the popup annotation dictionary (`/Popup`).
    pub fn popup(&self) -> Option<Annotation<'a>> {
        self.dict
            .get::<Dict<'_>>(POPUP)
            .map(|d| Annotation { dict: d })
    }

    /// Return quad points (`/QuadPoints`).
    pub fn quad_points(&self) -> Option<QuadPoints> {
        self.dict
            .get::<Array<'_>>(QUADPOINTS)
            .map(|arr| QuadPoints::from_array(&arr))
    }
}

/// Convert a PDF string (possibly UTF-16BE with BOM) to a Rust `String`.
pub fn pdf_string_to_string(s: &pdf_syntax::object::String) -> alloc::string::String {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let utf16: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        alloc::string::String::from_utf16_lossy(&utf16)
    } else if bytes.len() >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF {
        // UTF-8 BOM
        alloc::string::String::from_utf8_lossy(&bytes[3..]).into_owned()
    } else {
        // PDFDocEncoding: 0x00–0x7F are ASCII, 0x80–0xFF mapped per ISO 32000-2 D.2.
        let mut s = alloc::string::String::with_capacity(bytes.len());
        for &b in bytes {
            s.push(pdfdoc_byte_to_char(b));
        }
        s
    }
}

/// Map a single PDFDocEncoding byte to a Unicode char.
///
/// 0x00–0x7F match ASCII/Latin-1. 0x80–0xAD use the table from
/// ISO 32000-2 Annex D, Table D.2. 0xAE–0xFF match U+00AE–U+00FF.
fn pdfdoc_byte_to_char(b: u8) -> char {
    #[rustfmt::skip]
    static HIGH: [char; 46] = [
        '\u{2022}', '\u{2020}', '\u{2021}', '\u{2026}', // 80–83
        '\u{2014}', '\u{2013}', '\u{0192}', '\u{2044}', // 84–87
        '\u{2039}', '\u{203A}', '\u{2212}', '\u{2030}', // 88–8B
        '\u{201E}', '\u{201C}', '\u{201D}', '\u{2018}', // 8C–8F
        '\u{2019}', '\u{201A}', '\u{2122}', '\u{FB01}', // 90–93
        '\u{FB02}', '\u{0141}', '\u{0152}', '\u{0160}', // 94–97
        '\u{0178}', '\u{017D}', '\u{0131}', '\u{0142}', // 98–9B
        '\u{0153}', '\u{0161}', '\u{017E}', '\u{FFFD}', // 9C–9F
        '\u{20AC}', '\u{00A1}', '\u{00A2}', '\u{00A3}', // A0–A3
        '\u{00A4}', '\u{00A5}', '\u{00A6}', '\u{00A7}', // A4–A7
        '\u{00A8}', '\u{00A9}', '\u{00AA}', '\u{00AB}', // A8–AB
        '\u{00AC}', '\u{00AD}',                          // AC–AD
    ];
    match b {
        0x00..=0x7F => b as char,
        0x80..=0xAD => HIGH[(b - 0x80) as usize],
        0xAE..=0xFF => char::from(b), // U+00AE–U+00FF same as Latin-1
    }
}
