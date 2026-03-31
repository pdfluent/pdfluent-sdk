#![cfg(feature = "pdfa-convert")]
#![allow(unused_imports, unused_variables, dead_code, unused_parens)]

/// Step-by-step pipeline test to identify which step crashes on a given PDF.
/// Run with: cargo test -p pdf-manip --features pdfa-convert --test debug_convert debug_step_by_step -- --ignored --nocapture
#[test]
#[ignore]
fn debug_step_by_step() {
    use pdf_manip::pdfa_xmp::PdfAConformance;
    use std::io::Write;

    macro_rules! step {
        ($label:expr, $e:expr) => {{
            eprint!("  {} ... ", $label);
            let _ = std::io::stderr().flush();
            $e;
            eprintln!("OK");
        }};
    }

    let path = std::env::var("DEBUG_PDF").unwrap_or_else(|_| "/tmp/poppler-43279-0.pdf".into());
    eprintln!("Testing: {path}");

    let data = std::fs::read(&path).unwrap();
    eprintln!("Loaded {} bytes", data.len());

    step!("lopdf load", {
        let doc = lopdf::Document::load_mem(&data);
        eprintln!(
            "(pages={})",
            doc.as_ref().map(|d| d.get_pages().len()).unwrap_or(0)
        );
    });

    let mut doc = lopdf::Document::load_mem(&data).unwrap_or_default();
    step!("cleanup_for_pdfa", {
        let _ = pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false);
    });
    step!("embed_fonts", {
        let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);
    });
    step!("fix_cff_widths", {
        pdf_manip::pdfa_fonts::fix_cff_widths(&mut doc);
    });
    step!("fix_truetype_cid_widths", {
        pdf_manip::pdfa_fonts::fix_truetype_cid_widths(&mut doc);
    });
    step!("fix_type1_charset", {
        pdf_manip::pdfa_fonts::fix_type1_charset(&mut doc);
    });
    step!("fix_truetype_encoding", {
        pdf_manip::pdfa_fonts::fix_truetype_encoding(&mut doc);
    });
    step!("fix_truetype_unicode_cmap", {
        pdf_manip::pdfa_fonts::fix_truetype_unicode_cmap(&mut doc);
    });
    step!("fix_notdef_glyph_refs", {
        pdf_manip::pdfa_fonts::fix_notdef_glyph_refs(&mut doc);
    });
    step!("fix_cid_font_notdef", {
        pdf_manip::pdfa_fonts::fix_cid_font_notdef(&mut doc);
    });
    step!("fix_symbolic_font_notdef_streams", {
        pdf_manip::pdfa_fonts::fix_symbolic_font_notdef_streams(&mut doc);
    });
    step!("fix_undefined_encoding_codes", {
        pdf_manip::pdfa_fonts::fix_undefined_encoding_codes(&mut doc);
    });
    step!("fix_symbolic_flags", {
        pdf_manip::pdfa_fonts::fix_symbolic_flags(&mut doc);
    });
    step!("fix_missing_simple_font_widths", {
        pdf_manip::pdfa_fonts::fix_missing_simple_font_widths(&mut doc);
    });
    step!("fix_font_width_mismatches", {
        pdf_manip::pdfa_fonts::fix_font_width_mismatches(&mut doc);
    });
    step!("fix_symbolic_font_widths", {
        pdf_manip::pdfa_fonts::fix_symbolic_font_widths(&mut doc);
    });
    step!("fix_cidset", {
        pdf_manip::pdfa_fonts::fix_cidset(&mut doc);
    });
    step!("normalize_colorspaces", {
        let _ = pdf_manip::pdfa_colorspace::normalize_colorspaces(&mut doc);
    });
    step!("run_fixups", {
        pdf_manip::pdfa_fixups::run_fixups(&mut doc);
    });
    step!("repair_xmp", {
        let _ = pdf_manip::pdfa_xmp::repair_xmp_metadata(&mut doc, PdfAConformance::A2b, None);
    });
    step!("save", {
        let mut saved = Vec::new();
        doc.save_to(&mut saved).unwrap();
    });
    eprintln!("ALL STEPS COMPLETED");
}

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
    let _ = pdf_manip::pdfa_fonts::fix_truetype_with_cff_program(&mut doc);
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
    let _ = pdf_manip::pdfa_fonts::fix_truetype_with_cff_program(&mut doc);
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
                        if let Object::Name(n) = o {
                            Some(String::from_utf8_lossy(n).to_string())
                        } else {
                            None
                        }
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
                    _ => continue,
                };
                result.push((base, fc, widths));
            }
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    }

    fn print_arial_widths(
        label: &str,
        before: &[(String, u32, Vec<i64>)],
        after: &[(String, u32, Vec<i64>)],
    ) {
        for ((bname, bfc, bw), (_, afc, aw)) in before.iter().zip(after.iter()) {
            let mut diffs = Vec::new();
            for (i, (bval, aval)) in bw.iter().zip(aw.iter()).enumerate() {
                if bval != aval {
                    let code = *bfc + i as u32;
                    diffs.push(format!("code{}:{}→{}", code, bval, aval));
                }
            }
            if !diffs.is_empty() {
                println!(
                    "{} [{}] fc={} changes: {}",
                    label,
                    bname,
                    bfc,
                    diffs.join(", ")
                );
            }
        }
    }

    let before = get_arial_widths(&doc);
    println!(
        "=== gen-772 Arial fonts: {} fonts, codes per font:",
        before.len()
    );
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
                if base != "GMNOAN+Arial0150" {
                    continue;
                }
                let fd_id = match dict.get(b"FontDescriptor").ok() {
                    Some(Object::Reference(r)) => *r,
                    _ => continue,
                };
                let fd = match doc.objects.get(&fd_id) {
                    Some(Object::Dictionary(d)) => d,
                    _ => continue,
                };
                println!(
                    "=== GMNOAN+Arial0150 FontDescriptor has: ff={} ff2={} ff3={}",
                    fd.has(b"FontFile"),
                    fd.has(b"FontFile2"),
                    fd.has(b"FontFile3")
                );
                let enc_name = dict
                    .get(b"Encoding")
                    .ok()
                    .and_then(|o| {
                        if let Object::Name(n) = o {
                            Some(String::from_utf8_lossy(n).to_string())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| "none/dict".to_string());
                println!("  Encoding: {}", enc_name);
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
                    .unwrap_or(0);
                println!("  FirstChar: {}", fc);
                // Try to get font bytes and parse
                let ff_key: &[u8] = if fd.has(b"FontFile3") {
                    b"FontFile3"
                } else if fd.has(b"FontFile2") {
                    b"FontFile2"
                } else {
                    b"FontFile"
                };
                let font_data = match fd.get(ff_key).ok() {
                    Some(Object::Reference(r)) => match doc.objects.get(r) {
                        Some(Object::Stream(s)) => {
                            let mut s2 = s.clone();
                            let _ = s2.decompress();
                            Some(s2.content.clone())
                        }
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(data) = font_data {
                    println!(
                        "  Font data: {} bytes, first 4: {:02x?}",
                        data.len(),
                        &data[..data.len().min(4)]
                    );
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
                            let name = cff
                                .glyph_name(cff_parser::GlyphId(gid))
                                .map(|s| s.to_string())
                                .unwrap_or("?".to_string());
                            let w = cff
                                .glyph_width(cff_parser::GlyphId(gid))
                                .map(|w| (w as f64 * scale).round() as i64);
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
                                println!(
                                    "  CFF header: major={} minor={} hdrSize={} offSize={}",
                                    cff_bytes[0], cff_bytes[1], cff_bytes[2], cff_bytes[3]
                                );
                            }
                        }
                        // Show what glyph_width returns for GID 0 raw (unscaled)
                        // and for codes 32, 64 in CFF encoding
                        println!(
                            "  cff.glyph_width(GID 0) raw: {:?}",
                            cff.glyph_width(cff_parser::GlyphId(0))
                        );
                        println!(
                            "  cff.glyph_width(GID 1) raw: {:?}",
                            cff.glyph_width(cff_parser::GlyphId(1))
                        );
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
                                None::<f64> // placeholder
                            });
                            println!(
                                "  cff.glyph_index({code}) = {:?} width_raw={:?} scaled={:?}",
                                gid.map(|g| g.0),
                                w,
                                w.map(|w| (w as f64 * cff_scale / 1000.0).round() as i64)
                            );
                            let _ = endchar_w;
                        }
                        // Show what find_cff_glyph_width_by_name_fractional returns for specific names
                        // We simulate it: search for "G32" in the charset
                        for gid in 0..n.min(65) {
                            if let Some(name) = cff.glyph_name(cff_parser::GlyphId(gid)) {
                                if name == "G32" || name == "G64" || name == "G65" || name == "G97"
                                {
                                    let w_raw =
                                        cff.glyph_width(cff_parser::GlyphId(gid)).unwrap_or(0);
                                    let w_scaled =
                                        (w_raw as f64 * cff_scale / 1000.0).round() as i64;
                                    println!(
                                        "  Glyph '{}' at GID {} raw={} scaled={}",
                                        name, gid, w_raw, w_scaled
                                    );
                                }
                            }
                        }
                        // Check specific names
                        for name in &["space", "A", "a", "exclam", ".notdef"] {
                            let found_gid = (0..n).find(|&gid| {
                                cff.glyph_name(cff_parser::GlyphId(gid))
                                    .map(|s| s == *name)
                                    .unwrap_or(false)
                            });
                            if let Some(gid) = found_gid {
                                let w = cff
                                    .glyph_width(cff_parser::GlyphId(gid))
                                    .map(|w| (w as f64 * scale).round() as i64);
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
        let slice = if start < w.len() {
            &w[start..end]
        } else {
            &w[..0]
        };
        println!(
            "  {} fc={} len={} widths[30-45]={:?}",
            name,
            fc,
            w.len(),
            slice
        );
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
    assert!(
        !after,
        "/Alternates should be removed by fix_image_alternates"
    );

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
        println!(
            "  [{}] {}: {}",
            issue.severity as u8, issue.rule, issue.message
        );
    }
    assert!(
        !has_alternates,
        "/Alternates should be gone after conversion"
    );
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
        eprintln!(
            "  [{}] {}: {}",
            issue.severity as u8, issue.rule, issue.message
        );
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
                eprintln!(
                    "  [{}] {}: {}",
                    issue.severity as u8, issue.rule, issue.message
                );
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
        (
            "cs-isartor-6-3-4-t01-fail-f.pdf",
            pdf_compliance::PdfALevel::A2b,
        ),
        (
            "cs-isartor-6-3-5-t01-fail-d.pdf",
            pdf_compliance::PdfALevel::A2b,
        ),
        (
            "cs-veraPDF test suite 6-1-6-2-t01-fail-b.pdf",
            pdf_compliance::PdfALevel::A2b,
        ),
        (
            "cs-veraPDF test suite 6-2-10-4-1-t02-fail-a.pdf",
            pdf_compliance::PdfALevel::A2b,
        ),
        (
            "cs-veraPDF test suite 6-2-7-1-t01-fail-a.pdf",
            pdf_compliance::PdfALevel::A4,
        ),
        (
            "cs-veraPDF test suite 6-6-2-3-1-t01-fail-c.pdf",
            pdf_compliance::PdfALevel::A2b,
        ),
        (
            "tagged-isartor-6-1-13-t01-fail-a.pdf",
            pdf_compliance::PdfALevel::A2b,
        ),
        (
            "tagged-veraPDF test suite 6-1-11-t01-fail-a.pdf",
            pdf_compliance::PdfALevel::A2b,
        ),
    ];
    for (name, level) in &pdfs {
        let path = format!("{dir}/{name}");
        if !std::path::Path::new(&path).exists() {
            continue;
        }
        let data = std::fs::read(&path).unwrap();
        match pdf_syntax::Pdf::new(data) {
            Ok(pdf) => {
                let detected = pdf_compliance::detect_pdfa_level(&pdf);
                let report = pdf_compliance::validate_pdfa(&pdf, *level);
                eprintln!(
                    "=== {name} (detected: {:?}, level: {:?}) ===",
                    detected, level
                );
                eprintln!("  Compliant: {}", report.compliant);
                for issue in report.issues.iter().take(5) {
                    eprintln!(
                        "  [{}] {}: {}",
                        issue.severity as u8, issue.rule, issue.message
                    );
                }
            }
            Err(e) => eprintln!("=== {name}: Pdf::new failed: {e:?}"),
        }
    }
}

/// Debug TT width=0: run ONLY fix_font_width_mismatches on gen-319, check if TT2 is fixed.
#[test]
#[ignore]
fn debug_tt_width_only() {
    use lopdf::Object;
    use pdf_syntax::Pdf;

    let path = "/tmp/gen-319_319905.pdf";
    let data = std::fs::read(path).unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    // Run only the width mismatch fix
    let n = pdf_manip::pdfa_fonts::fix_font_width_mismatches(&mut doc);
    println!("fix_font_width_mismatches: fixed {} fonts", n);

    // Save and validate
    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    match Pdf::new(saved) {
        Ok(pdf) => {
            let report = pdf_compliance::validate_pdfa(&pdf, pdf_compliance::PdfALevel::A2b);
            for issue in &report.issues {
                if issue.rule.contains("6.2.11.5") {
                    println!(
                        "  [{}] {}: {}",
                        issue.rule, issue.severity as u8, issue.message
                    );
                }
            }
            println!(
                "  §6.2.11.5 errors: {}",
                report
                    .issues
                    .iter()
                    .filter(|i| i.rule.contains("6.2.11.5"))
                    .count()
            );
        }
        Err(e) => println!("Pdf::new failed: {e:?}"),
    }

    // Also dump TrueType font widths from the doc after fix
    for (id, obj) in &doc.objects {
        let Object::Dictionary(dict) = obj else {
            continue;
        };
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
        if subtype != "TrueType" {
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
        let fc = match dict.get(b"FirstChar").ok() {
            Some(Object::Integer(i)) => *i as u32,
            _ => continue,
        };
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
            _ => continue,
        };
        // Check width at code 33
        if fc <= 33 {
            let idx = (33 - fc) as usize;
            if idx < widths.len() {
                println!(
                    "  Font {:?} base={} fc={}: width[code33]={}",
                    id, base, fc, widths[idx]
                );
            }
        }
    }
}

/// Convert and validate the round-23 failing PDFs locally.
/// Usage: cargo test -p pdf-manip --test debug_convert -- --ignored debug_round23_failures --nocapture
#[test]
#[ignore]
fn debug_round23_failures() {
    use pdf_manip::pdfa_xmp::PdfAConformance;
    use pdf_syntax::Pdf;

    let pdfs: &[(&str, &str)] = &[
        ("/tmp/gen-582_582444.pdf", "gen-582 §6.2.11.5:1"),
        ("/tmp/gen-319_319905.pdf", "gen-319 §6.2.11.5:1"),
        ("/tmp/gen-698_698323.pdf", "gen-698 §6.2.11.4.1:2"),
        ("/tmp/gen-152_152696.pdf", "gen-152 §6.2.2:1"),
        ("/tmp/r3-654_654445.pdf", "r3-654 §6.2.11.6:2"),
        ("/tmp/gen-301_301608.pdf", "gen-301 §6.10:2"),
        ("/tmp/gen-108_108037.pdf", "gen-108 own:90"),
    ];

    for (path, label) in pdfs {
        println!("\n{}", "=".repeat(60));
        println!("=== {label} ({path}) ===");

        if !std::path::Path::new(path).exists() {
            println!("  SKIP: file not found");
            continue;
        }

        let data = std::fs::read(path).unwrap();

        // Detect original PDF/A level
        let orig_pdf = match Pdf::new(data.clone()) {
            Ok(p) => p,
            Err(e) => {
                println!("  Pdf::new failed on original: {e:?}");
                continue;
            }
        };
        let detected = pdf_compliance::detect_pdfa_level(&orig_pdf);
        let target_level = detected.unwrap_or(pdf_compliance::PdfALevel::A2b);
        println!(
            "  Detected level: {:?} → converting to {:?}",
            detected, target_level
        );

        let conformance = match target_level {
            pdf_compliance::PdfALevel::A1b | pdf_compliance::PdfALevel::A1a => PdfAConformance::A1b,
            pdf_compliance::PdfALevel::A3b
            | pdf_compliance::PdfALevel::A3u
            | pdf_compliance::PdfALevel::A3a => PdfAConformance::A3b,
            _ => PdfAConformance::A2b,
        };
        let is_part1 = matches!(
            target_level,
            pdf_compliance::PdfALevel::A1b | pdf_compliance::PdfALevel::A1a
        );

        // Load + convert
        let mut doc = match lopdf::Document::load_mem(&data) {
            Ok(d) => d,
            Err(e) => {
                println!("  lopdf load failed: {e:?}");
                continue;
            }
        };

        let _ = pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, is_part1);
        let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);
        let _ = pdf_manip::pdfa_fonts::fix_pfb_font_streams(&mut doc);
        let _ = pdf_manip::pdfa_fonts::fix_type1_stub_font_files(&mut doc);
        let _ = pdf_manip::pdfa_fonts::fix_mislabeled_truetype_as_cff(&mut doc);
        let _ = pdf_manip::pdfa_fonts::fix_truetype_with_cff_program(&mut doc);
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
        let _ = pdf_manip::pdfa_colorspace::normalize_colorspaces(&mut doc);
        pdf_manip::pdfa_fixups::run_fixups(&mut doc);
        let _ = pdf_manip::pdfa_xmp::repair_xmp_metadata(&mut doc, conformance, None);

        let mut saved = Vec::new();
        doc.save_to(&mut saved).unwrap();
        pdf_manip::pdfa_cleanup::fix_pdf_header(&mut saved);
        pdf_manip::pdfa_cleanup::fix_startxref(&mut saved);

        // Save converted for manual inspection
        let out_path = format!("{}-converted.pdf", path.trim_end_matches(".pdf"));
        std::fs::write(&out_path, &saved).unwrap();
        println!("  Saved converted to {out_path}");

        // Validate with our checker
        match Pdf::new(saved) {
            Ok(pdf) => {
                let report = pdf_compliance::validate_pdfa(&pdf, target_level);
                println!(
                    "  Compliant: {} | Errors: {}",
                    report.compliant,
                    report.issues.len()
                );
                for issue in report.issues.iter().take(20) {
                    println!("  [{:?}] {}: {}", issue.severity, issue.rule, issue.message);
                }
                if report.issues.len() > 20 {
                    println!("  ... ({} more)", report.issues.len() - 20);
                }
            }
            Err(e) => println!("  Pdf::new failed on converted output: {e:?}"),
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
        if !std::path::Path::new(&path).exists() {
            continue;
        }
        let data = std::fs::read(&path).unwrap();
        match pdf_syntax::Pdf::new(data) {
            Ok(pdf) => {
                let level = pdf_compliance::detect_pdfa_level(&pdf)
                    .unwrap_or(pdf_compliance::PdfALevel::A2b);
                let report = pdf_compliance::validate_pdfa(&pdf, level);
                eprintln!("=== {name} (level: {:?}) ===", level);
                for issue in report.issues.iter().take(3) {
                    eprintln!(
                        "  [{}] {}: {}",
                        issue.severity as u8, issue.rule, issue.message
                    );
                }
                if report.compliant {
                    eprintln!("  COMPLIANT (should not be!)");
                }
            }
            Err(e) => eprintln!("=== {name}: Pdf::new failed: {e:?}"),
        }
    }
}

/// Trace gen-319 font F1 structure and code 39 width handling.
#[test]
#[ignore]
fn debug_gen319_cff_code39() {
    use lopdf::Object;
    use pdf_manip::pdfa_xmp::PdfAConformance;

    let data = std::fs::read("/tmp/gen-319_319905.pdf").unwrap();
    let doc = lopdf::Document::load_mem(&data).unwrap();

    println!("=== gen-319 font F1 structure ===");
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
            if !matches!(
                subtype.as_str(),
                "Type1" | "TrueType" | "Type1C" | "CIDFontType0C" | "CIDFontType2" | "MMType1"
            ) {
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
            let enc = dict
                .get(b"Encoding")
                .ok()
                .map(|o| match o {
                    Object::Name(n) => String::from_utf8_lossy(n).to_string(),
                    Object::Reference(r) => format!("ref({} {})", r.0, r.1),
                    Object::Dictionary(_) => "dict".to_string(),
                    _ => "other".to_string(),
                })
                .unwrap_or_default();
            let has_ff = dict.get(b"FontDescriptor").is_ok();
            println!(
                "  [{id:?}] {subtype} BaseFont={base:?} FC={fc} LC={lc} Enc={enc:?} HasFD={has_ff}"
            );
            if fc <= 39 && 39 <= lc {
                let widths_ref = dict.get(b"Widths").ok();
                if let Some(Object::Array(ws)) = widths_ref {
                    let idx = (39 - fc as usize);
                    if idx < ws.len() {
                        println!("    /Widths[{idx}] (code 39) = {:?}", ws[idx]);
                    }
                } else if let Some(Object::Reference(r)) = widths_ref {
                    println!("    /Widths ref: {r:?}");
                }
            }
        }
    }

    // Run pipeline and check result
    let mut doc2 = lopdf::Document::load_mem(&data).unwrap();
    let _ = pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc2, false);
    let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc2);
    pdf_manip::pdfa_fonts::fix_cff_widths(&mut doc2);
    pdf_manip::pdfa_fonts::fix_font_width_mismatches(&mut doc2);

    println!("\n=== After fix_cff_widths + fix_font_width_mismatches ===");
    for (id, obj) in &doc2.objects {
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
            if !matches!(subtype.as_str(), "Type1" | "TrueType") {
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
            if fc <= 39 && 39 <= lc {
                let widths_ref = dict.get(b"Widths").ok();
                if let Some(Object::Array(ws)) = widths_ref {
                    let idx = (39 - fc as usize);
                    if idx < ws.len() {
                        println!(
                            "  [{id:?}] {subtype} {base:?}: /Widths[{idx}] (code 39) = {:?}",
                            ws[idx]
                        );
                    }
                }
            }
        }
    }
}

#[test]
#[ignore]
fn debug_gen490_cff_glyphs() {
    // Dump all CFF glyph names and widths for fonts in gen-490_490200.pdf
    use lopdf::{Document, Object};

    let data = std::fs::read("/tmp/gen-490_490200.pdf").unwrap();
    let doc = Document::load_mem(&data).unwrap();

    for (id, obj) in &doc.objects {
        let dict = match obj {
            Object::Dictionary(d) => d,
            Object::Stream(s) => &s.dict,
            _ => continue,
        };
        let subtype = dict
            .get(b"Subtype")
            .ok()
            .and_then(|o| {
                if let Object::Name(n) = o {
                    std::str::from_utf8(n).ok().map(str::to_string)
                } else {
                    None
                }
            })
            .unwrap_or_default();
        if !["Type1", "MMType1"].contains(&subtype.as_str()) {
            continue;
        }
        let base = dict
            .get(b"BaseFont")
            .ok()
            .and_then(|o| {
                if let Object::Name(n) = o {
                    std::str::from_utf8(n).ok().map(str::to_string)
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

        println!("  Font {id:?} BaseFont={base} fc={fc} lc={lc}");

        // Only look at fonts with codes 222/223
        if !(fc <= 222 && 222 <= lc) {
            continue;
        }

        println!("\n=== Font {id:?} BaseFont={base} fc={fc} lc={lc} (HAS 222/223) ===");

        // Show /Widths for code 222/223
        if let Some(Object::Array(ws)) = dict.get(b"Widths").ok() {
            for code in [222i64, 223i64] {
                let idx = (code - fc) as usize;
                if idx < ws.len() {
                    println!("  /Widths[code {code}] = {:?}", ws[idx]);
                }
            }
        }

        // Show encoding
        if let Some(enc) = dict.get(b"Encoding").ok() {
            println!("  /Encoding = {:?}", enc);
        }

        // Find FontDescriptor → FontFile3
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
        let ff3_stream = match doc.objects.get(&ff3_id) {
            Some(Object::Stream(s)) => s,
            _ => continue,
        };
        let cff_bytes = ff3_stream.decompressed_content().unwrap();

        // Parse CFF
        let cff = match cff_parser::Table::parse(&cff_bytes) {
            Some(t) => t,
            None => {
                println!("  CFF parse failed");
                continue;
            }
        };

        println!("  CFF: num_glyphs={}", cff.number_of_glyphs());
        println!("  All glyph names:");
        for gid_raw in 0..cff.number_of_glyphs() {
            let gid = cff_parser::GlyphId(gid_raw);
            let name = cff.glyph_name(gid).unwrap_or("<no name>");
            let width = cff.glyph_width(gid).unwrap_or(0);
            println!("    GID {gid_raw}: name={name:?} width={width}");
        }

        // Try glyph_index_by_name for "fi" and "fl"
        for name in ["fi", "fl"] {
            let gid = cff.glyph_index_by_name(name);
            println!("  glyph_index_by_name({name:?}) = {gid:?}");
        }

        // Try glyph_index for code 222/223
        for code in [222u8, 223u8] {
            let gid = cff.glyph_index(code);
            println!("  glyph_index({code}) = {gid:?}");
        }
    }
}

/// Trace gen-319 font NNHDHP+Helvetica code 39 through each pipeline step.
#[test]
#[ignore]
fn debug_gen319_code39_trace() {
    use lopdf::Object;
    use pdf_manip::pdfa_xmp::PdfAConformance;

    fn get_helvetica_code39(doc: &lopdf::Document) -> Option<i64> {
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
                if !base.contains("Helvetica") {
                    continue;
                }
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
                if fc > 39 || 39 > lc {
                    continue;
                }
                if let Some(Object::Array(ws)) = dict.get(b"Widths").ok() {
                    let idx = (39 - fc) as usize;
                    if idx < ws.len() {
                        if let Object::Integer(w) = &ws[idx] {
                            return Some(*w);
                        }
                        if let Object::Real(w) = &ws[idx] {
                            return Some(*w as i64);
                        }
                    }
                }
            }
        }
        None
    }

    let data = std::fs::read("/tmp/gen-319_319905.pdf").unwrap();

    macro_rules! step {
        ($label:expr, $doc:ident) => {
            println!("After {}: code39={:?}", $label, get_helvetica_code39(&$doc));
        };
    }

    let mut doc = lopdf::Document::load_mem(&data).unwrap();
    println!("Original: code39={:?}", get_helvetica_code39(&doc));
    let _ = pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false);
    step!("cleanup", doc);
    let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);
    step!("embed_fonts", doc);
    pdf_manip::pdfa_fonts::fix_cff_widths(&mut doc);
    step!("fix_cff_widths", doc);
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
    step!("fix_missing_simple_font_widths", doc);
    let _ = pdf_manip::pdfa_fonts::fix_type3_font_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_font_width_mismatches(&mut doc);
    step!("fix_font_width_mismatches", doc);
    pdf_manip::pdfa_fonts::fix_symbolic_font_widths(&mut doc);
    step!("fix_symbolic_font_widths", doc);
}

/// Trace gen-319 Helvetica font descriptor and FontFile.
#[test]
#[ignore]
fn debug_gen319_helvetica_fd() {
    use lopdf::Object;

    let data = std::fs::read("/tmp/gen-319_319905.pdf").unwrap();
    let doc = lopdf::Document::load_mem(&data).unwrap();

    println!("=== Helvetica font info ===");
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
            if subtype != "Type1" {
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
            if !base.contains("Helvetica") {
                continue;
            }

            let enc = dict
                .get(b"Encoding")
                .ok()
                .map(|o| match o {
                    Object::Name(n) => String::from_utf8_lossy(n).to_string(),
                    Object::Reference(r) => format!("ref({} {})", r.0, r.1),
                    Object::Dictionary(_) => "dict".to_string(),
                    _ => "other".to_string(),
                })
                .unwrap_or("none".to_string());
            println!("Font {id:?}: {base} Enc={enc}");

            // Get FontDescriptor
            if let Some(Object::Reference(fd_ref)) = dict.get(b"FontDescriptor").ok() {
                println!("  FontDescriptor ref: {fd_ref:?}");
                if let Some(Object::Dictionary(fd)) = doc.objects.get(fd_ref) {
                    let ff1 = fd.get(b"FontFile").is_ok();
                    let ff2 = fd.get(b"FontFile2").is_ok();
                    let ff3 = fd.get(b"FontFile3").is_ok();
                    println!("  FontFile1={ff1} FontFile2={ff2} FontFile3={ff3}");
                    if ff3 {
                        if let Some(Object::Reference(ff3_ref)) = fd.get(b"FontFile3").ok() {
                            println!("  FontFile3 ref: {ff3_ref:?}");
                            if let Some(Object::Stream(s)) = doc.objects.get(ff3_ref) {
                                let subtype2 = s
                                    .dict
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
                                println!(
                                    "  FontFile3 stream len={} subtype={subtype2:?}",
                                    s.content.len()
                                );
                            }
                        }
                    }
                }
            } else {
                println!("  No FontDescriptor ref");
            }

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
            println!("  FirstChar={fc}");
        }
    }
}

/// Directly test cff_width_for_code on gen-319 Helvetica FontFile3.
#[test]
#[ignore]
fn debug_gen319_cff_width_direct() {
    use lopdf::Object;

    let data = std::fs::read("/tmp/gen-319_319905.pdf").unwrap();
    let doc = lopdf::Document::load_mem(&data).unwrap();

    // Find NNHDHP+Helvetica FontFile3 bytes
    let mut font_data: Option<Vec<u8>> = None;
    for (_id, obj) in &doc.objects {
        if let Object::Dictionary(dict) = obj {
            let st = dict
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
            if st != "Type1" {
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
            if !base.contains("Helvetica") {
                continue;
            }
            if let Some(Object::Reference(fd_ref)) = dict.get(b"FontDescriptor").ok() {
                if let Some(Object::Dictionary(fd)) = doc.objects.get(fd_ref) {
                    if let Some(Object::Reference(ff3_ref)) = fd.get(b"FontFile3").ok() {
                        if let Some(Object::Stream(s)) = doc.objects.get(ff3_ref) {
                            font_data = Some(s.content.clone());
                            break;
                        }
                    }
                }
            }
        }
    }

    let font_data = font_data.expect("FontFile3 not found");
    println!("FontFile3 len={}", font_data.len());

    // Parse as CFF
    let cff = cff_parser::Table::parse(&font_data).expect("CFF parse failed");
    let num_glyphs = cff.number_of_glyphs();
    let matrix = cff.matrix();
    let scale = matrix.sx;
    println!(
        "num_glyphs={num_glyphs} matrix.sx={:.4} (scale from 1000/upem would be {:.4})",
        scale,
        1000.0 * scale
    );

    // List all glyph names
    println!("Glyph names:");
    for gid in 0..num_glyphs {
        let g = cff_parser::GlyphId(gid);
        let name = cff.glyph_name(g);
        let w = cff.glyph_width(g);
        println!(
            "  GID {gid}: name={name:?} width={w:?} (scaled={:.1})",
            w.unwrap_or(0) as f64 * 1000.0 * scale as f64
        );
    }

    // Check cff.glyph_index(39)
    let gix = cff.glyph_index(39u8);
    println!("cff.glyph_index(39) = {gix:?}");
}

/// Test cff.glyph_index on gen-319 Helvetica after decompression.
#[test]
#[ignore]
fn debug_gen319_cff_glyph_index() {
    use lopdf::Object;

    let data = std::fs::read("/tmp/gen-319_319905.pdf").unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    // Find FontFile3 stream object ID for NNHDHP+Helvetica
    let mut ff3_id: Option<lopdf::ObjectId> = None;
    for (id, obj) in &doc.objects {
        if let Object::Dictionary(dict) = obj {
            let st = dict
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
            if st != "Type1" {
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
            if !base.contains("Helvetica") {
                continue;
            }
            if let Some(Object::Reference(fd_ref)) = dict.get(b"FontDescriptor").ok() {
                if let Some(Object::Dictionary(fd)) = doc.objects.get(fd_ref) {
                    if let Some(Object::Reference(ff3_ref)) = fd.get(b"FontFile3").ok() {
                        ff3_id = Some(*ff3_ref);
                    }
                }
            }
        }
    }

    let ff3_id = ff3_id.expect("FontFile3 not found");
    println!("FontFile3 id: {ff3_id:?}");

    // Decompress stream
    let font_data = if let Some(Object::Stream(s)) = doc.objects.get_mut(&ff3_id) {
        let _ = s.decompress();
        println!("Raw content len before decompress: {}", s.content.len());
        s.content.clone()
    } else {
        panic!("FontFile3 not a stream");
    };

    println!("Font data len after decompress: {}", font_data.len());
    println!("First 4 bytes: {:?}", &font_data[..4.min(font_data.len())]);

    if let Some(cff) = cff_parser::Table::parse(&font_data) {
        let n = cff.number_of_glyphs();
        println!("num_glyphs={n}");
        for gid in 0..n {
            let g = cff_parser::GlyphId(gid);
            let name = cff.glyph_name(g);
            let w = cff.glyph_width(g);
            println!("  GID {gid}: name={name:?} w={w:?}");
        }
        let gix39 = cff.glyph_index(39u8);
        println!("cff.glyph_index(39) = {gix39:?}");
        let scale = cff.matrix().sx as f64;
        if let Some(gid) = gix39 {
            let w = cff.glyph_width(gid);
            println!(
                "  width at gid={gid:?}: {w:?} scaled={:.1}",
                w.unwrap_or(0) as f64 * 1000.0 * scale
            );
        }
    } else {
        println!("CFF parse failed");
    }
}

/// Check Flags of Helvetica FontDescriptor.
#[test]
#[ignore]
fn debug_gen319_helvetica_flags() {
    use lopdf::Object;

    let data = std::fs::read("/tmp/gen-319_319905.pdf").unwrap();
    let doc = lopdf::Document::load_mem(&data).unwrap();

    for (_id, obj) in &doc.objects {
        if let Object::Dictionary(dict) = obj {
            let st = dict
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
            if st != "Type1" {
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
            if !base.contains("Helvetica") {
                continue;
            }
            if let Some(Object::Reference(fd_ref)) = dict.get(b"FontDescriptor").ok() {
                if let Some(Object::Dictionary(fd)) = doc.objects.get(fd_ref) {
                    let flags = fd.get(b"Flags").ok().and_then(|o| {
                        if let Object::Integer(i) = o {
                            Some(*i)
                        } else {
                            None
                        }
                    });
                    println!(
                        "Flags={flags:?} bit18(262144)={}",
                        flags.map(|f| (f & 262144) != 0).unwrap_or(false)
                    );
                }
            }
        }
    }
}

/// Trace why fix_font_width_mismatches doesn't fix code 39 for gen-319.
#[test]
#[ignore]
fn debug_gen319_fix_trace() {
    use lopdf::Object;

    let data = std::fs::read("/tmp/gen-319_319905.pdf").unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    // Run pipeline up to before fix_font_width_mismatches
    let _ = pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false);
    let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_pfb_font_streams(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_type1_stub_font_files(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_mislabeled_truetype_as_cff(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_truetype_with_cff_program(&mut doc);
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

    // Now inspect the Helvetica font before fix_font_width_mismatches
    println!("=== Helvetica font state before fix_font_width_mismatches ===");
    for (id, obj) in &doc.objects {
        if let Object::Dictionary(dict) = obj {
            let st = dict
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
            if st != "Type1" {
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
            if !base.contains("Helvetica") {
                continue;
            }
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
            let enc = dict
                .get(b"Encoding")
                .ok()
                .map(|o| match o {
                    Object::Name(n) => String::from_utf8_lossy(n).to_string(),
                    Object::Dictionary(_) => "dict".to_string(),
                    _ => "other".to_string(),
                })
                .unwrap_or("none".to_string());
            let widths_obj = dict
                .get(b"Widths")
                .ok()
                .map(|o| match o {
                    Object::Array(arr) => format!(
                        "array[{}] code39={:?}",
                        arr.len(),
                        arr.get((39 - fc) as usize)
                    ),
                    Object::Reference(r) => format!("ref({} {})", r.0, r.1),
                    _ => "other".to_string(),
                })
                .unwrap_or("none".to_string());
            println!("  Font {id:?} {base} FC={fc} Enc={enc:?} Widths={widths_obj}");

            // Check FontDescriptor
            if let Some(Object::Reference(fd_ref)) = dict.get(b"FontDescriptor").ok() {
                if let Some(Object::Dictionary(fd)) = doc.objects.get(fd_ref) {
                    let ff3 = fd.get(b"FontFile3").is_ok();
                    let ff2 = fd.get(b"FontFile2").is_ok();
                    let ff1 = fd.get(b"FontFile").is_ok();
                    let flags = fd.get(b"Flags").ok().and_then(|o| {
                        if let Object::Integer(i) = o {
                            Some(*i)
                        } else {
                            None
                        }
                    });
                    println!("  FD {fd_ref:?}: ff1={ff1} ff2={ff2} ff3={ff3} Flags={flags:?}");
                }
            }
        }
    }
}

/// Check encoding dict structure for gen-319 Helvetica.
#[test]
#[ignore]
fn debug_gen319_enc_dict() {
    use lopdf::Object;

    let data = std::fs::read("/tmp/gen-319_319905.pdf").unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    let _ = pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false);
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

    for (font_id, obj) in &doc.objects {
        if let Object::Dictionary(dict) = obj {
            let st = dict
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
            if st != "Type1" {
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
            if !base.contains("Helvetica") {
                continue;
            }
            println!("Font {font_id:?} {base}");
            match dict.get(b"Encoding").ok() {
                Some(Object::Name(n)) => {
                    println!("  Encoding: Name={:?}", String::from_utf8_lossy(n))
                }
                Some(Object::Dictionary(ed)) => {
                    let base_enc = ed
                        .get(b"BaseEncoding")
                        .ok()
                        .and_then(|o| {
                            if let Object::Name(n) = o {
                                Some(String::from_utf8_lossy(n).to_string())
                            } else {
                                None
                            }
                        })
                        .unwrap_or("(none)".to_string());
                    println!("  Encoding: Dict BaseEncoding={base_enc:?}");
                    if let Some(Object::Array(diffs)) = ed.get(b"Differences").ok() {
                        println!(
                            "  Differences[{}]: {:?}",
                            diffs.len(),
                            &diffs[..diffs.len().min(20)]
                        );
                    }
                }
                Some(Object::Reference(r)) => {
                    println!("  Encoding: Ref({r:?})");
                    if let Ok(Object::Dictionary(ed)) = doc.get_object(*r) {
                        let base_enc = ed
                            .get(b"BaseEncoding")
                            .ok()
                            .and_then(|o| {
                                if let Object::Name(n) = o {
                                    Some(String::from_utf8_lossy(n).to_string())
                                } else {
                                    None
                                }
                            })
                            .unwrap_or("(none)".to_string());
                        println!("    BaseEncoding={base_enc:?}");
                    }
                }
                _ => println!("  Encoding: none/other"),
            }
        }
    }
}

/// Direct trace of object (49,0) Widths[39] through pipeline for gen-319.
#[test]
#[ignore]
fn debug_gen319_direct_obj49() {
    use lopdf::Object;

    fn get_code39_at_49(doc: &lopdf::Document) -> String {
        match doc.objects.get(&(49, 0)) {
            Some(Object::Dictionary(dict)) => {
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
                    .unwrap_or(0);
                let idx = (39 - fc) as usize;
                match dict.get(b"Widths").ok() {
                    Some(Object::Array(ws)) => ws
                        .get(idx)
                        .map(|o| format!("{o:?}"))
                        .unwrap_or("out_of_bounds".to_string()),
                    Some(Object::Reference(r)) => match doc.objects.get(r) {
                        Some(Object::Array(ws)) => ws
                            .get(idx)
                            .map(|o| format!("{o:?}"))
                            .unwrap_or("oob_ref".to_string()),
                        _ => format!("ref({r:?})->?"),
                    },
                    Some(other) => format!("other:{other:?}"),
                    None => "no_widths".to_string(),
                }
            }
            _ => "no_obj".to_string(),
        }
    }

    let data = std::fs::read("/tmp/gen-319_319905.pdf").unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();
    println!("Initial (49,0)[39]: {}", get_code39_at_49(&doc));
    let _ = pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false);
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
    println!(
        "After fix_missing_simple_font_widths (49,0)[39]: {}",
        get_code39_at_49(&doc)
    );
    let _ = pdf_manip::pdfa_fonts::fix_type3_font_widths(&mut doc);
    println!(
        "Before fix_font_width_mismatches (49,0)[39]: {}",
        get_code39_at_49(&doc)
    );
    let n = pdf_manip::pdfa_fonts::fix_font_width_mismatches(&mut doc);
    println!(
        "After fix_font_width_mismatches (fixed={n}) (49,0)[39]: {}",
        get_code39_at_49(&doc)
    );
    println!(
        "Object (49,0) still font? {:?}",
        doc.objects
            .get(&(49, 0))
            .map(|o| matches!(o, Object::Dictionary(_)))
    );
}

/// Trace CFF width computation for code 39 in NNHDHP+Helvetica.
/// Directly verifies that find_cff_glyph_width_by_name_fractional finds "quoteright".
#[test]
#[ignore]
fn debug_gen319_cff_trace() {
    use lopdf::Object;

    let data = std::fs::read("/tmp/gen-319_319905.pdf").unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    // Find FontFile3 stream for NNHDHP+Helvetica
    let mut ff3_id: Option<lopdf::ObjectId> = None;
    for (_id, obj) in &doc.objects {
        if let Object::Dictionary(dict) = obj {
            let st = dict
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
            if st != "Type1" {
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
            if !base.contains("Helvetica") {
                continue;
            }
            if let Some(Object::Reference(fd_ref)) = dict.get(b"FontDescriptor").ok() {
                if let Some(Object::Dictionary(fd)) = doc.objects.get(fd_ref) {
                    if let Some(Object::Reference(ff3_ref)) = fd.get(b"FontFile3").ok() {
                        ff3_id = Some(*ff3_ref);
                    }
                }
            }
        }
    }
    let ff3_id = ff3_id.expect("FontFile3 not found");

    // Decompress CFF stream
    let font_data = if let Some(Object::Stream(s)) = doc.objects.get_mut(&ff3_id) {
        let _ = s.decompress();
        s.content.clone()
    } else {
        panic!("FontFile3 not a stream");
    };

    // Check if ttf_parser succeeds on raw CFF bytes
    let ttf_ok = ttf_parser::Face::parse(&font_data, 0).is_ok();
    println!("ttf_parser::Face::parse succeeds: {ttf_ok}");

    // Parse CFF and look for "quotesingle" and "quoteright"
    let cff = cff_parser::Table::parse(&font_data).expect("CFF parse failed");
    let scale = {
        let sx = cff.matrix().sx;
        let raw = sx as f64 * 1000.0;
        (raw * 1_000_000.0).round() / 1_000_000.0
    };
    println!("cff_matrix_scale={scale}");

    for name in &["'", "quotesingle", "quoteright"] {
        let w: Option<f64> = {
            let n = cff.number_of_glyphs();
            (0..n).find_map(|gid_raw| {
                let gid = cff_parser::GlyphId(gid_raw);
                if cff.glyph_name(gid).as_deref() == Some(name) {
                    cff.glyph_width(gid).map(|w| w as f64 * scale)
                } else {
                    None
                }
            })
        };
        println!("find_by_name({name:?}) => {w:?}");
    }

    // Simulate cff_width_for_code for code 39, enc="WinAnsiEncoding", diffs={}
    let code39_glyph_index = cff.glyph_index(39u8);
    println!("glyph_index(39) = {code39_glyph_index:?}");
    if let Some(gid) = code39_glyph_index {
        let w = cff.glyph_width(gid).map(|w| w as f64 * scale);
        println!(
            "  => width = {w:?}  rounded = {:?}",
            w.map(|x| x.round() as i64)
        );
    }
}

/// Check Helvetica font data after all preprocessing pipeline steps.
#[test]
#[ignore]
fn debug_gen319_after_preprocess() {
    use lopdf::Object;

    let data = std::fs::read("/tmp/gen-319_319905.pdf").unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    let _ = pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false);
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

    // Collect needed IDs first (avoid borrow conflict)
    let (enc_desc, fc, lc, w39_val, ff3_id_opt) = {
        match doc.objects.get(&(49, 0)) {
            Some(Object::Dictionary(dict)) => {
                let enc_desc = match dict.get(b"Encoding").ok() {
                    Some(Object::Name(n)) => format!("Name({})", String::from_utf8_lossy(n)),
                    Some(Object::Dictionary(_)) => "Dict".to_string(),
                    Some(Object::Reference(r)) => format!("Ref({r:?})"),
                    other => format!("{other:?}"),
                };
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
                let w39_val = match dict.get(b"Widths").ok() {
                    Some(Object::Array(ws)) => ws
                        .get((39 - fc) as usize)
                        .map(|o| format!("{o:?}"))
                        .unwrap_or("oob".to_string()),
                    other => format!("{other:?}"),
                };
                let ff3_id_opt: Option<lopdf::ObjectId> = (|| {
                    let fd_ref = dict.get(b"FontDescriptor").ok()?.as_reference().ok()?;
                    let fd = doc.objects.get(&fd_ref)?.as_dict().ok()?;
                    Some(fd.get(b"FontFile3").ok()?.as_reference().ok()?.clone())
                })();
                (enc_desc, fc, lc, w39_val, ff3_id_opt)
            }
            _ => ("not_a_dict".to_string(), -1, -1, "no_obj".to_string(), None),
        }
    };
    println!("(49,0): FC={fc} LC={lc} Enc={enc_desc} Widths[39]={w39_val}");
    // Print Encoding dict details
    if let Some(Object::Dictionary(dict)) = doc.objects.get(&(49, 0)) {
        if let Some(Object::Dictionary(enc_dict)) = dict.get(b"Encoding").ok() {
            let base = enc_dict.get(b"BaseEncoding").ok().and_then(|o| {
                if let Object::Name(n) = o {
                    Some(String::from_utf8_lossy(n).to_string())
                } else {
                    None
                }
            });
            println!("  Encoding.BaseEncoding={base:?}");
            if let Some(Object::Array(diffs)) = enc_dict.get(b"Differences").ok() {
                let mut code: u32 = 0;
                let mut entries = Vec::new();
                for obj in diffs {
                    match obj {
                        Object::Integer(i) => {
                            code = *i as u32;
                        }
                        Object::Name(n) => {
                            let name = String::from_utf8_lossy(n).to_string();
                            if (37..=41).contains(&code) {
                                entries.push(format!("code{code}={name}"));
                            }
                            code += 1;
                        }
                        _ => {}
                    }
                }
                println!("  Encoding.Differences[codes 37-41]: {entries:?}");
            }
        }
    }

    if let Some(ff3_id) = ff3_id_opt {
        if let Some(Object::Stream(s)) = doc.objects.get_mut(&ff3_id) {
            let _ = s.decompress();
            let font_data = s.content.clone();
            println!(
                "  FontFile3 len={} first4={:?}",
                font_data.len(),
                &font_data[..4.min(font_data.len())]
            );
            let ttf_ok = ttf_parser::Face::parse(&font_data, 0).is_ok();
            println!("  ttf_parser success={ttf_ok}");
            if let Some(cff) = cff_parser::Table::parse(&font_data) {
                let sx = cff.matrix().sx;
                let scale = (sx as f64 * 1000.0 * 1_000_000.0).round() / 1_000_000.0;
                println!("  CFF scale={scale}");
                let n = cff.number_of_glyphs();
                let qr = (0..n).find_map(|g| {
                    let gid = cff_parser::GlyphId(g);
                    if cff.glyph_name(gid).as_deref() == Some("quoteright") {
                        cff.glyph_width(gid).map(|w| w as f64 * scale)
                    } else {
                        None
                    }
                });
                println!("  quoteright width={qr:?}");
                let gix39 = cff.glyph_index(39u8);
                let w39 = gix39
                    .and_then(|g| cff.glyph_width(g))
                    .map(|w| w as f64 * scale);
                println!("  glyph_index(39)={gix39:?} width={w39:?}");
            } else {
                println!("  CFF parse failed");
            }
        }
    }
}

/// Minimal test: run ONLY fix_missing_simple_font_widths then fix_font_width_mismatches on gen-319.
#[test]
#[ignore]
fn debug_gen319_minimal_fix() {
    use lopdf::Object;

    // Load pre-processed state: apply all steps up to fix_missing_simple_font_widths
    let data = std::fs::read("/tmp/gen-319_319905.pdf").unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    let _ = pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false);
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

    // Print state of ALL Type1/TrueType fonts at code 39
    println!("=== Before fix_font_width_mismatches ===");
    for (id, obj) in &doc.objects {
        if let Object::Dictionary(dict) = obj {
            let st = dict
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
            if !matches!(st.as_str(), "Type1" | "TrueType") {
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
            if fc > 39 || lc < 39 {
                continue;
            }
            let idx = (39 - fc) as usize;
            let widths_info = match dict.get(b"Widths").ok() {
                Some(Object::Array(ws)) => {
                    format!("inline[{}] code39={:?}", ws.len(), ws.get(idx))
                }
                Some(Object::Reference(r)) => match doc.objects.get(r) {
                    Some(Object::Array(ws)) => {
                        format!("ref[{}] code39={:?}", ws.len(), ws.get(idx))
                    }
                    _ => format!("ref({r:?})->?"),
                },
                _ => "no_widths".to_string(),
            };
            println!("  {id:?} {st} {base}: FC={fc} LC={lc} Widths: {widths_info}");
        }
    }

    let n = pdf_manip::pdfa_fonts::fix_font_width_mismatches(&mut doc);
    println!("\n=== After fix_font_width_mismatches (fixed={n}) ===");
    for (id, obj) in &doc.objects {
        if let Object::Dictionary(dict) = obj {
            let st = dict
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
            if !matches!(st.as_str(), "Type1" | "TrueType") {
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
            if fc > 39 || lc < 39 {
                continue;
            }
            let idx = (39 - fc) as usize;
            let widths_info = match dict.get(b"Widths").ok() {
                Some(Object::Array(ws)) => {
                    format!("inline[{}] code39={:?}", ws.len(), ws.get(idx))
                }
                Some(Object::Reference(r)) => match doc.objects.get(r) {
                    Some(Object::Array(ws)) => {
                        format!("ref[{}] code39={:?}", ws.len(), ws.get(idx))
                    }
                    _ => format!("ref({r:?})->?"),
                },
                _ => "no_widths".to_string(),
            };
            println!("  {id:?} {st} {base}: FC={fc} LC={lc} Widths: {widths_info}");
        }
    }
}

#[test]
#[ignore]
fn debug_gen319_full_pipeline() {
    use lopdf::Object;
    use pdf_manip::pdfa_xmp::PdfAConformance;

    let data = std::fs::read("/tmp/gen-319_319905.pdf").unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).unwrap();
    pdf_manip::pdfa_fonts::promote_inline_font_dicts(&mut doc);
    let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_pfb_font_streams(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_type1_stub_font_files(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_mislabeled_truetype_as_cff(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_truetype_with_cff_program(&mut doc);
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
    pdf_manip::pdfa_fonts::strip_control_chars_from_streams(&mut doc);
    pdf_manip::pdfa_fonts::fix_undefined_encoding_codes(&mut doc);
    pdf_manip::pdfa_fonts::fix_symbolic_flags(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_classic_symbolic_base14_encoding(&mut doc);
    pdf_manip::pdfa_fonts::fix_missing_simple_font_widths(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_type3_font_widths(&mut doc);

    // Helper: dump encoding info + width at a given code for a font dict
    let dump_font_at_code = |dict: &lopdf::Dictionary,
                             objects: &std::collections::BTreeMap<lopdf::ObjectId, Object>,
                             check_code: i64| {
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
        let enc_entry = if let Some(Object::Dictionary(enc_dict)) = dict.get(b"Encoding").ok() {
            let base = enc_dict
                .get(b"BaseEncoding")
                .ok()
                .and_then(|o| {
                    if let Object::Name(n) = o {
                        Some(String::from_utf8_lossy(n).to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            if let Some(Object::Array(diffs)) = enc_dict.get(b"Differences").ok() {
                let mut code: i64 = 0;
                let mut found = String::new();
                for d in diffs {
                    match d {
                        Object::Integer(n) => code = *n,
                        Object::Name(n) => {
                            if code == check_code {
                                found = String::from_utf8_lossy(n).to_string();
                            }
                            code += 1;
                        }
                        _ => {}
                    }
                }
                format!(
                    "Base={base} Diffs[{check_code}]={}",
                    if found.is_empty() { "(none)" } else { &found }
                )
            } else {
                format!("EncDict(Base={base},no Diffs)")
            }
        } else if let Some(Object::Name(n)) = dict.get(b"Encoding").ok() {
            format!("Enc={}", String::from_utf8_lossy(n))
        } else {
            "no Encoding".to_string()
        };
        let widths_entry = if fc <= check_code && lc >= check_code {
            let idx = (check_code - fc) as usize;
            match dict.get(b"Widths").ok() {
                Some(Object::Array(ws)) => format!("W[{check_code}]={:?}", ws.get(idx)),
                Some(Object::Reference(r)) => match objects.get(r) {
                    Some(Object::Array(ws)) => format!("W[{check_code}]=ref:{:?}", ws.get(idx)),
                    _ => format!("W[{check_code}]=ref->?"),
                },
                _ => "no_widths".to_string(),
            }
        } else {
            format!("({check_code} outside FC={fc}..LC={lc})")
        };
        (enc_entry, widths_entry)
    };

    println!("=== Before fix_font_width_mismatches ===");
    for (id, obj) in &doc.objects {
        if let Object::Dictionary(dict) = obj {
            let st = dict
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
            if !matches!(st.as_str(), "Type1" | "TrueType") {
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
            if base.contains("Times") {
                let (enc, w) = dump_font_at_code(dict, &doc.objects, 146);
                println!("  {id:?} {st} {base} | {enc} | {w}");
            } else if base.contains("Helvetica") {
                let (enc, w) = dump_font_at_code(dict, &doc.objects, 39);
                println!("  {id:?} {st} {base} | {enc} | {w}");
            }
        }
    }

    let n = pdf_manip::pdfa_fonts::fix_font_width_mismatches(&mut doc);
    println!("fix_font_width_mismatches fixed={n}");

    // Dump after fix
    println!("=== After fix_font_width_mismatches ===");
    for (id, obj) in &doc.objects {
        if let Object::Dictionary(dict) = obj {
            let st = dict
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
            if !matches!(st.as_str(), "Type1" | "TrueType") {
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
            if base.contains("Times") {
                let (_, w) = dump_font_at_code(dict, &doc.objects, 146);
                println!("  {id:?} {st} {base} | {w}");
            } else if base.contains("Helvetica") {
                let (_, w) = dump_font_at_code(dict, &doc.objects, 39);
                println!("  {id:?} {st} {base} | {w}");
            }
        }
    }

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

    std::fs::write("/tmp/gen-319-full-converted.pdf", &saved).unwrap();
    println!(
        "Saved {} bytes to /tmp/gen-319-full-converted.pdf",
        saved.len()
    );
}

#[test]
#[ignore]
fn debug_gen698_save_converted() {
    use pdf_manip::pdfa_xmp::PdfAConformance;

    let data = std::fs::read("/tmp/gen-698_698323.pdf").unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).unwrap();
    pdf_manip::pdfa_fonts::promote_inline_font_dicts(&mut doc);
    let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_pfb_font_streams(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_type1_stub_font_files(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_mislabeled_truetype_as_cff(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_truetype_with_cff_program(&mut doc);
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
    pdf_manip::pdfa_fonts::strip_control_chars_from_streams(&mut doc);
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

    std::fs::write("/tmp/gen-698-converted-new.pdf", &saved).unwrap();
    println!("Saved {} bytes", saved.len());
}

#[test]
#[ignore]
fn debug_gen152_save_converted() {
    use pdf_manip::pdfa_xmp::PdfAConformance;

    let data = std::fs::read("/tmp/gen-152_152696.pdf").unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).unwrap();
    pdf_manip::pdfa_fonts::promote_inline_font_dicts(&mut doc);
    let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_pfb_font_streams(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_type1_stub_font_files(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_mislabeled_truetype_as_cff(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_truetype_with_cff_program(&mut doc);
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
    pdf_manip::pdfa_fonts::strip_control_chars_from_streams(&mut doc);
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

    std::fs::write("/tmp/gen-152-converted-new.pdf", &saved).unwrap();
    println!("Saved {} bytes", saved.len());
}

#[test]
#[ignore]
fn debug_gen698_font_inspect() {
    use lopdf::Object;

    let data = std::fs::read("/tmp/gen-698_698323.pdf").unwrap();
    let doc = lopdf::Document::load_mem(&data).unwrap();

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
            if !base.contains("HFGEBB") {
                continue;
            }
            let st = dict
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
            let widths_info = match dict.get(b"Widths").ok() {
                Some(Object::Array(ws)) => {
                    let w32 = if fc <= 32 && lc >= 32 {
                        format!("{:?}", ws.get((32 - fc) as usize))
                    } else {
                        "(32 out)".into()
                    };
                    format!("len={} W[32]={}", ws.len(), w32)
                }
                Some(Object::Reference(r)) => format!("ref={r:?}"),
                _ => "none".into(),
            };
            let enc = dict
                .get(b"Encoding")
                .ok()
                .map(|o| format!("{o:?}"))
                .unwrap_or_default();
            println!("{id:?} {st} {base}: FC={fc} LC={lc} {widths_info}");
            println!("   enc: {enc}");
        }
    }
}

#[test]
#[ignore]
fn debug_gen698_width_trace() {
    use lopdf::Object;

    let data = std::fs::read("/tmp/gen-698_698323.pdf").unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).unwrap();
    pdf_manip::pdfa_fonts::promote_inline_font_dicts(&mut doc);
    let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_pfb_font_streams(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_type1_stub_font_files(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_mislabeled_truetype_as_cff(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_truetype_with_cff_program(&mut doc);
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
    pdf_manip::pdfa_fonts::strip_control_chars_from_streams(&mut doc);
    pdf_manip::pdfa_fonts::fix_undefined_encoding_codes(&mut doc);
    pdf_manip::pdfa_fonts::fix_symbolic_flags(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_classic_symbolic_base14_encoding(&mut doc);
    pdf_manip::pdfa_fonts::fix_missing_simple_font_widths(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_type3_font_widths(&mut doc);

    // Check HFGEBB+TimesNewRoman widths BEFORE fix_font_width_mismatches
    println!("=== HFGEBB+TimesNewRoman BEFORE fix_font_width_mismatches ===");
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
            if !base.contains("HFGEBB") {
                continue;
            }
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
            let w32 = if fc <= 32 && lc >= 32 {
                let idx = (32 - fc) as usize;
                match dict.get(b"Widths").ok() {
                    Some(Object::Array(ws)) => format!("{:?}", ws.get(idx)),
                    _ => "?".into(),
                }
            } else {
                format!("(32 outside FC={fc}..LC={lc})")
            };
            println!("  {id:?} {base}: FC={fc} LC={lc} W[32]={w32}");
        }
    }

    pdf_manip::pdfa_fonts::fix_font_width_mismatches(&mut doc);

    println!("=== HFGEBB+TimesNewRoman AFTER fix_font_width_mismatches ===");
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
            if !base.contains("HFGEBB") {
                continue;
            }
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
            let w32 = if fc <= 32 && lc >= 32 {
                let idx = (32 - fc) as usize;
                match dict.get(b"Widths").ok() {
                    Some(Object::Array(ws)) => {
                        // Check for any huge values in the array
                        let huge: Vec<_> = ws
                            .iter()
                            .enumerate()
                            .filter(|(_, o)| matches!(o, Object::Integer(v) if v.abs() > 32767))
                            .map(|(i, o)| (fc + i as i64, o))
                            .collect();
                        if !huge.is_empty() {
                            println!("  HUGE VALUES: {:?}", huge);
                        }
                        format!("{:?}", ws.get(idx))
                    }
                    _ => "?".into(),
                }
            } else {
                format!("(32 outside FC={fc}..LC={lc})")
            };
            println!("  {id:?} {base}: FC={fc} LC={lc} W[32]={w32}");
        }
    }
}

#[test]
#[ignore]
fn debug_gen698_cff_charset() {
    use lopdf::Object;

    let data = std::fs::read("/tmp/gen-698_698323.pdf").unwrap();
    let doc = lopdf::Document::load_mem(&data).unwrap();

    // Find HFGEBB+TimesNewRoman FontFile3 data
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
            if !base.contains("HFGEBB") {
                continue;
            }

            let fd_ref = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(r)) => *r,
                _ => continue,
            };
            let ff3_ref = match doc.objects.get(&fd_ref) {
                Some(Object::Dictionary(fd)) => match fd.get(b"FontFile3").ok() {
                    Some(Object::Reference(r)) => *r,
                    _ => {
                        println!("{id:?} no FontFile3");
                        continue;
                    }
                },
                _ => continue,
            };

            let font_data = match doc.objects.get(&ff3_ref) {
                Some(Object::Stream(stream)) => stream.decompressed_content().unwrap_or_default(),
                _ => {
                    println!("{id:?} not a stream");
                    continue;
                }
            };

            let Some(cff) = cff_parser::Table::parse(&font_data) else {
                println!("{id:?} CFF parse failed");
                continue;
            };

            println!("{id:?} {base}: {} glyphs", cff.number_of_glyphs());
            let mut has_space = false;
            for gid in 0..cff.number_of_glyphs() {
                let gid = cff_parser::GlyphId(gid);
                let name = cff.glyph_name(gid);
                let w = cff.glyph_width(gid);
                if name == Some("space") || name == Some(".notdef") {
                    println!("  GID={} name={:?} width={:?}", gid.0, name, w);
                    if name == Some("space") {
                        has_space = true;
                    }
                }
            }
            if !has_space {
                println!("  ** space NOT in charset");
            }
            // Check CFF encoding for code 32
            if let Some(gid32) = cff.glyph_index(32) {
                println!(
                    "  CFF encoding[32] -> GID {} name={:?}",
                    gid32.0,
                    cff.glyph_name(gid32)
                );
            } else {
                println!("  CFF encoding[32] -> None (code 32 not encoded)");
            }
            break; // just first one
        }
    }
}

#[test]
#[ignore]
fn debug_gen698_encoding() {
    use lopdf::Object;

    let data = std::fs::read("/tmp/gen-698_698323.pdf").unwrap();
    let doc = lopdf::Document::load_mem(&data).unwrap();

    // Encoding object is (165, 0)
    if let Some(Object::Dictionary(enc)) = doc.objects.get(&(165u32, 0u16).into()) {
        println!(
            "Encoding (165,0) keys: {:?}",
            enc.iter()
                .map(|(k, _)| String::from_utf8_lossy(k).to_string())
                .collect::<Vec<_>>()
        );
        if let Ok(base) = enc.get(b"BaseEncoding") {
            println!("  BaseEncoding: {base:?}");
        }
        if let Ok(diffs) = enc.get(b"Differences") {
            println!("  Differences: {diffs:?}");
        }
    }

    // Also inspect the CONVERTED PDF to see if we add differences for code 32
    let data2 = std::fs::read("/tmp/gen-698-converted-new.pdf").unwrap();
    let doc2 = lopdf::Document::load_mem(&data2).unwrap();
    // Find font (168,0) encoding after conversion
    for (id, obj) in &doc2.objects {
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
            if !base.contains("HFGEBB") {
                continue;
            }
            let enc = dict
                .get(b"Encoding")
                .ok()
                .map(|o| format!("{o:?}"))
                .unwrap_or_default();
            println!("converted {id:?} {base}: enc={enc}");
            if let Ok(Object::Reference(enc_ref)) = dict.get(b"Encoding") {
                if let Some(Object::Dictionary(enc_dict)) = doc2.objects.get(enc_ref) {
                    let diffs = enc_dict
                        .get(b"Differences")
                        .ok()
                        .map(|o| format!("{o:?}"))
                        .unwrap_or_default();
                    println!("  Differences: {diffs}");
                }
            }
            break;
        }
    }
}

#[test]
#[ignore]
fn debug_gen698_font_desc() {
    use lopdf::Object;

    let data = std::fs::read("/tmp/gen-698_698323.pdf").unwrap();
    let doc = lopdf::Document::load_mem(&data).unwrap();

    // Check objects 168, 172, 183 — the known HFGEBB font dicts
    for oid in [(168u32, 0u16), (172, 0), (183, 0)] {
        let id = lopdf::ObjectId::from(oid);
        if let Some(Object::Dictionary(dict)) = doc.objects.get(&id) {
            println!(
                "Object {id:?} keys: {:?}",
                dict.iter()
                    .map(|(k, _)| String::from_utf8_lossy(k).to_string())
                    .collect::<Vec<_>>()
            );
            if let Ok(fd) = dict.get(b"FontDescriptor") {
                println!("  FontDescriptor: {fd:?}");
                if let Object::Reference(fd_ref) = fd {
                    if let Some(Object::Dictionary(fd_dict)) = doc.objects.get(fd_ref) {
                        println!(
                            "  FD keys: {:?}",
                            fd_dict
                                .iter()
                                .map(|(k, _)| String::from_utf8_lossy(k).to_string())
                                .collect::<Vec<_>>()
                        );
                        if let Ok(ff3) = fd_dict.get(b"FontFile3") {
                            println!("  FontFile3: {ff3:?}");
                            if let Object::Reference(ff3_ref) = ff3 {
                                if let Some(Object::Stream(stream)) = doc.objects.get(ff3_ref) {
                                    let bytes = stream.decompressed_content().unwrap_or_default();
                                    println!("  CFF bytes: {}", bytes.len());
                                    let Some(cff) = cff_parser::Table::parse(&bytes) else {
                                        println!("  CFF parse failed");
                                        continue;
                                    };
                                    println!("  CFF glyphs: {}", cff.number_of_glyphs());
                                    for gid in 0..cff.number_of_glyphs() {
                                        let gid = cff_parser::GlyphId(gid);
                                        let name = cff.glyph_name(gid);
                                        let w = cff.glyph_width(gid);
                                        println!("    GID={} name={:?} width={:?}", gid.0, name, w);
                                    }
                                    if let Some(gid32) = cff.glyph_index(32) {
                                        println!(
                                            "  CFF encoding[32]->GID{} name={:?}",
                                            gid32.0,
                                            cff.glyph_name(gid32)
                                        );
                                    } else {
                                        println!("  CFF encoding[32]->None");
                                    }
                                }
                            }
                        } else {
                            println!("  No FontFile3 in FD");
                        }
                    }
                }
            } else {
                println!("  No FontDescriptor key");
            }
        } else {
            println!("Object {id:?} not found or not dict");
        }
    }
}

#[test]
#[ignore]
fn debug_gen152_find_corruption() {
    // Bisect: find which pipeline step introduces the #/?T undefined operators
    use lopdf::Object;

    fn check_content(data: &[u8]) -> bool {
        // Check if content stream object (1,0) contains `# ` or `?T` tokens
        // by running verapdf... but we can't do that here.
        // Instead: save to /tmp and run verapdf externally.
        false
    }

    fn save_state(doc: &lopdf::Document, path: &str) {
        let mut saved = Vec::new();
        doc.clone().save_to(&mut saved).unwrap();
        std::fs::write(path, &saved).unwrap();
    }

    let data = std::fs::read("/tmp/gen-152_152696.pdf").unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).unwrap();
    save_state(&doc, "/tmp/gen152_step00_cleanup.pdf");

    pdf_manip::pdfa_fonts::promote_inline_font_dicts(&mut doc);
    let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_pfb_font_streams(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_type1_stub_font_files(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_mislabeled_truetype_as_cff(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_truetype_with_cff_program(&mut doc);
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
    save_state(&doc, "/tmp/gen152_step01_prestrip.pdf");

    let n = pdf_manip::pdfa_fonts::strip_control_chars_from_streams(&mut doc);
    println!("strip_control_chars_from_streams: {n} modified");
    save_state(&doc, "/tmp/gen152_step02_poststrip.pdf");

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
    save_state(&doc, "/tmp/gen152_step03_prefixups.pdf");

    pdf_manip::pdfa_fixups::run_fixups(&mut doc);
    save_state(&doc, "/tmp/gen152_step04_postfixups.pdf");

    println!("All intermediate states saved to /tmp/gen152_stepXX_*.pdf");
    println!("Run: for f in /tmp/gen152_step*.pdf; do echo -n \"$f: \"; verapdf --flavour 2b $f 2>/dev/null | grep -o 'failedChecks=\"[0-9]*\"' | head -1; done");
}

/// Full pipeline conversion for gen-152.
#[test]
#[ignore]
fn debug_gen152_full_convert() {
    use pdf_manip::pdfa_xmp::PdfAConformance;
    let data = std::fs::read("/tmp/gen-152_152696.pdf").unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).unwrap();
    pdf_manip::pdfa_fonts::promote_inline_font_dicts(&mut doc);
    let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_pfb_font_streams(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_type1_stub_font_files(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_mislabeled_truetype_as_cff(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_truetype_with_cff_program(&mut doc);
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
    let _ = pdf_manip::pdfa_fonts::strip_control_chars_from_streams(&mut doc);
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

    std::fs::write("/tmp/gen152_full_converted.pdf", &saved).unwrap();
    println!(
        "Saved {} bytes to /tmp/gen152_full_converted.pdf",
        saved.len()
    );
}

/// Check if fix_ascii85_inline_images actually modifies the gen-152 content stream.
#[test]
#[ignore]
fn debug_gen152_ascii85_fix() {
    use lopdf::Object;

    let data = std::fs::read("/tmp/gen-152_152696.pdf").unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).unwrap();
    pdf_manip::pdfa_fixups::run_fixups(&mut doc);

    // Find content stream object 1 0
    let stream_data = match doc.objects.get(&(1, 0)) {
        Some(Object::Stream(s)) => s
            .decompressed_content()
            .unwrap_or_else(|_| s.content.clone()),
        _ => panic!("object (1,0) not found or not a stream"),
    };

    println!("Stream length: {}", stream_data.len());

    // Count BI blocks
    let mut bi_count = 0;
    let mut a85_count = 0;
    let mut i = 0;
    while i + 1 < stream_data.len() {
        if &stream_data[i..i + 2] == b"BI"
            && (i == 0 || stream_data[i - 1].is_ascii_whitespace())
            && stream_data[i + 2].is_ascii_whitespace()
        {
            // look for /F or /Filter in dict
            let dict_end = stream_data[i + 2..]
                .windows(3)
                .position(|w| w[0].is_ascii_whitespace() && &w[1..3] == b"ID");
            let dict_bytes = if let Some(end) = dict_end {
                &stream_data[i + 2..i + 2 + end]
            } else {
                &stream_data[i + 2..i + 200.min(stream_data.len() - i - 2)]
            };
            bi_count += 1;
            let has_a85 = dict_bytes.windows(3).any(|w| w == b"A85");
            if has_a85 {
                a85_count += 1;
            }
            println!(
                "BI at {i}: has_a85={has_a85}, dict[..80]={:?}",
                &dict_bytes[..dict_bytes.len().min(80)]
            );
        }
        i += 1;
    }
    println!("Total BI: {bi_count}, with A85: {a85_count}");

    // Save modified doc
    let mut saved = Vec::new();
    doc.clone().save_to(&mut saved).unwrap();
    std::fs::write("/tmp/gen152_ascii85_fixed.pdf", &saved).unwrap();
    println!("Saved to /tmp/gen152_ascii85_fixed.pdf");
}

#[test]
#[ignore]
fn debug_gen698_charset() {
    let data = std::fs::read("/tmp/gen-698_698323-converted.pdf").unwrap();
    let pdf = pdf_syntax::Pdf::new(data).unwrap();

    // Validate at A2b and show 6.2.11.4.1 issues
    let report = pdf_compliance::validate_pdfa(&pdf, pdf_compliance::PdfALevel::A2b);
    println!(
        "Compliant: {} | Issues: {}",
        report.compliant,
        report.issues.len()
    );
    for issue in &report.issues {
        if issue.rule.contains("6.2.11.4") || issue.rule.contains("6.3.5") {
            println!(
                "  CHARSET: {} {}",
                issue.rule,
                &issue.message[..issue.message.len().min(120)]
            );
        }
    }
    println!("--- all issues ---");
    for issue in &report.issues {
        println!(
            "  [{}] {}: {}",
            issue.severity as u8,
            issue.rule,
            &issue.message[..issue.message.len().min(100)]
        );
    }
}

#[test]
#[ignore]
fn debug_gen698_original_charset() {
    // Check the ORIGINAL (unconverted) gen-698 for charset violations
    let data = std::fs::read("/tmp/gen-698_698323.pdf").unwrap();
    let pdf = pdf_syntax::Pdf::new(data).unwrap();
    let report = pdf_compliance::validate_pdfa(&pdf, pdf_compliance::PdfALevel::A2b);
    println!(
        "Original gen-698 | Compliant: {} | Issues: {}",
        report.compliant,
        report.issues.len()
    );
    for issue in &report.issues {
        println!(
            "  [{}] {}: {}",
            issue.severity as u8,
            issue.rule,
            &issue.message[..issue.message.len().min(120)]
        );
    }
}

#[test]
#[ignore]
fn debug_gen152_oi_trace() {
    // Trace exactly why §6.2.2 doesn't fire for gen-152.
    use lopdf::Object;
    let data = std::fs::read("/tmp/gen-152_152696-converted.pdf").unwrap();
    let doc = lopdf::Document::load_mem(&data).unwrap();

    // Check via pdf-syntax: can it find the OutputIntent?
    let pdf = pdf_syntax::Pdf::new(data.clone()).unwrap();
    use pdf_syntax::object::{Array, Dict, Name, Stream};

    let xref = pdf.xref();
    let root_id = xref.root_id();
    println!("Root id: {:?}", root_id);

    // Get catalog
    let cat_opt: Option<Dict<'_>> = xref.get::<Dict<'_>>(root_id);
    if cat_opt.is_none() {
        println!("Could not get Catalog as Dict!");
    }

    if let Some(cat) = cat_opt {
        let has_oi_key = cat.contains_key(b"OutputIntents" as &[u8]);
        println!("Catalog has OutputIntents key: {}", has_oi_key);

        if let Some(intents) = cat.get::<Array<'_>>(b"OutputIntents" as &[u8]) {
            println!("OutputIntents array len: {}", intents.data().len());
            for dict in intents.iter::<Dict<'_>>() {
                let s = dict
                    .get::<Name>(b"S" as &[u8])
                    .map(|n| String::from_utf8_lossy(n.as_ref()).to_string())
                    .unwrap_or_else(|| "(none)".to_string());
                let has_dest = dict.contains_key(b"DestOutputProfile" as &[u8]);
                println!("  OutputIntent: S={} has_DestOutputProfile={}", s, has_dest);
                if let Some(stream) = dict.get::<Stream<'_>>(b"DestOutputProfile" as &[u8]) {
                    let decoded = stream.decoded();
                    println!(
                        "  DestOutputProfile decoded: {:?} bytes",
                        decoded.as_ref().map(|d| d.len())
                    );
                } else {
                    println!("  DestOutputProfile: can't get as Stream (returns None)");
                    // Try to get the raw ref
                    // Check the ICC stream (object 40,0) via lopdf
                    let icc_id: lopdf::ObjectId = (40u32, 0u16).into();
                    if let Some(Object::Stream(stream)) = doc.objects.get(&icc_id) {
                        let bytes = stream.decompressed_content().unwrap_or_default();
                        println!("  lopdf ICC (40,0) bytes: {}", bytes.len());
                        println!("  lopdf raw stream len: {}", stream.content.len());
                        let keys: Vec<_> = stream
                            .dict
                            .iter()
                            .map(|(k, _)| String::from_utf8_lossy(k).to_string())
                            .collect();
                        println!("  lopdf stream dict keys: {:?}", keys);
                    }
                }
            }
        } else {
            println!("Could not get OutputIntents as Array!");
        }
    }
}

#[test]
#[ignore]
fn debug_gen152_outputintent() {
    // Check OutputIntent in converted gen-152 to investigate §6.2.2 FN.
    use lopdf::Object;

    let data = std::fs::read("/tmp/gen-152_152696-converted.pdf").unwrap();
    let doc = lopdf::Document::load_mem(&data).unwrap();

    // Find Catalog
    let catalog_ref = doc.trailer.get(b"Root").ok().and_then(|o| {
        if let Object::Reference(r) = o {
            Some(*r)
        } else {
            None
        }
    });
    println!("Root ref: {:?}", catalog_ref);

    if let Some(root_id) = catalog_ref {
        if let Some(Object::Dictionary(catalog)) = doc.objects.get(&root_id) {
            // Find OutputIntents
            let oi = catalog
                .get(b"OutputIntents")
                .ok()
                .map(|o| format!("{o:?}"))
                .unwrap_or_else(|| "(none)".to_string());
            println!("OutputIntents: {}", &oi[..oi.len().min(200)]);

            // Get the actual OutputIntents array
            if let Ok(Object::Array(oi_arr)) = catalog.get(b"OutputIntents") {
                println!("OutputIntents count: {}", oi_arr.len());
                for (i, item) in oi_arr.iter().enumerate() {
                    let oi_dict = match item {
                        Object::Dictionary(d) => Some(d.clone()),
                        Object::Reference(r) => {
                            if let Some(Object::Dictionary(d)) = doc.objects.get(r) {
                                Some(d.clone())
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };
                    if let Some(dict) = oi_dict {
                        let s = dict
                            .get(b"S")
                            .ok()
                            .map(|o| format!("{o:?}"))
                            .unwrap_or_default();
                        let icc_ref = dict
                            .get(b"DestOutputProfile")
                            .ok()
                            .map(|o| format!("{o:?}"))
                            .unwrap_or_default();
                        println!(
                            "  OutputIntent[{}]: S={} DestOutputProfile={}",
                            i, s, icc_ref
                        );
                        // Check ICC profile
                        if let Ok(Object::Reference(icc_r)) = dict.get(b"DestOutputProfile") {
                            if let Some(Object::Stream(stream)) = doc.objects.get(icc_r) {
                                let bytes = stream.decompressed_content().unwrap_or_default();
                                println!("    ICC bytes: {}", bytes.len());
                                if bytes.len() >= 36 {
                                    let sig = &bytes[36..40];
                                    println!("    ICC signature: {:?}", std::str::from_utf8(sig));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Also run our checker specifically for OutputIntent
    let pdf = pdf_syntax::Pdf::new(data).unwrap();
    let report = pdf_compliance::validate_pdfa(&pdf, pdf_compliance::PdfALevel::A2b);
    let oi_issues: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.rule.contains("6.2.2") || i.rule.contains("6.2.3"))
        .collect();
    println!("\nOur §6.2.2/6.2.3 issues: {}", oi_issues.len());
    for issue in &oi_issues {
        println!(
            "  [{}] {}: {}",
            issue.rule,
            issue.severity as u8,
            &issue.message[..issue.message.len().min(100)]
        );
    }
}

#[test]
#[ignore]
fn debug_gen152_resources() {
    // Deep inspection of gen-152 resources to find §6.2.2 source.
    use lopdf::Object;

    let data = std::fs::read("/tmp/gen-152_152696-converted.pdf").unwrap();
    let pdf = pdf_syntax::Pdf::new(data.clone()).unwrap();
    let doc = lopdf::Document::load_mem(&data).unwrap();

    // Check all pages for explicit /Resources
    println!("=== Page resources check ===");
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let has_resources = page_dict.contains_key(b"Resources" as &[u8]);
        let n_fonts = page.resources().fonts.entries().count();
        let n_xobjs = page.resources().x_objects.entries().count();
        println!(
            "Page {}: has_Resources={} fonts={} xobjs={}",
            page_idx + 1,
            has_resources,
            n_fonts,
            n_xobjs
        );
    }

    // Check all Form XObjects in the document
    println!("\n=== Form XObjects check ===");
    for (oid, obj) in &doc.objects {
        let Object::Stream(stream) = obj else {
            continue;
        };
        let subtype = stream.dict.get(b"Subtype").ok().and_then(|o| {
            if let Object::Name(n) = o {
                Some(n.clone())
            } else {
                None
            }
        });
        if subtype.as_deref() != Some(b"Form") {
            continue;
        }
        let has_resources = stream.dict.get(b"Resources").is_ok();
        let bytes = stream.decompressed_content().unwrap_or_default();
        let text = String::from_utf8_lossy(&bytes);
        let uses_tf = text.contains("Tf");
        let uses_do = text.contains(" Do");
        println!(
            "Form XObj {:?}: has_Resources={} uses_Tf={} uses_Do={} len={}",
            oid,
            has_resources,
            uses_tf,
            uses_do,
            bytes.len()
        );
        if !has_resources && (uses_tf || uses_do) {
            println!("  ** VIOLATION: uses resources but no /Resources dict **");
            // Show first 200 bytes of content
            println!("  Content: {}", &text.chars().take(200).collect::<String>());
        }
    }
}

#[test]
#[ignore]
fn debug_gen152_annots() {
    // Inspect annotation appearances in converted gen-152 to find §6.2.2 source.
    use lopdf::Object;

    let data = std::fs::read("/tmp/gen-152_152696-converted.pdf").unwrap();
    let doc = lopdf::Document::load_mem(&data).unwrap();

    println!("=== Annotation appearances in gen-152 ===");
    let mut annot_count = 0;
    for (oid, obj) in &doc.objects {
        let Object::Dictionary(dict) = obj else {
            continue;
        };
        let has_rect = dict.get(b"Rect").is_ok();
        let subtype = dict.get(b"Subtype").ok().and_then(|o| {
            if let Object::Name(n) = o {
                Some(String::from_utf8_lossy(n).to_string())
            } else {
                None
            }
        });
        if !has_rect || subtype.is_none() {
            continue;
        }
        annot_count += 1;
        println!(
            "Annot {:?}: Subtype={}",
            oid,
            subtype.as_deref().unwrap_or("?")
        );
        // Check AP entry
        if let Ok(Object::Dictionary(ap_dict)) = dict.get(b"AP") {
            for (ap_key, ap_obj) in ap_dict.iter() {
                let stream_ref = match ap_obj {
                    Object::Reference(r) => Some(*r),
                    _ => None,
                };
                if let Some(r) = stream_ref {
                    if let Some(Object::Stream(stream)) = doc.objects.get(&r) {
                        let has_res = stream.dict.get(b"Resources").is_ok();
                        println!(
                            "  AP/{}: ref={:?} has_Resources={}",
                            String::from_utf8_lossy(ap_key),
                            r,
                            has_res
                        );
                        if !has_res {
                            let decoded = stream.decompressed_content().unwrap_or_default();
                            // Check if content uses any resource operators
                            let text = String::from_utf8_lossy(&decoded);
                            let has_tf = text.contains(" Tf") || text.contains("\nTf");
                            let has_do = text.contains(" Do") || text.contains("\nDo");
                            println!(
                                "    content: has_Tf={} has_Do={} len={}",
                                has_tf,
                                has_do,
                                decoded.len()
                            );
                        }
                    }
                }
            }
        }
    }
    println!("Total annotations found: {}", annot_count);
}

#[test]
#[ignore]
fn debug_gen698_charset_trace() {
    // Trace what check_type1_charset_coverage sees for gen-698 pages 16 and 21.
    // Find what codes are used with HFNCFP+Arial-Bold and HFNCEN+Swis721BT-Bold.
    let data = std::fs::read("/tmp/gen-698_698323-converted.pdf").unwrap();
    let pdf = pdf_syntax::Pdf::new(data).unwrap();
    let target_bases = ["HFNCFP+Arial-Bold", "HFNCEN+Swis721BT-Bold"];

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let fonts = &page.resources().fonts;
        // Find target font resource names on this page
        let mut target_rnames: Vec<(Vec<u8>, String)> = Vec::new();
        for (rname, _) in fonts.entries() {
            if let Some(fd) = fonts.get::<pdf_syntax::object::Dict<'_>>(rname.as_ref()) {
                let base = fd
                    .get::<pdf_syntax::object::Name>(b"BaseFont" as &[u8])
                    .map(|n| String::from_utf8_lossy(n.as_ref()).to_string())
                    .unwrap_or_default();
                if target_bases.iter().any(|t| base == *t) {
                    target_rnames.push((rname.as_ref().to_vec(), base.clone()));
                    println!(
                        "Page {}: font {} = {}",
                        page_idx + 1,
                        String::from_utf8_lossy(rname.as_ref()),
                        base
                    );
                }
            }
        }
        if target_rnames.is_empty() {
            continue;
        }

        // Now scan the content stream for text operators using these fonts
        let Some(content) = page.page_stream() else {
            continue;
        };
        let tokens: Vec<Vec<u8>> = {
            let mut result = Vec::new();
            let mut i = 0;
            let bytes = content;
            while i < bytes.len() {
                // skip whitespace
                while i < bytes.len()
                    && (bytes[i] == b' '
                        || bytes[i] == b'\n'
                        || bytes[i] == b'\r'
                        || bytes[i] == b'\t')
                {
                    i += 1;
                }
                if i >= bytes.len() {
                    break;
                }
                // read token (simple split)
                let start = i;
                while i < bytes.len()
                    && bytes[i] != b' '
                    && bytes[i] != b'\n'
                    && bytes[i] != b'\r'
                    && bytes[i] != b'\t'
                {
                    i += 1;
                }
                if start < i {
                    result.push(bytes[start..i].to_vec());
                }
            }
            result
        };

        let mut active_font: Option<&str> = None;
        for (i, tok) in tokens.iter().enumerate() {
            if tok == b"Tf" && i >= 2 {
                let rname = tokens[i - 2].as_slice();
                if let Some(name_bytes) = rname.strip_prefix(b"/") {
                    let fname = std::str::from_utf8(name_bytes).unwrap_or("");
                    if target_rnames.iter().any(|(r, _)| r == name_bytes) {
                        let base = target_rnames
                            .iter()
                            .find(|(r, _)| r == name_bytes)
                            .map(|(_, b)| b.as_str())
                            .unwrap_or("");
                        active_font = Some(base);
                        println!(
                            "  Page {}: activated {} = {} at token {}",
                            page_idx + 1,
                            fname,
                            base,
                            i
                        );
                    } else {
                        active_font = None;
                    }
                }
            }
            if active_font.is_some() && (tok == b"Tj" || tok == b"'" || tok == b"\"") && i >= 1 {
                let stok = &tokens[i - 1];
                println!(
                    "  Page {}: {:?} {:?} -> codes: {:?}",
                    page_idx + 1,
                    tok,
                    String::from_utf8_lossy(stok),
                    stok.iter()
                        .filter(|&&b| b >= 32)
                        .copied()
                        .collect::<Vec<_>>()
                );
            }
        }
    }
}

#[test]
#[ignore]
fn debug_gen698_page_font_resources() {
    // Check whether Type1 subset fonts appear in page resources or inherited from parent.
    // Diagnosis for §6.2.11.4.1 FN: if fonts are only in parent node, check_type1_charset_coverage
    // may iterate an empty page.resources().fonts dict.
    let data = std::fs::read("/tmp/gen-698_698323-converted.pdf").unwrap();
    let pdf = pdf_syntax::Pdf::new(data).unwrap();
    let target_fonts = ["HFNCFP+Arial-Bold", "HFNCEN+Swis721BT-Bold"];

    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let fonts = &page.resources().fonts;
        let entries: Vec<String> = fonts
            .entries()
            .filter_map(|(k, _)| {
                let font = fonts.get::<pdf_syntax::object::Dict<'_>>(k.as_ref())?;
                let base = font
                    .get::<pdf_syntax::object::Name>(b"BaseFont" as &[u8])
                    .map(|n| String::from_utf8_lossy(n.as_ref()).to_string())
                    .unwrap_or_default();
                if target_fonts.iter().any(|t| base.contains(&t[7..])) {
                    Some(format!("{}={}", String::from_utf8_lossy(k.as_ref()), base))
                } else {
                    None
                }
            })
            .collect();
        if !entries.is_empty() {
            println!("Page {}: {:?}", page_idx + 1, entries);
        }
        // Also check entry count
        let n_entries = fonts.entries().count();
        let n_target = entries.len();
        if n_target > 0 || n_entries > 0 {
            println!(
                "  page {} has {} fonts total, {} target fonts in OWN resources",
                page_idx + 1,
                n_entries,
                n_target
            );
        }
    }
    println!("Done checking pages");
}

#[test]
#[ignore]
fn debug_gen698_type1_fonts_converted() {
    // Inspect all Type1 subset fonts in the converted gen-698 PDF.
    // Helps diagnose §6.2.11.4.1 FN: find what encoding each font uses,
    // what the CFF glyph set is, and what content codes are used.
    use lopdf::Object;

    let data = std::fs::read("/tmp/gen-698_698323-converted.pdf").unwrap();
    let doc = lopdf::Document::load_mem(&data).unwrap();

    println!("=== Type1 subset fonts in converted gen-698 ===");
    let mut found = false;
    for (oid, obj) in &doc.objects {
        let Object::Dictionary(dict) = obj else {
            continue;
        };
        let Ok(Object::Name(stype)) = dict.get(b"Subtype") else {
            continue;
        };
        if stype != b"Type1" {
            continue;
        }
        let base = match dict.get(b"BaseFont") {
            Ok(Object::Name(n)) => String::from_utf8_lossy(n).to_string(),
            _ => continue,
        };
        // Only subset fonts (ABCDEF+ prefix)
        let bytes = base.as_bytes();
        if bytes.len() < 7 || bytes[6] != b'+' {
            continue;
        }
        found = true;
        println!("\n--- Font {:?}: {} ---", oid, base);

        // Encoding
        let enc = dict.get(b"Encoding").ok();
        match enc {
            Some(Object::Name(n)) => println!("  Encoding: Name({})", String::from_utf8_lossy(n)),
            Some(Object::Reference(r)) => {
                println!("  Encoding: Reference({:?})", r);
                if let Some(Object::Dictionary(enc_dict)) = doc.objects.get(r) {
                    let base_enc = enc_dict
                        .get(b"BaseEncoding")
                        .ok()
                        .map(|o| format!("{o:?}"))
                        .unwrap_or_default();
                    println!("    BaseEncoding: {base_enc}");
                    if let Ok(Object::Array(diffs)) = enc_dict.get(b"Differences") {
                        println!("    Differences[..20]: {:?}", &diffs[..diffs.len().min(20)]);
                    }
                }
            }
            Some(o) => println!("  Encoding: {o:?}"),
            None => println!("  Encoding: (none)"),
        }

        // CharSet
        let charset = dict.get(b"CharSet").ok().or_else(|| {
            // Try FontDescriptor for CharSet
            if let Ok(Object::Reference(fd_ref)) = dict.get(b"FontDescriptor") {
                if let Some(Object::Dictionary(fd)) = doc.objects.get(fd_ref) {
                    return fd.get(b"CharSet").ok();
                }
            }
            None
        });
        match charset {
            Some(Object::String(s, _)) => println!("  CharSet: {}", String::from_utf8_lossy(s)),
            _ => println!("  CharSet: (none or other type)"),
        }

        // FontDescriptor + CFF
        if let Ok(Object::Reference(fd_ref)) = dict.get(b"FontDescriptor") {
            if let Some(Object::Dictionary(fd)) = doc.objects.get(fd_ref) {
                if let Ok(Object::Reference(ff3_ref)) = fd.get(b"FontFile3") {
                    if let Some(Object::Stream(stream)) = doc.objects.get(ff3_ref) {
                        let bytes = stream.decompressed_content().unwrap_or_default();
                        println!("  CFF bytes: {}", bytes.len());
                        if let Some(cff) = cff_parser::Table::parse(&bytes) {
                            println!("  CFF nGlyphs: {}", cff.number_of_glyphs());
                            for gid in 0..cff.number_of_glyphs() {
                                let gid = cff_parser::GlyphId(gid);
                                println!("    GID={} name={:?}", gid.0, cff.glyph_name(gid));
                            }
                        } else {
                            println!("  CFF parse failed");
                        }
                    }
                }
            }
        }
    }
    if !found {
        println!("No Type1 subset fonts found in converted PDF");
    }
}

/// Verify §6.7.3.5 fix: /Info /Keywords → XMP pdf:Keywords.
/// Verify §6.1.7.1 fix: no /F in non-EmbeddedFile stream dicts after conversion.
#[test]
#[ignore]
fn debug_gen698_converter_fixes() {
    use lopdf::Object;
    use pdf_manip::pdfa_xmp::PdfAConformance;

    let data = std::fs::read("/tmp/gen-698_698323.pdf").expect("gen-698 not found in /tmp");
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    // Check /Info keywords before conversion
    let info_keywords = {
        if let Some(Object::Reference(iid)) = doc.trailer.get(b"Info").ok().cloned() {
            if let Some(Object::Dictionary(info)) = doc.objects.get(&iid) {
                if let Ok(Object::String(kw, _)) = info.get(b"Keywords") {
                    Some(String::from_utf8_lossy(kw).to_string())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    };
    println!("Source /Info Keywords: {:?}", info_keywords);

    pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).unwrap();
    let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);
    pdf_manip::pdfa_fonts::fix_cff_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_truetype_cid_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_type1_charset(&mut doc);
    pdf_manip::pdfa_fonts::fix_truetype_encoding(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_existing_symbolic_truetype_cmaps(&mut doc);
    pdf_manip::pdfa_fonts::fix_truetype_unicode_cmap(&mut doc);
    pdf_manip::pdfa_fonts::fix_notdef_glyph_refs(&mut doc);
    pdf_manip::pdfa_fonts::fix_cid_font_notdef(&mut doc);
    pdf_manip::pdfa_fonts::fix_symbolic_font_notdef_streams(&mut doc);
    pdf_manip::pdfa_fonts::fix_undefined_encoding_codes(&mut doc);
    pdf_manip::pdfa_fonts::fix_symbolic_flags(&mut doc);
    pdf_manip::pdfa_fonts::fix_missing_simple_font_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_font_width_mismatches(&mut doc);
    pdf_manip::pdfa_fonts::fix_symbolic_font_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_cidset(&mut doc);
    pdf_manip::pdfa_colorspace::normalize_colorspaces(&mut doc).unwrap();
    pdf_manip::pdfa_fixups::run_fixups(&mut doc);
    pdf_manip::pdfa_xmp::repair_xmp_metadata(&mut doc, PdfAConformance::A2b, None).unwrap();

    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    pdf_manip::pdfa_cleanup::fix_pdf_header(&mut saved);
    pdf_manip::pdfa_cleanup::fix_startxref(&mut saved);

    // Check XMP contains pdf:Keywords
    let doc2 = lopdf::Document::load_mem(&saved).unwrap();
    let catalog_id = match doc2.trailer.get(b"Root").ok().cloned() {
        Some(Object::Reference(id)) => id,
        _ => panic!("no catalog"),
    };
    if let Some(Object::Dictionary(cat)) = doc2.objects.get(&catalog_id) {
        if let Ok(Object::Reference(mid)) = cat.get(b"Metadata") {
            if let Some(Object::Stream(ms)) = doc2.objects.get(mid) {
                let xmp = String::from_utf8_lossy(&ms.content);
                let has_keywords = xmp.contains("pdf:Keywords") || xmp.contains("<pdf:Keywords>");
                println!("XMP has pdf:Keywords: {has_keywords}");
                if info_keywords.is_some() {
                    assert!(
                        has_keywords,
                        "pdf:Keywords must be in XMP when /Info has Keywords"
                    );
                }
            }
        }
    }

    // Check no /F in non-EmbeddedFile stream dicts
    let mut f_violations = 0usize;
    for (_id, obj) in &doc2.objects {
        if let Object::Stream(s) = obj {
            let is_ef = matches!(
                s.dict.get(b"Type").ok(),
                Some(Object::Name(ref n)) if n == b"EmbeddedFile"
            );
            if !is_ef && s.dict.has(b"F") {
                f_violations += 1;
                println!("  Stream with /F: {:?}", _id);
            }
        }
    }
    println!("§6.1.7.1 /F violations in converted PDF: {f_violations}");
    assert_eq!(
        f_violations, 0,
        "No /F in non-EmbeddedFile stream dicts after conversion"
    );

    std::fs::write("/tmp/gen-698-converted-fixed.pdf", &saved).unwrap();
    println!("Saved converted PDF to /tmp/gen-698-converted-fixed.pdf");
}

/// Bisect which pipeline step introduces the §6.1.7.1 /F violation in gen-698.
#[test]
#[ignore]
fn debug_gen698_bisect_f_violation() {
    use lopdf::Object;
    use pdf_manip::pdfa_xmp::PdfAConformance;

    fn count_f_violations(doc: &lopdf::Document) -> usize {
        let mut count = 0;
        for (id, obj) in &doc.objects {
            if let Object::Stream(s) = obj {
                let is_ef = matches!(
                    s.dict.get(b"Type").ok(),
                    Some(Object::Name(ref n)) if n == b"EmbeddedFile"
                );
                if !is_ef && s.dict.has(b"F") {
                    println!("  Stream {:?} has /F: {:?}", id, s.dict.get(b"F").ok());
                    count += 1;
                }
            }
        }
        count
    }

    let data = std::fs::read("/tmp/gen-698_698323.pdf").expect("gen-698 not found");
    let mut doc = lopdf::Document::load_mem(&data).unwrap();
    println!("After load: {} /F violations", count_f_violations(&doc));

    pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).unwrap();
    println!("After cleanup: {} /F violations", count_f_violations(&doc));

    let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);
    println!(
        "After embed_fonts: {} /F violations",
        count_f_violations(&doc)
    );

    pdf_manip::pdfa_fonts::fix_cff_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_truetype_cid_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_type1_charset(&mut doc);
    pdf_manip::pdfa_fonts::fix_truetype_encoding(&mut doc);
    println!(
        "After font fixes 1: {} /F violations",
        count_f_violations(&doc)
    );

    let _ = pdf_manip::pdfa_fonts::fix_existing_symbolic_truetype_cmaps(&mut doc);
    pdf_manip::pdfa_fonts::fix_truetype_unicode_cmap(&mut doc);
    pdf_manip::pdfa_fonts::fix_notdef_glyph_refs(&mut doc);
    pdf_manip::pdfa_fonts::fix_cid_font_notdef(&mut doc);
    pdf_manip::pdfa_fonts::fix_symbolic_font_notdef_streams(&mut doc);
    pdf_manip::pdfa_fonts::fix_undefined_encoding_codes(&mut doc);
    pdf_manip::pdfa_fonts::fix_symbolic_flags(&mut doc);
    pdf_manip::pdfa_fonts::fix_missing_simple_font_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_font_width_mismatches(&mut doc);
    pdf_manip::pdfa_fonts::fix_symbolic_font_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_cidset(&mut doc);
    println!(
        "After font fixes 2: {} /F violations",
        count_f_violations(&doc)
    );

    pdf_manip::pdfa_colorspace::normalize_colorspaces(&mut doc).unwrap();
    println!(
        "After normalize_colorspaces: {} /F violations",
        count_f_violations(&doc)
    );

    pdf_manip::pdfa_fixups::run_fixups(&mut doc);
    println!(
        "After run_fixups: {} /F violations",
        count_f_violations(&doc)
    );

    pdf_manip::pdfa_xmp::repair_xmp_metadata(&mut doc, PdfAConformance::A2b, None).unwrap();
    println!(
        "After repair_xmp: {} /F violations",
        count_f_violations(&doc)
    );

    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    let doc2 = lopdf::Document::load_mem(&saved).unwrap();
    println!(
        "After save+reload: {} /F violations",
        count_f_violations(&doc2)
    );
}

#[test]
#[ignore]
fn debug_gen152_check_622() {
    // Analyze the converted gen-152 PDF to understand §6.2.2 violation
    use pdf_syntax::Pdf;

    let data = std::fs::read("/tmp/gen-152_152696-converted.pdf").unwrap();
    let pdf = Pdf::new(data.clone()).unwrap();
    let level = pdf_compliance::detect_pdfa_level(&pdf).unwrap();
    let report = pdf_compliance::validate_pdfa(&pdf, level);
    println!("=== Compliance results for converted gen-152 (level={level:?}) ===");
    println!("Compliant: {}", report.is_compliant());
    println!("Issues ({}):", report.issues.len());
    for e in &report.issues {
        println!(
            "  [{:?}] [{}] {} at {:?}",
            e.severity, e.rule, e.message, e.location
        );
    }
    println!();

    // Also check resource dict details
    for (page_idx, page) in pdf.pages().iter().enumerate() {
        let res = page.resources();
        println!("Page {} resources:", page_idx + 1);
        let fonts: Vec<_> = res
            .fonts
            .entries()
            .map(|(n, _)| String::from_utf8_lossy(n.as_ref()).to_string())
            .collect();
        println!("  Fonts ({}): {:?}", fonts.len(), fonts);
        let xobjs: Vec<_> = res
            .x_objects
            .entries()
            .map(|(n, _)| String::from_utf8_lossy(n.as_ref()).to_string())
            .collect();
        println!("  XObjects ({}): {:?}", xobjs.len(), xobjs);
        let props: Vec<_> = res
            .properties
            .entries()
            .map(|(n, _)| String::from_utf8_lossy(n.as_ref()).to_string())
            .collect();
        println!("  Properties ({}): {:?}", props.len(), props);
        // Check page dict for own /Resources key
        let page_dict = page.raw();
        println!(
            "  Page has own /Resources: {}",
            page_dict.contains_key(b"Resources" as &[u8])
        );
    }
}

#[test]
#[ignore]
fn debug_gen152_long_string_location() {
    // Find exactly where in the decoded content stream the "long string" FP is detected.
    use pdf_syntax::Pdf;

    let data = std::fs::read("/tmp/gen-152_152696-converted.pdf").unwrap();
    let pdf = Pdf::new(data).unwrap();
    let page = pdf.pages().first().unwrap();
    let content = page.page_stream().unwrap().to_vec();
    println!("Decoded content stream length: {}", content.len());

    // Find all 'ID' and 'EI' keyword positions (whitespace-delimited)
    let mut id_positions = vec![];
    let mut ei_positions = vec![];
    let len = content.len();
    let is_ws = |b: u8| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\x0C');
    for i in 0..len.saturating_sub(1) {
        let pre = if i == 0 { true } else { is_ws(content[i - 1]) };
        let post = if i + 2 >= len {
            true
        } else {
            is_ws(content[i + 2])
        };
        if content[i] == b'I' && content[i + 1] == b'D' && pre && post {
            id_positions.push(i);
        }
        if content[i] == b'E' && content[i + 1] == b'I' && pre && post {
            ei_positions.push(i);
        }
    }
    println!(
        "ID positions (ws-delimited): {:?}",
        &id_positions[..id_positions.len().min(10)]
    );
    println!(
        "EI positions (ws-delimited, first 20): {:?}",
        &ei_positions[..ei_positions.len().min(20)]
    );

    // Find first '(' that starts a string > 32767 bytes
    let mut pos = 0usize;
    while pos < len {
        if content[pos] == b'(' {
            let start = pos;
            let mut depth: i32 = 1;
            let mut decoded = 0usize;
            pos += 1;
            while pos < len && depth > 0 {
                match content[pos] {
                    b'\\' => {
                        pos += 2;
                        decoded += 1;
                    }
                    b'(' => {
                        depth += 1;
                        pos += 1;
                        decoded += 1;
                    }
                    b')' => {
                        depth -= 1;
                        if depth > 0 {
                            decoded += 1;
                        }
                        pos += 1;
                    }
                    _ => {
                        decoded += 1;
                        pos += 1;
                    }
                }
                if decoded > 32767 {
                    break;
                }
            }
            if decoded > 32767 {
                println!(
                    "Long string at offset {}: decoded {} bytes, depth={}",
                    start, decoded, depth
                );
                println!(
                    "Bytes around start: {:?}",
                    &content[start.saturating_sub(20)..start.min(len)]
                );
                // Print context before
                let ctx_start = start.saturating_sub(100);
                let ctx: Vec<u8> = content[ctx_start..start]
                    .iter()
                    .map(|&b| {
                        if b.is_ascii_graphic() || b == b' ' {
                            b
                        } else {
                            b'.'
                        }
                    })
                    .collect();
                println!(
                    "Context before (100 bytes): {}",
                    String::from_utf8_lossy(&ctx)
                );
                break;
            }
        } else {
            pos += 1;
        }
    }
}

/// Convert isartor-6-3-5 through the full pipeline and validate with veraPDF.
/// Verifies fix for tt_glyph_has_data false-negative for space glyph (#504).
#[test]
#[ignore]
fn debug_isartor635_convert() {
    use pdf_manip::pdfa_xmp::PdfAConformance;
    use pdf_syntax::Pdf;

    let path = "/tmp/isartor-6-3-5.pdf";
    if !std::path::Path::new(path).exists() {
        println!("SKIP: {path} not found");
        return;
    }

    let data = std::fs::read(path).unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    let _ = pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false);
    let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_pfb_font_streams(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_type1_stub_font_files(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_mislabeled_truetype_as_cff(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_truetype_with_cff_program(&mut doc);
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
    let _ = pdf_manip::pdfa_colorspace::normalize_colorspaces(&mut doc);
    pdf_manip::pdfa_fixups::run_fixups(&mut doc);
    let _ = pdf_manip::pdfa_xmp::repair_xmp_metadata(&mut doc, PdfAConformance::A2b, None);

    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    pdf_manip::pdfa_cleanup::fix_pdf_header(&mut saved);
    pdf_manip::pdfa_cleanup::fix_startxref(&mut saved);

    let out = "/tmp/isartor-6-3-5-converted.pdf";
    std::fs::write(out, &saved).unwrap();
    println!("Saved to {out}");

    // Validate with own checker
    match Pdf::new(saved.clone()) {
        Ok(pdf) => {
            let report = pdf_compliance::validate_pdfa(&pdf, pdf_compliance::PdfALevel::A2b);
            println!(
                "Own checker: compliant={} errors={} warnings={}",
                report.compliant,
                report.error_count(),
                report.warning_count()
            );
            for issue in report.issues.iter().take(20) {
                println!("  [{:?}] {}: {}", issue.severity, issue.rule, issue.message);
            }
        }
        Err(e) => println!("Pdf::new failed: {e:?}"),
    }

    // Validate with veraPDF if available
    let verapdf = std::path::Path::new("/Users/jasperdewinter/verapdf/verapdf");
    if verapdf.exists() {
        let result = std::process::Command::new("bash")
            .arg("-c")
            .arg(format!(
                "/Users/jasperdewinter/verapdf/verapdf --format mrr --flavour 2b {out} 2>/dev/null | grep -o 'failedChecks=\"[0-9]*\"'"
            ))
            .output();
        match result {
            Ok(out) => println!(
                "veraPDF failedChecks: {}",
                String::from_utf8_lossy(&out.stdout).trim()
            ),
            Err(e) => println!("veraPDF error: {e}"),
        }
    }
}

/// Convert all 8 pdfa_fails PDFs and validate each with veraPDF.
/// Regression test for issue #504.
#[test]
#[ignore]
fn debug_pdfa_fails_all_convert() {
    use pdf_manip::pdfa_xmp::PdfAConformance;

    fn run_pipeline(data: &[u8], conformance: PdfAConformance) -> Vec<u8> {
        let mut doc = lopdf::Document::load_mem(data).unwrap();
        let _ = pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false);
        let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);
        let _ = pdf_manip::pdfa_fonts::fix_pfb_font_streams(&mut doc);
        let _ = pdf_manip::pdfa_fonts::fix_type1_stub_font_files(&mut doc);
        let _ = pdf_manip::pdfa_fonts::fix_mislabeled_truetype_as_cff(&mut doc);
        let _ = pdf_manip::pdfa_fonts::fix_truetype_with_cff_program(&mut doc);
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
        let _ = pdf_manip::pdfa_colorspace::normalize_colorspaces(&mut doc);
        pdf_manip::pdfa_fixups::run_fixups(&mut doc);
        let _ = pdf_manip::pdfa_xmp::repair_xmp_metadata(&mut doc, conformance, None);
        let mut saved = Vec::new();
        doc.save_to(&mut saved).unwrap();
        pdf_manip::pdfa_cleanup::fix_pdf_header(&mut saved);
        pdf_manip::pdfa_cleanup::fix_startxref(&mut saved);
        saved
    }

    let pdfs: &[(&str, PdfAConformance, &str)] = &[
        (
            "cs-isartor-6-3-4-t01-fail-f.pdf",
            PdfAConformance::A2b,
            "2b",
        ),
        (
            "cs-isartor-6-3-5-t01-fail-d.pdf",
            PdfAConformance::A2b,
            "2b",
        ),
        (
            "cs-veraPDF test suite 6-1-6-2-t01-fail-b.pdf",
            PdfAConformance::A2b,
            "2b",
        ),
        (
            "cs-veraPDF test suite 6-2-10-4-1-t02-fail-a.pdf",
            PdfAConformance::A2b,
            "2b",
        ),
        (
            "cs-veraPDF test suite 6-2-7-1-t01-fail-a.pdf",
            PdfAConformance::A2b,
            "2b",
        ),
        (
            "cs-veraPDF test suite 6-6-2-3-1-t01-fail-c.pdf",
            PdfAConformance::A2b,
            "2b",
        ),
        (
            "tagged-isartor-6-1-13-t01-fail-a.pdf",
            PdfAConformance::A2b,
            "2b",
        ),
        (
            "tagged-veraPDF test suite 6-1-11-t01-fail-a.pdf",
            PdfAConformance::A2b,
            "2b",
        ),
    ];

    let verapdf_bin = "/Users/jasperdewinter/verapdf/verapdf";
    let mut all_pass = true;

    for (name, conformance, flavour) in pdfs {
        let path = format!("/tmp/pdfa_fails/{name}");
        if !std::path::Path::new(&path).exists() {
            println!("{name}: SKIP (not found)");
            continue;
        }
        let data = std::fs::read(&path).unwrap();
        let converted = run_pipeline(&data, *conformance);
        let out = format!("/tmp/{name}-converted.pdf");
        std::fs::write(&out, &converted).unwrap();

        if std::path::Path::new(verapdf_bin).exists() {
            let result = std::process::Command::new("bash")
                .arg("-c")
                .arg(format!(
                    "{verapdf_bin} --format mrr --flavour {flavour} \"{out}\" 2>/dev/null | grep -o 'failedChecks=\"[0-9]*\"'"
                ))
                .output();
            match result {
                Ok(o) => {
                    let s = String::from_utf8_lossy(&o.stdout);
                    let s = s.trim();
                    let pass = s == "failedChecks=\"0\"";
                    println!("{}: {} {}", name, if pass { "PASS" } else { "FAIL" }, s);
                    if !pass {
                        all_pass = false;
                    }
                }
                Err(e) => println!("{name}: veraPDF error {e}"),
            }
        } else {
            println!("{name}: veraPDF not found");
        }
    }

    assert!(all_pass, "Some PDFs failed veraPDF validation");
}

#[test]
#[ignore]
fn debug_gen490_corrections() {
    // Run the full conversion pipeline on gen-490 and trace corrections for code 222
    use lopdf::{Document, Object};
    use pdf_manip::pdfa_xmp::PdfAConformance;

    let data = std::fs::read("/tmp/gen-490_490200.pdf").unwrap();
    let mut doc = Document::load_mem(&data).unwrap();

    pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).unwrap();
    let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);

    // Check font widths for code 222 BEFORE fixing
    fn print_widths_222(label: &str, doc: &Document) {
        for (id, obj) in &doc.objects {
            let dict = match obj {
                Object::Dictionary(d) => d,
                _ => continue,
            };
            let subtype = dict
                .get(b"Subtype")
                .ok()
                .and_then(|o| {
                    if let Object::Name(n) = o {
                        std::str::from_utf8(n).ok().map(str::to_string)
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            if !["Type1", "MMType1"].contains(&subtype.as_str()) {
                continue;
            }
            let base = dict
                .get(b"BaseFont")
                .ok()
                .and_then(|o| {
                    if let Object::Name(n) = o {
                        std::str::from_utf8(n).ok().map(str::to_string)
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
            if !(fc <= 222 && 222 <= lc) {
                continue;
            }
            if let Some(Object::Array(ws)) = dict.get(b"Widths").ok() {
                let idx222 = (222 - fc) as usize;
                let idx223 = (223 - fc) as usize;
                let w222 = ws
                    .get(idx222)
                    .map(|o| format!("{:?}", o))
                    .unwrap_or("N/A".to_string());
                let w223 = ws
                    .get(idx223)
                    .map(|o| format!("{:?}", o))
                    .unwrap_or("N/A".to_string());
                println!("{label}: Font {id:?} {base} code222={w222} code223={w223}");
            }
        }
    }

    print_widths_222("BEFORE fix_cff_widths", &doc);
    let n = pdf_manip::pdfa_fonts::fix_cff_widths(&mut doc);
    println!("fix_cff_widths: {n} fonts fixed");
    print_widths_222("AFTER fix_cff_widths", &doc);

    pdf_manip::pdfa_fonts::fix_truetype_cid_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_type1_charset(&mut doc);
    pdf_manip::pdfa_fonts::fix_truetype_encoding(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_existing_symbolic_truetype_cmaps(&mut doc);
    pdf_manip::pdfa_fonts::fix_truetype_unicode_cmap(&mut doc);
    pdf_manip::pdfa_fonts::fix_notdef_glyph_refs(&mut doc);
    pdf_manip::pdfa_fonts::fix_cid_font_notdef(&mut doc);
    pdf_manip::pdfa_fonts::fix_symbolic_font_notdef_streams(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_simple_font_out_of_range_codes(&mut doc);
    pdf_manip::pdfa_fonts::fix_undefined_encoding_codes(&mut doc);
    pdf_manip::pdfa_fonts::fix_symbolic_flags(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_classic_symbolic_base14_encoding(&mut doc);
    pdf_manip::pdfa_fonts::fix_missing_simple_font_widths(&mut doc);

    let n = pdf_manip::pdfa_fonts::fix_font_width_mismatches(&mut doc);
    println!("fix_font_width_mismatches: {n} fonts fixed");
    print_widths_222("AFTER fix_font_width_mismatches", &doc);
}

#[test]
#[ignore]
fn debug_gen490_simple_check() {
    // Simple raw check on gen-490 before any pipeline steps
    use lopdf::{Document, Object};

    let data = std::fs::read("/tmp/gen-490_490200.pdf").unwrap();
    let doc = Document::load_mem(&data).unwrap();

    let mut count = 0;
    for (id, obj) in &doc.objects {
        let dict = match obj {
            Object::Dictionary(d) => d,
            _ => continue,
        };
        let subtype = dict
            .get(b"Subtype")
            .ok()
            .and_then(|o| {
                if let Object::Name(n) = o {
                    std::str::from_utf8(n).ok().map(str::to_string)
                } else {
                    None
                }
            })
            .unwrap_or_default();
        if subtype.is_empty() {
            continue;
        }
        count += 1;
        let base = dict
            .get(b"BaseFont")
            .ok()
            .and_then(|o| {
                if let Object::Name(n) = o {
                    std::str::from_utf8(n).ok().map(str::to_string)
                } else {
                    None
                }
            })
            .unwrap_or("?".to_string());
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
        println!("Font {id:?}: subtype={subtype:?} base={base:?} fc={fc} lc={lc}");
    }
    println!("Total dicts with Subtype: {count}");
}

#[test]
#[ignore]
fn debug_gen490_after_cleanup() {
    use lopdf::{Document, Object};
    use pdf_manip::pdfa_xmp::PdfAConformance;

    let data = std::fs::read("/tmp/gen-490_490200.pdf").unwrap();
    let mut doc = Document::load_mem(&data).unwrap();

    println!("=== BEFORE cleanup ===");
    let mut found = 0;
    for (id, obj) in &doc.objects {
        let dict = match obj {
            Object::Dictionary(d) => d,
            _ => continue,
        };
        let subtype = dict
            .get(b"Subtype")
            .ok()
            .and_then(|o| {
                if let Object::Name(n) = o {
                    std::str::from_utf8(n).ok().map(str::to_string)
                } else {
                    None
                }
            })
            .unwrap_or_default();
        if !["Type1", "MMType1"].contains(&subtype.as_str()) {
            continue;
        }
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
        if !(fc <= 222 && 222 <= lc) {
            continue;
        }
        found += 1;
        let base = dict
            .get(b"BaseFont")
            .ok()
            .and_then(|o| {
                if let Object::Name(n) = o {
                    std::str::from_utf8(n).ok().map(str::to_string)
                } else {
                    None
                }
            })
            .unwrap_or("?".to_string());
        println!("  Font {id:?}: {base} fc={fc} lc={lc}");
    }
    println!("Found {found} Type1 fonts with code 222");

    pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).unwrap();
    let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);

    println!("\n=== AFTER cleanup+embed_fonts ===");
    found = 0;
    for (id, obj) in &doc.objects {
        let dict = match obj {
            Object::Dictionary(d) => d,
            _ => continue,
        };
        let subtype = dict
            .get(b"Subtype")
            .ok()
            .and_then(|o| {
                if let Object::Name(n) = o {
                    std::str::from_utf8(n).ok().map(str::to_string)
                } else {
                    None
                }
            })
            .unwrap_or_default();
        if !["Type1", "MMType1"].contains(&subtype.as_str()) {
            continue;
        }
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
        let base = dict
            .get(b"BaseFont")
            .ok()
            .and_then(|o| {
                if let Object::Name(n) = o {
                    std::str::from_utf8(n).ok().map(str::to_string)
                } else {
                    None
                }
            })
            .unwrap_or("?".to_string());
        if !(fc <= 222 && 222 <= lc) {
            continue;
        }
        found += 1;
        println!("  Font {id:?}: {base} fc={fc} lc={lc}");
    }
    println!("Found {found} Type1 fonts with code 222");
}

#[test]
#[ignore]
fn debug_gen490_trace_corrections() {
    use lopdf::{Document, Object};
    use pdf_manip::pdfa_xmp::PdfAConformance;

    let data = std::fs::read("/tmp/gen-490_490200.pdf").unwrap();
    let mut doc = Document::load_mem(&data).unwrap();

    pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).unwrap();
    let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);

    // Count all Type1 fonts
    let count: usize = doc
        .objects
        .values()
        .filter(|o| {
            if let Object::Dictionary(d) = o {
                let sub = d
                    .get(b"Subtype")
                    .ok()
                    .and_then(|o| {
                        if let Object::Name(n) = o {
                            std::str::from_utf8(n).ok().map(str::to_string)
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default();
                sub == "Type1"
            } else {
                false
            }
        })
        .count();
    println!("After cleanup: {count} Type1 font dicts");

    // Print first few Type1 fonts
    for (id, obj) in doc.objects.iter().take(200) {
        if let Object::Dictionary(d) = obj {
            let sub = d
                .get(b"Subtype")
                .ok()
                .and_then(|o| {
                    if let Object::Name(n) = o {
                        std::str::from_utf8(n).ok().map(str::to_string)
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            if sub != "Type1" {
                continue;
            }
            let fc = d
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
            let lc = d
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
            let base = d
                .get(b"BaseFont")
                .ok()
                .and_then(|o| {
                    if let Object::Name(n) = o {
                        std::str::from_utf8(n).ok().map(str::to_string)
                    } else {
                        None
                    }
                })
                .unwrap_or("?".to_string());
            println!("  Type1 {id:?}: base={base} fc={fc} lc={lc}");
            if fc <= 222 && 222 <= lc {
                if let Some(Object::Array(ws)) = d.get(b"Widths").ok() {
                    let idx = (222 - fc) as usize;
                    if idx < ws.len() {
                        println!("    code222 width = {:?}", ws[idx]);
                    }
                }
            }
        }
    }
}

#[test]
#[ignore]
fn debug_gen490_widths_ref() {
    use lopdf::{Document, Object};

    let data = std::fs::read("/tmp/gen-490_490200.pdf").unwrap();
    let doc = Document::load_mem(&data).unwrap();

    // Check font (13,0) - VZSLCA+CMBX12 fc=40 lc=222
    let font_id = (13u32, 0u16);
    if let Some(Object::Dictionary(d)) = doc.objects.get(&font_id) {
        println!(
            "Font dict keys: {:?}",
            d.iter()
                .map(|(k, _)| String::from_utf8_lossy(k).to_string())
                .collect::<Vec<_>>()
        );
        if let Some(widths_obj) = d.get(b"Widths").ok() {
            println!("Widths obj type: {:?}", std::mem::discriminant(widths_obj));
            match widths_obj {
                Object::Array(a) => println!("  Direct array len={}", a.len()),
                Object::Reference(r) => {
                    println!("  Indirect ref {:?}", r);
                    if let Some(arr_obj) = doc.objects.get(r) {
                        println!("  Resolved: {:?}", std::mem::discriminant(arr_obj));
                    }
                }
                _ => println!("  Other: {:?}", widths_obj),
            }
        } else {
            println!("No Widths key found");
        }
    }
}

#[test]
#[ignore]
fn debug_gen490_widths_ref2() {
    use lopdf::{Document, Object};

    let data = std::fs::read("/tmp/gen-490_490200.pdf").unwrap();
    let doc = Document::load_mem(&data).unwrap();

    // Find any Type1 fonts with code 222 in range and check Widths
    for (id, obj) in &doc.objects {
        if let Object::Dictionary(d) = obj {
            let sub = d
                .get(b"Subtype")
                .ok()
                .and_then(|o| {
                    if let Object::Name(n) = o {
                        std::str::from_utf8(n).ok().map(str::to_string)
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            if sub != "Type1" {
                continue;
            }
            let fc = d
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
            let lc = d
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
            if !(fc <= 222 && 222 <= lc) {
                continue;
            }

            // Check Widths
            let widths_val = d.get(b"Widths").ok();
            let base = d
                .get(b"BaseFont")
                .ok()
                .and_then(|o| {
                    if let Object::Name(n) = o {
                        std::str::from_utf8(n).ok().map(str::to_string)
                    } else {
                        None
                    }
                })
                .unwrap_or("?".to_string());
            match widths_val {
                None => println!("Font {id:?} {base}: NO Widths key"),
                Some(Object::Array(a)) => {
                    let idx = (222 - fc) as usize;
                    println!(
                        "Font {id:?} {base}: Widths direct array len={}, code222={:?}",
                        a.len(),
                        a.get(idx)
                    );
                }
                Some(Object::Reference(r)) => {
                    println!("Font {id:?} {base}: Widths is indirect ref {:?}", r);
                    match doc.objects.get(r) {
                        Some(Object::Array(a)) => {
                            let idx = (222 - fc) as usize;
                            println!("  Resolved array len={}, code222={:?}", a.len(), a.get(idx));
                        }
                        other => println!(
                            "  Resolved to {:?}",
                            other.map(|o| std::mem::discriminant(o))
                        ),
                    }
                }
                Some(other) => println!(
                    "Font {id:?} {base}: Widths is {:?}",
                    std::mem::discriminant(other)
                ),
            }
        }
    }
}

/// Batch-convert §6.2.11.5 failing PDFs, save outputs, then report veraPDF errors.
#[test]
#[ignore]
fn debug_batch_6_2_11_5() {
    use pdf_manip::pdfa_xmp::PdfAConformance;
    use std::process::Command;

    let dir = "/tmp/pdf-test-6.2.11.5";
    let out_dir = "/tmp/pdf-test-6.2.11.5/out";
    std::fs::create_dir_all(out_dir).unwrap();

    let files = [
        "c4k-459_459601.pdf",
        "gen-131_131159.pdf",
        "gen-181_181221.pdf",
        "gen-181_181486.pdf",
        "gen-488_488313.pdf",
        "gen-529_529771.pdf",
        "gen-530_530303.pdf",
        "gen-530_530612.pdf",
        "gen-724_724164.pdf",
        "gen-764_764288.pdf",
        "gen-765_765033.pdf",
        "gen-857_857413.pdf",
        "gen-997_997093.pdf",
        "r3-327_327075.pdf",
        "r3-656_656510.pdf",
        "r3-FOP-32-0.zip-1.pdf",
    ];

    for name in &files {
        let path = format!("{dir}/{name}");
        let out_path = format!("{out_dir}/{name}");

        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("MISSING {name}: {e}");
                continue;
            }
        };

        let mut doc = match lopdf::Document::load_mem(&data) {
            Ok(d) if !d.objects.is_empty() => d,
            _ => {
                eprintln!("LOAD FAIL {name}");
                continue;
            }
        };

        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).unwrap();
        }));
        let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);
        let _ = pdf_manip::pdfa_fonts::fix_pfb_font_streams(&mut doc);
        let _ = pdf_manip::pdfa_fonts::fix_type1_stub_font_files(&mut doc);
        let _ = pdf_manip::pdfa_fonts::fix_mislabeled_truetype_as_cff(&mut doc);
        let _ = pdf_manip::pdfa_fonts::fix_truetype_with_cff_program(&mut doc);
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
        let _ = pdf_manip::pdfa_colorspace::normalize_colorspaces(&mut doc);
        pdf_manip::pdfa_fixups::run_fixups(&mut doc);
        let _ = pdf_manip::pdfa_xmp::repair_xmp_metadata(&mut doc, PdfAConformance::A2b, None);

        let mut saved = Vec::new();
        if doc.save_to(&mut saved).is_err() {
            eprintln!("SAVE FAIL {name}");
            continue;
        }
        pdf_manip::pdfa_cleanup::fix_pdf_header(&mut saved);
        pdf_manip::pdfa_cleanup::fix_startxref(&mut saved);
        std::fs::write(&out_path, &saved).unwrap();

        // Run veraPDF and capture errors.
        let vp = Command::new("/usr/local/bin/verapdf")
            .args(["--format", "text", "--flavour", "2b", &out_path])
            .output();
        match vp {
            Ok(o) => {
                let txt = String::from_utf8_lossy(&o.stdout);
                // Detect pass/fail from the summary line (contains PASS or FAIL + path).
                // Filter rule violation lines: they start with a clause number like "6.2.X"
                // (not the path which also contains "6.2.11.5" as the directory name).
                let is_pass = txt.lines().any(|l| {
                    (l.contains("PASS") || l.starts_with("PASS")) && l.contains(&out_path)
                });
                let violations: Vec<&str> = txt
                    .lines()
                    .filter(|l| {
                        // Rule violation lines start with digit (clause number) or contain
                        // "context" for detail lines. Exclude summary/path lines.
                        (l.trim_start().starts_with(|c: char| c.is_ascii_digit())
                            && !l.contains(&out_path))
                            || l.contains("context")
                    })
                    .collect();
                if is_pass {
                    eprintln!("{name}: PASS");
                } else {
                    eprintln!("{name}: FAIL");
                    for e in &violations[..violations.len().min(15)] {
                        eprintln!("  {e}");
                    }
                }
            }
            Err(e) => eprintln!("{name}: verapdf error: {e}"),
        }
    }
}

/// Trace widths at each pipeline step for §6.2.11.5 problem fonts.
#[test]
#[ignore]
fn debug_diagnose_6_2_11_5() {
    // (pdf_path, font_contains, codes_to_trace)
    let cases: Vec<(&str, &str, Vec<u32>)> = vec![
        (
            "/tmp/pdf-test-6.2.11.5/gen-765_765033.pdf",
            "Helvetica-Bold",
            vec![225, 233],
        ),
        (
            "/tmp/pdf-test-6.2.11.5/gen-765_765033.pdf",
            "JBGCOD",
            vec![149],
        ),
        (
            "/tmp/pdf-test-6.2.11.5/gen-764_764288.pdf",
            "Helvetica-Condensed-Bold",
            vec![39],
        ),
        (
            "/tmp/pdf-test-6.2.11.5/gen-131_131159.pdf",
            "AvantGarde-Book",
            vec![129],
        ),
        (
            "/tmp/pdf-test-6.2.11.5/gen-488_488313.pdf",
            "Frutiger-Black",
            vec![233],
        ),
        (
            "/tmp/pdf-test-6.2.11.5/gen-488_488313.pdf",
            "FHAEJO",
            vec![144, 176],
        ),
        (
            "/tmp/pdf-test-6.2.11.5/gen-488_488313.pdf",
            "EuroSans",
            vec![1, 2],
        ),
        (
            "/tmp/pdf-test-6.2.11.5/gen-724_724164.pdf",
            "Times-Roman",
            vec![243],
        ),
        (
            "/tmp/pdf-test-6.2.11.5/r3-327_327075.pdf",
            "XPUCKR",
            vec![147, 148, 149, 150],
        ),
    ];

    fn get_font_widths(
        doc: &lopdf::Document,
        font_substr: &str,
        codes: &[u32],
    ) -> Vec<(String, u32, i64)> {
        use lopdf::Object;
        let mut results = Vec::new();
        for (id, obj) in &doc.objects {
            let Object::Dictionary(dict) = obj else {
                continue;
            };
            let base = match dict.get(b"BaseFont").ok() {
                Some(Object::Name(n)) => String::from_utf8_lossy(n).to_string(),
                _ => continue,
            };
            if !base.contains(font_substr) {
                continue;
            }
            let fc = match dict.get(b"FirstChar").ok() {
                Some(Object::Integer(i)) => *i as u32,
                _ => continue,
            };
            let widths: Vec<i64> = match dict.get(b"Widths").ok() {
                Some(Object::Array(arr)) => arr
                    .iter()
                    .map(|o| match o {
                        Object::Integer(i) => *i,
                        Object::Real(r) => *r as i64,
                        _ => -1,
                    })
                    .collect(),
                Some(Object::Reference(r)) => match doc.get_object(*r) {
                    Ok(Object::Array(arr)) => arr
                        .iter()
                        .map(|o| match o {
                            Object::Integer(i) => *i,
                            Object::Real(r) => *r as i64,
                            _ => -1,
                        })
                        .collect(),
                    _ => vec![],
                },
                _ => vec![],
            };
            for &code in codes {
                if code >= fc && (code - fc) < widths.len() as u32 {
                    let w = widths[(code - fc) as usize];
                    results.push((base.clone(), code, w));
                } else {
                    results.push((base.clone(), code, -999)); // out of range
                }
            }
            let enc_name = match dict.get(b"Encoding").ok() {
                Some(Object::Name(n)) => String::from_utf8_lossy(n).to_string(),
                _ => "(custom/none)".to_string(),
            };
            eprintln!(
                "  font_obj={id:?} base={base} fc={fc} enc={enc_name} widths_len={}",
                widths.len()
            );
            break;
        }
        results
    }

    for (path, font_substr, codes) in &cases {
        eprintln!("=== {path} / font contains: {font_substr} ===");
        let data = std::fs::read(path).unwrap();

        // Step 1: original widths
        let doc0 = lopdf::Document::load_mem(&data).unwrap();
        let w0 = get_font_widths(&doc0, font_substr, codes);
        eprintln!("  ORIGINAL:");
        for (base, code, w) in &w0 {
            eprintln!("    {base} code {code} = {w}");
        }

        // Step 2: after cleanup
        let mut doc = lopdf::Document::load_mem(&data).unwrap();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).unwrap();
        }));
        let w1 = get_font_widths(&doc, font_substr, codes);
        eprintln!("  AFTER cleanup:");
        for (base, code, w) in &w1 {
            eprintln!("    {base} code {code} = {w}");
        }

        // Step 3: after fix_cff_widths
        let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);
        pdf_manip::pdfa_fonts::fix_cff_widths(&mut doc);
        let w2 = get_font_widths(&doc, font_substr, codes);
        eprintln!("  AFTER fix_cff_widths:");
        for (base, code, w) in &w2 {
            eprintln!("    {base} code {code} = {w}");
        }

        // Step 4: after fix_missing_simple_font_widths
        pdf_manip::pdfa_fonts::fix_truetype_cid_widths(&mut doc);
        pdf_manip::pdfa_fonts::fix_type1_charset(&mut doc);
        pdf_manip::pdfa_fonts::fix_truetype_encoding(&mut doc);
        let _ = pdf_manip::pdfa_fonts::fix_existing_symbolic_truetype_cmaps(&mut doc);
        pdf_manip::pdfa_fonts::fix_truetype_unicode_cmap(&mut doc);
        pdf_manip::pdfa_fonts::fix_notdef_glyph_refs(&mut doc);
        pdf_manip::pdfa_fonts::fix_cid_font_notdef(&mut doc);
        pdf_manip::pdfa_fonts::fix_symbolic_font_notdef_streams(&mut doc);
        let _ = pdf_manip::pdfa_fonts::fix_simple_font_out_of_range_codes(&mut doc);
        pdf_manip::pdfa_fonts::fix_undefined_encoding_codes(&mut doc);
        pdf_manip::pdfa_fonts::fix_symbolic_flags(&mut doc);
        let _ = pdf_manip::pdfa_fonts::fix_classic_symbolic_base14_encoding(&mut doc);
        pdf_manip::pdfa_fonts::fix_missing_simple_font_widths(&mut doc);
        let _ = pdf_manip::pdfa_fonts::fix_type3_font_widths(&mut doc);
        let w3 = get_font_widths(&doc, font_substr, codes);
        eprintln!("  AFTER fix_missing_simple_font_widths:");
        for (base, code, w) in &w3 {
            eprintln!("    {base} code {code} = {w}");
        }

        // Step 5: after fix_font_width_mismatches
        pdf_manip::pdfa_fonts::fix_font_width_mismatches(&mut doc);
        let w4 = get_font_widths(&doc, font_substr, codes);
        eprintln!("  AFTER fix_font_width_mismatches:");
        for (base, code, w) in &w4 {
            eprintln!("    {base} code {code} = {w}");
        }
        eprintln!();
    }
}

/// Trace what the CFF charset and glyph_index return for HJPIGJ+Helvetica-Bold.
#[test]
#[ignore]
fn debug_cff_charset_helv_bold() {
    use lopdf::Object;

    let data = std::fs::read("/tmp/pdf-test-6.2.11.5/gen-765_765033.pdf").unwrap();
    let doc = lopdf::Document::load_mem(&data).unwrap();

    // Find HJPIGJ+Helvetica-Bold font
    for (id, obj) in &doc.objects {
        let Object::Dictionary(dict) = obj else {
            continue;
        };
        let subtype = match dict.get(b"Subtype").ok() {
            Some(Object::Name(n)) => String::from_utf8_lossy(n).to_string(),
            _ => continue,
        };
        if subtype != "Type1" {
            continue;
        }
        let base = match dict.get(b"BaseFont").ok() {
            Some(Object::Name(n)) => String::from_utf8_lossy(n).to_string(),
            _ => continue,
        };
        if !base.contains("Helvetica-Bold") {
            continue;
        }
        eprintln!("=== Font {id:?}: {base} ===");

        // Get FontDescriptor
        let fd_id = match dict.get(b"FontDescriptor").ok() {
            Some(Object::Reference(r)) => *r,
            _ => continue,
        };
        let Some(Object::Dictionary(fd)) = doc.objects.get(&fd_id) else {
            continue;
        };
        let ff3_id = match fd.get(b"FontFile3").ok() {
            Some(Object::Reference(r)) => *r,
            _ => {
                eprintln!("No FontFile3");
                continue;
            }
        };
        let Some(Object::Stream(ref stream)) = doc.objects.get(&ff3_id) else {
            continue;
        };
        let font_data = match stream.decompressed_content() {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Decomp error: {e}");
                continue;
            }
        };
        eprintln!(
            "  font bytes: {} first4={:02x?}",
            font_data.len(),
            &font_data[..4.min(font_data.len())]
        );

        // Check glyph_index and enc_map for codes 225 and 233
        let cff = match cff_parser::Table::parse(&font_data) {
            Some(c) => c,
            None => {
                eprintln!("CFF parse failed on raw");
                continue;
            }
        };
        let ng = cff.number_of_glyphs();
        eprintln!("  raw CFF num_glyphs={ng}");
        for gid_raw in 0..ng {
            let gid = cff_parser::GlyphId(gid_raw);
            let name = cff.glyph_name(gid);
            let w = cff.glyph_width(gid);
            let s = cff.matrix().sx as f64 * 1000.0;
            let ws = w.map(|w| w as f64 * s);
            eprintln!("    GID {gid_raw}: name={name:?} w_raw={w:?} w_scaled={ws:?}");
        }
        for code in [225u8, 233u8] {
            let gid = cff.glyph_index(code);
            eprintln!("  raw glyph_index({code}) = {gid:?}");
        }
    }
}

#[test]
#[ignore]
fn debug_fonttype_all_611() {
    use lopdf::Object;
    for (path, font_substr, codes) in &[
        (
            "/tmp/pdf-test-6.2.11.5/gen-764_764288.pdf",
            "Helvetica-Cond",
            &[39u32][..],
        ),
        (
            "/tmp/pdf-test-6.2.11.5/gen-131_131159.pdf",
            "AvantGarde-Book",
            &[129u32][..],
        ),
        (
            "/tmp/pdf-test-6.2.11.5/gen-724_724164.pdf",
            "Times-Roman",
            &[243u32][..],
        ),
        (
            "/tmp/pdf-test-6.2.11.5/gen-529_529771.pdf",
            "NewCenturySchlbk",
            &[173u32, 237u32][..],
        ),
        (
            "/tmp/pdf-test-6.2.11.5/gen-997_997093.pdf",
            "Symbol",
            &[183u32][..],
        ),
        (
            "/tmp/pdf-test-6.2.11.5/r3-656_656510.pdf",
            "Symbol",
            &[43u32][..],
        ),
        (
            "/tmp/pdf-test-6.2.11.5/r3-FOP-32-0.zip-1.pdf",
            "Symbol",
            &[183u32][..],
        ),
        (
            "/tmp/pdf-test-6.2.11.5/gen-530_530303.pdf",
            "CMR10",
            &[147u32][..],
        ),
        (
            "/tmp/pdf-test-6.2.11.5/gen-530_530612.pdf",
            "Myriad-Italic",
            &[237u32][..],
        ),
        (
            "/tmp/pdf-test-6.2.11.5/gen-181_181486.pdf",
            "HelveticaNeue-LightCond",
            &[243u32][..],
        ),
    ] {
        let Ok(data) = std::fs::read(path) else {
            eprintln!("missing {path}");
            continue;
        };
        let doc = lopdf::Document::load_mem(&data).unwrap();
        let mut found_fd_id = None;
        for (id, obj) in &doc.objects {
            let Object::Dictionary(dict) = obj else {
                continue;
            };
            let Ok(Object::Name(n)) = dict.get(b"BaseFont") else {
                continue;
            };
            let base = String::from_utf8_lossy(n);
            if !base.contains(font_substr) {
                continue;
            }
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
            let enc = dict
                .get(b"Encoding")
                .ok()
                .and_then(|o| match o {
                    Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
                    Object::Dictionary(_) => Some("(dict)".to_string()),
                    _ => None,
                })
                .unwrap_or_default();
            let first_char = dict
                .get(b"FirstChar")
                .ok()
                .and_then(|o| {
                    if let Object::Integer(i) = o {
                        Some(*i)
                    } else {
                        None
                    }
                })
                .unwrap_or(0);
            let widths: Vec<i64> = match dict.get(b"Widths").ok() {
                Some(Object::Array(arr)) => arr
                    .iter()
                    .map(|o| match o {
                        Object::Integer(i) => *i,
                        Object::Real(r) => *r as i64,
                        _ => -1,
                    })
                    .collect(),
                Some(Object::Reference(r)) => {
                    if let Ok(Object::Array(arr)) = doc.get_object(*r) {
                        arr.iter()
                            .map(|o| match o {
                                Object::Integer(i) => *i,
                                Object::Real(r) => *r as i64,
                                _ => -1,
                            })
                            .collect()
                    } else {
                        vec![]
                    }
                }
                _ => vec![],
            };
            eprintln!("=== {path} / {font_substr} ===  obj={id:?} subtype={subtype} enc={enc} fc={first_char}");
            for &code in *codes {
                let w = if code >= first_char as u32
                    && (code - first_char as u32) < widths.len() as u32
                {
                    widths[(code - first_char as u32) as usize]
                } else {
                    -999
                };
                eprintln!("  code {code} width={w}");
            }
            found_fd_id = dict.get(b"FontDescriptor").ok().and_then(|o| {
                if let Object::Reference(r) = o {
                    Some(*r)
                } else {
                    None
                }
            });
            break;
        }
        if let Some(fd_id) = found_fd_id {
            if let Some(Object::Dictionary(dict)) = doc.objects.get(&fd_id) {
                let has_ff1 = dict.has(b"FontFile");
                let has_ff2 = dict.has(b"FontFile2");
                let has_ff3 = dict.has(b"FontFile3");
                eprintln!("  FontDescriptor={fd_id:?} ff1={has_ff1} ff2={has_ff2} ff3={has_ff3}");
                if has_ff3 {
                    if let Ok(Object::Reference(r)) = dict.get(b"FontFile3").cloned() {
                        if let Some(Object::Stream(s)) = doc.objects.get(&r) {
                            let st = s
                                .dict
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
                            let len = s.content.len();
                            eprintln!("  FontFile3 subtype={st} len={len}");
                            // Check first bytes for font magic
                            if len >= 4 {
                                let magic: Vec<_> = s.content[..4.min(len)]
                                    .iter()
                                    .map(|b| format!("{b:02x}"))
                                    .collect();
                                eprintln!("  FontFile3 magic=[{}]", magic.join(" "));
                            }
                        }
                    }
                }
                if has_ff1 {
                    if let Ok(Object::Reference(r)) = dict.get(b"FontFile").cloned() {
                        if let Some(Object::Stream(s)) = doc.objects.get(&r) {
                            let len = s.content.len();
                            eprintln!("  FontFile len={len}");
                        }
                    }
                }
            }
        }
    }
}

/// Trace cff_width_for_code and compute_cff_corrections_by_name for gen-724 code 243.
#[test]
#[ignore]
fn debug_gen724_width_trace() {
    use lopdf::Object;

    let data = std::fs::read("/tmp/pdf-test-6.2.11.5/gen-724_724164.pdf").unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).unwrap();
    }));
    let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_pfb_font_streams(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_type1_stub_font_files(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_mislabeled_truetype_as_cff(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_truetype_with_cff_program(&mut doc);
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

    for (id, obj) in &doc.objects {
        let Object::Dictionary(dict) = obj else {
            continue;
        };
        let Ok(Object::Name(n)) = dict.get(b"BaseFont") else {
            continue;
        };
        let base = String::from_utf8_lossy(n);
        if !base.contains("Times-Roman") {
            continue;
        }
        let enc = dict
            .get(b"Encoding")
            .ok()
            .and_then(|o| match o {
                Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
                _ => None,
            })
            .unwrap_or_default();
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
        eprintln!("Font: {base} enc={enc} fc={fc} obj={id:?}");

        if let Ok(Object::Reference(fd_ref)) = dict.get(b"FontDescriptor").cloned() {
            if let Some(Object::Dictionary(fd)) = doc.objects.get(&fd_ref) {
                if let Ok(Object::Reference(ff3_ref)) = fd.get(b"FontFile3").cloned() {
                    if let Some(Object::Stream(s)) = doc.objects.get(&ff3_ref) {
                        let mut s2 = s.clone();
                        let _ = s2.decompress();
                        let font_data = s2.content.clone();
                        eprintln!("  FontFile3 decompressed len={}", font_data.len());

                        if let Some(cff) = cff_parser::Table::parse(&font_data) {
                            eprintln!("  CFF num_glyphs={}", cff.number_of_glyphs());
                            eprintln!(
                                "  GID 0 width={:?}",
                                cff.glyph_width(cff_parser::GlyphId(0))
                            );
                            eprintln!("  glyph_index(243)={:?}", cff.glyph_index(243u8));
                            for gid in 0..cff.number_of_glyphs().min(50) {
                                let name = cff.glyph_name(cff_parser::GlyphId(gid));
                                if name
                                    .map(|n| n.contains("acute") || n.contains("notdef"))
                                    .unwrap_or(false)
                                {
                                    let w = cff.glyph_width(cff_parser::GlyphId(gid));
                                    eprintln!("    GID {gid}: name={name:?} width={w:?}");
                                }
                            }
                        }
                    }
                }
            }
        }
        break;
    }
}

/// Trace HUBBIG+Symbol font file type at each pipeline step for gen-997.
#[test]
#[ignore]
fn debug_gen997_symbol_trace() {
    use lopdf::Object;

    fn get_symbol_info(doc: &lopdf::Document) -> Option<(String, bool, bool, bool, i64)> {
        for (_id, obj) in &doc.objects {
            let Object::Dictionary(dict) = obj else {
                continue;
            };
            let base = match dict.get(b"BaseFont").ok() {
                Some(Object::Name(n)) => String::from_utf8_lossy(n).to_string(),
                _ => continue,
            };
            if !base.contains("Symbol") {
                continue;
            }
            let fc = match dict.get(b"FirstChar").ok() {
                Some(Object::Integer(i)) => *i as u32,
                _ => continue,
            };
            let widths: Vec<i64> = match dict.get(b"Widths").ok() {
                Some(Object::Array(a)) => a
                    .iter()
                    .map(|o| match o {
                        Object::Integer(i) => *i,
                        Object::Real(r) => *r as i64,
                        _ => -1,
                    })
                    .collect(),
                Some(Object::Reference(r)) => match doc.get_object(*r) {
                    Ok(Object::Array(a)) => a
                        .iter()
                        .map(|o| match o {
                            Object::Integer(i) => *i,
                            Object::Real(r) => *r as i64,
                            _ => -1,
                        })
                        .collect(),
                    _ => vec![],
                },
                _ => vec![],
            };
            let w183 = if 183 >= fc && (183 - fc) < widths.len() as u32 {
                widths[(183 - fc) as usize]
            } else {
                -1
            };
            let fd_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(r)) => *r,
                _ => return Some((base, false, false, false, w183)),
            };
            let (has_ff, has_ff2, has_ff3) = match doc.objects.get(&fd_id) {
                Some(Object::Dictionary(fd)) => (
                    fd.has(b"FontFile"),
                    fd.has(b"FontFile2"),
                    fd.has(b"FontFile3"),
                ),
                _ => (false, false, false),
            };
            return Some((base, has_ff, has_ff2, has_ff3, w183));
        }
        None
    }

    macro_rules! report {
        ($label:expr, $doc:expr) => {
            if let Some((base, ff, ff2, ff3, w183)) = get_symbol_info($doc) {
                eprintln!(
                    "{}: {} FF={ff} FF2={ff2} FF3={ff3} w183={w183}",
                    $label, base
                );
            } else {
                eprintln!("{}: no Symbol font found", $label);
            }
        };
    }

    let data = std::fs::read("/tmp/pdf-test-6.2.11.5/gen-997_997093.pdf").unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();
    // Check object (21, 0) directly
    let ff2_id = (21u32, 0u16);
    eprintln!(
        "Object (21,0) on load: {:?}",
        doc.objects.get(&ff2_id).map(|o| match o {
            lopdf::Object::Stream(_) => "Stream",
            lopdf::Object::Dictionary(_) => "Dictionary",
            lopdf::Object::Null => "Null",
            _ => "Other",
        })
    );
    report!("ORIGINAL", &doc);
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).unwrap();
    }));
    report!("after cleanup", &doc);
    // Debug: check if Symbol FontFile2 data is valid
    eprintln!("DEBUG: doc has {} objects", doc.objects.len());
    for (id, obj) in &doc.objects {
        let lopdf::Object::Dictionary(dict) = obj else {
            continue;
        };
        let base = match dict.get(b"BaseFont").ok() {
            Some(lopdf::Object::Name(n)) => {
                let s = String::from_utf8_lossy(n).to_string();
                if s.contains("ymbol") {
                    eprintln!("  Found: {s} at {id:?}");
                }
                s
            }
            _ => continue,
        };
        if !base.contains("Symbol") {
            continue;
        }
        eprintln!("  Processing {base} at {id:?}");
        let fd_id = match dict.get(b"FontDescriptor").ok() {
            Some(lopdf::Object::Reference(r)) => *r,
            _ => {
                let fd_obj = dict.get(b"FontDescriptor").ok();
                eprintln!("No FD reference for {base} at {id:?}: fd={fd_obj:?}");
                continue;
            }
        };
        eprintln!("  fd_id={fd_id:?}");
        let Some(lopdf::Object::Dictionary(fd)) = doc.objects.get(&fd_id) else {
            eprintln!("  No FD dict at {fd_id:?}");
            continue;
        };
        eprintln!("  FD has FontFile2: {}", fd.has(b"FontFile2"));
        let ff2_id = match fd.get(b"FontFile2").ok() {
            Some(lopdf::Object::Reference(r)) => *r,
            _ => {
                eprintln!("No FontFile2 for {base} fd={fd_id:?}");
                continue;
            }
        };
        let obj_at_ff2 = doc.objects.get(&ff2_id);
        eprintln!(
            "  Object at ff2_id {ff2_id:?}: {:?}",
            obj_at_ff2.map(|o| match o {
                lopdf::Object::Stream(_) => "Stream",
                lopdf::Object::Dictionary(_) => "Dictionary",
                lopdf::Object::Array(_) => "Array",
                lopdf::Object::Integer(_i) => "Integer",
                lopdf::Object::Null => "Null",
                lopdf::Object::Reference(_r) => "Reference",
                _ => "Other",
            })
        );
        let Some(lopdf::Object::Stream(s)) = doc.objects.get(&ff2_id) else {
            eprintln!("FontFile2 not a stream");
            continue;
        };
        let mut s2 = s.clone();
        let decomp_result = s2.decompress();
        eprintln!("Font {base} obj={id:?} ff2={ff2_id:?}");
        eprintln!("  decompress result: {:?}", decomp_result.is_ok());
        eprintln!("  content len after decompress: {}", s2.content.len());
        if s2.content.len() >= 4 {
            eprintln!(
                "  first 4 bytes: {:02x}{:02x}{:02x}{:02x}",
                s2.content[0], s2.content[1], s2.content[2], s2.content[3]
            );
            let parse_ok = ttf_parser::Face::parse(&s2.content, 0).is_ok();
            eprintln!("  ttf_parser parse: {parse_ok}");
            if parse_ok {
                let f = ttf_parser::Face::parse(&s2.content, 0).unwrap();
                eprintln!("  num_glyphs: {}", f.number_of_glyphs());
            }
        }
        break;
    }
    let embed_report = pdf_manip::pdfa_fonts::embed_fonts(&mut doc).unwrap();
    eprintln!(
        "embed_fonts: inspected={} non_embedded={} embedded={} failed={:?}",
        embed_report.fonts_inspected,
        embed_report.non_embedded_found,
        embed_report.fonts_embedded,
        embed_report.failed
    );
    report!("after embed_fonts", &doc);
    let _ = pdf_manip::pdfa_fonts::fix_pfb_font_streams(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_type1_stub_font_files(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_mislabeled_truetype_as_cff(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_truetype_with_cff_program(&mut doc);
    report!("after fix_mislabeled", &doc);
    let _ = pdf_manip::pdfa_fonts::fix_cff_invalid_bcd(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_type1_nonstandard_charstrings(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_type1_eexec_space_prefix(&mut doc);
    pdf_manip::pdfa_fonts::fix_cff_widths(&mut doc);
    report!("after fix_cff_widths", &doc);
    pdf_manip::pdfa_fonts::fix_truetype_cid_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_type1_charset(&mut doc);
    pdf_manip::pdfa_fonts::fix_truetype_encoding(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_existing_symbolic_truetype_cmaps(&mut doc);
    report!("after fix_existing_symbolic_truetype_cmaps", &doc);
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
    report!("before fix_font_width_mismatches", &doc);
    pdf_manip::pdfa_fonts::fix_font_width_mismatches(&mut doc);
    report!("after fix_font_width_mismatches", &doc);
    pdf_manip::pdfa_fonts::fix_symbolic_font_widths(&mut doc);
    report!("after fix_symbolic_font_widths", &doc);
}

/// Regression test for Round 25 §6.2.11.5 regressions fixed in fc3a9db.
/// c4k-348 (Lucida CFF custom-encoding), c4k-001, c4k-002 were failing with
/// wrong cff_corrections routing in 7bed210. Verify all 3 pass veraPDF. (#507)
#[test]
#[ignore]
fn debug_r25_6211_5_regressions() {
    use pdf_manip::pdfa_xmp::PdfAConformance;

    fn run_pipeline(data: &[u8]) -> Vec<u8> {
        let mut doc = lopdf::Document::load_mem(data).unwrap();
        let _ = pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false);
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
        pdf_manip::pdfa_fonts::fix_font_width_mismatches(&mut doc);
        pdf_manip::pdfa_fonts::fix_symbolic_font_widths(&mut doc);
        pdf_manip::pdfa_fonts::fix_cidset(&mut doc);
        let _ = pdf_manip::pdfa_fonts::fix_missing_cidtogidmap(&mut doc);
        let _ = pdf_manip::pdfa_colorspace::normalize_colorspaces(&mut doc);
        pdf_manip::pdfa_fixups::run_fixups(&mut doc);
        let _ = pdf_manip::pdfa_xmp::repair_xmp_metadata(&mut doc, PdfAConformance::A2b, None);
        let mut saved = Vec::new();
        doc.save_to(&mut saved).unwrap();
        pdf_manip::pdfa_cleanup::fix_pdf_header(&mut saved);
        pdf_manip::pdfa_cleanup::fix_startxref(&mut saved);
        saved
    }

    let verapdf_bin = "/Users/jasperdewinter/verapdf/verapdf";
    let pdfs = [
        "/tmp/pdfa_r25_reg_1.pdf", // c4k-348_348108: Lucida CFF custom-encoding
        "/tmp/pdfa_r25_reg_2.pdf", // c4k-001_001688: regression sample
        "/tmp/pdfa_r25_reg_3.pdf", // c4k-002_002202: regression sample
    ];

    let mut all_pass = true;
    for path in &pdfs {
        if !std::path::Path::new(path).exists() {
            println!("{path}: SKIP (not found)");
            continue;
        }
        let data = std::fs::read(path).unwrap();
        let converted = run_pipeline(&data);
        let out = format!("{path}-converted.pdf");
        std::fs::write(&out, &converted).unwrap();

        let result = std::process::Command::new("bash")
            .arg("-c")
            .arg(format!(
                "{verapdf_bin} --format mrr --flavour 2b \"{out}\" 2>/dev/null | grep -o 'failedChecks=\"[0-9]*\"'"
            ))
            .output();
        match result {
            Ok(o) => {
                let s = String::from_utf8_lossy(&o.stdout);
                let s = s.trim();
                let pass = s == "failedChecks=\"0\"";
                println!("{}: {} {}", path, if pass { "PASS" } else { "FAIL" }, s);
                if !pass {
                    all_pass = false;
                }
            }
            Err(e) => println!("{path}: error {e}"),
        }
    }
    assert!(all_pass, "R25 §6.2.11.5 regressions not fixed");
}

/// Trace which pipeline step zeros out codes 222/240 for MFFDPL+AdvP4C4E74. (#507)
#[test]
#[ignore]
fn debug_r25_trace_width_zeroing() {
    use lopdf::{Document, Object};

    fn get_widths_for_font(doc: &Document, base_font_prefix: &str) -> Option<(u32, Vec<i64>)> {
        for obj in doc.objects.values() {
            if let Object::Dictionary(dict) = obj {
                let subtype = dict
                    .get(b"Subtype")
                    .ok()
                    .and_then(|o| {
                        if let Object::Name(n) = o {
                            std::str::from_utf8(n).ok().map(str::to_string)
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default();
                if !matches!(subtype.as_str(), "Type1" | "TrueType" | "MMType1") {
                    continue;
                }
                let bf = dict
                    .get(b"BaseFont")
                    .ok()
                    .and_then(|o| {
                        if let Object::Name(n) = o {
                            std::str::from_utf8(n).ok().map(str::to_string)
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default();
                if !bf.contains(base_font_prefix) {
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
                let widths = match dict.get(b"Widths").ok() {
                    Some(Object::Array(arr)) => arr
                        .iter()
                        .map(|o| match o {
                            Object::Integer(i) => *i,
                            Object::Real(r) => *r as i64,
                            _ => 0,
                        })
                        .collect::<Vec<i64>>(),
                    Some(Object::Reference(r)) => match doc.objects.get(r) {
                        Some(Object::Array(arr)) => arr
                            .iter()
                            .map(|o| match o {
                                Object::Integer(i) => *i,
                                Object::Real(r2) => *r2 as i64,
                                _ => 0,
                            })
                            .collect::<Vec<i64>>(),
                        _ => vec![],
                    },
                    _ => vec![],
                };
                return Some((fc, widths));
            }
        }
        None
    }

    fn check_codes(doc: &Document, label: &str, font: &str, codes: &[u32]) {
        if let Some((fc, widths)) = get_widths_for_font(doc, font) {
            for &code in codes {
                let idx = code as usize - fc as usize;
                let w = widths.get(idx).copied().unwrap_or(-1);
                if w != 385 && w != 500 {
                    println!("  [{}] {} code {} = {} (CHANGED!)", label, font, code, w);
                }
            }
        }
    }

    let data = std::fs::read("/tmp/pdfa_r25_reg_2.pdf").unwrap();
    let mut doc = Document::load_mem(&data).unwrap();

    macro_rules! step {
        ($label:expr, $fn:expr) => {
            $fn;
            check_codes(&doc, $label, "AdvP4C4E74", &[222, 240]);
        };
    }

    check_codes(&doc, "INITIAL", "AdvP4C4E74", &[222, 240]);
    step!("cleanup", {
        let _ = pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false);
    });
    step!("embed_fonts", {
        let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);
    });
    step!("fix_cff_widths", {
        pdf_manip::pdfa_fonts::fix_cff_widths(&mut doc);
    });
    step!("fix_tt_cid_widths", {
        pdf_manip::pdfa_fonts::fix_truetype_cid_widths(&mut doc);
    });
    step!("fix_type1_charset", {
        pdf_manip::pdfa_fonts::fix_type1_charset(&mut doc);
    });
    step!("fix_tt_encoding", {
        pdf_manip::pdfa_fonts::fix_truetype_encoding(&mut doc);
    });
    step!("fix_existing_tt_cmaps", {
        let _ = pdf_manip::pdfa_fonts::fix_existing_symbolic_truetype_cmaps(&mut doc);
    });
    step!("fix_tt_unicode_cmap", {
        pdf_manip::pdfa_fonts::fix_truetype_unicode_cmap(&mut doc);
    });
    step!("fix_t1_tounicode", {
        let _ = pdf_manip::pdfa_fonts::fix_type1_tounicode_from_encoding(&mut doc);
    });
    step!("fix_notdef_refs", {
        pdf_manip::pdfa_fonts::fix_notdef_glyph_refs(&mut doc);
    });
    step!("fix_cid_notdef", {
        pdf_manip::pdfa_fonts::fix_cid_font_notdef(&mut doc);
    });
    step!("fix_sym_notdef_streams", {
        pdf_manip::pdfa_fonts::fix_symbolic_font_notdef_streams(&mut doc);
    });
    step!("fix_simple_oor", {
        let _ = pdf_manip::pdfa_fonts::fix_simple_font_out_of_range_codes(&mut doc);
    });
    step!("fix_undef_enc", {
        pdf_manip::pdfa_fonts::fix_undefined_encoding_codes(&mut doc);
    });
    step!("fix_sym_flags", {
        pdf_manip::pdfa_fonts::fix_symbolic_flags(&mut doc);
    });
    step!("fix_classic_sym", {
        let _ = pdf_manip::pdfa_fonts::fix_classic_symbolic_base14_encoding(&mut doc);
    });
    step!("fix_missing_widths", {
        pdf_manip::pdfa_fonts::fix_missing_simple_font_widths(&mut doc);
    });
    step!("fix_t3_widths", {
        let _ = pdf_manip::pdfa_fonts::fix_type3_font_widths(&mut doc);
    });
    step!("fix_font_width_mism", {
        pdf_manip::pdfa_fonts::fix_font_width_mismatches(&mut doc);
    });
    step!("fix_sym_font_widths", {
        pdf_manip::pdfa_fonts::fix_symbolic_font_widths(&mut doc);
    });
    println!("Trace complete");
}

/// Trace MELEBE+NCSchlbk code 227 width through pipeline for gen-539.
#[test]
#[ignore]
fn debug_melebe_trace() {
    use lopdf::Object;

    fn melebe_w227(doc: &lopdf::Document, label: &str) {
        for (_id, obj) in &doc.objects {
            let Object::Dictionary(dict) = obj else {
                continue;
            };
            let base = match dict.get(b"BaseFont").ok() {
                Some(Object::Name(n)) => String::from_utf8_lossy(n).to_string(),
                _ => continue,
            };
            if !base.contains("MELEBE") {
                continue;
            }
            let fc = match dict.get(b"FirstChar").ok() {
                Some(Object::Integer(i)) => *i as u32,
                _ => continue,
            };
            let widths: Vec<i64> = match dict.get(b"Widths").ok() {
                Some(Object::Array(arr)) => arr
                    .iter()
                    .map(|o| match o {
                        Object::Integer(i) => *i,
                        Object::Real(r) => *r as i64,
                        _ => -1,
                    })
                    .collect(),
                Some(Object::Reference(r)) => match doc.get_object(*r) {
                    Ok(Object::Array(arr)) => arr
                        .iter()
                        .map(|o| match o {
                            Object::Integer(i) => *i,
                            Object::Real(r) => *r as i64,
                            _ => -1,
                        })
                        .collect(),
                    _ => vec![],
                },
                _ => vec![],
            };
            let w227 = if 227 >= fc && (227 - fc) < widths.len() as u32 {
                widths[(227 - fc) as usize]
            } else {
                -999
            };
            // Check font file type
            let ff_type = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(fd_ref)) => match doc.objects.get(fd_ref) {
                    Some(Object::Dictionary(fd)) => {
                        let ff1 = fd.has(b"FontFile");
                        let ff2 = fd.has(b"FontFile2");
                        let ff3 = fd.has(b"FontFile3");
                        format!("FF1={ff1} FF2={ff2} FF3={ff3}")
                    }
                    _ => "no FD".into(),
                },
                _ => "no ref".into(),
            };
            eprintln!("[{label}] {base}: fc={fc} w[227]={w227} {ff_type}");
            break;
        }
    }

    let data = std::fs::read("/tmp/w6211/gen-539_539969.pdf").unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    melebe_w227(&doc, "original");

    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).unwrap();
    }));
    melebe_w227(&doc, "cleanup");

    let _ = pdf_manip::pdfa_fonts::promote_inline_font_dicts(&mut doc);
    let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);
    melebe_w227(&doc, "embed_fonts");

    let _ = pdf_manip::pdfa_fonts::fix_pfb_font_streams(&mut doc);
    melebe_w227(&doc, "fix_pfb");

    let _ = pdf_manip::pdfa_fonts::fix_type1_stub_font_files(&mut doc);
    melebe_w227(&doc, "fix_stub");

    let _ = pdf_manip::pdfa_fonts::fix_mislabeled_truetype_as_cff(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_truetype_with_cff_program(&mut doc);
    melebe_w227(&doc, "fix_mislabel");

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
    melebe_w227(&doc, "fix_missing_widths");
    // Check Differences for MELEBE at this point
    for (_id, obj) in &doc.objects {
        let lopdf::Object::Dictionary(dict) = obj else {
            continue;
        };
        let base = match dict.get(b"BaseFont").ok() {
            Some(lopdf::Object::Name(n)) => String::from_utf8_lossy(n).to_string(),
            _ => continue,
        };
        if !base.contains("MELEBE") {
            continue;
        }
        // Parse encoding manually
        let mut enc_name = String::new();
        let mut diffs = std::collections::HashMap::new();
        match dict.get(b"Encoding").ok() {
            Some(lopdf::Object::Name(n)) => enc_name = String::from_utf8_lossy(n).to_string(),
            Some(lopdf::Object::Reference(r)) => {
                if let Some(lopdf::Object::Dictionary(ed)) = doc.objects.get(r) {
                    if let Ok(lopdf::Object::Name(n)) = ed.get(b"BaseEncoding") {
                        enc_name = String::from_utf8_lossy(n).to_string();
                    }
                    if let Ok(lopdf::Object::Array(arr)) = ed.get(b"Differences") {
                        let mut code = 0u32;
                        for item in arr {
                            match item {
                                lopdf::Object::Integer(c) => code = *c as u32,
                                lopdf::Object::Name(n) => {
                                    diffs.insert(code, String::from_utf8_lossy(n).to_string());
                                    code += 1;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        let d227 = diffs.get(&227).cloned().unwrap_or_default();
        let d252 = diffs.get(&252).cloned().unwrap_or_default();
        let d144 = diffs.get(&144).cloned().unwrap_or_default();
        let ndiffs = diffs.len();
        eprintln!(
            "[MELEBE enc] enc={enc_name} ndiffs={ndiffs} d[144]={d144} d[227]={d227} d[252]={d252}"
        );
        break;
    }

    let _ = pdf_manip::pdfa_fonts::fix_type3_font_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_font_width_mismatches(&mut doc);
    melebe_w227(&doc, "fix_width_mismatch");

    pdf_manip::pdfa_fonts::fix_symbolic_font_widths(&mut doc);
    melebe_w227(&doc, "final");

    // Save and run veraPDF (no manual width overrides)
    use pdf_manip::pdfa_xmp::PdfAConformance;
    pdf_manip::pdfa_fonts::fix_cidset(&mut doc);
    let _ = pdf_manip::pdfa_fonts::fix_missing_cidtogidmap(&mut doc);
    let _ = pdf_manip::pdfa_colorspace::normalize_colorspaces(&mut doc);
    pdf_manip::pdfa_fixups::run_fixups(&mut doc);
    let _ = pdf_manip::pdfa_xmp::repair_xmp_metadata(&mut doc, PdfAConformance::A2b, None);
    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    pdf_manip::pdfa_cleanup::fix_pdf_header(&mut saved);
    pdf_manip::pdfa_cleanup::fix_startxref(&mut saved);
    std::fs::write("/tmp/w6211/out/gen-539_manual.pdf", &saved).unwrap();
    let vp = std::process::Command::new("/usr/local/bin/verapdf")
        .args([
            "--format",
            "mrr",
            "--flavour",
            "2b",
            "/tmp/w6211/out/gen-539_manual.pdf",
        ])
        .output()
        .unwrap();
    let txt = String::from_utf8_lossy(&vp.stdout);
    if txt.contains("isCompliant=\"true\"") {
        eprintln!("[MELEBE manual] veraPDF: PASS");
    } else {
        for line in txt.lines() {
            if line.contains("errorMessage") || line.contains("context") {
                eprintln!("  {}", line.trim());
            }
        }
        eprintln!("[MELEBE manual] veraPDF: FAIL");
    }
}

/// Convert a batch of PDFs from /tmp/w6211/ and show veraPDF violations.
#[test]
#[ignore]
fn debug_w6211_batch() {
    use pdf_manip::pdfa_xmp::PdfAConformance;
    use std::process::Command;

    let dir = "/tmp/w6211";
    let out_dir = "/tmp/w6211/out";
    std::fs::create_dir_all(out_dir).unwrap();

    let entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "pdf").unwrap_or(false))
        .collect();

    for entry in &entries {
        let path = entry.path();
        let name = path.file_name().unwrap().to_str().unwrap();
        let out_path = format!("{out_dir}/{name}");

        let data = std::fs::read(&path).unwrap();
        let mut doc = match lopdf::Document::load_mem(&data) {
            Ok(d) if !d.objects.is_empty() => d,
            _ => {
                eprintln!("LOAD FAIL {name}");
                continue;
            }
        };

        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).unwrap();
        }));
        let _ = pdf_manip::pdfa_fonts::promote_inline_font_dicts(&mut doc);
        let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);
        let _ = pdf_manip::pdfa_fonts::fix_pfb_font_streams(&mut doc);
        let _ = pdf_manip::pdfa_fonts::fix_type1_stub_font_files(&mut doc);
        let _ = pdf_manip::pdfa_fonts::fix_mislabeled_truetype_as_cff(&mut doc);
        let _ = pdf_manip::pdfa_fonts::fix_truetype_with_cff_program(&mut doc);
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
        let _ = pdf_manip::pdfa_colorspace::normalize_colorspaces(&mut doc);
        pdf_manip::pdfa_fixups::run_fixups(&mut doc);
        let _ = pdf_manip::pdfa_xmp::repair_xmp_metadata(&mut doc, PdfAConformance::A2b, None);

        let mut saved = Vec::new();
        if doc.save_to(&mut saved).is_err() {
            eprintln!("SAVE FAIL {name}");
            continue;
        }
        pdf_manip::pdfa_cleanup::fix_pdf_header(&mut saved);
        pdf_manip::pdfa_cleanup::fix_startxref(&mut saved);
        std::fs::write(&out_path, &saved).unwrap();

        let vp = Command::new("/usr/local/bin/verapdf")
            .args(["--format", "mrr", "--flavour", "2b", &out_path])
            .output();
        match vp {
            Ok(o) => {
                let txt = String::from_utf8_lossy(&o.stdout);
                let is_pass = txt.contains("isCompliant=\"true\"");
                if is_pass {
                    eprintln!("{name}: PASS");
                } else {
                    eprintln!("{name}: FAIL");
                    for line in txt.lines() {
                        if line.contains("errorMessage") || line.contains("context") {
                            eprintln!("  {}", line.trim());
                        }
                    }
                }
            }
            Err(e) => eprintln!("{name}: verapdf error: {e}"),
        }
    }
}

/// Quick test for §6.9 and §6.1.4 fixes.
/// Run: cargo test -p pdf-manip --features pdfa-convert --test debug_convert test_filespec_and_xref_fixes -- --ignored --nocapture
#[test]
#[ignore]
fn test_filespec_and_xref_fixes() {
    use pdf_manip::pdfa_xmp::PdfAConformance;

    let mut paths: Vec<String> = vec![
        "/tmp/test_6_9.pdf".into(),
        "/tmp/test_6_1_4.pdf".into(),
    ];
    // Add batch test files if present.
    for dir in ["/tmp/test_69", "/tmp/test_614"] {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for e in entries.flatten() {
                if e.path().extension().map_or(false, |x| x == "pdf") {
                    paths.push(e.path().to_string_lossy().into_owned());
                }
            }
        }
    }
    let paths: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
    for path in &paths {
        let Ok(data) = std::fs::read(path) else {
            eprintln!("SKIP {path}: file not found");
            continue;
        };
        let Ok(mut doc) = lopdf::Document::load_mem(&data) else {
            eprintln!("SKIP {path}: parse error");
            continue;
        };

        let _ = pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false);
        pdf_manip::pdfa_fixups::run_fixups(&mut doc);
        let _ = pdf_manip::pdfa_xmp::repair_xmp_metadata(&mut doc, PdfAConformance::A2b, None);

        let mut saved = Vec::new();
        doc.save_to(&mut saved).unwrap();

        let pdf2 = pdf_syntax::Pdf::new(saved).unwrap();
        let report = pdf_compliance::validate_pdfa(&pdf2, pdf_compliance::PdfALevel::A2b);

        let issues_69: Vec<_> = report.issues.iter().filter(|i| i.rule == "6.9").collect();
        let issues_614: Vec<_> = report.issues.iter().filter(|i| i.rule == "6.1.4").collect();

        if issues_69.is_empty() && issues_614.is_empty() {
            eprintln!("PASS {path}: no §6.9 or §6.1.4 issues");
        } else {
            for i in &issues_69 {
                eprintln!("FAIL {path}: §6.9 — {}", i.message);
            }
            for i in &issues_614 {
                eprintln!("FAIL {path}: §6.1.4 — {}", i.message);
            }
        }

        let total = report.issues.len();
        eprintln!("  ({total} total issues)");
    }
}

/// Diagnose §6.1.7.1, §6.2.10, §6.2.11.7 failures.
/// Run: cargo test -p pdf-manip --features pdfa-convert --test debug_convert diagnose_three_patterns -- --ignored --nocapture
#[test]
#[ignore]
fn diagnose_three_patterns() {
    use pdf_manip::pdfa_xmp::PdfAConformance;

    let dirs = ["/tmp/test_617", "/tmp/test_6210", "/tmp/test_62117"];
    let rules = ["6.1.7", "6.2.10", "6.2.11.7"];

    for (dir, rule_prefix) in dirs.iter().zip(rules.iter()) {
        eprintln!("\n=== Pattern {rule_prefix} ===");
        let Ok(entries) = std::fs::read_dir(dir) else {
            eprintln!("SKIP {dir}: not found");
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(true, |x| x != "pdf") { continue; }
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let Ok(data) = std::fs::read(&path) else { continue; };
            let Ok(mut doc) = lopdf::Document::load_mem(&data) else {
                eprintln!("  {name}: parse error");
                continue;
            };

            let _ = pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false);
            let _ = pdf_manip::pdfa_colorspace::normalize_colorspaces(&mut doc);
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                pdf_manip::pdfa_fonts::fix_type1_tounicode_from_encoding(&mut doc)
            }));
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                pdf_manip::pdfa_fonts::fix_type0_tounicode(&mut doc)
            }));
            pdf_manip::pdfa_fixups::run_fixups(&mut doc);
            let _ = pdf_manip::pdfa_xmp::repair_xmp_metadata(&mut doc, PdfAConformance::A2b, None);

            let mut saved = Vec::new();
            doc.save_to(&mut saved).unwrap();

            let pdf2 = pdf_syntax::Pdf::new(saved).unwrap();
            let report = pdf_compliance::validate_pdfa(&pdf2, pdf_compliance::PdfALevel::A2b);

            let matching: Vec<_> = report.issues.iter()
                .filter(|i| i.rule.starts_with(rule_prefix))
                .collect();

            if matching.is_empty() {
                eprintln!("  {name}: PASS (no {rule_prefix} issues)");
            } else {
                for i in &matching {
                    eprintln!("  {name}: FAIL {} — {}", i.rule, i.message);
                }
            }
        }
    }
}

/// Deep-dive §6.1.7.1: find which /F key survives in saved output.
/// Run: cargo test -p pdf-manip --features pdfa-convert --test debug_convert debug_617_f_key -- --ignored --nocapture
#[test]
#[ignore]
fn debug_617_f_key() {
    use pdf_manip::pdfa_xmp::PdfAConformance;

    let path = "/tmp/test_617/629_629523.pdf";
    let data = std::fs::read(path).unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    let _ = pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false);
    pdf_manip::pdfa_fixups::run_fixups(&mut doc);
    let _ = pdf_manip::pdfa_xmp::repair_xmp_metadata(&mut doc, PdfAConformance::A2b, None);

    // Check lopdf objects for /F keys in stream dicts
    let mut f_count = 0;
    for (&id, obj) in &doc.objects {
        if let lopdf::Object::Stream(s) = obj {
            if s.dict.has(b"F") {
                let f_val = s.dict.get(b"F").ok();
                eprintln!("Stream {:?} has /F key: {:?}", id, f_val);
                // Check what other keys are in this dict
                let keys: Vec<String> = s.dict.iter()
                    .map(|(k, _)| String::from_utf8_lossy(k).to_string())
                    .collect();
                eprintln!("  Keys: {:?}", keys);
                f_count += 1;
            }
        }
    }
    eprintln!("Total stream objects with /F: {f_count}");

    // Also save and scan raw bytes
    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    // Find /F in raw output (not /Filter, /Font, etc.)
    let mut i = 0;
    let mut hits = 0;
    while i + 2 < saved.len() {
        if saved[i] == b'/' && saved[i + 1] == b'F' {
            let next = saved.get(i + 2).copied().unwrap_or(0);
            if !next.is_ascii_alphanumeric() && next != b'_' {
                // Found bare /F — show context
                let start = i.saturating_sub(80);
                let end = saved.len().min(i + 80);
                let ctx = String::from_utf8_lossy(&saved[start..end]);
                eprintln!("Raw /F at offset {i}: ...{}...", ctx);
                hits += 1;
                if hits >= 5 { break; }
            }
        }
        i += 1;
    }
    eprintln!("Raw /F hits: {hits}");
}

/// Deep-dive §6.1.7.1: scan saved bytes around /F matches.
/// Run: cargo test -p pdf-manip --features pdfa-convert --test debug_convert debug_617_raw_scan -- --ignored --nocapture
#[test]
#[ignore]
fn debug_617_raw_scan() {
    use pdf_manip::pdfa_xmp::PdfAConformance;

    let path = "/tmp/test_617/629_629523.pdf";
    let data = std::fs::read(path).unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    let _ = pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false);
    pdf_manip::pdfa_fixups::run_fixups(&mut doc);
    let _ = pdf_manip::pdfa_xmp::repair_xmp_metadata(&mut doc, PdfAConformance::A2b, None);

    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();

    // Reproduce the compliance checker's raw scan logic
    let data = &saved;
    let mut i = 0;
    let mut hits = 0;
    while i + 2 < data.len() {
        if data[i] == b'/' && data[i + 1] == b'F' {
            let next = data.get(i + 2).copied().unwrap_or(0);
            if !next.is_ascii_alphanumeric() && next != b'_' {
                let before_start = i.saturating_sub(300);
                let before = &data[before_start..i];
                let after_end = data.len().min(i + 200);
                let after = &data[i..after_end];
                let has_dict_start = before.windows(2).any(|w| w == b"<<");
                let has_stream_ahead = after.windows(6).any(|w| w == b"stream");
                let current_dict_start = before
                    .windows(2)
                    .enumerate()
                    .rfind(|(_, w)| *w == b"<<")
                    .map(|(idx, _)| idx)
                    .unwrap_or(0);
                let current_dict_ctx = &before[current_dict_start..];
                let is_filespec = current_dict_ctx.windows(9).any(|w| w == b"/Filespec" as &[u8]);
                let is_embedded = current_dict_ctx.windows(13).any(|w| w == b"/EmbeddedFile" as &[u8]);
                let is_opi_subdict = current_dict_ctx.windows(4).any(|w| w == b"/OPI" as &[u8]);
                let no_endobj_between = !after
                    .windows(6)
                    .take_while(|w| *w != b"stream")
                    .any(|w| w == b"endobj");

                if has_dict_start && has_stream_ahead && no_endobj_between && !is_filespec && !is_embedded && !is_opi_subdict {
                    let ctx_before = String::from_utf8_lossy(&data[i.saturating_sub(60)..i]);
                    let ctx_after = String::from_utf8_lossy(&data[i..data.len().min(i+100)]);
                    eprintln!("HIT at offset {i}:");
                    eprintln!("  BEFORE: ...{ctx_before}");
                    eprintln!("  AFTER:  {ctx_after}...");
                    eprintln!("  no_endobj_between={no_endobj_between}, has_stream_ahead={has_stream_ahead}");
                    hits += 1;
                    if hits >= 3 { break; }
                }
            }
        }
        i += 1;
    }
    eprintln!("Total raw /F hits matching all conditions: {hits}");
}

/// Deep-dive §6.2.10: what transparency is detected?
/// Run: cargo test -p pdf-manip --features pdfa-convert --test debug_convert debug_6210_transparency -- --ignored --nocapture
#[test]
#[ignore]
fn debug_6210_transparency() {
    use pdf_manip::pdfa_xmp::PdfAConformance;

    let path = "/tmp/test_6210/gen-018_018133.pdf";
    let data = std::fs::read(path).unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    let _ = pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false);
    let cs_report = pdf_manip::pdfa_colorspace::normalize_colorspaces(&mut doc);
    eprintln!("normalize_colorspaces: {:?}", cs_report.as_ref().map(|r| (r.had_output_intent, r.output_intent_added)));
    pdf_manip::pdfa_fixups::run_fixups(&mut doc);
    let _ = pdf_manip::pdfa_xmp::repair_xmp_metadata(&mut doc, PdfAConformance::A2b, None);

    // Check OutputIntent in saved bytes
    let mut saved_check = Vec::new();
    doc.save_to(&mut saved_check).unwrap();
    let has_gts = saved_check.windows(11).any(|w| w == b"GTS_PDFA1");
    eprintln!("GTS_PDFA1 in saved bytes: {has_gts}");
    let has_outputintents = saved_check.windows(14).any(|w| w == b"OutputIntents");
    eprintln!("OutputIntents in saved bytes: {has_outputintents}");

    // Run compliance check and show 6.2.10 issues
    let pdf_check = pdf_syntax::Pdf::new(saved_check).unwrap();
    let report = pdf_compliance::validate_pdfa(&pdf_check, pdf_compliance::PdfALevel::A2b);
    for issue in &report.issues {
        if issue.rule.starts_with("6.2.10") {
            eprintln!("Compliance §{}: {} at {:?}", issue.rule, issue.message, issue.location);
        }
    }

    // Check each page for transparency clues
    let pages = doc.get_pages();
    for (&page_num, &page_id) in &pages {
        let Some(lopdf::Object::Dictionary(page_dict)) = doc.objects.get(&page_id) else { continue };
        let has_group = page_dict.has(b"Group");
        
        // Check ExtGState
        let mut has_ext_transparency = false;
        let res_dict = match page_dict.get(b"Resources").ok() {
            Some(lopdf::Object::Dictionary(d)) => Some(d.clone()),
            Some(lopdf::Object::Reference(id)) => match doc.objects.get(id) {
                Some(lopdf::Object::Dictionary(d)) => Some(d.clone()),
                _ => None,
            },
            _ => None,
        };
        if let Some(res) = res_dict.as_ref() {
            if let Ok(lopdf::Object::Dictionary(gs)) = res.get(b"ExtGState") {
                for (name, val) in gs.iter() {
                    let gs_dict = match val {
                        lopdf::Object::Reference(id) => match doc.objects.get(id) {
                            Some(lopdf::Object::Dictionary(d)) => Some(d),
                            _ => None,
                        },
                        lopdf::Object::Dictionary(d) => Some(d),
                        _ => None,
                    };
                    if let Some(gsd) = gs_dict {
                        let ca = gsd.get(b"CA").ok();
                        let ca2 = gsd.get(b"ca").ok();
                        let bm = gsd.get(b"BM").ok();
                        let sm = gsd.get(b"SMask").ok();
                        let name_str = String::from_utf8_lossy(name);
                        if ca.is_some() || ca2.is_some() || bm.is_some() || sm.is_some() {
                            eprintln!("  Page {page_num}: GS {name_str} CA={ca:?} ca={ca2:?} BM={bm:?} SMask={sm:?}");
                            has_ext_transparency = true;
                        }
                    }
                }
            }
        }
        
        // Check annotations
        let annots = match page_dict.get(b"Annots").ok() {
            Some(lopdf::Object::Array(arr)) => Some(arr.clone()),
            Some(lopdf::Object::Reference(id)) => match doc.objects.get(id) {
                Some(lopdf::Object::Array(arr)) => Some(arr.clone()),
                _ => None,
            },
            _ => None,
        };
        let annot_count = annots.as_ref().map_or(0, |a| a.len());

        // Check XObjects (Image with /SMask, Form with /Group)
        let mut has_xobj_transparency = false;
        if let Some(res) = res_dict.as_ref() {
            if let Ok(xobj_dict) = res.get(b"XObject") {
                let xd = match xobj_dict {
                    lopdf::Object::Dictionary(d) => Some(d.clone()),
                    lopdf::Object::Reference(id) => match doc.objects.get(id) {
                        Some(lopdf::Object::Dictionary(d)) => Some(d.clone()),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(xd) = xd {
                    for (name, val) in xd.iter() {
                        let stream_id = match val {
                            lopdf::Object::Reference(id) => *id,
                            _ => continue,
                        };
                        let Some(lopdf::Object::Stream(s)) = doc.objects.get(&stream_id) else { continue };
                        let subtype = s.dict.get(b"Subtype").ok().and_then(|o| if let lopdf::Object::Name(n) = o { Some(n.clone()) } else { None });
                        let subtype_str = subtype.as_ref().map(|n| String::from_utf8_lossy(n).to_string()).unwrap_or_default();
                        // Image with /SMask
                        if subtype_str == "Image" && s.dict.has(b"SMask") {
                            eprintln!("  Page {page_num}: XObject {} (Image) has /SMask!", String::from_utf8_lossy(name));
                            has_xobj_transparency = true;
                        }
                        // Form with Group/Transparency
                        if subtype_str == "Form" {
                            if let Ok(lopdf::Object::Dictionary(grp)) = s.dict.get(b"Group") {
                                if grp.get(b"S").ok() == Some(&lopdf::Object::Name(b"Transparency".to_vec())) {
                                    eprintln!("  Page {page_num}: XObject {} (Form) has /Group /S /Transparency!", String::from_utf8_lossy(name));
                                    has_xobj_transparency = true;
                                }
                            }
                            // Check Form XObject's own ExtGState
                            if let Ok(lopdf::Object::Dictionary(form_res)) = s.dict.get(b"Resources") {
                                if let Ok(lopdf::Object::Dictionary(gs)) = form_res.get(b"ExtGState") {
                                    for (gsname, gsval) in gs.iter() {
                                        let gsd = match gsval {
                                            lopdf::Object::Reference(id) => match doc.objects.get(id) {
                                                Some(lopdf::Object::Dictionary(d)) => Some(d),
                                                _ => None,
                                            },
                                            lopdf::Object::Dictionary(d) => Some(d),
                                            _ => None,
                                        };
                                        if let Some(gsd) = gsd {
                                            let sm = gsd.get(b"SMask").ok();
                                            let is_trans = match sm {
                                                Some(lopdf::Object::Name(n)) if n == b"None" => false,
                                                Some(lopdf::Object::Name(_)) | Some(lopdf::Object::Dictionary(_)) | Some(lopdf::Object::Reference(_)) => true,
                                                _ => false,
                                            };
                                            if is_trans {
                                                eprintln!("  Page {page_num}: Form XObject ExtGState {} has SMask transparency!", String::from_utf8_lossy(gsname));
                                                has_xobj_transparency = true;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Check Patterns
            if let Ok(pat_obj) = res.get(b"Pattern") {
                let pat_dict = match pat_obj {
                    lopdf::Object::Dictionary(d) => Some(d.clone()),
                    lopdf::Object::Reference(id) => match doc.objects.get(id) {
                        Some(lopdf::Object::Dictionary(d)) => Some(d.clone()),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(pd) = pat_dict {
                    for (name, val) in pd.iter() {
                        let sid = match val {
                            lopdf::Object::Reference(id) => *id,
                            _ => continue,
                        };
                        if let Some(lopdf::Object::Stream(s)) = doc.objects.get(&sid) {
                            if let Ok(lopdf::Object::Dictionary(pr)) = s.dict.get(b"Resources") {
                                if let Ok(lopdf::Object::Dictionary(gs)) = pr.get(b"ExtGState") {
                                    for (gsname, gsval) in gs.iter() {
                                        let gsd = match gsval {
                                            lopdf::Object::Reference(id) => match doc.objects.get(id) {
                                                Some(lopdf::Object::Dictionary(d)) => Some(d),
                                                _ => None,
                                            },
                                            lopdf::Object::Dictionary(d) => Some(d),
                                            _ => None,
                                        };
                                        if let Some(gsd) = gsd {
                                            let sm = gsd.get(b"SMask").ok();
                                            let is_trans = match sm {
                                                Some(lopdf::Object::Name(n)) if n == b"None" => false,
                                                Some(_) => true,
                                                _ => false,
                                            };
                                            let bm = gsd.get(b"BM").ok();
                                            let bm_trans = match bm {
                                                Some(lopdf::Object::Name(n)) if n == b"Normal" || n == b"Compatible" => false,
                                                Some(lopdf::Object::Name(_)) => true,
                                                _ => false,
                                            };
                                            if is_trans || bm_trans {
                                                eprintln!("  Page {page_num}: Pattern {} ExtGState {} has transparency!", String::from_utf8_lossy(name), String::from_utf8_lossy(gsname));
                                                has_xobj_transparency = true;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Check Type3 fonts
            if let Ok(font_obj) = res.get(b"Font") {
                let font_dict = match font_obj {
                    lopdf::Object::Dictionary(d) => Some(d.clone()),
                    lopdf::Object::Reference(id) => match doc.objects.get(id) {
                        Some(lopdf::Object::Dictionary(d)) => Some(d.clone()),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(fd) = font_dict {
                    for (fname, fval) in fd.iter() {
                        let fid = match fval {
                            lopdf::Object::Reference(id) => *id,
                            _ => continue,
                        };
                        let Some(lopdf::Object::Dictionary(fdict)) = doc.objects.get(&fid) else { continue };
                        if fdict.get(b"Subtype").ok() == Some(&lopdf::Object::Name(b"Type3".to_vec())) {
                            eprintln!("  Page {page_num}: Found Type3 font {}", String::from_utf8_lossy(fname));
                        }
                    }
                }
            }
        }

        eprintln!("Page {page_num}: Group={has_group}, ExtGState_transparency={has_ext_transparency}, XObj_transparency={has_xobj_transparency}, annots={annot_count}");
    }
}

/// Deep-dive §6.2.11.7.2: what kind of fonts are C2_0, C2_1?
/// Run: cargo test -p pdf-manip --features pdfa-convert --test debug_convert debug_62117_fonts -- --ignored --nocapture
#[test]
#[ignore]
fn debug_62117_fonts() {
    use pdf_manip::pdfa_xmp::PdfAConformance;
    let path = "/tmp/test_62117/667_667109.pdf";
    let data = std::fs::read(path).unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    // Run through conversion pipeline first
    let _ = pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false);
    let _ = pdf_manip::pdfa_colorspace::normalize_colorspaces(&mut doc);
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pdf_manip::pdfa_fonts::fix_type1_tounicode_from_encoding(&mut doc)
    }));
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pdf_manip::pdfa_fonts::fix_type0_tounicode(&mut doc)
    }));
    pdf_manip::pdfa_fixups::run_fixups(&mut doc);
    let _ = pdf_manip::pdfa_xmp::repair_xmp_metadata(&mut doc, PdfAConformance::A2b, None);

    for (&id, obj) in &doc.objects {
        let lopdf::Object::Dictionary(dict) = obj else { continue };
        let subtype = dict.get(b"Subtype").ok().and_then(|o| {
            if let lopdf::Object::Name(n) = o { Some(String::from_utf8_lossy(n).to_string()) } else { None }
        });
        let base_font = dict.get(b"BaseFont").ok().and_then(|o| {
            if let lopdf::Object::Name(n) = o { Some(String::from_utf8_lossy(n).to_string()) } else { None }
        });
        let has_tounicode = dict.get(b"ToUnicode").is_ok();
        let has_encoding = dict.get(b"Encoding").is_ok();
        // Also check page resource fonts for C2_0/C2_1 names
        if false {
            {
                let encoding = dict.get(b"Encoding").ok().map(|o| format!("{:?}", o));
                let descendants = dict.get(b"DescendantFonts").ok().map(|o| format!("{:?}", o));
                eprintln!("Font {:?}: Subtype={:?} BaseFont={:?} Encoding={:?} ToUnicode={} Descendants={:?}",
                    id, subtype, base_font, encoding, has_tounicode, descendants);
                // If Type0, check descendant
                if let Some(lopdf::Object::Array(desc)) = dict.get(b"DescendantFonts").ok() {
                    for d in desc {
                        if let lopdf::Object::Reference(cid_id) = d {
                            if let Some(lopdf::Object::Dictionary(cid_dict)) = doc.objects.get(cid_id) {
                                let cid_sub = cid_dict.get(b"Subtype").ok().and_then(|o| {
                                    if let lopdf::Object::Name(n) = o { Some(String::from_utf8_lossy(n).to_string()) } else { None }
                                });
                                let cid_bf = cid_dict.get(b"BaseFont").ok().and_then(|o| {
                                    if let lopdf::Object::Name(n) = o { Some(String::from_utf8_lossy(n).to_string()) } else { None }
                                });
                                let cidtogid = cid_dict.get(b"CIDToGIDMap").ok().map(|o| format!("{:?}", o));
                                let fd = cid_dict.get(b"FontDescriptor").ok().map(|o| format!("{:?}", o));
                                eprintln!("  CIDFont {:?}: Subtype={:?} BaseFont={:?} CIDToGIDMap={:?} FD={:?}",
                                    cid_id, cid_sub, cid_bf, cidtogid, fd);
                                // Check font file
                                if let Some(lopdf::Object::Reference(fd_id)) = cid_dict.get(b"FontDescriptor").ok() {
                                    if let Some(lopdf::Object::Dictionary(fd_dict)) = doc.objects.get(fd_id) {
                                        let has_ff2 = fd_dict.get(b"FontFile2").is_ok();
                                        let has_ff3 = fd_dict.get(b"FontFile3").is_ok();
                                        let has_ff = fd_dict.get(b"FontFile").is_ok();
                                        eprintln!("    FD {:?}: FontFile={} FontFile2={} FontFile3={}",
                                            fd_id, has_ff, has_ff2, has_ff3);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Dump page resource font names
    eprintln!("\n=== Page Font Resources ===");
    let pages = doc.get_pages();
    for (&pg, &pg_id) in &pages {
        let Some(lopdf::Object::Dictionary(pd)) = doc.objects.get(&pg_id) else { continue };
        let res2 = match pd.get(b"Resources").ok() {
            Some(lopdf::Object::Dictionary(d)) => Some(d.clone()),
            Some(lopdf::Object::Reference(rid)) => match doc.objects.get(rid) {
                Some(lopdf::Object::Dictionary(d)) => Some(d.clone()),
                _ => None,
            },
            _ => None,
        };
        let Some(res2) = res2 else { continue };
        let fd3 = match res2.get(b"Font").ok() {
            Some(lopdf::Object::Dictionary(d)) => Some(d.clone()),
            Some(lopdf::Object::Reference(rid)) => match doc.objects.get(rid) {
                Some(lopdf::Object::Dictionary(d)) => Some(d.clone()),
                _ => None,
            },
            _ => None,
        };
        let Some(fd3) = fd3 else { continue };
        for (nm, val) in fd3.iter() {
            let fid = match val {
                lopdf::Object::Reference(id) => *id,
                _ => continue,
            };
            let Some(lopdf::Object::Dictionary(fdict)) = doc.objects.get(&fid) else { continue };
            let st = fdict.get(b"Subtype").ok().and_then(|o| if let lopdf::Object::Name(n) = o { Some(String::from_utf8_lossy(n).to_string()) } else { None });
            let bf = fdict.get(b"BaseFont").ok().and_then(|o| if let lopdf::Object::Name(n) = o { Some(String::from_utf8_lossy(n).to_string()) } else { None });
            let has_tu = fdict.get(b"ToUnicode").is_ok();
            eprintln!("  Page {pg}: /{} -> {:?} Subtype={st:?} BaseFont={bf:?} ToUnicode={has_tu}", String::from_utf8_lossy(nm), fid);
        }
    }
}
