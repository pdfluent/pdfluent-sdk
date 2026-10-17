// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.
//
// The fonts in `assets/` are third-party and under the SIL Open Font License
// 1.1, which is a separate licence with its own terms: Liberation 2.1.5 (see
// LICENSE-Liberation.txt) and Noto Naskh Arabic v2.021 (see
// LICENSE-NotoNaskhArabic.txt).

#![warn(missing_docs)]
//! Substitute fonts, compiled in.
//!
//! building-block: font data behind the PDF/A conversion. A customer reaches
//! this through `convert_to_pdfa`, never directly.
//!
//! PDF/A requires every font to be embedded, and plenty of documents only
//! reference a typeface without carrying it. Taking the substitute from the
//! host makes the output depend on which fonts are installed: measured on 119
//! govdocs holdout documents on 24-08-2026, 37 borrowed from the machine, 129
//! times over.
//!
//! Liberation is metric-compatible with Helvetica, Times and Courier -- same
//! advance widths, so substituting does not reflow the text.
//!
//! Liberation carries Latin, Greek and Cyrillic and no Arabic, so a document
//! that references an Arabic typeface without embedding it got a face with no
//! glyph for a single one of its characters. Noto Naskh Arabic is here for
//! that: it covers all 171 characters `2033091`\'s three unembedded fonts
//! declare, presentation forms included (#502).

/// Every face this crate carries, by file name.
pub const FACES: &[&str] = &[
    "LiberationMono-Bold.ttf",
    "LiberationMono-BoldItalic.ttf",
    "LiberationMono-Italic.ttf",
    "LiberationMono-Regular.ttf",
    "LiberationSans-Bold.ttf",
    "LiberationSans-BoldItalic.ttf",
    "LiberationSans-Italic.ttf",
    "LiberationSans-Regular.ttf",
    "LiberationSerif-Bold.ttf",
    "LiberationSerif-BoldItalic.ttf",
    "LiberationSerif-Italic.ttf",
    "LiberationSerif-Regular.ttf",
    "NotoNaskhArabic-Bold.ttf",
    "NotoNaskhArabic-Regular.ttf",
];

/// The bytes of a bundled face, by file name.
///
/// Matching is on the file name alone, so a caller holding a full path -- the
/// candidate lists are written as `/usr/share/fonts/.../LiberationSans-Regular.ttf`
/// -- should pass the last component.
///
/// Returns `None` when the face is not bundled, and always `None` when the
/// `bundled` feature is off. A caller that gets `None` is free to look on the
/// host, but the result is then machine-dependent and should be reported as
/// such.
#[cfg(feature = "bundled")]
pub fn face(file_name: &str) -> Option<&'static [u8]> {
    match file_name {
        "LiberationMono-Bold.ttf" => Some(include_bytes!("../assets/LiberationMono-Bold.ttf")),
        "LiberationMono-BoldItalic.ttf" => {
            Some(include_bytes!("../assets/LiberationMono-BoldItalic.ttf"))
        }
        "LiberationMono-Italic.ttf" => Some(include_bytes!("../assets/LiberationMono-Italic.ttf")),
        "LiberationMono-Regular.ttf" => {
            Some(include_bytes!("../assets/LiberationMono-Regular.ttf"))
        }
        "LiberationSans-Bold.ttf" => Some(include_bytes!("../assets/LiberationSans-Bold.ttf")),
        "LiberationSans-BoldItalic.ttf" => {
            Some(include_bytes!("../assets/LiberationSans-BoldItalic.ttf"))
        }
        "LiberationSans-Italic.ttf" => Some(include_bytes!("../assets/LiberationSans-Italic.ttf")),
        "LiberationSans-Regular.ttf" => {
            Some(include_bytes!("../assets/LiberationSans-Regular.ttf"))
        }
        "LiberationSerif-Bold.ttf" => Some(include_bytes!("../assets/LiberationSerif-Bold.ttf")),
        "LiberationSerif-BoldItalic.ttf" => {
            Some(include_bytes!("../assets/LiberationSerif-BoldItalic.ttf"))
        }
        "LiberationSerif-Italic.ttf" => {
            Some(include_bytes!("../assets/LiberationSerif-Italic.ttf"))
        }
        "LiberationSerif-Regular.ttf" => {
            Some(include_bytes!("../assets/LiberationSerif-Regular.ttf"))
        }
        "NotoNaskhArabic-Bold.ttf" => Some(include_bytes!("../assets/NotoNaskhArabic-Bold.ttf")),
        "NotoNaskhArabic-Regular.ttf" => {
            Some(include_bytes!("../assets/NotoNaskhArabic-Regular.ttf"))
        }
        _ => None,
    }
}

/// Without the `bundled` feature there are no fonts to hand out.
#[cfg(not(feature = "bundled"))]
pub fn face(_file_name: &str) -> Option<&'static [u8]> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_face_resolves() {
        // FACES and the match arms are generated from the same directory, and
        // they can still drift if someone edits one by hand.
        for name in FACES {
            assert!(
                face(name).is_some(),
                "{name} is listed in FACES and has no bytes"
            );
        }
    }

    #[test]
    fn a_face_we_do_not_carry_answers_none() {
        assert!(face("Wingdings.ttf").is_none());
        assert!(face("").is_none());
    }

    #[test]
    fn the_bytes_are_truetype() {
        // A file that is not a font would still compile in and then fail much
        // later, inside a conversion, as an unreadable substitute.
        for name in FACES {
            let bytes = face(name).expect("bundled");
            assert!(
                bytes.starts_with(&[0x00, 0x01, 0x00, 0x00]) || bytes.starts_with(b"true"),
                "{name} does not begin with a TrueType signature"
            );
        }
    }
}
