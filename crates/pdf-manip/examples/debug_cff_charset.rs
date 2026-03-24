//! Dump CFF charset format and CIDs for CIDFontType0 fonts.
use lopdf::{Document, Object};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: debug_cff_charset <pdf>");
    let data = std::fs::read(&path).expect("read");
    let doc = Document::load_mem(&data).expect("parse");

    let mut font_ids: Vec<_> = doc.objects.keys().copied().collect();
    font_ids.sort();

    for font_id in font_ids {
        let obj = &doc.objects[&font_id];
        let dict = match obj {
            Object::Dictionary(d) => d,
            _ => continue,
        };
        let _subtype = match dict.get(b"Subtype").ok() {
            Some(Object::Name(n)) if n == b"CIDFontType0" => "CIDFontType0",
            _ => continue,
        };
        let base_font = match dict.get(b"BaseFont").ok() {
            Some(Object::Name(n)) => String::from_utf8_lossy(n).to_string(),
            _ => "?".to_string(),
        };
        let fd_id = match dict.get(b"FontDescriptor").ok() {
            Some(Object::Reference(r)) => *r,
            _ => continue,
        };
        let fd = match doc.objects.get(&fd_id) {
            Some(Object::Dictionary(d)) => d,
            _ => continue,
        };
        let ff3_id = match fd.get(b"FontFile3").ok() {
            Some(Object::Reference(r)) => *r,
            _ => continue,
        };
        let cff_data = match doc.objects.get(&ff3_id) {
            Some(Object::Stream(s)) => {
                let mut s2 = s.clone();
                let _ = s2.decompress();
                s2.content.clone()
            }
            _ => continue,
        };

        // Manually parse the CFF charset format byte
        // CFF structure: header (4 bytes) → Name INDEX → Top DICT INDEX → ...
        // Top DICT has charset_offset which points into the CFF stream.
        // Charset byte 0: 0=Format0, 1=Format1, 2=Format2
        if cff_data.len() < 4 {
            println!("{base_font}: CFF too small");
            continue;
        }
        let _hdr_size = cff_data[2] as usize;
        // After header is Name INDEX
        // Skip Name INDEX to find Top DICT INDEX
        // Skip Top DICT INDEX to find charset
        // Parse via cff_parser
        let table = match cff_parser::Table::parse(&cff_data) {
            Some(t) => t,
            None => {
                println!("{base_font}: CFF parse FAILED");
                continue;
            }
        };
        let n = table.number_of_glyphs();
        let mut cids: Vec<u16> = Vec::new();
        let mut has_none = false;
        for i in 0..n {
            match table.glyph_cid(cff_parser::GlyphId(i)) {
                Some(cid) => cids.push(cid),
                None => has_none = true,
            }
        }

        // Find charset format by scanning CFF structure manually
        let charset_format = find_charset_format(&cff_data);
        println!(
            "{base_font}: n_glyphs={n} charset_format={charset_format:?} cids={cids:?} has_none={has_none}"
        );
        if cids.len() != n as usize {
            println!(
                "  WARNING: only {} CIDs returned for {} glyphs!",
                cids.len(),
                n
            );
        }
    }
}

fn find_charset_format(data: &[u8]) -> Option<u8> {
    if data.len() < 4 {
        return None;
    }
    let hdr_size = data[2] as usize;
    // Skip Name INDEX
    let name_idx_start = hdr_size;
    let name_idx_end = skip_cff_index(data, name_idx_start)?;
    // Skip Top DICT INDEX header to get Top DICT data
    let (td_start, td_end) = get_first_cff_index_item(data, name_idx_end)?;
    // Scan Top DICT for charset offset (operator 15)
    let td = &data[td_start..td_end];
    let mut i = 0;
    let mut last_int: Option<usize> = None;
    while i < td.len() {
        let b = td[i];
        match b {
            15 => {
                // CharSet operator
                let offset = last_int?;
                if offset >= 3 && offset < data.len() {
                    return Some(data[offset]);
                }
                return None;
            }
            17 => break, // CharStrings (stop before)
            12 => {
                // Two-byte operator
                i += 2;
                last_int = None;
            }
            28 => {
                if i + 2 >= td.len() {
                    break;
                }
                last_int = Some(i16::from_be_bytes([td[i + 1], td[i + 2]]) as usize);
                i += 3;
            }
            29 => {
                if i + 4 >= td.len() {
                    break;
                }
                last_int =
                    Some(u32::from_be_bytes([td[i + 1], td[i + 2], td[i + 3], td[i + 4]]) as usize);
                i += 5;
            }
            30 => {
                // BCD real — skip
                i += 1;
                while i < td.len() && (td[i] & 0x0f) != 0x0f {
                    i += 1;
                }
                i += 1;
                last_int = None;
            }
            0..=27 | 31 => {
                // Other operators, skip
                last_int = None;
                i += 1;
            }
            32..=246 => {
                last_int = Some((b as usize).wrapping_sub(139));
                i += 1;
            }
            247..=250 => {
                if i + 1 >= td.len() {
                    break;
                }
                last_int = Some(((b as usize - 247) * 256 + td[i + 1] as usize) + 108);
                i += 2;
            }
            251..=254 => {
                if i + 1 >= td.len() {
                    break;
                }
                last_int = Some(
                    (256usize.wrapping_sub(b as usize))
                        .wrapping_mul(256)
                        .wrapping_sub(td[i + 1] as usize)
                        .wrapping_sub(108),
                );
                i += 2;
            }
            _ => {
                i += 1;
                last_int = None;
            }
        }
    }
    None
}

fn skip_cff_index(data: &[u8], start: usize) -> Option<usize> {
    if start + 2 > data.len() {
        return None;
    }
    let count = u16::from_be_bytes([data[start], data[start + 1]]) as usize;
    if count == 0 {
        return Some(start + 2);
    }
    let off_size = *data.get(start + 2)? as usize;
    if off_size < 1 || off_size > 4 {
        return None;
    }
    let last_off_pos = start + 3 + count * off_size;
    if last_off_pos + off_size > data.len() {
        return None;
    }
    let mut last_off = 0usize;
    for i in 0..off_size {
        last_off = (last_off << 8) | data[last_off_pos + i] as usize;
    }
    // Offsets are relative to the byte AFTER the offset array
    let data_start = start + 3 + (count + 1) * off_size;
    Some(data_start + last_off - 1)
}

fn get_first_cff_index_item(data: &[u8], start: usize) -> Option<(usize, usize)> {
    if start + 2 > data.len() {
        return None;
    }
    let count = u16::from_be_bytes([data[start], data[start + 1]]) as usize;
    if count == 0 {
        return None;
    }
    let off_size = *data.get(start + 2)? as usize;
    if off_size < 1 || off_size > 4 {
        return None;
    }
    // Read first offset (offset 0 = relative start)
    let off0_pos = start + 3;
    let mut off0 = 0usize;
    for i in 0..off_size {
        off0 = (off0 << 8) | data[off0_pos + i] as usize;
    }
    // Read second offset (offset 1 = relative end of first item)
    let off1_pos = start + 3 + off_size;
    let mut off1 = 0usize;
    for i in 0..off_size {
        off1 = (off1 << 8) | data[off1_pos + i] as usize;
    }
    let data_start = start + 3 + (count + 1) * off_size;
    Some((data_start + off0 - 1, data_start + off1 - 1))
}
