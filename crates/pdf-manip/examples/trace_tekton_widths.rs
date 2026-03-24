//! Trace width computations for HHCOAA+Tekton-Bold codes 236-239.
fn main() {
    let data = std::fs::read("/tmp/fail2.pdf").expect("read");
    let doc = lopdf::Document::load_mem(&data).expect("parse");

    for (id, obj) in &doc.objects {
        if let lopdf::Object::Dictionary(dict) = obj {
            let base = dict
                .get(b"BaseFont")
                .ok()
                .and_then(|o| {
                    if let lopdf::Object::Name(n) = o {
                        Some(String::from_utf8_lossy(n).to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            if !base.contains("Tekton-Bold") {
                continue;
            }
            if !matches!(dict.get(b"Subtype").ok(), Some(lopdf::Object::Name(n)) if n == b"Type1") {
                continue;
            }

            let fc = match dict.get(b"FirstChar").ok() {
                Some(lopdf::Object::Integer(i)) => *i as u32,
                _ => continue,
            };

            let lc = match dict.get(b"LastChar").ok() {
                Some(lopdf::Object::Integer(i)) => *i as u32,
                _ => continue,
            };

            eprintln!("Font: {} (obj {:?}), fc={}, lc={}", base, id, fc, lc);

            // Get Widths
            let widths_obj = dict.get(b"Widths").ok();
            let widths: Vec<i64> = match widths_obj {
                Some(lopdf::Object::Array(arr)) => arr
                    .iter()
                    .map(|o| match o {
                        lopdf::Object::Integer(i) => *i,
                        lopdf::Object::Real(f) => *f as i64,
                        _ => 0,
                    })
                    .collect(),
                _ => vec![],
            };

            for code in 234u32..=242u32 {
                let idx = (code - fc) as usize;
                let dict_w = if idx < widths.len() { widths[idx] } else { -1 };
                eprintln!("  Code {}: dict_width={}", code, dict_w);
            }

            // Check FontDescriptor
            if let Some(lopdf::Object::Reference(fd_ref)) = dict.get(b"FontDescriptor").ok() {
                if let Ok(lopdf::Object::Dictionary(fd)) = doc.get_object(*fd_ref) {
                    let flags = fd
                        .get(b"Flags")
                        .ok()
                        .and_then(|o| {
                            if let lopdf::Object::Integer(i) = o {
                                Some(*i)
                            } else {
                                None
                            }
                        })
                        .unwrap_or(0);
                    let has_ff3 = fd.has(b"FontFile3");
                    eprintln!(
                        "  Flags={flags} (bit18={}), has_ff3={}",
                        flags & 262144 != 0,
                        has_ff3
                    );

                    // Read font file
                    if let Some(lopdf::Object::Reference(ff_ref)) = fd.get(b"FontFile3").ok() {
                        if let Ok(lopdf::Object::Stream(stream)) = doc.get_object(*ff_ref) {
                            if let Ok(font_data) = stream.decompressed_content() {
                                eprintln!("  FontFile3 size: {} bytes", font_data.len());
                                let cff_bytes =
                                    pdf_manip::pdfa_fonts::extract_cff_bytes_from_otf(&font_data)
                                        .unwrap_or(&font_data);
                                if let Some(cff) = cff_parser::Table::parse(cff_bytes) {
                                    let scale =
                                        pdf_manip::pdfa_fonts::cff_matrix_scale(cff.matrix().sx);
                                    eprintln!(
                                        "  CFF scale={}, defaultWidthX={:?}",
                                        scale,
                                        cff.default_width_x()
                                    );
                                    // CFF encoding
                                    let enc_map =
                                        pdf_manip::pdfa_fonts::parse_cff_encoding_map(&font_data);
                                    for code in 234u32..=242u32 {
                                        let gid = enc_map.get(&(code as u8)).copied();
                                        let gid_via = cff.glyph_index(code as u8).map(|g| g.0);
                                        let w = gid.and_then(|g| {
                                            if g != 0 {
                                                cff.glyph_width(cff_parser::GlyphId(g))
                                                    .map(|w| (w as f64 * scale).round() as i64)
                                            } else {
                                                None
                                            }
                                        });
                                        eprintln!("  CFF enc[{}]: custom_gid={:?}, glyph_index={:?}, width={:?}", code, gid, gid_via, w);
                                    }
                                    // Print SIDs for GIDs 82-90 (igrave/icircumflex/idieresis range)
                                    eprintln!("  --- SID dump for GIDs 82-90 ---");
                                    for gid_raw in 82u16..=90 {
                                        let gid = cff_parser::GlyphId(gid_raw);
                                        let name = cff.glyph_name(gid);
                                        let sid = cff.charset.gid_to_sid(gid).map(|s| s.0);
                                        let is_std = sid.map_or(false, |s| (s as usize) < cff_parser::STANDARD_NAMES.len());
                                        eprintln!("  GID {}: name={:?} SID={:?} is_standard={}", gid_raw, name, sid, is_std);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
