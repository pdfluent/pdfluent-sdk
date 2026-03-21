#![cfg(feature = "pdfa-convert")]

#[test]
#[ignore]
fn save_converted_for_debug() {
    use pdf_manip::pdfa_xmp::PdfAConformance;

    let data = std::fs::read("/tmp/gen-626.pdf").unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).unwrap();
    let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_pfb_font_streams(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_type1_stub_font_files(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_mislabeled_truetype_as_cff(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_cff_invalid_bcd(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_type1_nonstandard_charstrings(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_type1_eexec_space_prefix(&mut doc);
    pdf_manip::pdfa_fonts::fix_cff_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_truetype_cid_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_type1_charset(&mut doc);
    pdf_manip::pdfa_fonts::fix_truetype_encoding(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_existing_symbolic_truetype_cmaps(&mut doc);
    pdf_manip::pdfa_fonts::fix_truetype_unicode_cmap(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_type1_tounicode_from_encoding(&mut doc);
    pdf_manip::pdfa_fonts::fix_notdef_glyph_refs(&mut doc);
    pdf_manip::pdfa_fonts::fix_cid_font_notdef(&mut doc);
    pdf_manip::pdfa_fonts::fix_symbolic_font_notdef_streams(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_simple_font_out_of_range_codes(&mut doc);
    pdf_manip::pdfa_fonts::fix_undefined_encoding_codes(&mut doc);
    pdf_manip::pdfa_fonts::fix_symbolic_flags(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_classic_symbolic_base14_encoding(&mut doc);
    pdf_manip::pdfa_fonts::fix_missing_simple_font_widths(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_type3_font_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_font_width_mismatches(&mut doc);
    pdf_manip::pdfa_fonts::fix_symbolic_font_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_cidset(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_missing_cidtogidmap(&mut doc);
    pdf_manip::pdfa_colorspace::normalize_colorspaces(&mut doc).unwrap();
    pdf_manip::pdfa_fixups::run_fixups(&mut doc);
    pdf_manip::pdfa_xmp::repair_xmp_metadata(&mut doc, PdfAConformance::A2b, None).unwrap();

    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    pdf_manip::pdfa_cleanup::fix_pdf_header(&mut saved);
    pdf_manip::pdfa_cleanup::fix_startxref(&mut saved);

    std::fs::write("/tmp/gen-626-converted.pdf", &saved).unwrap();
    println!("Saved {} bytes to /tmp/gen-626-converted.pdf", saved.len());
}

/// Trace ABDHHO+Symbol widths through each pipeline step.
#[test]
#[ignore]
fn debug_gen626_symbol_widths_trace() {
    use lopdf::Object;
    use pdf_manip::pdfa_xmp::PdfAConformance;

    let data = std::fs::read("/tmp/gen-626.pdf").unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    fn get_symbol_widths(doc: &lopdf::Document) -> (Option<i64>, Option<i64>) {
        for (_id, obj) in &doc.objects {
            if let Object::Dictionary(dict) = obj {
                let base = dict
                    .get(b"BaseFont")
                    .ok()
                    .and_then(|o| {
                        if let Object::Name(n) = o {
                            Some(String::from_utf8_lossy(n).to_string())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default();
                if !base.contains("Symbol") {
                    continue;
                }
                let fc = dict
                    .get(b"FirstChar")
                    .ok()
                    .and_then(|o| {
                        if let Object::Integer(i) = o {
                            Some(*i as u32)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0);
                let widths: Vec<i64> = match dict.get(b"Widths").ok() {
                    Some(Object::Array(a)) => a
                        .iter()
                        .map(|o| match o {
                            Object::Integer(i) => *i,
                            Object::Real(r) => *r as i64,
                            _ => -1,
                        })
                        .collect(),
                    Some(Object::Reference(r)) => doc
                        .get_object(*r)
                        .ok()
                        .and_then(|o| {
                            if let Object::Array(a) = o {
                                Some(
                                    a.iter()
                                        .map(|o| match o {
                                            Object::Integer(i) => *i,
                                            Object::Real(r) => *r as i64,
                                            _ => -1,
                                        })
                                        .collect(),
                                )
                            } else {
                                None
                            }
                        })
                        .unwrap_or_default(),
                    _ => vec![],
                };
                let w1 = if fc <= 1 {
                    widths.get((1 - fc) as usize).copied()
                } else {
                    None
                };
                let w128 = if fc <= 128 {
                    widths.get((128 - fc) as usize).copied()
                } else {
                    None
                };
                return (w1, w128);
            }
        }
        (None, None)
    }

    macro_rules! check {
        ($label:expr) => {
            let (w1, w128) = get_symbol_widths(&doc);
            println!("{}: code1={:?} code128={:?}", $label, w1, w128);
        };
    }

    check!("original");
    pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).unwrap();
    check!("after cleanup");
    let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);
    check!("after embed_fonts");
    let _ = pdf_manip::pdfa_fonts::fix_mislabeled_truetype_as_cff(&mut doc);
    pdf_manip::pdfa_fonts::fix_cff_widths(&mut doc);
    check!("after fix_cff_widths");
    pdf_manip::pdfa_fonts::fix_type1_charset(&mut doc);
    pdf_manip::pdfa_fonts::fix_truetype_encoding(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_existing_symbolic_truetype_cmaps(&mut doc);
    pdf_manip::pdfa_fonts::fix_truetype_unicode_cmap(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_type1_tounicode_from_encoding(&mut doc);
    pdf_manip::pdfa_fonts::fix_notdef_glyph_refs(&mut doc);
    pdf_manip::pdfa_fonts::fix_cid_font_notdef(&mut doc);
    pdf_manip::pdfa_fonts::fix_symbolic_font_notdef_streams(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_simple_font_out_of_range_codes(&mut doc);
    check!("after fix_simple_font_out_of_range_codes");
    pdf_manip::pdfa_fonts::fix_undefined_encoding_codes(&mut doc);
    check!("after fix_undefined_encoding_codes");
    pdf_manip::pdfa_fonts::fix_symbolic_flags(&mut doc);
    check!("after fix_symbolic_flags");
    let _ = pdf_manip::pdfa_fonts::fix_classic_symbolic_base14_encoding(&mut doc);
    check!("after fix_classic_symbolic_base14_encoding");
    pdf_manip::pdfa_fonts::fix_missing_simple_font_widths(&mut doc);
    check!("after fix_missing_simple_font_widths");
    let _ = pdf_manip::pdfa_fonts::fix_type3_font_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_font_width_mismatches(&mut doc);
    check!("after fix_font_width_mismatches");
    pdf_manip::pdfa_fonts::fix_symbolic_font_widths(&mut doc);
    check!("after fix_symbolic_font_widths");
}

/// Show what corrections fix_symbolic_font_widths computes for ABDHHO+Symbol.
#[test]
#[ignore]
fn debug_gen626_symbol_cff_widths() {
    use lopdf::Object;

    let data = std::fs::read("/tmp/gen-626.pdf").unwrap();
    let doc = lopdf::Document::load_mem(&data).unwrap();

    // Find the ABDHHO+Symbol font
    for (id, obj) in &doc.objects {
        if let Object::Dictionary(dict) = obj {
            let base = dict
                .get(b"BaseFont")
                .ok()
                .and_then(|o| {
                    if let Object::Name(n) = o {
                        Some(String::from_utf8_lossy(n).to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            if !base.contains("Symbol") {
                continue;
            }

            println!("Font {id:?} BaseFont={base}");

            let fd_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(r)) => *r,
                _ => {
                    println!("No FontDescriptor");
                    continue;
                }
            };
            let fd = match doc.objects.get(&fd_id) {
                Some(Object::Dictionary(d)) => d,
                _ => {
                    println!("No FD dict");
                    continue;
                }
            };
            let has_ff = fd.has(b"FontFile");
            let has_ff2 = fd.has(b"FontFile2");
            let has_ff3 = fd.has(b"FontFile3");
            println!("FontFile={has_ff} FontFile2={has_ff2} FontFile3={has_ff3}");

            // Try to extract font data
            let ff_key = if has_ff {
                b"FontFile" as &[u8]
            } else if has_ff2 {
                b"FontFile2"
            } else if has_ff3 {
                b"FontFile3"
            } else {
                continue;
            };
            let font_data = match fd.get(ff_key).ok() {
                Some(Object::Reference(r)) => match doc.objects.get(r) {
                    Some(Object::Stream(s)) => {
                        let mut s2 = s.clone();
                        let _ = s2.decompress();
                        s2.content.clone()
                    }
                    _ => {
                        println!("No stream");
                        continue;
                    }
                },
                _ => {
                    println!("No ff reference");
                    continue;
                }
            };
            println!("Font data size: {} bytes", font_data.len());

            // Check if parseable as OTF
            match ttf_parser::Face::parse(&font_data, 0) {
                Ok(face) => println!("OTF parse: OK, upem={}", face.units_per_em()),
                Err(e) => println!("OTF parse: FAILED ({:?})", e),
            }

            // Check CFF parse
            match cff_parser::Table::parse(&font_data) {
                Some(cff) => {
                    let n = cff.number_of_glyphs();
                    println!("CFF parse: OK, glyphs={n}");
                    let scale = cff.matrix().sx as f64 * 1000.0;
                    // Print all glyph widths
                    for gid in 0..n {
                        let gid_obj = cff_parser::GlyphId(gid);
                        let w = cff
                            .glyph_width(gid_obj)
                            .map(|w| (w as f64 * scale).round() as i64);
                        let name = cff
                            .glyph_name(gid_obj)
                            .map(|n| n.to_string())
                            .unwrap_or_else(|| format!("?"));
                        println!("  GID {gid}: name={name} width={w:?}");
                    }
                    // Print encoding for codes 1 and 128
                    for code in [1u8, 2, 32, 65, 128, 129, 215] {
                        let gid = cff.glyph_index(code);
                        println!("  encoding: code {code} → GID {:?}", gid.map(|g| g.0));
                    }
                }
                None => println!("CFF parse: FAILED"),
            }
            // Print raw CFF bytes to understand encoding
            println!(
                "Raw CFF ({} bytes): {:02x?}",
                font_data.len(),
                &font_data[..font_data.len().min(100)]
            );
        }
    }
}

/// Dump font info for all simple fonts in gen-626 before + after conversion.
#[test]
#[ignore]
fn debug_gen626_font_widths() {
    use lopdf::Object;

    let data = std::fs::read("/tmp/gen-626.pdf").unwrap();
    let doc = lopdf::Document::load_mem(&data).unwrap();

    for (id, obj) in &doc.objects {
        if let Object::Dictionary(dict) = obj {
            let subtype = dict
                .get(b"Subtype")
                .ok()
                .and_then(|o| {
                    if let Object::Name(n) = o {
                        Some(String::from_utf8_lossy(n).to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            if !matches!(subtype.as_str(), "TrueType" | "Type1") {
                continue;
            }

            let base = dict
                .get(b"BaseFont")
                .ok()
                .and_then(|o| {
                    if let Object::Name(n) = o {
                        Some(String::from_utf8_lossy(n).to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();

            let fc = dict
                .get(b"FirstChar")
                .ok()
                .and_then(|o| {
                    if let Object::Integer(i) = o {
                        Some(*i)
                    } else {
                        None
                    }
                })
                .unwrap_or(-1);
            let lc = dict
                .get(b"LastChar")
                .ok()
                .and_then(|o| {
                    if let Object::Integer(i) = o {
                        Some(*i)
                    } else {
                        None
                    }
                })
                .unwrap_or(-1);

            let widths_len = match dict.get(b"Widths").ok() {
                Some(Object::Array(a)) => a.len(),
                Some(Object::Reference(r)) => doc
                    .get_object(*r)
                    .ok()
                    .and_then(|o| {
                        if let Object::Array(a) = o {
                            Some(a.len())
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0),
                _ => 0,
            };

            // Show widths for codes around 1 and 128
            let widths: Vec<i64> = match dict.get(b"Widths").ok() {
                Some(Object::Array(a)) => a
                    .iter()
                    .map(|o| match o {
                        Object::Integer(i) => *i,
                        Object::Real(r) => *r as i64,
                        _ => -1,
                    })
                    .collect(),
                Some(Object::Reference(r)) => doc
                    .get_object(*r)
                    .ok()
                    .and_then(|o| {
                        if let Object::Array(a) = o {
                            Some(
                                a.iter()
                                    .map(|o| match o {
                                        Object::Integer(i) => *i,
                                        Object::Real(r) => *r as i64,
                                        _ => -1,
                                    })
                                    .collect(),
                            )
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default(),
                _ => vec![],
            };

            println!(
                "Font {id:?} subtype={subtype} base={base} fc={fc} lc={lc} widths_len={widths_len}"
            );

            // Print widths at codes 1 and 128
            for code in [1u32, 128] {
                if fc >= 0 && code >= fc as u32 {
                    let idx = (code - fc as u32) as usize;
                    if idx < widths.len() {
                        println!("  code {code}: width = {}", widths[idx]);
                    }
                }
            }

            // Check if the font has a FontFile2 (TrueType)
            let has_ff2 = dict
                .get(b"FontDescriptor")
                .ok()
                .and_then(|o| {
                    if let Object::Reference(r) = o {
                        doc.objects.get(r).and_then(|fd| {
                            if let Object::Dictionary(d) = fd {
                                Some(d.has(b"FontFile2"))
                            } else {
                                None
                            }
                        })
                    } else {
                        None
                    }
                })
                .unwrap_or(false);
            println!("  has_ff2={has_ff2}");
        }
    }
}

/// Trace Arial font widths through the pipeline for gen-772 (§6.2.11.5:1).
///
/// gen-772 has GMNOAN+Arial0150 (OTF-wrapped CFF subset). This test shows
/// which codes get corrected by fix_font_width_mismatches and whether
/// those corrections match what veraPDF expects.
#[test]
#[ignore]
fn debug_gen772_arial_widths_trace() {
    use lopdf::Object;
    use pdf_manip::pdfa_xmp::PdfAConformance;

    let data = std::fs::read("/tmp/gen-772.pdf").unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    // Collect (FirstChar, Widths) for all Arial fonts
    fn get_arial_widths(doc: &lopdf::Document) -> Vec<(String, u32, Vec<i64>)> {
        let mut result = Vec::new();
        for (_id, obj) in &doc.objects {
            if let Object::Dictionary(dict) = obj {
                let base = dict
                    .get(b"BaseFont")
                    .ok()
                    .and_then(|o| {
                        if let Object::Name(n) = o { Some(String::from_utf8_lossy(n).to_string()) } else { None }
                    })
                    .unwrap_or_default();
                if !base.to_lowercase().contains("arial") {
                    continue;
                }
                let fc = match dict.get(b"FirstChar").ok() {
                    Some(Object::Integer(i)) => *i as u32,
                    _ => continue,
                };
                let widths: Vec<i64> = match dict.get(b"Widths").ok() {
                    Some(Object::Array(a)) => a.iter().map(|o| match o {
                        Object::Integer(i) => *i,
                        Object::Real(r) => *r as i64,
                        _ => -1,
                    }).collect(),
                    Some(Object::Reference(r)) => doc.get_object(*r).ok()
                        .and_then(|o| if let Object::Array(a) = o {
                            Some(a.iter().map(|o| match o {
                                Object::Integer(i) => *i,
                                Object::Real(r) => *r as i64,
                                _ => -1,
                            }).collect())
                        } else { None })
                        .unwrap_or_default(),
                    _ => continue,
                };
                result.push((base, fc, widths));
            }
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    }

    fn print_arial_widths(label: &str, before: &[(String, u32, Vec<i64>)], after: &[(String, u32, Vec<i64>)]) {
        for ((bname, bfc, bw), (_, afc, aw)) in before.iter().zip(after.iter()) {
            let mut diffs = Vec::new();
            for (i, (bval, aval)) in bw.iter().zip(aw.iter()).enumerate() {
                if bval != aval {
                    let code = *bfc + i as u32;
                    diffs.push(format!("code{}:{}→{}", code, bval, aval));
                }
            }
            if !diffs.is_empty() {
                println!("{} [{}] fc={} changes: {}", label, bname, bfc, diffs.join(", "));
            }
        }
    }

    let before = get_arial_widths(&doc);
    println!("=== gen-772 Arial fonts: {} fonts, codes per font:", before.len());
    for (name, fc, w) in &before {
        println!("  {} fc={} len={}", name, fc, w.len());
    }

    pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).unwrap();
    let after = get_arial_widths(&doc);
    print_arial_widths("after cleanup", &before, &after);
    let before = after;

    let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);
    pdf_manip::pdfa_fonts::fix_cff_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_truetype_cid_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_type1_charset(&mut doc);
    pdf_manip::pdfa_fonts::fix_truetype_encoding(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_existing_symbolic_truetype_cmaps(&mut doc);
    pdf_manip::pdfa_fonts::fix_truetype_unicode_cmap(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_type1_tounicode_from_encoding(&mut doc);
    pdf_manip::pdfa_fonts::fix_notdef_glyph_refs(&mut doc);
    pdf_manip::pdfa_fonts::fix_cid_font_notdef(&mut doc);
    pdf_manip::pdfa_fonts::fix_symbolic_font_notdef_streams(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_simple_font_out_of_range_codes(&mut doc);
    pdf_manip::pdfa_fonts::fix_undefined_encoding_codes(&mut doc);
    pdf_manip::pdfa_fonts::fix_symbolic_flags(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_classic_symbolic_base14_encoding(&mut doc);
    pdf_manip::pdfa_fonts::fix_missing_simple_font_widths(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_type3_font_widths(&mut doc);

    // Debug: inspect the CFF font data directly for one Arial font
    {
        use lopdf::Object;
        for (_id, obj) in &doc.objects {
            if let Object::Dictionary(dict) = obj {
                let base = dict.get(b"BaseFont").ok()
                    .and_then(|o| if let Object::Name(n) = o { Some(String::from_utf8_lossy(n).to_string()) } else { None })
                    .unwrap_or_default();
                if base != "GMNOAN+Arial0150" { continue; }
                let fd_id = match dict.get(b"FontDescriptor").ok() {
                    Some(Object::Reference(r)) => *r,
                    _ => continue,
                };
                let fd = match doc.objects.get(&fd_id) {
                    Some(Object::Dictionary(d)) => d,
                    _ => continue,
                };
                println!("=== GMNOAN+Arial0150 FontDescriptor has: ff={} ff2={} ff3={}",
                    fd.has(b"FontFile"), fd.has(b"FontFile2"), fd.has(b"FontFile3"));
                let enc_name = dict.get(b"Encoding").ok()
                    .and_then(|o| if let Object::Name(n) = o { Some(String::from_utf8_lossy(n).to_string()) } else { None })
                    .unwrap_or_else(|| "none/dict".to_string());
                println!("  Encoding: {}", enc_name);
                let fc = dict.get(b"FirstChar").ok()
                    .and_then(|o| if let Object::Integer(i) = o { Some(*i) } else { None }).unwrap_or(0);
                println!("  FirstChar: {}", fc);
                // Try to get font bytes and parse
                let ff_key: &[u8] = if fd.has(b"FontFile3") { b"FontFile3" }
                    else if fd.has(b"FontFile2") { b"FontFile2" } else { b"FontFile" };
                let font_data = match fd.get(ff_key).ok() {
                    Some(Object::Reference(r)) => match doc.objects.get(r) {
                        Some(Object::Stream(s)) => {
                            let mut s2 = s.clone();
                            let _ = s2.decompress();
                            Some(s2.content.clone())
                        },
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(data) = font_data {
                    println!("  Font data: {} bytes, first 4: {:02x?}", data.len(), &data[..data.len().min(4)]);
                    // Try OTF parse
                    match ttf_parser::Face::parse(&data, 0) {
                        Ok(face) => println!("  ttf_parser: OK upem={}", face.units_per_em()),
                        Err(e) => println!("  ttf_parser: FAIL {:?}", e),
                    }
                    // Try raw CFF parse
                    if let Some(cff) = cff_parser::Table::parse(&data) {
                        let n = cff.number_of_glyphs();
                        println!("  cff_parser: OK glyphs={}", n);
                        let mx = cff.matrix();
                        let scale = (mx.sx as f64) * 1000.0;
                        println!("  CFF matrix sx={} scale={}", mx.sx, scale);
                        // Check glyph names for first 5 non-notdef
                        for gid in 0..n.min(10) {
                            let name = cff.glyph_name(cff_parser::GlyphId(gid)).map(|s| s.to_string()).unwrap_or("?".to_string());
                            let w = cff.glyph_width(cff_parser::GlyphId(gid)).map(|w| (w as f64 * scale).round() as i64);
                            println!("    GID {}: name='{}' width={:?}", gid, name, w);
                        }
                        // Print CFF encoding map
                        let cff_bytes = &data;
                        println!("  CFF encoding map (code→GID):");
                        if cff_bytes.len() >= 4 {
                            let header_size = cff_bytes[2] as usize;
                            // Find encoding offset
                            if header_size < cff_bytes.len() {
                                // Parse manually: skip Name INDEX, read first Top DICT entry
                                // (just print first 16 bytes of raw CFF for diagnosis)
                                println!("  CFF header: major={} minor={} hdrSize={} offSize={}",
                                    cff_bytes[0], cff_bytes[1], cff_bytes[2], cff_bytes[3]);
                            }
                        }
                        // Show what glyph_width returns for GID 0 raw (unscaled)
                        // and for codes 32, 64 in CFF encoding
                        println!("  cff.glyph_width(GID 0) raw: {:?}", cff.glyph_width(cff_parser::GlyphId(0)));
                        println!("  cff.glyph_width(GID 1) raw: {:?}", cff.glyph_width(cff_parser::GlyphId(1)));
                        // Check private dict widths by parsing CFF top dict
                        let mx = cff.matrix();
                        let cff_scale = (mx.sx as f64 * 1_000_000.0).round() / 1_000_000.0 * 1000.0;
                        // Show cff.glyph_index for codes 32 and 64
                        for code in [32u8, 40, 41, 64, 65, 97] {
                            let gid = cff.glyph_index(code);
                            let w = gid.and_then(|g| cff.glyph_width(g));
                            // Also show what cff_type2_endchar_default_width returns
                            let endchar_w = gid.and_then(|g| {
                                // Simulate: parse private dict then charstring
                                None::<f64>  // placeholder
                            });
                            println!("  cff.glyph_index({code}) = {:?} width_raw={:?} scaled={:?}",
                                gid.map(|g| g.0), w,
                                w.map(|w| (w as f64 * cff_scale / 1000.0).round() as i64));
                            let _ = endchar_w;
                        }
                        // Show what find_cff_glyph_width_by_name_fractional returns for specific names
                        // We simulate it: search for "G32" in the charset
                        for gid in 0..n.min(65) {
                            if let Some(name) = cff.glyph_name(cff_parser::GlyphId(gid)) {
                                if name == "G32" || name == "G64" || name == "G65" || name == "G97" {
                                    let w_raw = cff.glyph_width(cff_parser::GlyphId(gid)).unwrap_or(0);
                                    let w_scaled = (w_raw as f64 * cff_scale / 1000.0).round() as i64;
                                    println!("  Glyph '{}' at GID {} raw={} scaled={}", name, gid, w_raw, w_scaled);
                                }
                            }
                        }
                        // Check specific names
                        for name in &["space", "A", "a", "exclam", ".notdef"] {
                            let found_gid = (0..n).find(|&gid| {
                                cff.glyph_name(cff_parser::GlyphId(gid)).map(|s| s == *name).unwrap_or(false)
                            });
                            if let Some(gid) = found_gid {
                                let w = cff.glyph_width(cff_parser::GlyphId(gid)).map(|w| (w as f64 * scale).round() as i64);
                                println!("  Name '{}' found at GID {} width={:?}", name, gid, w);
                            } else {
                                println!("  Name '{}' NOT in CFF charset", name);
                            }
                        }
                    } else {
                        println!("  cff_parser: FAIL");
                    }
                }
                break;
            }
        }
    }

    let before_fix = get_arial_widths(&doc);
    pdf_manip::pdfa_fonts::fix_font_width_mismatches(&mut doc);
    let after_fix = get_arial_widths(&doc);
    print_arial_widths("fix_font_width_mismatches", &before_fix, &after_fix);

    pdf_manip::pdfa_fonts::fix_symbolic_font_widths(&mut doc);
    let after_sym = get_arial_widths(&doc);
    print_arial_widths("fix_symbolic_font_widths", &after_fix, &after_sym);

    pdf_manip::pdfa_fonts::fix_cidset(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_missing_cidtogidmap(&mut doc);
    pdf_manip::pdfa_colorspace::normalize_colorspaces(&mut doc).unwrap();
    pdf_manip::pdfa_fixups::run_fixups(&mut doc);
    pdf_manip::pdfa_xmp::repair_xmp_metadata(&mut doc, PdfAConformance::A2b, None).unwrap();

    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    pdf_manip::pdfa_cleanup::fix_pdf_header(&mut saved);
    pdf_manip::pdfa_cleanup::fix_startxref(&mut saved);

    std::fs::write("/tmp/gen-772-converted.pdf", &saved).unwrap();
    println!("Saved to /tmp/gen-772-converted.pdf");

    // Also print final widths for all arial fonts
    let doc2 = lopdf::Document::load_mem(&saved).unwrap();
    println!("\n=== Final widths after full conversion:");
    for (name, fc, w) in get_arial_widths(&doc2) {
        // Print widths at codes 30-45 specifically (brackets the original fc=32)
        let start = (30u32.saturating_sub(fc)) as usize;
        let end = ((45u32.saturating_sub(fc)) as usize + 1).min(w.len());
        let slice = if start < w.len() { &w[start..end] } else { &w[..0] };
        println!("  {} fc={} len={} widths[30-45]={:?}", name, fc, w.len(), slice);
        println!("  {} first 35: {:?}", name, &w[..w.len().min(35)]);
    }
}

/// Test fix_image_alternates removes /Alternates from image XObjects.
#[test]
#[ignore]
fn test_fix_image_alternates() {
    let path = "/tmp/pdfa_fails/cs-veraPDF test suite 6-2-7-1-t01-fail-a.pdf";
    if !std::path::Path::new(path).exists() {
        println!("Test PDF not found, skipping");
        return;
    }
    let data = std::fs::read(path).unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    // Verify /Alternates IS present before conversion
    let before = doc.objects.values().any(|o| {
        if let lopdf::Object::Stream(s) = o {
            s.dict.get(b"Alternates").is_ok()
        } else {
            false
        }
    });
    println!("Has /Alternates before conversion: {before}");
    assert!(before, "Test PDF should have /Alternates in image XObject");

    pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).unwrap();

    // Verify /Alternates is GONE after conversion
    let after = doc.objects.values().any(|o| {
        if let lopdf::Object::Stream(s) = o {
            s.dict.get(b"Alternates").is_ok()
        } else {
            false
        }
    });
    println!("Has /Alternates after conversion: {after}");
    assert!(!after, "/Alternates should be removed by fix_image_alternates");

    println!("fix_image_alternates works correctly!");
}

/// Test that converting 6-2-7-1-t01-fail-a.pdf produces a clean PDF/A-2b.
#[test]
#[ignore]
fn test_627_full_conversion() {
    let path = "/tmp/pdfa_fails/cs-veraPDF test suite 6-2-7-1-t01-fail-a.pdf";
    if !std::path::Path::new(path).exists() {
        println!("Test PDF not found, skipping");
        return;
    }
    let data = std::fs::read(path).unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).unwrap();

    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    pdf_manip::pdfa_cleanup::fix_pdf_header(&mut saved);
    pdf_manip::pdfa_cleanup::fix_startxref(&mut saved);

    let has_alternates = saved.windows(10).any(|w| w == b"/Alternates");
    println!("Has /Alternates in converted: {has_alternates}");

    let pdf = pdf_syntax::Pdf::new(saved.clone()).unwrap();
    let report = pdf_compliance::validate_pdfa(&pdf, pdf_compliance::PdfALevel::A2b);
    println!("Compliant: {}", report.compliant);
    for issue in &report.issues {
        println!("  [{}] {}: {}", issue.severity as u8, issue.rule, issue.message);
    }
    assert!(!has_alternates, "/Alternates should be gone after conversion");
}

/// Quick validation check - save converted 627 PDF for manual inspection.
#[test]
#[ignore]
fn test_627_save_converted() {
    let path = "/tmp/pdfa_fails/cs-veraPDF test suite 6-2-7-1-t01-fail-a.pdf";
    if !std::path::Path::new(path).exists() {
        eprintln!("Test PDF not found, skipping");
        return;
    }
    let data = std::fs::read(path).unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();
    pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).unwrap();
    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    pdf_manip::pdfa_cleanup::fix_pdf_header(&mut saved);
    pdf_manip::pdfa_cleanup::fix_startxref(&mut saved);

    let has_alternates = saved.windows(10).any(|w| w == b"/Alternates");
    eprintln!("Has /Alternates in converted: {has_alternates}");

    let pdf = pdf_syntax::Pdf::new(saved.clone()).unwrap();
    let report = pdf_compliance::validate_pdfa(&pdf, pdf_compliance::PdfALevel::A2b);
    eprintln!("Compliant: {}", report.compliant);
    for issue in &report.issues {
        eprintln!("  [{}] {}: {}", issue.severity as u8, issue.rule, issue.message);
    }
    std::fs::write("/tmp/627-converted.pdf", &saved).unwrap();
    eprintln!("Saved to /tmp/627-converted.pdf");
    assert!(!has_alternates);
}

/// Test compliance check on the original 627 PDF.
#[test]
#[ignore]
fn test_627_original_check() {
    let path = "/tmp/pdfa_fails/cs-veraPDF test suite 6-2-7-1-t01-fail-a.pdf";
    if !std::path::Path::new(path).exists() {
        eprintln!("Test PDF not found, skipping");
        return;
    }
    let data = std::fs::read(path).unwrap();
    
    match pdf_syntax::Pdf::new(data) {
        Ok(pdf) => {
            let detected = pdf_compliance::detect_pdfa_level(&pdf);
            eprintln!("Detected PDF/A level: {:?}", detected);
            
            let report = pdf_compliance::validate_pdfa(&pdf, pdf_compliance::PdfALevel::A4);
            eprintln!("Compliant (as A4): {}", report.compliant);
            for issue in &report.issues {
                eprintln!("  [{}] {}: {}", issue.severity as u8, issue.rule, issue.message);
            }
        }
        Err(e) => eprintln!("Pdf::new failed: {e:?}"),
    }
}

/// Check compliance on all test suite PDFs.
#[test]
#[ignore]
fn test_all_failing_pdfs() {
    let dir = "/tmp/pdfa_fails";
    let pdfs = [
        ("cs-isartor-6-3-4-t01-fail-f.pdf", pdf_compliance::PdfALevel::A2b),
        ("cs-isartor-6-3-5-t01-fail-d.pdf", pdf_compliance::PdfALevel::A2b),
        ("cs-veraPDF test suite 6-1-6-2-t01-fail-b.pdf", pdf_compliance::PdfALevel::A2b),
        ("cs-veraPDF test suite 6-2-10-4-1-t02-fail-a.pdf", pdf_compliance::PdfALevel::A2b),
        ("cs-veraPDF test suite 6-2-7-1-t01-fail-a.pdf", pdf_compliance::PdfALevel::A4),
        ("cs-veraPDF test suite 6-6-2-3-1-t01-fail-c.pdf", pdf_compliance::PdfALevel::A2b),
        ("tagged-isartor-6-1-13-t01-fail-a.pdf", pdf_compliance::PdfALevel::A2b),
        ("tagged-veraPDF test suite 6-1-11-t01-fail-a.pdf", pdf_compliance::PdfALevel::A2b),
    ];
    for (name, level) in &pdfs {
        let path = format!("{dir}/{name}");
        if !std::path::Path::new(&path).exists() { continue; }
        let data = std::fs::read(&path).unwrap();
        match pdf_syntax::Pdf::new(data) {
            Ok(pdf) => {
                let detected = pdf_compliance::detect_pdfa_level(&pdf);
                let report = pdf_compliance::validate_pdfa(&pdf, *level);
                eprintln!("=== {name} (detected: {:?}, level: {:?}) ===", detected, level);
                eprintln!("  Compliant: {}", report.compliant);
                for issue in report.issues.iter().take(5) {
                    eprintln!("  [{}] {}: {}", issue.severity as u8, issue.rule, issue.message);
                }
            }
            Err(e) => eprintln!("=== {name}: Pdf::new failed: {e:?}"),
        }
    }
}

/// Check compliance on each PDF at its own detected level.
#[test]
#[ignore]
fn test_all_failing_pdfs_detected() {
    let dir = "/tmp/pdfa_fails";
    let names = [
        "cs-isartor-6-3-4-t01-fail-f.pdf",
        "cs-isartor-6-3-5-t01-fail-d.pdf",
        "cs-veraPDF test suite 6-1-6-2-t01-fail-b.pdf",
        "cs-veraPDF test suite 6-2-10-4-1-t02-fail-a.pdf",
        "cs-veraPDF test suite 6-2-7-1-t01-fail-a.pdf",
        "cs-veraPDF test suite 6-6-2-3-1-t01-fail-c.pdf",
        "tagged-isartor-6-1-13-t01-fail-a.pdf",
        "tagged-veraPDF test suite 6-1-11-t01-fail-a.pdf",
    ];
    for name in &names {
        let path = format!("{dir}/{name}");
        if !std::path::Path::new(&path).exists() { continue; }
        let data = std::fs::read(&path).unwrap();
        match pdf_syntax::Pdf::new(data) {
            Ok(pdf) => {
                let level = pdf_compliance::detect_pdfa_level(&pdf)
                    .unwrap_or(pdf_compliance::PdfALevel::A2b);
                let report = pdf_compliance::validate_pdfa(&pdf, level);
                eprintln!("=== {name} (level: {:?}) ===", level);
                for issue in report.issues.iter().take(3) {
                    eprintln!("  [{}] {}: {}", issue.severity as u8, issue.rule, issue.message);
                }
                if report.compliant { eprintln!("  COMPLIANT (should not be!)"); }
            }
            Err(e) => eprintln!("=== {name}: Pdf::new failed: {e:?}"),
        }
    }
}
