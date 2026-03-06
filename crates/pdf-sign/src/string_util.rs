//! PDF string to Rust string conversion utilities.

/// Convert a PDF string (possibly UTF-16BE with BOM) to a Rust `String`.
pub fn pdf_string_to_string(s: &pdf_syntax::object::String) -> String {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let utf16: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&utf16)
    } else if bytes.len() >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF {
        String::from_utf8_lossy(&bytes[3..]).into_owned()
    } else {
        let mut s = String::with_capacity(bytes.len());
        for &b in bytes {
            s.push(pdfdoc_byte_to_char(b));
        }
        s
    }
}

/// Map a single PDFDocEncoding byte to a Unicode char.
fn pdfdoc_byte_to_char(b: u8) -> char {
    #[rustfmt::skip]
    static HIGH: [char; 46] = [
        '\u{2022}', '\u{2020}', '\u{2021}', '\u{2026}',
        '\u{2014}', '\u{2013}', '\u{0192}', '\u{2044}',
        '\u{2039}', '\u{203A}', '\u{2212}', '\u{2030}',
        '\u{201E}', '\u{201C}', '\u{201D}', '\u{2018}',
        '\u{2019}', '\u{201A}', '\u{2122}', '\u{FB01}',
        '\u{FB02}', '\u{0141}', '\u{0152}', '\u{0160}',
        '\u{0178}', '\u{017D}', '\u{0131}', '\u{0142}',
        '\u{0153}', '\u{0161}', '\u{017E}', '\u{FFFD}',
        '\u{20AC}', '\u{00A1}', '\u{00A2}', '\u{00A3}',
        '\u{00A4}', '\u{00A5}', '\u{00A6}', '\u{00A7}',
        '\u{00A8}', '\u{00A9}', '\u{00AA}', '\u{00AB}',
        '\u{00AC}', '\u{00AD}',
    ];
    match b {
        0x00..=0x7F => b as char,
        0x80..=0xAD => HIGH[(b - 0x80) as usize],
        0xAE..=0xFF => char::from(b),
    }
}
