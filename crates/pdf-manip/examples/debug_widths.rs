//! Debug width computation for a specific font object
//! Usage: cargo run -p pdf-manip --example debug_widths -- <pdf> <font_obj_id>

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <pdf> <font_obj_id>", args[0]);
        std::process::exit(1);
    }

    let data = std::fs::read(&args[1]).expect("read");
    let doc = lopdf::Document::load_mem(&data).expect("load");
    let target_id: u32 = args[2].parse().unwrap_or(0);

    let Some(lopdf::Object::Dictionary(dict)) = doc.objects.get(&(target_id, 0)) else {
        eprintln!("Object ({},0) not found or not a dict", target_id);
        return;
    };

    let base_font = dict
        .get(b"BaseFont")
        .ok()
        .and_then(|n| n.as_name().ok())
        .map(|n| String::from_utf8_lossy(n).to_string())
        .unwrap_or_default();
    let subtype = dict
        .get(b"Subtype")
        .ok()
        .and_then(|n| n.as_name().ok())
        .map(|n| String::from_utf8_lossy(n).to_string())
        .unwrap_or_default();
    println!(
        "Font ({},0): BaseFont={} Subtype={}",
        target_id, base_font, subtype
    );

    // Get FontDescriptor
    let fd_id = match dict.get(b"FontDescriptor").ok() {
        Some(lopdf::Object::Reference(r)) => *r,
        _ => {
            eprintln!("No FontDescriptor");
            return;
        }
    };

    // Get font file
    let Some(lopdf::Object::Dictionary(fd)) = doc.objects.get(&fd_id) else {
        eprintln!("FontDescriptor not found");
        return;
    };

    let ff_key: &[u8] = if fd.has(b"FontFile2") {
        b"FontFile2"
    } else if fd.has(b"FontFile3") {
        b"FontFile3"
    } else if fd.has(b"FontFile") {
        b"FontFile"
    } else {
        eprintln!("No FontFile");
        return;
    };
    let ff_ref = match fd.get(ff_key).ok() {
        Some(lopdf::Object::Reference(r)) => *r,
        _ => {
            eprintln!("FontFile not a ref");
            return;
        }
    };
    let font_data = match doc.get_object(ff_ref) {
        Ok(lopdf::Object::Stream(s)) => s
            .decompressed_content()
            .unwrap_or_else(|_| s.content.clone()),
        _ => {
            eprintln!("Can't read font stream");
            return;
        }
    };
    println!(
        "FontFile data: {} bytes (key={})",
        font_data.len(),
        String::from_utf8_lossy(ff_key)
    );
    if font_data.len() >= 4 {
        println!(
            "Magic: {:02x} {:02x} {:02x} {:02x}",
            font_data[0], font_data[1], font_data[2], font_data[3]
        );
    }

    // Parse as TrueType
    if let Ok(face) = ttf_parser::Face::parse(&font_data, 0) {
        let upem = face.units_per_em();
        let scale = 1000.0 / upem as f64;
        println!("unitsPerEm={} scale={}", upem, scale);
        println!("numGlyphs={}", face.number_of_glyphs());

        // Show cmap subtables
        if let Some(cmap) = face.tables().cmap.as_ref() {
            println!("cmap subtables:");
            for st in cmap.subtables {
                println!(
                    "  platform={:?} encoding={} format={:?}",
                    st.platform_id, st.encoding_id, st.format
                );
            }
        }

        // Get encoding info
        let enc_name = dict
            .get(b"Encoding")
            .ok()
            .and_then(|e| match e {
                lopdf::Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
                _ => None,
            })
            .unwrap_or_default();
        println!("Encoding: {:?}", enc_name);

        // Get FirstChar, Widths
        let first_char = dict
            .get(b"FirstChar")
            .ok()
            .and_then(|o| {
                if let lopdf::Object::Integer(i) = o {
                    Some(*i as u32)
                } else {
                    None
                }
            })
            .unwrap_or(0);
        let widths = match dict.get(b"Widths").ok() {
            Some(lopdf::Object::Array(arr)) => arr.clone(),
            Some(lopdf::Object::Reference(r)) => match doc.get_object(*r) {
                Ok(lopdf::Object::Array(arr)) => arr.clone(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        };

        // For specific test codes: 65, 100, 107, 109, 110, 119
        let test_codes = [65u32, 84, 100, 107, 109, 110, 119, 70];
        for &code in &test_codes {
            if code < first_char {
                continue;
            }
            let idx = (code - first_char) as usize;
            if idx >= widths.len() {
                continue;
            }

            let pdf_w = match &widths[idx] {
                lopdf::Object::Integer(w) => *w as f64,
                lopdf::Object::Real(r) => *r as f64,
                _ => -1.0,
            };

            // Map code to Unicode via encoding
            let ch = encoding_to_char(code, &enc_name);
            let gid_via_unicode = face.glyph_index(ch);
            let width_via_unicode =
                gid_via_unicode.and_then(|g| face.glyph_hor_advance(g).map(|w| w as f64 * scale));

            // Direct GID lookup (code as GID)
            let gid_direct = ttf_parser::GlyphId(code as u16);
            let width_direct = face.glyph_hor_advance(gid_direct).map(|w| w as f64 * scale);

            // Mac cmap lookup
            let mac_gid = if let Some(cmap) = face.tables().cmap.as_ref() {
                let mut result = None;
                for st in cmap.subtables {
                    if st.platform_id == ttf_parser::PlatformId::Macintosh && st.encoding_id == 0 {
                        result = st.glyph_index(code);
                        break;
                    }
                }
                result
            } else {
                None
            };
            let width_mac = mac_gid.and_then(|g| {
                if g.0 != 0 {
                    face.glyph_hor_advance(g).map(|w| w as f64 * scale)
                } else {
                    None
                }
            });

            println!("  code={} char={:?} pdf_w={:.0}", code, ch, pdf_w);
            println!(
                "    unicode_gid={:?} width={:?}",
                gid_via_unicode, width_via_unicode
            );
            println!("    direct_gid={} width={:?}", gid_direct.0, width_direct);
            println!("    mac_gid={:?} width={:?}", mac_gid, width_mac);
        }
    }
}

fn encoding_to_char(code: u32, enc_name: &str) -> char {
    match enc_name {
        "WinAnsiEncoding" => {
            // CP-1252 mapping
            match code {
                0x80 => '\u{20AC}',
                0x82 => '\u{201A}',
                0x83 => '\u{0192}',
                0x84 => '\u{201E}',
                0x85 => '\u{2026}',
                0x86 => '\u{2020}',
                0x87 => '\u{2021}',
                0x88 => '\u{02C6}',
                0x89 => '\u{2030}',
                0x8A => '\u{0160}',
                0x8B => '\u{2039}',
                0x8C => '\u{0152}',
                0x8E => '\u{017D}',
                0x91 => '\u{2018}',
                0x92 => '\u{2019}',
                0x93 => '\u{201C}',
                0x94 => '\u{201D}',
                0x95 => '\u{2022}',
                0x96 => '\u{2013}',
                0x97 => '\u{2014}',
                0x98 => '\u{02DC}',
                0x99 => '\u{2122}',
                0x9A => '\u{0161}',
                0x9B => '\u{203A}',
                0x9C => '\u{0153}',
                0x9E => '\u{017E}',
                0x9F => '\u{0178}',
                _ => char::from_u32(code).unwrap_or('\u{FFFD}'),
            }
        }
        "MacRomanEncoding" => char::from_u32(code).unwrap_or('\u{FFFD}'),
        _ => char::from_u32(code).unwrap_or('\u{FFFD}'),
    }
}
