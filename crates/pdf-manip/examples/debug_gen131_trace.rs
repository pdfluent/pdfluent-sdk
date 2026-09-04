// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/// Trace gen-131 AvantGarde-Book code 129 through each pipeline step.
fn main() {
    let data = std::fs::read("/tmp/pdf-test-6.2.11.5/gen-131_131159.pdf").unwrap();
    let mut doc = lopdf::Document::load_mem(&data).unwrap();

    fn print_code129(doc: &lopdf::Document, step: &str) {
        for obj in doc.objects.values() {
            let lopdf::Object::Dictionary(dict) = obj else {
                continue;
            };
            let base = match dict.get(b"BaseFont").ok() {
                Some(lopdf::Object::Name(n)) => String::from_utf8_lossy(n).to_string(),
                _ => continue,
            };
            if !base.contains("AvantGarde-Book") {
                continue;
            }
            let fc = match dict.get(b"FirstChar").ok() {
                Some(lopdf::Object::Integer(i)) => *i as u32,
                _ => continue,
            };
            let w129 = match dict.get(b"Widths").ok() {
                Some(lopdf::Object::Array(arr)) => {
                    let idx = (129u32.saturating_sub(fc)) as usize;
                    arr.get(idx)
                        .map(|o| match o {
                            lopdf::Object::Integer(i) => *i,
                            lopdf::Object::Real(r) => *r as i64,
                            _ => -999,
                        })
                        .unwrap_or(-1)
                }
                Some(lopdf::Object::Reference(r)) => match doc.get_object(*r) {
                    Ok(lopdf::Object::Array(arr)) => {
                        let idx = (129u32.saturating_sub(fc)) as usize;
                        arr.get(idx)
                            .map(|o| match o {
                                lopdf::Object::Integer(i) => *i,
                                lopdf::Object::Real(r) => *r as i64,
                                _ => -999,
                            })
                            .unwrap_or(-1)
                    }
                    _ => -2,
                },
                _ => -3,
            };
            let has_ff = match dict.get(b"FontDescriptor").ok() {
                Some(lopdf::Object::Reference(fd_ref)) => match doc.objects.get(fd_ref) {
                    Some(lopdf::Object::Dictionary(fd)) => {
                        let ff1 = fd.has(b"FontFile");
                        let ff2 = fd.has(b"FontFile2");
                        let ff3 = fd.has(b"FontFile3");
                        format!("ff1={ff1} ff2={ff2} ff3={ff3}")
                    }
                    _ => "no fd obj".to_string(),
                },
                _ => "no fd".to_string(),
            };
            println!("[{step}] {base}: fc={fc} w[129]={w129} {has_ff}");
            break;
        }
    }

    print_code129(&doc, "original");

    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, false).unwrap();
    }));
    print_code129(&doc, "after cleanup");

    let _ = pdf_manip::pdfa_fonts::embed_fonts(&mut doc);
    print_code129(&doc, "after embed_fonts");

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
    print_code129(&doc, "after fix_missing_simple_font_widths");

    let _ = pdf_manip::pdfa_fonts::fix_type3_font_widths(&mut doc);
    pdf_manip::pdfa_fonts::fix_font_width_mismatches(&mut doc);
    print_code129(&doc, "after fix_font_width_mismatches");
}
