//! Utilities for PDF font encoding: Adobe Glyph List lookup and
//! Differences-based code→Unicode mapping.
//!
//! Used by text_run.rs to build reverse encoding maps for fonts that
//! have no ToUnicode CMap but do have a standard or Differences-based Encoding.

use lopdf::{Document, Object};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Adobe Glyph List lookup
// ---------------------------------------------------------------------------

/// Map an Adobe glyph name to its Unicode character.
///
/// Covers the most common names (AGL, Latin-1 extensions). Returns `None` for
/// unknown names — callers should fall back to omitting the mapping.
pub(crate) fn glyph_name_to_char(name: &str) -> Option<char> {
    // "uniXXXX" format.
    if let Some(hex) = name.strip_prefix("uni") {
        if hex.len() == 4 {
            if let Ok(cp) = u32::from_str_radix(hex, 16) {
                return char::from_u32(cp);
            }
        }
    }

    // Single ASCII character names (A-Z, a-z, zero-nine etc.).
    if name.len() == 1 {
        return name.chars().next();
    }

    // Common AGL names.
    match name {
        "space" => Some(' '),
        "exclam" => Some('!'),
        "quotedbl" => Some('"'),
        "numbersign" => Some('#'),
        "dollar" => Some('$'),
        "percent" => Some('%'),
        "ampersand" => Some('&'),
        "quotesingle" => Some('\''),
        "parenleft" => Some('('),
        "parenright" => Some(')'),
        "asterisk" => Some('*'),
        "plus" => Some('+'),
        "comma" => Some(','),
        "hyphen" => Some('-'),
        "minus" => Some('\u{2212}'),
        "period" => Some('.'),
        "slash" => Some('/'),
        "zero" => Some('0'),
        "one" => Some('1'),
        "two" => Some('2'),
        "three" => Some('3'),
        "four" => Some('4'),
        "five" => Some('5'),
        "six" => Some('6'),
        "seven" => Some('7'),
        "eight" => Some('8'),
        "nine" => Some('9'),
        "colon" => Some(':'),
        "semicolon" => Some(';'),
        "less" => Some('<'),
        "equal" => Some('='),
        "greater" => Some('>'),
        "question" => Some('?'),
        "at" => Some('@'),
        "bracketleft" => Some('['),
        "backslash" => Some('\\'),
        "bracketright" => Some(']'),
        "asciicircum" => Some('^'),
        "underscore" => Some('_'),
        "grave" => Some('`'),
        "braceleft" => Some('{'),
        "bar" => Some('|'),
        "braceright" => Some('}'),
        "asciitilde" => Some('~'),
        "bullet" => Some('\u{2022}'),
        "ellipsis" => Some('\u{2026}'),
        "emdash" => Some('\u{2014}'),
        "endash" => Some('\u{2013}'),
        "fi" => Some('\u{FB01}'),
        "fl" => Some('\u{FB02}'),
        "ff" => Some('\u{FB00}'),
        "ffi" => Some('\u{FB03}'),
        "ffl" => Some('\u{FB04}'),
        "quotedblleft" => Some('\u{201C}'),
        "quotedblright" => Some('\u{201D}'),
        "quoteleft" => Some('\u{2018}'),
        "quoteright" => Some('\u{2019}'),
        "quotesinglbase" => Some('\u{201A}'),
        "quotedblbase" => Some('\u{201E}'),
        "dagger" => Some('\u{2020}'),
        "daggerdbl" => Some('\u{2021}'),
        "trademark" => Some('\u{2122}'),
        "copyright" => Some('\u{00A9}'),
        "registered" => Some('\u{00AE}'),
        "degree" => Some('\u{00B0}'),
        "Euro" => Some('\u{20AC}'),
        "sterling" => Some('\u{00A3}'),
        "yen" => Some('\u{00A5}'),
        "cent" => Some('\u{00A2}'),
        "section" => Some('\u{00A7}'),
        "paragraph" => Some('\u{00B6}'),
        "germandbls" => Some('\u{00DF}'),
        "onesuperior" => Some('\u{00B9}'),
        "twosuperior" => Some('\u{00B2}'),
        "threesuperior" => Some('\u{00B3}'),
        "onequarter" => Some('\u{00BC}'),
        "onehalf" => Some('\u{00BD}'),
        "threequarters" => Some('\u{00BE}'),
        "ordfeminine" => Some('\u{00AA}'),
        "ordmasculine" => Some('\u{00BA}'),
        "exclamdown" => Some('\u{00A1}'),
        "questiondown" => Some('\u{00BF}'),
        "guillemotleft" => Some('\u{00AB}'),
        "guillemotright" => Some('\u{00BB}'),
        "guilsinglleft" => Some('\u{2039}'),
        "guilsinglright" => Some('\u{203A}'),
        "periodcentered" => Some('\u{00B7}'),
        "multiply" => Some('\u{00D7}'),
        "divide" => Some('\u{00F7}'),
        "plusminus" => Some('\u{00B1}'),
        "logicalnot" => Some('\u{00AC}'),
        "mu" => Some('\u{00B5}'),
        "macron" => Some('\u{00AF}'),
        "cedilla" => Some('\u{00B8}'),
        "dieresis" => Some('\u{00A8}'),
        "acute" => Some('\u{00B4}'),
        "circumflex" => Some('\u{02C6}'),
        "tilde" => Some('\u{02DC}'),
        "ring" => Some('\u{02DA}'),
        "breve" => Some('\u{02D8}'),
        "dotaccent" => Some('\u{02D9}'),
        "hungarumlaut" => Some('\u{02DD}'),
        "ogonek" => Some('\u{02DB}'),
        "caron" => Some('\u{02C7}'),
        "Adieresis" => Some('\u{00C4}'),
        "Odieresis" => Some('\u{00D6}'),
        "Udieresis" => Some('\u{00DC}'),
        "adieresis" => Some('\u{00E4}'),
        "odieresis" => Some('\u{00F6}'),
        "Aacute" => Some('\u{00C1}'),
        "Agrave" => Some('\u{00C0}'),
        "Acircumflex" => Some('\u{00C2}'),
        "Atilde" => Some('\u{00C3}'),
        "Aring" => Some('\u{00C5}'),
        "AE" => Some('\u{00C6}'),
        "Ccedilla" => Some('\u{00C7}'),
        "Eacute" => Some('\u{00C9}'),
        "Egrave" => Some('\u{00C8}'),
        "Ecircumflex" => Some('\u{00CA}'),
        "Edieresis" => Some('\u{00CB}'),
        "Iacute" => Some('\u{00CD}'),
        "Igrave" => Some('\u{00CC}'),
        "Icircumflex" => Some('\u{00CE}'),
        "Idieresis" => Some('\u{00CF}'),
        "Ntilde" => Some('\u{00D1}'),
        "Oacute" => Some('\u{00D3}'),
        "Ograve" => Some('\u{00D2}'),
        "Ocircumflex" => Some('\u{00D4}'),
        "Otilde" => Some('\u{00D5}'),
        "Oslash" => Some('\u{00D8}'),
        "Scaron" => Some('\u{0160}'),
        "Uacute" => Some('\u{00DA}'),
        "Ugrave" => Some('\u{00D9}'),
        "Ucircumflex" => Some('\u{00DB}'),
        "Yacute" => Some('\u{00DD}'),
        "Thorn" => Some('\u{00DE}'),
        "aacute" => Some('\u{00E1}'),
        "agrave" => Some('\u{00E0}'),
        "acircumflex" => Some('\u{00E2}'),
        "atilde" => Some('\u{00E3}'),
        "aring" => Some('\u{00E5}'),
        "ae" => Some('\u{00E6}'),
        "ccedilla" => Some('\u{00E7}'),
        "eacute" => Some('\u{00E9}'),
        "egrave" => Some('\u{00E8}'),
        "ecircumflex" => Some('\u{00EA}'),
        "edieresis" => Some('\u{00EB}'),
        "iacute" => Some('\u{00ED}'),
        "igrave" => Some('\u{00EC}'),
        "icircumflex" => Some('\u{00EE}'),
        "idieresis" => Some('\u{00EF}'),
        "ntilde" => Some('\u{00F1}'),
        "oacute" => Some('\u{00F3}'),
        "ograve" => Some('\u{00F2}'),
        "ocircumflex" => Some('\u{00F4}'),
        "otilde" => Some('\u{00F5}'),
        "oslash" => Some('\u{00F8}'),
        "scaron" => Some('\u{0161}'),
        "uacute" => Some('\u{00FA}'),
        "ugrave" => Some('\u{00F9}'),
        "ucircumflex" => Some('\u{00FB}'),
        "udieresis" => Some('\u{00FC}'), // lowercase u with dieresis
        "yacute" => Some('\u{00FD}'),
        "thorn" => Some('\u{00FE}'),
        "ydieresis" => Some('\u{00FF}'),
        "OE" => Some('\u{0152}'),
        "oe" => Some('\u{0153}'),
        "Zcaron" => Some('\u{017D}'),
        "zcaron" => Some('\u{017E}'),
        "florin" => Some('\u{0192}'),
        "dotlessi" => Some('\u{0131}'),
        "Lslash" => Some('\u{0141}'),
        "lslash" => Some('\u{0142}'),
        "notequal" => Some('\u{2260}'),
        "infinity" => Some('\u{221E}'),
        "integral" => Some('\u{222B}'),
        "summation" => Some('\u{2211}'),
        "product" => Some('\u{220F}'),
        "pi" => Some('\u{03C0}'),
        "Omega" => Some('\u{03A9}'),
        "Delta" => Some('\u{2206}'),
        "radical" => Some('\u{221A}'),
        "lozenge" => Some('\u{25CA}'),
        "fraction" => Some('\u{2044}'),
        "currency" => Some('\u{00A4}'),
        "perthousand" => Some('\u{2030}'),
        // TeX special chars (OT1 encoding names)
        "compwordmark" => None, // TeX \| — invisible
        "visiblespace" => Some(' '),
        "dotlessj" => Some('\u{0237}'),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Standard encoding tables
// ---------------------------------------------------------------------------

/// Map a byte code to a Unicode char using the named standard PDF encoding.
///
/// Falls back to Latin-1 for codes 0-127 when the encoding is unknown.
pub(crate) fn standard_encoding_char(code: u8, enc_name: &str) -> Option<char> {
    match enc_name {
        "WinAnsiEncoding" => winansi_char(code),
        "MacRomanEncoding" => macroman_char(code),
        "StandardEncoding" => standard_char(code),
        "MacExpertEncoding" => None, // Skip — obscure, rarely used for body text
        _ => {
            // Unknown encoding: use Latin-1 for printable ASCII, nothing for the rest.
            if (0x20..=0x7E).contains(&code) {
                char::from_u32(code as u32)
            } else {
                None
            }
        }
    }
}

fn winansi_char(code: u8) -> Option<char> {
    if code < 0x20 || code == 0x7F {
        return None;
    }
    const WIN_128_159: [u32; 32] = [
        0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160,
        0x2039, 0x0152, 0x008D, 0x017D, 0x008F, 0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022,
        0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x009D, 0x017E, 0x0178,
    ];
    if code < 128 {
        char::from_u32(code as u32)
    } else if code < 160 {
        char::from_u32(WIN_128_159[(code - 128) as usize])
    } else {
        char::from_u32(code as u32) // Latin-1 supplement
    }
}

fn macroman_char(code: u8) -> Option<char> {
    if code < 0x20 {
        return None;
    }
    if code < 128 {
        return char::from_u32(code as u32);
    }
    const MAC_128_255: [u32; 128] = [
        0x00C4, 0x00C5, 0x00C7, 0x00C9, 0x00D1, 0x00D6, 0x00DC, 0x00E1, 0x00E0, 0x00E2, 0x00E4,
        0x00E3, 0x00E5, 0x00E7, 0x00E9, 0x00E8, 0x00EA, 0x00EB, 0x00ED, 0x00EC, 0x00EE, 0x00EF,
        0x00F1, 0x00F3, 0x00F2, 0x00F4, 0x00F6, 0x00F5, 0x00FA, 0x00F9, 0x00FB, 0x00FC, 0x2020,
        0x00B0, 0x00A2, 0x00A3, 0x00A7, 0x2022, 0x00B6, 0x00DF, 0x00AE, 0x00A9, 0x2122, 0x00B4,
        0x00A8, 0x2260, 0x00C6, 0x00D8, 0x221E, 0x00B1, 0x2264, 0x2265, 0x00A5, 0x00B5, 0x2202,
        0x2211, 0x220F, 0x03C0, 0x222B, 0x00AA, 0x00BA, 0x03A9, 0x00E6, 0x00F8, 0x00BF, 0x00A1,
        0x00AC, 0x221A, 0x0192, 0x2248, 0x2206, 0x00AB, 0x00BB, 0x2026, 0x00A0, 0x00C0, 0x00C3,
        0x00D5, 0x0152, 0x0153, 0x2013, 0x2014, 0x201C, 0x201D, 0x2018, 0x2019, 0x00F7, 0x25CA,
        0x00FF, 0x0178, 0x2044, 0x20AC, 0x2039, 0x203A, 0xFB01, 0xFB02, 0x2021, 0x00B7, 0x201A,
        0x201E, 0x2030, 0x00C2, 0x00CA, 0x00C1, 0x00CB, 0x00C8, 0x00CD, 0x00CE, 0x00CF, 0x00CC,
        0x00D3, 0x00D4, 0xF8FF, 0x00D2, 0x00DA, 0x00DB, 0x00D9, 0x0131, 0x02C6, 0x02DC, 0x00AF,
        0x02D8, 0x02D9, 0x02DA, 0x00B8, 0x02DD, 0x02DB, 0x02C7,
    ];
    char::from_u32(MAC_128_255[(code - 128) as usize])
}

/// Adobe Standard Encoding — code → glyph name → char.
///
/// Only covers the commonly used codes. Returns None for uncommon ones.
fn standard_char(code: u8) -> Option<char> {
    // Standard encoding covers codes 0x21-0x7E with standard ASCII glyph names.
    // Most match Latin-1. Exceptions in range:
    match code {
        0x27 => Some('\u{2019}'), // quoteright (not apostrophe)
        0x60 => Some('\u{2018}'), // quoteleft (not grave)
        0x7C => None,             // /bar not in Standard Encoding
        0x7E => None,             // /asciitilde not in Standard Encoding (some versions)
        c if (0x21..=0x7E).contains(&c) => char::from_u32(c as u32),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Differences-based encoding builder
// ---------------------------------------------------------------------------

/// Build a code→Unicode mapping from a font's Encoding entry.
///
/// Returns an empty map if the font has no Encoding or uses a format we
/// cannot decode (e.g. only a bare encoding name without Differences). An
/// empty map signals "fall back to Latin-1" to the caller.
///
/// When the Encoding is a dictionary (with optional `BaseEncoding` and/or
/// `Differences`), the returned map is populated with the full code→Unicode
/// table and any Differences overrides applied on top.
pub(crate) fn build_font_encoding(
    doc: &Document,
    font_dict: &lopdf::Dictionary,
) -> HashMap<u8, char> {
    let enc_obj = match font_dict.get(b"Encoding") {
        Ok(o) => o.clone(),
        Err(_) => return HashMap::new(),
    };

    // Resolve reference.
    let enc_resolved = match &enc_obj {
        Object::Reference(id) => match doc.get_object(*id) {
            Ok(obj) => obj.clone(),
            Err(_) => return HashMap::new(),
        },
        other => other.clone(),
    };

    match enc_resolved {
        Object::Name(ref name_bytes) => {
            // Just a named encoding — no Differences applied.
            // Build the full standard table.
            let enc_name = String::from_utf8_lossy(name_bytes);
            build_standard_table(&enc_name)
        }
        Object::Dictionary(ref enc_dict) => {
            // Optional BaseEncoding.
            let base_enc = enc_dict
                .get(b"BaseEncoding")
                .ok()
                .and_then(|o| match o {
                    Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
                    _ => None,
                })
                .unwrap_or_default();

            let mut map = if base_enc.is_empty() {
                // No BaseEncoding specified: use StandardEncoding for Type1 or
                // a minimal Latin-1-like table as fallback.
                build_standard_table("StandardEncoding")
            } else {
                build_standard_table(&base_enc)
            };

            // Apply Differences.
            if let Ok(Object::Array(ref diffs)) = enc_dict.get(b"Differences") {
                apply_differences(&mut map, diffs);
            }

            map
        }
        _ => HashMap::new(),
    }
}

/// Build the full code→char table for a named standard PDF encoding.
fn build_standard_table(enc_name: &str) -> HashMap<u8, char> {
    let mut map = HashMap::with_capacity(224);
    for code in 0u8..=255 {
        if let Some(ch) = standard_encoding_char(code, enc_name) {
            map.insert(code, ch);
        }
    }
    map
}

/// Apply `/Differences [ N /glyphname /glyphname ... M /glyphname ... ]` to a map.
fn apply_differences(map: &mut HashMap<u8, char>, diffs: &[Object]) {
    let mut code: u8 = 0;
    for item in diffs {
        match item {
            Object::Integer(n) => {
                code = (*n).clamp(0, 255) as u8;
            }
            Object::Name(name_bytes) => {
                let name = String::from_utf8_lossy(name_bytes);
                // Map glyph name → char and store (or remove if unknown).
                match glyph_name_to_char(&name) {
                    Some(ch) => {
                        map.insert(code, ch);
                    }
                    None => {
                        // Unknown glyph — remove any existing mapping so the
                        // reverse map doesn't map that char to this code.
                        map.remove(&code);
                    }
                }
                code = code.wrapping_add(1);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_name_underscore() {
        assert_eq!(glyph_name_to_char("underscore"), Some('_'));
    }

    #[test]
    fn glyph_name_single_char() {
        assert_eq!(glyph_name_to_char("A"), Some('A'));
        assert_eq!(glyph_name_to_char("z"), Some('z'));
    }

    #[test]
    fn winansi_standard_ascii() {
        // 0x5F = underscore in WinAnsi
        assert_eq!(winansi_char(0x5F), Some('_'));
        assert_eq!(winansi_char(0x41), Some('A'));
    }

    #[test]
    fn standard_encoding_fallback() {
        // Unknown encoding uses Latin-1 for printable ASCII.
        assert_eq!(standard_encoding_char(0x5F, "UnknownEncoding"), Some('_'));
        assert_eq!(standard_encoding_char(0x41, "UnknownEncoding"), Some('A'));
    }
}
