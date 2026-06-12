//! Text encoding for AcroForm values and appearance streams.
//!
//! Two distinct encoding domains live here — they are NOT interchangeable:
//!
//! 1. **`/V` text strings** (ISO 32000-1 §7.9.2.2): either PDFDocEncoding
//!    bytes or UTF-16BE with a leading `FE FF` BOM. The writer policy follows
//!    mupdf/pdf.js: pure-ASCII values become literal strings, anything else
//!    becomes UTF-16BE+BOM for the *whole* string (mixing is not allowed).
//!    Delegated to [`lopdf::text_string`], which implements exactly this.
//!
//! 2. **Appearance-stream show-text bytes**: the bytes inside `(…) Tj` are
//!    decoded through the *font's* encoding. The Standard-14 fonts in form
//!    `/DR` dictionaries conventionally use WinAnsiEncoding, so values must be
//!    re-encoded Unicode → WinAnsi before they land in the content stream.
//!    Writing raw UTF-8 there is the classic "Café → CafÃ©" mojibake.
//!
//! Values that cannot be fully represented in WinAnsi (e.g. Cyrillic, CJK)
//! have no appearance-stream fallback font in 1.x: callers must skip
//! appearance generation and set `/NeedAppearances true` instead, the same
//! degradation pdf.js uses. `/V` itself is always lossless via UTF-16BE.

/// Decode PDF text-string bytes (ISO 32000-1 §7.9.2.2): UTF-16BE with BOM,
/// UTF-8 with BOM (PDF 2.0), else PDFDocEncoding.
///
/// Reuses [`lopdf::decode_text_string`] so the read side cannot drift from
/// the write side's policy. Falls back to lossy UTF-8 on malformed input
/// rather than erroring — model reads should never fail on a bad byte.
pub(crate) fn decode_pdf_text_bytes(bytes: &[u8]) -> String {
    let obj = lopdf::Object::String(bytes.to_vec(), lopdf::StringFormat::Literal);
    lopdf::decode_text_string(&obj).unwrap_or_else(|_| String::from_utf8_lossy(bytes).into_owned())
}

/// Decode PDF /Name bytes to a display string.
///
/// Names are not text strings — the spec leaves their interpretation to the
/// producer. Real forms carry UTF-8 names and Latin-1 names (Dutch tax forms
/// use raw `0xF6` for `ö`); try UTF-8 first, else decode as Latin-1, which is
/// lossless and round-trips through the writeback's WinAnsi candidate
/// matching.
pub(crate) fn decode_name_bytes(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

/// Encode a Unicode string to WinAnsiEncoding (Windows-1252 superset used by
/// the Standard-14 fonts).
///
/// Returns `None` if any character has no WinAnsi code point — callers must
/// then fall back to `/NeedAppearances` instead of generating an appearance.
///
/// Coverage: ASCII passthrough, Latin-1 range `0xA0..=0xFF` passthrough, and
/// the full Windows-1252 `0x80..=0x9F` block (€ ‚ ƒ „ … † ‡ ˆ ‰ Š ‹ Œ Ž
/// ' ' " " • – — ˜ ™ š › œ ž Ÿ).
pub(crate) fn encode_winansi(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len());
    for ch in text.chars() {
        out.push(winansi_byte(ch)?);
    }
    Some(out)
}

/// Map a single Unicode scalar to its WinAnsi byte, or `None`.
fn winansi_byte(ch: char) -> Option<u8> {
    let cp = ch as u32;
    match cp {
        // ASCII (incl. control chars callers may pass through for multiline
        // handling — \r/\n never reach Tj, they are split into lines first).
        0x00..=0x7F => Some(cp as u8),
        // Latin-1 supplement maps 1:1 in WinAnsi.
        0xA0..=0xFF => Some(cp as u8),
        // Windows-1252 0x80–0x9F block.
        0x20AC => Some(0x80), // €
        0x201A => Some(0x82), // ‚
        0x0192 => Some(0x83), // ƒ
        0x201E => Some(0x84), // „
        0x2026 => Some(0x85), // …
        0x2020 => Some(0x86), // †
        0x2021 => Some(0x87), // ‡
        0x02C6 => Some(0x88), // ˆ
        0x2030 => Some(0x89), // ‰
        0x0160 => Some(0x8A), // Š
        0x2039 => Some(0x8B), // ‹
        0x0152 => Some(0x8C), // Œ
        0x017D => Some(0x8E), // Ž
        0x2018 => Some(0x91), // '
        0x2019 => Some(0x92), // '
        0x201C => Some(0x93), // "
        0x201D => Some(0x94), // "
        0x2022 => Some(0x95), // •
        0x2013 => Some(0x96), // –
        0x2014 => Some(0x97), // —
        0x02DC => Some(0x98), // ˜
        0x2122 => Some(0x99), // ™
        0x0161 => Some(0x9A), // š
        0x203A => Some(0x9B), // ›
        0x0153 => Some(0x9C), // œ
        0x017E => Some(0x9E), // ž
        0x0178 => Some(0x9F), // Ÿ
        _ => None,
    }
}

/// Escape WinAnsi-encoded bytes for a PDF literal string inside a content
/// stream (`(…) Tj`).
///
/// Operates on *bytes*, never on chars: escaping after encoding keeps
/// multi-byte mistakes structurally impossible. Balanced parentheses do not
/// strictly need escaping per spec, but escaping all three metacharacters is
/// what every reference implementation emits.
pub(crate) fn escape_string_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 4);
    for &b in bytes {
        match b {
            b'(' | b')' | b'\\' => {
                out.push(b'\\');
                out.push(b);
            }
            b'\r' => out.extend_from_slice(b"\\r"),
            b'\n' => out.extend_from_slice(b"\\n"),
            _ => out.push(b),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_passthrough() {
        assert_eq!(encode_winansi("Hello").unwrap(), b"Hello");
    }

    #[test]
    fn latin1_and_euro() {
        // Café Zürich € ñ — the exact reference-form values.
        assert_eq!(
            encode_winansi("Café Zürich € ñ").unwrap(),
            vec![
                b'C', b'a', b'f', 0xE9, b' ', b'Z', 0xFC, b'r', b'i', b'c', b'h', b' ', 0x80, b' ',
                0xF1
            ]
        );
    }

    #[test]
    fn windows_1252_block() {
        assert_eq!(encode_winansi("\u{2019}").unwrap(), vec![0x92]); // '
        assert_eq!(encode_winansi("\u{2013}").unwrap(), vec![0x96]); // –
        assert_eq!(encode_winansi("\u{0153}").unwrap(), vec![0x9C]); // œ
    }

    #[test]
    fn unmappable_returns_none() {
        assert!(encode_winansi("Привет").is_none()); // Cyrillic
        assert!(encode_winansi("日本語").is_none()); // CJK
        assert!(encode_winansi("a\u{0101}b").is_none()); // ā (Latin Ext-A, not in 1252)
    }

    #[test]
    fn escape_metachars_after_encoding() {
        assert_eq!(escape_string_bytes(b"a(b)c\\"), b"a\\(b\\)c\\\\".to_vec());
        // 0xE9 (é in WinAnsi) passes through unescaped.
        assert_eq!(escape_string_bytes(&[0xE9]), vec![0xE9]);
    }
}
