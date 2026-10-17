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

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

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

    /// The face's built-in encoding as fixed by ISO 32000-1, Annex D.5
    /// (Symbol) and D.6 (Zapf Dingbats): `(code, glyph name)` pairs in
    /// ascending code order. Codes not listed are undefined.
    ///
    /// `None` for the twelve text faces — they take their encoding from the
    /// PDF (`/Encoding`), not from the program.
    ///
    /// The bundled Foxit programs carry the right glyphs for these faces but
    /// declare StandardEncoding as their built-in CFF encoding, so a font
    /// dictionary without an `/Encoding` resolves e.g. code 108 to `l`
    /// instead of `a71`. Writing this table out as an explicit
    /// `/Encoding /Differences` restates the encoding the face is required
    /// to have.
    pub fn encoding_table(self) -> Option<&'static [(u8, &'static str)]> {
        match self {
            Self::Symbol => Some(SYMBOL_ENCODING),
            Self::ZapfDingbats => Some(ZAPF_DINGBATS_ENCODING),
            _ => None,
        }
    }

    /// What one of this face's glyph names means in Unicode, per the Adobe
    /// glyph list.
    ///
    /// Extraction and `/ToUnicode` need this the moment a symbolic face is
    /// resolved through its built-in encoding: Annex D says code 174 of Symbol
    /// draws `arrowright`, and only the glyph list says `arrowright` is U+2192.
    ///
    /// Scoped to the face on purpose. The Symbol names are unambiguous, but
    /// Zapf Dingbats calls its glyphs `a1`, `a12`, `a191` -- names a Latin font
    /// uses for variants of `a`. Resolving those globally would turn a
    /// producer's `a1` into a scissors glyph, so `ZapfDingbats` is the only
    /// face that answers for them.
    ///
    /// Returns `None` for a name the list does not know. That is not the same
    /// as U+0000, and a caller must write no mapping rather than guess one:
    /// a wrong character extracts as confidently as a right one.
    pub fn glyph_name_to_char(self, name: &str) -> Option<char> {
        if let Some(ch) = lookup(SYMBOL_GLYPH_UNICODE, name) {
            return Some(ch);
        }
        if self == Self::ZapfDingbats {
            return lookup(ZAPF_DINGBATS_GLYPH_UNICODE, name);
        }
        None
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

/// Binary search over a name-sorted table.
fn lookup(table: &[(&str, char)], name: &str) -> Option<char> {
    table
        .binary_search_by(|(candidate, _)| (*candidate).cmp(name))
        .ok()
        .map(|i| table[i].1)
}

/// A Symbol glyph name, wherever it appears.
///
/// The 189 names of Annex D.5 belong to no other face -- nothing else calls a
/// glyph `arrowright` or `suchthat` -- so they resolve without knowing which
/// font declared them. Zapf Dingbats' `aNN` names do not have that property;
/// they go through [`StandardFont::glyph_name_to_char`].
pub fn symbol_glyph_name_to_char(name: &str) -> Option<char> {
    lookup(SYMBOL_GLYPH_UNICODE, name)
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

/// Adobe glyph list for the Symbol face (Annex D.5 plus `apple`,
/// which producers reach through `/Differences`).
const SYMBOL_GLYPH_UNICODE: &[(&str, char)] = &[
    ("Alpha", '\u{0391}'),
    ("Beta", '\u{0392}'),
    ("Chi", '\u{03A7}'),
    ("Delta", '\u{2206}'),
    ("Epsilon", '\u{0395}'),
    ("Eta", '\u{0397}'),
    ("Euro", '\u{20AC}'),
    ("Gamma", '\u{0393}'),
    ("Ifraktur", '\u{2111}'),
    ("Iota", '\u{0399}'),
    ("Kappa", '\u{039A}'),
    ("Lambda", '\u{039B}'),
    ("Mu", '\u{039C}'),
    ("Nu", '\u{039D}'),
    ("Omega", '\u{2126}'),
    ("Omicron", '\u{039F}'),
    ("Phi", '\u{03A6}'),
    ("Pi", '\u{03A0}'),
    ("Psi", '\u{03A8}'),
    ("Rfraktur", '\u{211C}'),
    ("Rho", '\u{03A1}'),
    ("Sigma", '\u{03A3}'),
    ("Tau", '\u{03A4}'),
    ("Theta", '\u{0398}'),
    ("Upsilon", '\u{03A5}'),
    ("Upsilon1", '\u{03D2}'),
    ("Xi", '\u{039E}'),
    ("Zeta", '\u{0396}'),
    ("aleph", '\u{2135}'),
    ("alpha", '\u{03B1}'),
    ("ampersand", '\u{0026}'),
    ("angle", '\u{2220}'),
    ("angleleft", '\u{2329}'),
    ("angleright", '\u{232A}'),
    ("apple", '\u{F8FF}'),
    ("approxequal", '\u{2248}'),
    ("arrowboth", '\u{2194}'),
    ("arrowdblboth", '\u{21D4}'),
    ("arrowdbldown", '\u{21D3}'),
    ("arrowdblleft", '\u{21D0}'),
    ("arrowdblright", '\u{21D2}'),
    ("arrowdblup", '\u{21D1}'),
    ("arrowdown", '\u{2193}'),
    ("arrowhorizex", '\u{F8E7}'),
    ("arrowleft", '\u{2190}'),
    ("arrowright", '\u{2192}'),
    ("arrowup", '\u{2191}'),
    ("arrowvertex", '\u{F8E6}'),
    ("asteriskmath", '\u{2217}'),
    ("bar", '\u{007C}'),
    ("beta", '\u{03B2}'),
    ("braceex", '\u{F8F4}'),
    ("braceleft", '\u{007B}'),
    ("braceleftbt", '\u{F8F3}'),
    ("braceleftmid", '\u{F8F2}'),
    ("bracelefttp", '\u{F8F1}'),
    ("braceright", '\u{007D}'),
    ("bracerightbt", '\u{F8FE}'),
    ("bracerightmid", '\u{F8FD}'),
    ("bracerighttp", '\u{F8FC}'),
    ("bracketleft", '\u{005B}'),
    ("bracketleftbt", '\u{F8F0}'),
    ("bracketleftex", '\u{F8EF}'),
    ("bracketlefttp", '\u{F8EE}'),
    ("bracketright", '\u{005D}'),
    ("bracketrightbt", '\u{F8FB}'),
    ("bracketrightex", '\u{F8FA}'),
    ("bracketrighttp", '\u{F8F9}'),
    ("bullet", '\u{2022}'),
    ("carriagereturn", '\u{21B5}'),
    ("chi", '\u{03C7}'),
    ("circlemultiply", '\u{2297}'),
    ("circleplus", '\u{2295}'),
    ("club", '\u{2663}'),
    ("colon", '\u{003A}'),
    ("comma", '\u{002C}'),
    ("congruent", '\u{2245}'),
    ("copyrightsans", '\u{F8E9}'),
    ("copyrightserif", '\u{F6D9}'),
    ("degree", '\u{00B0}'),
    ("delta", '\u{03B4}'),
    ("diamond", '\u{2666}'),
    ("divide", '\u{00F7}'),
    ("dotmath", '\u{22C5}'),
    ("eight", '\u{0038}'),
    ("element", '\u{2208}'),
    ("ellipsis", '\u{2026}'),
    ("emptyset", '\u{2205}'),
    ("epsilon", '\u{03B5}'),
    ("equal", '\u{003D}'),
    ("equivalence", '\u{2261}'),
    ("eta", '\u{03B7}'),
    ("exclam", '\u{0021}'),
    ("existential", '\u{2203}'),
    ("five", '\u{0035}'),
    ("florin", '\u{0192}'),
    ("four", '\u{0034}'),
    ("fraction", '\u{2044}'),
    ("gamma", '\u{03B3}'),
    ("gradient", '\u{2207}'),
    ("greater", '\u{003E}'),
    ("greaterequal", '\u{2265}'),
    ("heart", '\u{2665}'),
    ("infinity", '\u{221E}'),
    ("integral", '\u{222B}'),
    ("integralbt", '\u{2321}'),
    ("integralex", '\u{F8F5}'),
    ("integraltp", '\u{2320}'),
    ("intersection", '\u{2229}'),
    ("iota", '\u{03B9}'),
    ("kappa", '\u{03BA}'),
    ("lambda", '\u{03BB}'),
    ("less", '\u{003C}'),
    ("lessequal", '\u{2264}'),
    ("logicaland", '\u{2227}'),
    ("logicalnot", '\u{00AC}'),
    ("logicalor", '\u{2228}'),
    ("lozenge", '\u{25CA}'),
    ("minus", '\u{2212}'),
    ("minute", '\u{2032}'),
    ("mu", '\u{00B5}'),
    ("multiply", '\u{00D7}'),
    ("nine", '\u{0039}'),
    ("notelement", '\u{2209}'),
    ("notequal", '\u{2260}'),
    ("notsubset", '\u{2284}'),
    ("nu", '\u{03BD}'),
    ("numbersign", '\u{0023}'),
    ("omega", '\u{03C9}'),
    ("omega1", '\u{03D6}'),
    ("omicron", '\u{03BF}'),
    ("one", '\u{0031}'),
    ("parenleft", '\u{0028}'),
    ("parenleftbt", '\u{F8ED}'),
    ("parenleftex", '\u{F8EC}'),
    ("parenlefttp", '\u{F8EB}'),
    ("parenright", '\u{0029}'),
    ("parenrightbt", '\u{F8F8}'),
    ("parenrightex", '\u{F8F7}'),
    ("parenrighttp", '\u{F8F6}'),
    ("partialdiff", '\u{2202}'),
    ("percent", '\u{0025}'),
    ("period", '\u{002E}'),
    ("perpendicular", '\u{22A5}'),
    ("phi", '\u{03C6}'),
    ("phi1", '\u{03D5}'),
    ("pi", '\u{03C0}'),
    ("plus", '\u{002B}'),
    ("plusminus", '\u{00B1}'),
    ("product", '\u{220F}'),
    ("propersubset", '\u{2282}'),
    ("propersuperset", '\u{2283}'),
    ("proportional", '\u{221D}'),
    ("psi", '\u{03C8}'),
    ("question", '\u{003F}'),
    ("radical", '\u{221A}'),
    ("radicalex", '\u{F8E5}'),
    ("reflexsubset", '\u{2286}'),
    ("reflexsuperset", '\u{2287}'),
    ("registersans", '\u{F8E8}'),
    ("registerserif", '\u{F6DA}'),
    ("rho", '\u{03C1}'),
    ("second", '\u{2033}'),
    ("semicolon", '\u{003B}'),
    ("seven", '\u{0037}'),
    ("sigma", '\u{03C3}'),
    ("sigma1", '\u{03C2}'),
    ("similar", '\u{223C}'),
    ("six", '\u{0036}'),
    ("slash", '\u{002F}'),
    ("space", '\u{0020}'),
    ("spade", '\u{2660}'),
    ("suchthat", '\u{220B}'),
    ("summation", '\u{2211}'),
    ("tau", '\u{03C4}'),
    ("therefore", '\u{2234}'),
    ("theta", '\u{03B8}'),
    ("theta1", '\u{03D1}'),
    ("three", '\u{0033}'),
    ("trademarksans", '\u{F8EA}'),
    ("trademarkserif", '\u{F6DB}'),
    ("two", '\u{0032}'),
    ("underscore", '\u{005F}'),
    ("union", '\u{222A}'),
    ("universal", '\u{2200}'),
    ("upsilon", '\u{03C5}'),
    ("weierstrass", '\u{2118}'),
    ("xi", '\u{03BE}'),
    ("zero", '\u{0030}'),
    ("zeta", '\u{03B6}'),
];

/// Adobe glyph list for the ZapfDingbats face: the `aNN` names of Annex D.6,
/// plus `a205` and `a206`, which the program carries but the encoding does not
/// reach. These names mean this only on this face -- `a1` is a Latin `a`
/// variant elsewhere -- so they are kept out of the general glyph list.
const ZAPF_DINGBATS_GLYPH_UNICODE: &[(&str, char)] = &[
    ("a1", '\u{2701}'),
    ("a10", '\u{2721}'),
    ("a100", '\u{275E}'),
    ("a101", '\u{2761}'),
    ("a102", '\u{2762}'),
    ("a103", '\u{2763}'),
    ("a104", '\u{2764}'),
    ("a105", '\u{2710}'),
    ("a106", '\u{2765}'),
    ("a107", '\u{2766}'),
    ("a108", '\u{2767}'),
    ("a109", '\u{2660}'),
    ("a11", '\u{261B}'),
    ("a110", '\u{2665}'),
    ("a111", '\u{2666}'),
    ("a112", '\u{2663}'),
    ("a117", '\u{2709}'),
    ("a118", '\u{2708}'),
    ("a119", '\u{2707}'),
    ("a12", '\u{261E}'),
    ("a120", '\u{2460}'),
    ("a121", '\u{2461}'),
    ("a122", '\u{2462}'),
    ("a123", '\u{2463}'),
    ("a124", '\u{2464}'),
    ("a125", '\u{2465}'),
    ("a126", '\u{2466}'),
    ("a127", '\u{2467}'),
    ("a128", '\u{2468}'),
    ("a129", '\u{2469}'),
    ("a13", '\u{270C}'),
    ("a130", '\u{2776}'),
    ("a131", '\u{2777}'),
    ("a132", '\u{2778}'),
    ("a133", '\u{2779}'),
    ("a134", '\u{277A}'),
    ("a135", '\u{277B}'),
    ("a136", '\u{277C}'),
    ("a137", '\u{277D}'),
    ("a138", '\u{277E}'),
    ("a139", '\u{277F}'),
    ("a14", '\u{270D}'),
    ("a140", '\u{2780}'),
    ("a141", '\u{2781}'),
    ("a142", '\u{2782}'),
    ("a143", '\u{2783}'),
    ("a144", '\u{2784}'),
    ("a145", '\u{2785}'),
    ("a146", '\u{2786}'),
    ("a147", '\u{2787}'),
    ("a148", '\u{2788}'),
    ("a149", '\u{2789}'),
    ("a15", '\u{270E}'),
    ("a150", '\u{278A}'),
    ("a151", '\u{278B}'),
    ("a152", '\u{278C}'),
    ("a153", '\u{278D}'),
    ("a154", '\u{278E}'),
    ("a155", '\u{278F}'),
    ("a156", '\u{2790}'),
    ("a157", '\u{2791}'),
    ("a158", '\u{2792}'),
    ("a159", '\u{2793}'),
    ("a16", '\u{270F}'),
    ("a160", '\u{2794}'),
    ("a161", '\u{2192}'),
    ("a162", '\u{27A3}'),
    ("a163", '\u{2194}'),
    ("a164", '\u{2195}'),
    ("a165", '\u{2799}'),
    ("a166", '\u{279B}'),
    ("a167", '\u{279C}'),
    ("a168", '\u{279D}'),
    ("a169", '\u{279E}'),
    ("a17", '\u{2711}'),
    ("a170", '\u{279F}'),
    ("a171", '\u{27A0}'),
    ("a172", '\u{27A1}'),
    ("a173", '\u{27A2}'),
    ("a174", '\u{27A4}'),
    ("a175", '\u{27A5}'),
    ("a176", '\u{27A6}'),
    ("a177", '\u{27A7}'),
    ("a178", '\u{27A8}'),
    ("a179", '\u{27A9}'),
    ("a18", '\u{2712}'),
    ("a180", '\u{27AB}'),
    ("a181", '\u{27AD}'),
    ("a182", '\u{27AF}'),
    ("a183", '\u{27B2}'),
    ("a184", '\u{27B3}'),
    ("a185", '\u{27B5}'),
    ("a186", '\u{27B8}'),
    ("a187", '\u{27BA}'),
    ("a188", '\u{27BB}'),
    ("a189", '\u{27BC}'),
    ("a19", '\u{2713}'),
    ("a190", '\u{27BD}'),
    ("a191", '\u{27BE}'),
    ("a192", '\u{279A}'),
    ("a193", '\u{27AA}'),
    ("a194", '\u{27B6}'),
    ("a195", '\u{27B9}'),
    ("a196", '\u{2798}'),
    ("a197", '\u{27B4}'),
    ("a198", '\u{27B7}'),
    ("a199", '\u{27AC}'),
    ("a2", '\u{2702}'),
    ("a20", '\u{2714}'),
    ("a200", '\u{27AE}'),
    ("a201", '\u{27B1}'),
    ("a202", '\u{2703}'),
    ("a203", '\u{2750}'),
    ("a204", '\u{2752}'),
    ("a205", '\u{276E}'),
    ("a206", '\u{2770}'),
    ("a21", '\u{2715}'),
    ("a22", '\u{2716}'),
    ("a23", '\u{2717}'),
    ("a24", '\u{2718}'),
    ("a25", '\u{2719}'),
    ("a26", '\u{271A}'),
    ("a27", '\u{271B}'),
    ("a28", '\u{271C}'),
    ("a29", '\u{2722}'),
    ("a3", '\u{2704}'),
    ("a30", '\u{2723}'),
    ("a31", '\u{2724}'),
    ("a32", '\u{2725}'),
    ("a33", '\u{2726}'),
    ("a34", '\u{2727}'),
    ("a35", '\u{2605}'),
    ("a36", '\u{2729}'),
    ("a37", '\u{272A}'),
    ("a38", '\u{272B}'),
    ("a39", '\u{272C}'),
    ("a4", '\u{260E}'),
    ("a40", '\u{272D}'),
    ("a41", '\u{272E}'),
    ("a42", '\u{272F}'),
    ("a43", '\u{2730}'),
    ("a44", '\u{2731}'),
    ("a45", '\u{2732}'),
    ("a46", '\u{2733}'),
    ("a47", '\u{2734}'),
    ("a48", '\u{2735}'),
    ("a49", '\u{2736}'),
    ("a5", '\u{2706}'),
    ("a50", '\u{2737}'),
    ("a51", '\u{2738}'),
    ("a52", '\u{2739}'),
    ("a53", '\u{273A}'),
    ("a54", '\u{273B}'),
    ("a55", '\u{273C}'),
    ("a56", '\u{273D}'),
    ("a57", '\u{273E}'),
    ("a58", '\u{273F}'),
    ("a59", '\u{2740}'),
    ("a6", '\u{271D}'),
    ("a60", '\u{2741}'),
    ("a61", '\u{2742}'),
    ("a62", '\u{2743}'),
    ("a63", '\u{2744}'),
    ("a64", '\u{2745}'),
    ("a65", '\u{2746}'),
    ("a66", '\u{2747}'),
    ("a67", '\u{2748}'),
    ("a68", '\u{2749}'),
    ("a69", '\u{274A}'),
    ("a7", '\u{271E}'),
    ("a70", '\u{274B}'),
    ("a71", '\u{25CF}'),
    ("a72", '\u{274D}'),
    ("a73", '\u{25A0}'),
    ("a74", '\u{274F}'),
    ("a75", '\u{2751}'),
    ("a76", '\u{25B2}'),
    ("a77", '\u{25BC}'),
    ("a78", '\u{25C6}'),
    ("a79", '\u{2756}'),
    ("a8", '\u{271F}'),
    ("a81", '\u{25D7}'),
    ("a82", '\u{2758}'),
    ("a83", '\u{2759}'),
    ("a84", '\u{275A}'),
    ("a85", '\u{276F}'),
    ("a86", '\u{2771}'),
    ("a87", '\u{2772}'),
    ("a88", '\u{2773}'),
    ("a89", '\u{2768}'),
    ("a9", '\u{2720}'),
    ("a90", '\u{2769}'),
    ("a91", '\u{276C}'),
    ("a92", '\u{276D}'),
    ("a93", '\u{276A}'),
    ("a94", '\u{276B}'),
    ("a95", '\u{2774}'),
    ("a96", '\u{2775}'),
    ("a97", '\u{275B}'),
    ("a98", '\u{275C}'),
    ("a99", '\u{275D}'),
];

/// Symbol encoding, ISO 32000-1 Annex D.5. Codes 0–31, 127–159, 240 and 255
/// are undefined.
const SYMBOL_ENCODING: &[(u8, &str)] = &[
    (32, "space"),
    (33, "exclam"),
    (34, "universal"),
    (35, "numbersign"),
    (36, "existential"),
    (37, "percent"),
    (38, "ampersand"),
    (39, "suchthat"),
    (40, "parenleft"),
    (41, "parenright"),
    (42, "asteriskmath"),
    (43, "plus"),
    (44, "comma"),
    (45, "minus"),
    (46, "period"),
    (47, "slash"),
    (48, "zero"),
    (49, "one"),
    (50, "two"),
    (51, "three"),
    (52, "four"),
    (53, "five"),
    (54, "six"),
    (55, "seven"),
    (56, "eight"),
    (57, "nine"),
    (58, "colon"),
    (59, "semicolon"),
    (60, "less"),
    (61, "equal"),
    (62, "greater"),
    (63, "question"),
    (64, "congruent"),
    (65, "Alpha"),
    (66, "Beta"),
    (67, "Chi"),
    (68, "Delta"),
    (69, "Epsilon"),
    (70, "Phi"),
    (71, "Gamma"),
    (72, "Eta"),
    (73, "Iota"),
    (74, "theta1"),
    (75, "Kappa"),
    (76, "Lambda"),
    (77, "Mu"),
    (78, "Nu"),
    (79, "Omicron"),
    (80, "Pi"),
    (81, "Theta"),
    (82, "Rho"),
    (83, "Sigma"),
    (84, "Tau"),
    (85, "Upsilon"),
    (86, "sigma1"),
    (87, "Omega"),
    (88, "Xi"),
    (89, "Psi"),
    (90, "Zeta"),
    (91, "bracketleft"),
    (92, "therefore"),
    (93, "bracketright"),
    (94, "perpendicular"),
    (95, "underscore"),
    (96, "radicalex"),
    (97, "alpha"),
    (98, "beta"),
    (99, "chi"),
    (100, "delta"),
    (101, "epsilon"),
    (102, "phi"),
    (103, "gamma"),
    (104, "eta"),
    (105, "iota"),
    (106, "phi1"),
    (107, "kappa"),
    (108, "lambda"),
    (109, "mu"),
    (110, "nu"),
    (111, "omicron"),
    (112, "pi"),
    (113, "theta"),
    (114, "rho"),
    (115, "sigma"),
    (116, "tau"),
    (117, "upsilon"),
    (118, "omega1"),
    (119, "omega"),
    (120, "xi"),
    (121, "psi"),
    (122, "zeta"),
    (123, "braceleft"),
    (124, "bar"),
    (125, "braceright"),
    (126, "similar"),
    (160, "Euro"),
    (161, "Upsilon1"),
    (162, "minute"),
    (163, "lessequal"),
    (164, "fraction"),
    (165, "infinity"),
    (166, "florin"),
    (167, "club"),
    (168, "diamond"),
    (169, "heart"),
    (170, "spade"),
    (171, "arrowboth"),
    (172, "arrowleft"),
    (173, "arrowup"),
    (174, "arrowright"),
    (175, "arrowdown"),
    (176, "degree"),
    (177, "plusminus"),
    (178, "second"),
    (179, "greaterequal"),
    (180, "multiply"),
    (181, "proportional"),
    (182, "partialdiff"),
    (183, "bullet"),
    (184, "divide"),
    (185, "notequal"),
    (186, "equivalence"),
    (187, "approxequal"),
    (188, "ellipsis"),
    (189, "arrowvertex"),
    (190, "arrowhorizex"),
    (191, "carriagereturn"),
    (192, "aleph"),
    (193, "Ifraktur"),
    (194, "Rfraktur"),
    (195, "weierstrass"),
    (196, "circlemultiply"),
    (197, "circleplus"),
    (198, "emptyset"),
    (199, "intersection"),
    (200, "union"),
    (201, "propersuperset"),
    (202, "reflexsuperset"),
    (203, "notsubset"),
    (204, "propersubset"),
    (205, "reflexsubset"),
    (206, "element"),
    (207, "notelement"),
    (208, "angle"),
    (209, "gradient"),
    (210, "registerserif"),
    (211, "copyrightserif"),
    (212, "trademarkserif"),
    (213, "product"),
    (214, "radical"),
    (215, "dotmath"),
    (216, "logicalnot"),
    (217, "logicaland"),
    (218, "logicalor"),
    (219, "arrowdblboth"),
    (220, "arrowdblleft"),
    (221, "arrowdblup"),
    (222, "arrowdblright"),
    (223, "arrowdbldown"),
    (224, "lozenge"),
    (225, "angleleft"),
    (226, "registersans"),
    (227, "copyrightsans"),
    (228, "trademarksans"),
    (229, "summation"),
    (230, "parenlefttp"),
    (231, "parenleftex"),
    (232, "parenleftbt"),
    (233, "bracketlefttp"),
    (234, "bracketleftex"),
    (235, "bracketleftbt"),
    (236, "bracelefttp"),
    (237, "braceleftmid"),
    (238, "braceleftbt"),
    (239, "braceex"),
    (241, "angleright"),
    (242, "integral"),
    (243, "integraltp"),
    (244, "integralex"),
    (245, "integralbt"),
    (246, "parenrighttp"),
    (247, "parenrightex"),
    (248, "parenrightbt"),
    (249, "bracketrighttp"),
    (250, "bracketrightex"),
    (251, "bracketrightbt"),
    (252, "bracerighttp"),
    (253, "bracerightmid"),
    (254, "bracerightbt"),
];

/// Zapf Dingbats encoding, ISO 32000-1 Annex D.6. Codes 0–31 and 232–255
/// are undefined.
const ZAPF_DINGBATS_ENCODING: &[(u8, &str)] = &[
    (32, "space"),
    (33, "a1"),
    (34, "a2"),
    (35, "a202"),
    (36, "a3"),
    (37, "a4"),
    (38, "a5"),
    (39, "a119"),
    (40, "a118"),
    (41, "a117"),
    (42, "a11"),
    (43, "a12"),
    (44, "a13"),
    (45, "a14"),
    (46, "a15"),
    (47, "a16"),
    (48, "a105"),
    (49, "a17"),
    (50, "a18"),
    (51, "a19"),
    (52, "a20"),
    (53, "a21"),
    (54, "a22"),
    (55, "a23"),
    (56, "a24"),
    (57, "a25"),
    (58, "a26"),
    (59, "a27"),
    (60, "a28"),
    (61, "a6"),
    (62, "a7"),
    (63, "a8"),
    (64, "a9"),
    (65, "a10"),
    (66, "a29"),
    (67, "a30"),
    (68, "a31"),
    (69, "a32"),
    (70, "a33"),
    (71, "a34"),
    (72, "a35"),
    (73, "a36"),
    (74, "a37"),
    (75, "a38"),
    (76, "a39"),
    (77, "a40"),
    (78, "a41"),
    (79, "a42"),
    (80, "a43"),
    (81, "a44"),
    (82, "a45"),
    (83, "a46"),
    (84, "a47"),
    (85, "a48"),
    (86, "a49"),
    (87, "a50"),
    (88, "a51"),
    (89, "a52"),
    (90, "a53"),
    (91, "a54"),
    (92, "a55"),
    (93, "a56"),
    (94, "a57"),
    (95, "a58"),
    (96, "a59"),
    (97, "a60"),
    (98, "a61"),
    (99, "a62"),
    (100, "a63"),
    (101, "a64"),
    (102, "a65"),
    (103, "a66"),
    (104, "a67"),
    (105, "a68"),
    (106, "a69"),
    (107, "a70"),
    (108, "a71"),
    (109, "a72"),
    (110, "a73"),
    (111, "a74"),
    (112, "a203"),
    (113, "a75"),
    (114, "a204"),
    (115, "a76"),
    (116, "a77"),
    (117, "a78"),
    (118, "a79"),
    (119, "a81"),
    (120, "a82"),
    (121, "a83"),
    (122, "a84"),
    (123, "a85"),
    (124, "a86"),
    (125, "a87"),
    (126, "a88"),
    (127, "a89"),
    (128, "a90"),
    (129, "a91"),
    (130, "a92"),
    (131, "a93"),
    (132, "a94"),
    (133, "a95"),
    (134, "a96"),
    (135, "a97"),
    (136, "a98"),
    (137, "a99"),
    (138, "a100"),
    (139, "a101"),
    (140, "a102"),
    (141, "a103"),
    (142, "a104"),
    (143, "a106"),
    (144, "a107"),
    (145, "a108"),
    (146, "a109"),
    (147, "a110"),
    (148, "a111"),
    (149, "a112"),
    (150, "a120"),
    (151, "a121"),
    (152, "a122"),
    (153, "a123"),
    (154, "a124"),
    (155, "a125"),
    (156, "a126"),
    (157, "a127"),
    (158, "a128"),
    (159, "a129"),
    (160, "a130"),
    (161, "a131"),
    (162, "a132"),
    (163, "a133"),
    (164, "a134"),
    (165, "a135"),
    (166, "a136"),
    (167, "a137"),
    (168, "a138"),
    (169, "a139"),
    (170, "a140"),
    (171, "a141"),
    (172, "a142"),
    (173, "a143"),
    (174, "a144"),
    (175, "a145"),
    (176, "a146"),
    (177, "a147"),
    (178, "a148"),
    (179, "a149"),
    (180, "a150"),
    (181, "a151"),
    (182, "a152"),
    (183, "a153"),
    (184, "a154"),
    (185, "a155"),
    (186, "a156"),
    (187, "a157"),
    (188, "a158"),
    (189, "a159"),
    (190, "a160"),
    (191, "a161"),
    (192, "a163"),
    (193, "a164"),
    (194, "a196"),
    (195, "a165"),
    (196, "a192"),
    (197, "a166"),
    (198, "a167"),
    (199, "a168"),
    (200, "a169"),
    (201, "a170"),
    (202, "a171"),
    (203, "a172"),
    (204, "a173"),
    (205, "a162"),
    (206, "a174"),
    (207, "a175"),
    (208, "a176"),
    (209, "a177"),
    (210, "a178"),
    (211, "a179"),
    (212, "a193"),
    (213, "a180"),
    (214, "a199"),
    (215, "a181"),
    (216, "a200"),
    (217, "a182"),
    (218, "a201"),
    (219, "a183"),
    (220, "a184"),
    (221, "a197"),
    (222, "a185"),
    (223, "a194"),
    (224, "a198"),
    (225, "a186"),
    (226, "a195"),
    (227, "a187"),
    (228, "a188"),
    (229, "a189"),
    (230, "a190"),
    (231, "a191"),
];

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

    /// The tables are searched by binary search, and every name the two
    /// encodings use has to be in them: a code whose name does not resolve
    /// gets no `/ToUnicode` entry at all, so a gap here is text that vanishes.
    #[test]
    fn every_encoded_glyph_name_has_a_meaning() {
        for (font, table) in [
            (StandardFont::Symbol, SYMBOL_GLYPH_UNICODE),
            (StandardFont::ZapfDingbats, ZAPF_DINGBATS_GLYPH_UNICODE),
        ] {
            assert!(
                table.windows(2).all(|w| w[0].0 < w[1].0),
                "{:?}'s table is not sorted by name; binary search would miss entries",
                font.postscript_name()
            );
            for (code, name) in font.encoding_table().unwrap() {
                assert!(
                    font.glyph_name_to_char(name).is_some(),
                    "{}: code {code} draws `{name}`, which resolves to nothing",
                    font.postscript_name()
                );
            }
        }
    }

    /// Zapf Dingbats names its glyphs the way a Latin font names variants.
    /// Answering for `a1` outside that face would turn a producer's `a`
    /// variant into a pair of scissors.
    #[test]
    fn dingbat_names_mean_nothing_on_another_face() {
        assert_eq!(
            StandardFont::ZapfDingbats.glyph_name_to_char("a12"),
            Some('\u{261E}')
        );
        assert_eq!(StandardFont::Symbol.glyph_name_to_char("a12"), None);
        assert_eq!(StandardFont::Helvetica.glyph_name_to_char("a1"), None);
        assert_eq!(symbol_glyph_name_to_char("a1"), None);
    }

    /// Symbol names belong to no other face, so they resolve without one.
    #[test]
    fn symbol_names_resolve_anywhere() {
        assert_eq!(symbol_glyph_name_to_char("arrowright"), Some('\u{2192}'));
        assert_eq!(symbol_glyph_name_to_char("approxequal"), Some('\u{2248}'));
        assert_eq!(symbol_glyph_name_to_char("pi"), Some('\u{03C0}'));
        // Reached only through /Differences: not in Annex D.5.
        assert_eq!(symbol_glyph_name_to_char("apple"), Some('\u{F8FF}'));
        assert_eq!(symbol_glyph_name_to_char("notaglyph"), None);
    }

    #[test]
    fn symbolic_faces_are_flagged() {
        assert!(StandardFont::Symbol.is_symbolic());
        assert!(StandardFont::ZapfDingbats.is_symbolic());
        assert!(!StandardFont::Helvetica.is_symbolic());
    }

    #[test]
    fn encoding_tables_match_annex_d() {
        let symbol = StandardFont::Symbol.encoding_table().unwrap();
        let dingbats = StandardFont::ZapfDingbats.encoding_table().unwrap();

        fn lookup(table: &[(u8, &'static str)], code: u8) -> Option<&'static str> {
            table.iter().find(|(c, _)| *c == code).map(|(_, n)| *n)
        }

        // Spot-checked against ISO 32000-1 Annex D.5 and D.6.
        assert_eq!(lookup(symbol, 34), Some("universal"));
        assert_eq!(lookup(symbol, 97), Some("alpha"));
        assert_eq!(lookup(symbol, 65), Some("Alpha"));
        assert_eq!(lookup(symbol, 108), Some("lambda"));
        assert_eq!(lookup(symbol, 229), Some("summation"));
        assert_eq!(lookup(symbol, 240), None); // undefined in Annex D.5

        assert_eq!(lookup(dingbats, 33), Some("a1"));
        assert_eq!(lookup(dingbats, 108), Some("a71"));
        assert_eq!(lookup(dingbats, 143), Some("a106")); // a113 does not exist
        assert_eq!(lookup(dingbats, 231), Some("a191"));
        assert_eq!(lookup(dingbats, 232), None); // undefined in Annex D.6

        // The twelve text faces have no built-in table.
        assert!(StandardFont::TimesRoman.encoding_table().is_none());

        // Tables are sorted and duplicate-free, or a Differences array built
        // from them would be malformed.
        for table in [symbol, dingbats] {
            for pair in table.windows(2) {
                assert!(pair[0].0 < pair[1].0, "table not strictly ascending");
            }
        }
    }

    #[test]
    fn encoding_tables_only_name_glyphs_the_program_has() {
        // A /Differences entry pointing at a glyph the embedded program does
        // not contain is its own validation failure (§6.2.11.4.1). Every name
        // in the Annex D tables must resolve in the bundled CFF's charset.
        for font in [StandardFont::Symbol, StandardFont::ZapfDingbats] {
            let cff = cff_parser::Table::parse(font.data()).expect("bundled CFF parses");
            for (code, name) in font.encoding_table().unwrap() {
                assert!(
                    cff.glyph_index_by_name(name).is_some(),
                    "{}: table maps code {code} to /{name}, absent from the bundled program",
                    font.postscript_name()
                );
            }
        }
    }
}
