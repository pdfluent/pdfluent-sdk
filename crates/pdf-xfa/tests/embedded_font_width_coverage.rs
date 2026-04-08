use lopdf::{Dictionary, Document, ObjectId};
use pdf_xfa::font_bridge::{EmbeddedFontData, XfaFontResolver, XfaFontSpec};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
struct EmbeddedFontRecord {
    name: String,
    data: Vec<u8>,
    pdf_widths: Option<(u16, Vec<u16>)>,
    stream_subtype: Option<String>,
    parseable_by_ttf_parser: bool,
}

#[derive(Debug, Clone, Copy)]
struct CorpusFontWidthReport {
    unique_embedded_fonts: usize,
    extracted_with_pdf_widths: usize,
    extracted_without_pdf_widths: usize,
    resolved_with_pdf_widths: usize,
    resolved_without_pdf_widths: usize,
    propagation_mismatches: usize,
}

fn corpus_pdf_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(name)
}

fn strip_subset_prefix(name: &str) -> String {
    if let Some(pos) = name.find('+') {
        name[pos + 1..].to_string()
    } else {
        name.to_string()
    }
}

fn extract_font_widths(dict: &Dictionary) -> Option<(u16, Vec<u16>)> {
    let first_char = dict.get(b"FirstChar").ok()?.as_i64().ok()? as u16;
    let widths_array = dict.get(b"Widths").ok()?.as_array().ok()?;
    let widths: Vec<u16> = widths_array
        .iter()
        .filter_map(|w| w.as_i64().ok().map(|v| v as u16))
        .collect();
    if widths.is_empty() {
        return None;
    }
    Some((first_char, widths))
}

fn extract_font_from_direct_fd(
    doc: &Document,
    font_dict: &Dictionary,
) -> Option<(ObjectId, Vec<u8>, Option<String>)> {
    let fd_id = font_dict.get(b"FontDescriptor").ok()?.as_reference().ok()?;
    let fd = doc.get_dictionary(fd_id).ok()?;
    let font_stream_id = fd
        .get(b"FontFile2")
        .or_else(|_| fd.get(b"FontFile3"))
        .or_else(|_| fd.get(b"FontFile"))
        .ok()?
        .as_reference()
        .ok()?;
    let stream = doc
        .get_object(font_stream_id)
        .and_then(|o| o.as_stream())
        .ok()?;
    let data = stream
        .get_plain_content()
        .unwrap_or_else(|_| stream.content.clone());
    if data.is_empty() {
        return None;
    }
    let stream_subtype = stream
        .dict
        .get(b"Subtype")
        .ok()
        .and_then(|o| o.as_name().ok())
        .map(|name| String::from_utf8_lossy(name).to_string());
    Some((font_stream_id, data, stream_subtype))
}

fn extract_cidfont_data(
    doc: &Document,
    font_dict: &Dictionary,
    seen: &HashSet<ObjectId>,
) -> Option<(ObjectId, Vec<u8>, Option<String>)> {
    let descendants = font_dict.get(b"DescendantFonts").ok()?.as_array().ok()?;
    for desc_ref in descendants {
        let desc_id = desc_ref.as_reference().ok()?;
        let desc_dict = doc.get_dictionary(desc_id).ok()?;
        let fd_id = desc_dict.get(b"FontDescriptor").ok()?.as_reference().ok()?;
        let fd = doc.get_dictionary(fd_id).ok()?;
        let font_stream_id = fd
            .get(b"FontFile3")
            .or_else(|_| fd.get(b"FontFile2"))
            .or_else(|_| fd.get(b"FontFile"))
            .ok()?
            .as_reference()
            .ok()?;
        if seen.contains(&font_stream_id) {
            continue;
        }
        let stream = doc
            .get_object(font_stream_id)
            .and_then(|o| o.as_stream())
            .ok()?;
        let data = stream
            .get_plain_content()
            .unwrap_or_else(|_| stream.content.clone());
        if !data.is_empty() {
            let stream_subtype = stream
                .dict
                .get(b"Subtype")
                .ok()
                .and_then(|o| o.as_name().ok())
                .map(|name| String::from_utf8_lossy(name).to_string());
            return Some((font_stream_id, data, stream_subtype));
        }
    }
    None
}

fn extract_unique_embedded_fonts(doc: &Document) -> Vec<EmbeddedFontRecord> {
    let mut fonts = Vec::new();
    let mut seen = HashSet::new();
    for obj in doc.objects.values() {
        let dict = match obj.as_dict() {
            Ok(d) => d,
            Err(_) => continue,
        };
        let is_font =
            dict.get(b"Type").ok().and_then(|o| o.as_name().ok()) == Some(b"Font".as_slice());
        if !is_font {
            continue;
        }
        let base_font = match dict.get(b"BaseFont").ok().and_then(|o| o.as_name().ok()) {
            Some(name) => String::from_utf8_lossy(name).to_string(),
            None => continue,
        };
        let pdf_widths = extract_font_widths(dict);

        if let Some((stream_id, data, stream_subtype)) = extract_font_from_direct_fd(doc, dict) {
            if seen.insert(stream_id) {
                fonts.push(EmbeddedFontRecord {
                    name: strip_subset_prefix(&base_font),
                    parseable_by_ttf_parser: ttf_parser::Face::parse(&data, 0).is_ok(),
                    data,
                    pdf_widths,
                    stream_subtype,
                });
            }
            continue;
        }

        if let Some((stream_id, data, stream_subtype)) = extract_cidfont_data(doc, dict, &seen) {
            if seen.insert(stream_id) {
                fonts.push(EmbeddedFontRecord {
                    name: strip_subset_prefix(&base_font),
                    parseable_by_ttf_parser: ttf_parser::Face::parse(&data, 0).is_ok(),
                    data,
                    pdf_widths,
                    stream_subtype,
                });
            }
        }
    }
    fonts
}

fn corpus_font_width_report(pdf_name: &str) -> CorpusFontWidthReport {
    let pdf_path = corpus_pdf_path(pdf_name);
    let pdf_bytes = std::fs::read(&pdf_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", pdf_path.display()));
    let doc = Document::load_mem(&pdf_bytes)
        .unwrap_or_else(|e| panic!("failed to load {}: {e}", pdf_path.display()));
    let extracted_fonts = extract_unique_embedded_fonts(&doc);

    let embedded_fonts_for_resolver: Vec<EmbeddedFontData> = extracted_fonts
        .iter()
        .map(|font| EmbeddedFontData {
            name: font.name.clone(),
            data: font.data.clone(),
            pdf_widths: font.pdf_widths.clone(),
            pdf_encoding: None,
        })
        .collect();
    let mut resolver = XfaFontResolver::new(embedded_fonts_for_resolver);

    let mut report = CorpusFontWidthReport {
        unique_embedded_fonts: extracted_fonts.len(),
        extracted_with_pdf_widths: 0,
        extracted_without_pdf_widths: 0,
        resolved_with_pdf_widths: 0,
        resolved_without_pdf_widths: 0,
        propagation_mismatches: 0,
    };

    for font in &extracted_fonts {
        let extracted_has_widths = font.pdf_widths.is_some();
        let spec = XfaFontSpec::from_xfa_attrs(&font.name, None, None, None, None);
        let resolved = resolver
            .resolve(&spec)
            .unwrap_or_else(|e| panic!("failed to resolve embedded font {}: {e}", font.name));
        let resolved_has_widths = resolved.pdf_widths.is_some();

        if extracted_has_widths {
            report.extracted_with_pdf_widths += 1;
        } else {
            report.extracted_without_pdf_widths += 1;
        }

        if resolved_has_widths {
            report.resolved_with_pdf_widths += 1;
        } else {
            report.resolved_without_pdf_widths += 1;
        }

        if extracted_has_widths != resolved_has_widths {
            report.propagation_mismatches += 1;
            eprintln!(
                "{}: width propagation mismatch for {} (extracted={}, resolved={}, parseable={}, stream_subtype={:?}, resolved_name={})",
                pdf_name,
                font.name,
                extracted_has_widths,
                resolved_has_widths,
                font.parseable_by_ttf_parser,
                font.stream_subtype,
                resolved.name
            );
        }
    }

    eprintln!(
        "{}: extracted widths {}/{} unique embedded fonts, resolved widths {}/{} (mismatches={})",
        pdf_name,
        report.extracted_with_pdf_widths,
        report.unique_embedded_fonts,
        report.resolved_with_pdf_widths,
        report.unique_embedded_fonts,
        report.propagation_mismatches
    );

    report
}

#[test]
fn corpus_embedded_fonts_pdf_width_coverage() {
    let reports = [
        ("f1040.pdf", corpus_font_width_report("f1040.pdf")),
        ("sf15.pdf", corpus_font_width_report("sf15.pdf")),
    ];

    let total_fonts: usize = reports
        .iter()
        .map(|(_, report)| report.unique_embedded_fonts)
        .sum();
    let total_extracted_with_widths: usize = reports
        .iter()
        .map(|(_, report)| report.extracted_with_pdf_widths)
        .sum();
    let total_resolved_with_widths: usize = reports
        .iter()
        .map(|(_, report)| report.resolved_with_pdf_widths)
        .sum();
    let total_mismatches: usize = reports
        .iter()
        .map(|(_, report)| report.propagation_mismatches)
        .sum();

    eprintln!(
        "aggregate: extracted widths {}/{} unique embedded fonts, resolved widths {}/{} (mismatches={})",
        total_extracted_with_widths,
        total_fonts,
        total_resolved_with_widths,
        total_fonts,
        total_mismatches
    );

    assert!(total_fonts > 0, "expected embedded fonts in corpus PDFs");
    assert!(total_extracted_with_widths <= total_fonts);
    assert!(total_resolved_with_widths <= total_fonts);
    assert_eq!(
        total_resolved_with_widths, total_extracted_with_widths,
        "all extracted PDF widths should propagate to the resolved fonts"
    );
    assert_eq!(
        total_mismatches, 0,
        "resolved fonts should preserve PDF widths even when embedded font bytes are unparseable"
    );
}
