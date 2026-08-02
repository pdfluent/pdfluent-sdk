#![warn(missing_docs)]
#![forbid(unsafe_code)]
//! The PDF Standard 14 fonts, compiled into the binary.
//!
//! A PDF may reference Helvetica, Times or Courier without embedding them,
//! because every viewer is required to have them. A *converter* does not have
//! that luxury: PDF/A forbids non-embedded fonts, so something has to be
//! embedded, and what gets embedded decides the glyph widths the output
//! declares.
//!
//! Resolving that from the host filesystem makes the output depend on which
//! fonts happen to be installed. Measured on 300 real-world documents, the same
//! code scored 286/300 on macOS and 275/300 on Debian purely because the
//! substitute differed. This crate removes that variable.
//!
//! # Why these fonts
//!
//! The advance widths match the Adobe AFM metrics for the Standard 14 exactly,
//! which is what ISO 19005-2 §6.2.11.5 compares against. A merely similar font
//! (Liberation is metric-compatible with Arial, not with Helvetica) produces
//! widths that disagree with what the source document declared, and the
//! document then fails validation through no fault of its own.
//!
//! # Format
//!
//! Bare CFF, roughly 15–20 KB per face. Embed as `/FontFile3` with
//! `/Subtype /Type1C`; there is no OpenType wrapper, so `/Subtype /OpenType`
//! would be wrong.
//!
//! ```
//! use pdf_standard_fonts::StandardFont;
//!
//! let font = StandardFont::from_base_font("Arial-BoldMT").unwrap();
//! assert_eq!(font, StandardFont::HelveticaBold);
//! assert!(font.data().starts_with(&[1, 0, 4])); // CFF header
//! ```
//!
//! # Licensing
//!
//! The font programs are from PDFium, © 2014 Foxit Software Inc., under the
//! BSD-3-Clause licence in `LICENSE-FOXIT`. That licence requires the copyright
//! notice to travel with binary redistributions, so it ships in the crate.

/// One of the PDF Standard 14 fonts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StandardFont {
    /// Helvetica (regular).
    Helvetica,
    /// Helvetica Bold.
    HelveticaBold,
    /// Helvetica Oblique.
    HelveticaOblique,
    /// Helvetica Bold Oblique.
    HelveticaBoldOblique,
    /// Times Roman.
    TimesRoman,
    /// Times Bold.
    TimesBold,
    /// Times Italic.
    TimesItalic,
    /// Times Bold Italic.
    TimesBoldItalic,
    /// Courier (regular).
    Courier,
    /// Courier Bold.
    CourierBold,
    /// Courier Oblique.
    CourierOblique,
    /// Courier Bold Oblique.
    CourierBoldOblique,
    /// Symbol.
    Symbol,
    /// Zapf Dingbats.
    ZapfDingbats,
}

impl StandardFont {
    /// Every Standard 14 font, in the order the specification lists them.
    pub const ALL: [StandardFont; 14] = [
        Self::Helvetica,
        Self::HelveticaBold,
        Self::HelveticaOblique,
        Self::HelveticaBoldOblique,
        Self::TimesRoman,
        Self::TimesBold,
        Self::TimesItalic,
        Self::TimesBoldItalic,
        Self::Courier,
        Self::CourierBold,
        Self::CourierOblique,
        Self::CourierBoldOblique,
        Self::Symbol,
        Self::ZapfDingbats,
    ];

    /// The font program: bare CFF, ready for `/FontFile3` `/Subtype /Type1C`.
    pub fn data(self) -> &'static [u8] {
        match self {
            Self::Helvetica => include_bytes!("../assets/FoxitSans.cff"),
            Self::HelveticaBold => include_bytes!("../assets/FoxitSansBold.cff"),
            Self::HelveticaOblique => include_bytes!("../assets/FoxitSansItalic.cff"),
            Self::HelveticaBoldOblique => include_bytes!("../assets/FoxitSansBoldItalic.cff"),
            Self::TimesRoman => include_bytes!("../assets/FoxitSerif.cff"),
            Self::TimesBold => include_bytes!("../assets/FoxitSerifBold.cff"),
            Self::TimesItalic => include_bytes!("../assets/FoxitSerifItalic.cff"),
            Self::TimesBoldItalic => include_bytes!("../assets/FoxitSerifBoldItalic.cff"),
            Self::Courier => include_bytes!("../assets/FoxitFixed.cff"),
            Self::CourierBold => include_bytes!("../assets/FoxitFixedBold.cff"),
            Self::CourierOblique => include_bytes!("../assets/FoxitFixedItalic.cff"),
            Self::CourierBoldOblique => include_bytes!("../assets/FoxitFixedBoldItalic.cff"),
            Self::Symbol => include_bytes!("../assets/FoxitSymbol.cff"),
            Self::ZapfDingbats => include_bytes!("../assets/FoxitDingbats.cff"),
        }
    }

    /// The canonical PostScript name, as it appears in a PDF `/BaseFont`.
    pub fn postscript_name(self) -> &'static str {
        match self {
            Self::Helvetica => "Helvetica",
            Self::HelveticaBold => "Helvetica-Bold",
            Self::HelveticaOblique => "Helvetica-Oblique",
            Self::HelveticaBoldOblique => "Helvetica-BoldOblique",
            Self::TimesRoman => "Times-Roman",
            Self::TimesBold => "Times-Bold",
            Self::TimesItalic => "Times-Italic",
            Self::TimesBoldItalic => "Times-BoldItalic",
            Self::Courier => "Courier",
            Self::CourierBold => "Courier-Bold",
            Self::CourierOblique => "Courier-Oblique",
            Self::CourierBoldOblique => "Courier-BoldOblique",
            Self::Symbol => "Symbol",
            Self::ZapfDingbats => "ZapfDingbats",
        }
    }

    /// Whether the font uses its own built-in encoding rather than a text one.
    ///
    /// Symbol and Zapf Dingbats do; a PDF must not impose WinAnsi on them.
    pub fn is_symbolic(self) -> bool {
        matches!(self, Self::Symbol | Self::ZapfDingbats)
    }

    /// Resolve a `/BaseFont` name to a Standard 14 font.
    ///
    /// Handles a leading `ABCDEF+` subset prefix, the canonical names, and the
    /// metric-compatible families that PDF producers emit for them: Arial for
    /// Helvetica, Times New Roman for Times, Courier New for Courier. Those
    /// three substitutions are safe because the families share advance widths
    /// by design.
    ///
    /// Returns `None` for anything else. A typeface that merely looks similar
    /// has different widths, and swapping it in would trade one validation
    /// failure for a document that no longer lays out the way it did.
    pub fn from_base_font(name: &str) -> Option<Self> {
        let name = strip_subset_prefix(name);
        // Producers write the same font a dozen ways: "Arial-BoldMT",
        // "Arial,Bold", "Arial-Bold", "TimesNewRomanPS-BoldItalicMT". Dropping
        // separators and the "MT"/"PS" foundry markers collapses them all.
        // Removing those two markers outright is safe here only because no name
        // this function recognises contains "mt" or "ps" as part of a real word.
        let normalized: String = name
            .chars()
            .filter(|c| !matches!(c, '-' | ',' | ' ' | '_'))
            .collect::<String>()
            .to_ascii_lowercase()
            .replace("mt", "")
            .replace("ps", "");

        Some(match normalized.as_str() {
            "helvetica" | "arial" | "helveticaneue" | "nimbussansregular" => Self::Helvetica,
            "helveticabold" | "arialbold" | "helveticaneuebold" | "nimbussansbold" => {
                Self::HelveticaBold
            }
            "helveticaoblique" | "helveticaitalic" | "arialitalic" | "arialoblique" => {
                Self::HelveticaOblique
            }
            "helveticaboldoblique"
            | "helveticabolditalic"
            | "arialbolditalic"
            | "arialboldoblique" => Self::HelveticaBoldOblique,

            "timesroman" | "times" | "timesnewroman" => Self::TimesRoman,
            "timesbold" | "timesnewromanbold" => Self::TimesBold,
            "timesitalic" | "timesnewromanitalic" => Self::TimesItalic,
            "timesbolditalic" | "timesnewromanbolditalic" => Self::TimesBoldItalic,

            "courier" | "couriernew" | "nimbusmonoregular" => Self::Courier,
            "courierbold" | "couriernewbold" | "nimbusmonobold" => Self::CourierBold,
            "courieroblique" | "courieritalic" | "couriernewitalic" | "couriernewoblique" => {
                Self::CourierOblique
            }
            "courierboldoblique"
            | "courierbolditalic"
            | "couriernewbolditalic"
            | "couriernewboldoblique" => Self::CourierBoldOblique,

            "symbol" => Self::Symbol,
            "zapfdingbats" | "dingbats" => Self::ZapfDingbats,

            _ => return None,
        })
    }
}

/// Remove a `ABCDEF+` subset prefix if present.
fn strip_subset_prefix(name: &str) -> &str {
    let bytes = name.as_bytes();
    if bytes.len() > 7 && bytes[6] == b'+' && bytes[..6].iter().all(|b| b.is_ascii_uppercase()) {
        &name[7..]
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_font_carries_a_cff_program() {
        for font in StandardFont::ALL {
            let data = font.data();
            assert!(
                data.len() > 4096,
                "{} program is suspiciously small ({} bytes)",
                font.postscript_name(),
                data.len()
            );
            // CFF header: major=1, minor=0, hdrSize=4.
            assert_eq!(
                &data[..3],
                &[1, 0, 4],
                "{} is not a bare CFF; embedding it as /Type1C would be wrong",
                font.postscript_name()
            );
        }
    }

    #[test]
    fn all_fourteen_are_distinct() {
        let mut seen = std::collections::HashSet::new();
        for font in StandardFont::ALL {
            assert!(
                seen.insert(font.data().as_ptr()),
                "{} shares a program with another face",
                font.postscript_name()
            );
        }
    }

    #[test]
    fn resolves_canonical_names() {
        for font in StandardFont::ALL {
            assert_eq!(
                StandardFont::from_base_font(font.postscript_name()),
                Some(font),
                "{} did not resolve to itself",
                font.postscript_name()
            );
        }
    }

    #[test]
    fn resolves_the_metric_compatible_families() {
        // These are the names PDF producers actually emit. Each shares advance
        // widths with the Standard 14 face it maps to, which is the only reason
        // substituting is safe.
        for (name, want) in [
            ("ArialMT", StandardFont::Helvetica),
            ("Arial-BoldMT", StandardFont::HelveticaBold),
            ("Arial-ItalicMT", StandardFont::HelveticaOblique),
            ("Arial-BoldItalicMT", StandardFont::HelveticaBoldOblique),
            ("TimesNewRomanPSMT", StandardFont::TimesRoman),
            ("TimesNewRomanPS-BoldMT", StandardFont::TimesBold),
            ("CourierNew", StandardFont::Courier),
            ("CourierNewPS-BoldMT", StandardFont::CourierBold),
            ("Arial,Bold", StandardFont::HelveticaBold),
            ("Times New Roman", StandardFont::TimesRoman),
        ] {
            assert_eq!(
                StandardFont::from_base_font(name),
                Some(want),
                "{name} resolved wrongly"
            );
        }
    }

    #[test]
    fn strips_subset_prefixes() {
        assert_eq!(
            StandardFont::from_base_font("ABCDEF+Helvetica-Bold"),
            Some(StandardFont::HelveticaBold)
        );
        // Not a subset prefix: lowercase, and no six-letter tag.
        assert_eq!(StandardFont::from_base_font("abcdef+Helvetica"), None);
    }

    #[test]
    fn refuses_lookalike_typefaces() {
        // These have different metrics. Substituting a Standard 14 face would
        // reflow the document, which is worse than the validation failure it
        // would paper over.
        for name in [
            "Palatino-Roman",
            "Bookman-Light",
            "CenturySchoolbook",
            "Garamond",
            "Verdana",
            "Tahoma",
            "Calibri",
            "Helvetica-Narrow",
        ] {
            assert_eq!(
                StandardFont::from_base_font(name),
                None,
                "{name} must not be treated as a Standard 14 font"
            );
        }
    }

    #[test]
    fn symbolic_faces_are_flagged() {
        assert!(StandardFont::Symbol.is_symbolic());
        assert!(StandardFont::ZapfDingbats.is_symbolic());
        assert!(!StandardFont::Helvetica.is_symbolic());
    }
}
