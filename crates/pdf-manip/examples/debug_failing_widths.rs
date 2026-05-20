/// Debug: trace width computations for specific failing §6.2.11.5 fonts.
/// Run: cargo run --example debug_failing_widths -p pdf-manip
fn main() {
    let cases: &[(&str, &str, &[u32])] = &[
        (
            "/tmp/fail2.pdf",
            "HGNBGB+Tekton",
            &[143, 152, 157, 237, 244],
        ),
        ("/tmp/fail2.pdf", "HHCOAA+Tekton-Bold", &[236, 238, 239]),
        (
            "/tmp/pdf-test-6.2.11.5/gen-181_181221.pdf",
            "MELEBE",
            &[227],
        ),
        (
            "/tmp/pdf-test-6.2.11.5/gen-529_529771.pdf",
            "BICKAF",
            &[237],
        ),
        (
            "/tmp/pdf-test-6.2.11.5/gen-857_857413.pdf",
            "DDEEDN",
            &[228],
        ),
        (
            "/tmp/pdf-test-6.2.11.5/gen-131_131159.pdf",
            "AvantGarde-Book",
            &[129],
        ),
        (
            "/tmp/pdf-test-6.2.11.5/gen-764_764288.pdf",
            "Helvetica-Condensed-Black",
            &[55, 50, 48],
        ),
        (
            "/tmp/pdf-test-6.2.11.5/gen-764_764288.pdf",
            "HelveticaNeue-Roman",
            &[70, 111],
        ),
    ];

    for (pdf_path, font_substr, codes) in cases {
        let data = match std::fs::read(pdf_path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("MISSING {pdf_path}: {e}");
                continue;
            }
        };
        let doc = match lopdf::Document::load_mem(&data) {
            Ok(d) if !d.objects.is_empty() => d,
            _ => {
                eprintln!("LOAD FAIL {pdf_path}");
                continue;
            }
        };

        println!("\n=== {pdf_path} / font contains: {font_substr} ===");

        for obj in doc.objects.values() {
            let lopdf::Object::Dictionary(dict) = obj else {
                continue;
            };
            let base = match dict.get(b"BaseFont").ok() {
                Some(lopdf::Object::Name(n)) => String::from_utf8_lossy(n).to_string(),
                _ => continue,
            };
            if !base.contains(font_substr) {
                continue;
            }
            let subtype = match dict.get(b"Subtype").ok() {
                Some(lopdf::Object::Name(n)) => String::from_utf8_lossy(n).to_string(),
                _ => continue,
            };
            if !matches!(subtype.as_str(), "Type1" | "TrueType" | "MMType1") {
                continue;
            }

            let fc = match dict.get(b"FirstChar").ok() {
                Some(lopdf::Object::Integer(i)) => *i as u32,
                _ => continue,
            };
            let widths: Vec<i64> = match dict.get(b"Widths").ok() {
                Some(lopdf::Object::Array(arr)) => arr
                    .iter()
                    .map(|o| match o {
                        lopdf::Object::Integer(i) => *i,
                        lopdf::Object::Real(r) => *r as i64,
                        _ => -1,
                    })
                    .collect(),
                Some(lopdf::Object::Reference(r)) => match doc.get_object(*r) {
                    Ok(lopdf::Object::Array(arr)) => arr
                        .iter()
                        .map(|o| match o {
                            lopdf::Object::Integer(i) => *i,
                            lopdf::Object::Real(r) => *r as i64,
                            _ => -1,
                        })
                        .collect(),
                    _ => vec![],
                },
                _ => continue,
            };

            println!(
                "  Font: {base} (subtype={subtype}) fc={fc} widths.len={}",
                widths.len()
            );

            // Print /Encoding info
            match dict.get(b"Encoding").ok() {
                Some(lopdf::Object::Name(n)) => {
                    println!("  /Encoding: {}", String::from_utf8_lossy(n));
                }
                Some(lopdf::Object::Reference(enc_ref)) => {
                    if let Some(lopdf::Object::Dictionary(enc_dict)) = doc.objects.get(enc_ref) {
                        print_encoding_dict(enc_dict, codes);
                    }
                }
                None => println!("  /Encoding: (none)"),
                Some(lopdf::Object::Dictionary(enc_dict)) => {
                    println!("  /Encoding: inline dict");
                    print_encoding_dict(enc_dict, codes);
                }
                _ => println!("  /Encoding: (other)"),
            }

            // Print /Widths for codes we care about
            for &code in *codes {
                if code >= fc && (code - fc) < widths.len() as u32 {
                    let w = widths[(code - fc) as usize];
                    println!("  /Widths[{code}] = {w}");
                } else {
                    println!(
                        "  /Widths[{code}] = (out of range, fc={fc}, len={})",
                        widths.len()
                    );
                }
            }

            // Get font file data
            let fd_ref = match dict.get(b"FontDescriptor").ok() {
                Some(lopdf::Object::Reference(r)) => *r,
                _ => {
                    println!("  (no FontDescriptor)");
                    break;
                }
            };
            let Some(lopdf::Object::Dictionary(fd)) = doc.objects.get(&fd_ref) else {
                println!("  (FontDescriptor not found)");
                break;
            };

            let (ff_type, ff_ref) = if let Ok(lopdf::Object::Reference(r)) = fd.get(b"FontFile3") {
                ("FF3", *r)
            } else if let Ok(lopdf::Object::Reference(r)) = fd.get(b"FontFile2") {
                ("FF2", *r)
            } else if let Ok(lopdf::Object::Reference(r)) = fd.get(b"FontFile") {
                ("FF1", *r)
            } else {
                println!("  (no font file)");
                break;
            };
            println!("  FontFile type: {ff_type}");

            let font_data = match doc.objects.get(&ff_ref) {
                Some(lopdf::Object::Stream(s)) => {
                    let mut s2 = s.clone();
                    let _ = s2.decompress();
                    s2.content.clone()
                }
                _ => {
                    println!("  (no font stream)");
                    break;
                }
            };
            println!("  FontFile bytes: {}", font_data.len());

            if ff_type == "FF3" {
                // Try to parse as CFF
                let cff_bytes = if font_data.starts_with(b"OTTO")
                    || font_data.len() > 12 && &font_data[0..4] == b"\x00\x01\x00\x00"
                {
                    // OTF/TTF wrapper — extract CFF table
                    extract_cff_bytes(&font_data)
                } else {
                    Some(font_data.as_slice())
                };

                if let Some(cff_bytes) = cff_bytes {
                    if let Some(cff) = cff_parser::Table::parse(cff_bytes) {
                        let matrix = cff.matrix();
                        let scale = (matrix.sx * 1_000_000.0).round() / 1_000_000.0 * 1000.0;
                        let default_w = cff.default_width_x();
                        println!(
                            "  CFF matrix.sx={} scale={:.4} num_glyphs={}",
                            matrix.sx,
                            scale,
                            cff.number_of_glyphs()
                        );
                        println!("  CFF defaultWidthX={default_w:?}");

                        // CFF encoding map
                        let enc_map = parse_cff_encoding(cff_bytes);
                        println!("  CFF encoding format: {} entries", enc_map.len());

                        // Also check parse_cff_encoding_map (the production function)
                        let prod_enc_map = pdf_manip::pdfa_fonts::parse_cff_encoding_map(cff_bytes);
                        println!("  prod_enc_map: {} entries", prod_enc_map.len());

                        for &code in *codes {
                            let prod_gid = prod_enc_map.get(&(code as u8)).copied();
                            if let Some(&gid) = enc_map.get(&(code as u8)) {
                                let glyph_name = cff.glyph_name(cff_parser::GlyphId(gid));
                                let w = cff.glyph_width(cff_parser::GlyphId(gid));
                                let w_scaled = w.map(|w| w as f64 * scale / 1000.0);
                                println!("  CFF enc[{code}] = GID {gid} ({glyph_name:?}) w={w:?} scaled={w_scaled:?}, prod_enc_map[{code}]={prod_gid:?}");
                            } else {
                                // Try glyph_index
                                let gid_gi = cff.glyph_index(code as u8);
                                println!("  CFF enc[{code}] = (not in enc_map), glyph_index({code})={gid_gi:?}, prod_enc_map[{code}]={prod_gid:?}");
                            }
                        }

                        // Print ALL charset entries
                        println!("  CFF charset (ALL GIDs):");
                        for gid in 0..cff.number_of_glyphs() {
                            let name = cff.glyph_name(cff_parser::GlyphId(gid));
                            let w = cff.glyph_width(cff_parser::GlyphId(gid));
                            let w_scaled = w.map(|w| (w as f64 * scale / 1000.0).round() as i64);
                            println!("    GID {gid}: {name:?} w={w:?} scaled={w_scaled:?}");
                        }

                        // Search for codes by name lookup
                        for &code in *codes {
                            // What glyph name would name lookup find?
                            let _target_name = format!("code_{code}_name_lookup");
                            for gid in 0..cff.number_of_glyphs() {
                                if let Some(_name) = cff.glyph_name(cff_parser::GlyphId(gid)) {
                                    let w = cff.glyph_width(cff_parser::GlyphId(gid));
                                    let _w_scaled =
                                        w.map(|w| (w as f64 * scale / 1000.0).round() as i64);
                                    // print glyphs with names that might be related
                                    let _code_str = code.to_string();
                                }
                            }
                        }
                    } else {
                        println!("  CFF parse failed");
                    }
                } else {
                    println!("  Could not extract CFF bytes");
                }
            }
            break; // Only first matching font
        }
    }
}

fn print_encoding_dict(enc_dict: &lopdf::Dictionary, codes: &[u32]) {
    let base_enc = match enc_dict.get(b"BaseEncoding").ok() {
        Some(lopdf::Object::Name(n)) => String::from_utf8_lossy(n).to_string(),
        _ => "(none)".to_string(),
    };
    println!("  /Encoding: BaseEncoding={base_enc}");
    if let Ok(lopdf::Object::Array(diffs)) = enc_dict.get(b"Differences") {
        let mut code = 0u32;
        for item in diffs {
            match item {
                lopdf::Object::Integer(c) => code = *c as u32,
                lopdf::Object::Name(n) => {
                    let name = String::from_utf8_lossy(n).to_string();
                    if codes.contains(&code) {
                        println!("    Differences: code {code} = {name}");
                    }
                    code += 1;
                }
                _ => {}
            }
        }
    }
}

fn extract_cff_bytes(data: &[u8]) -> Option<&[u8]> {
    // Parse OTF table directory
    if data.len() < 12 {
        return None;
    }
    let num_tables = u16::from_be_bytes([data[4], data[5]]) as usize;
    let table_dir_size = 12 + num_tables * 16;
    if data.len() < table_dir_size {
        return None;
    }
    for i in 0..num_tables {
        let offset = 12 + i * 16;
        let tag = &data[offset..offset + 4];
        let table_offset = u32::from_be_bytes([
            data[offset + 8],
            data[offset + 9],
            data[offset + 10],
            data[offset + 11],
        ]) as usize;
        let table_len = u32::from_be_bytes([
            data[offset + 12],
            data[offset + 13],
            data[offset + 14],
            data[offset + 15],
        ]) as usize;
        if tag == b"CFF " && table_offset + table_len <= data.len() {
            return Some(&data[table_offset..table_offset + table_len]);
        }
    }
    None
}

fn parse_cff_encoding(cff_bytes: &[u8]) -> std::collections::HashMap<u8, u16> {
    // Minimal CFF encoding parser
    // CFF header: version(1), hdrSize(1), offSize(1)
    let mut map = std::collections::HashMap::new();
    if cff_bytes.len() < 4 {
        return map;
    }
    let _hdr_size = cff_bytes[2] as usize;
    // Skip: header + Name INDEX + Top DICT INDEX to find Encoding offset
    // This is complex — use cff_parser's encoding instead
    // Use our cff_parser to get the encoding
    if let Some(cff) = cff_parser::Table::parse(cff_bytes) {
        for code in 0u8..=255 {
            if let Some(gid) = cff.encoding.code_to_gid(&cff.charset, code) {
                map.insert(code, gid.0);
            }
        }
    }
    map
}
