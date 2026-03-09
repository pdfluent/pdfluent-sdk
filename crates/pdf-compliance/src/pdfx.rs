//! PDF/X validation (ISO 15930).
//!
//! Checks conformance to PDF/X-1a:2003, PDF/X-3:2003, and PDF/X-4.

use crate::check::{self, catalog, error, error_at, warning};
use crate::{ComplianceReport, PdfXLevel};
use pdf_syntax::object::{Array, Dict, MaybeRef, Name, Object};
use pdf_syntax::Pdf;

/// Validate a PDF against a PDF/X conformance level.
pub fn validate(pdf: &Pdf, level: PdfXLevel) -> ComplianceReport {
    let mut report = ComplianceReport {
        issues: Vec::new(),
        pdfa_level: None,
        compliant: false,
    };

    if check::is_encrypted(pdf) {
        error(&mut report, "6.1.13", "PDF/X files must not be encrypted");
    }

    check_output_intents(pdf, level, &mut report);
    check_page_boxes(pdf, &mut report);
    check_fonts_embedded(pdf, &mut report);
    check_color_spaces(pdf, level, &mut report);

    if level.forbids_transparency() {
        check_no_transparency(pdf, &mut report);
    }

    check_xmp_pdfx(pdf, &mut report);
    check_trapped_key(pdf, &mut report);

    report.compliant = report.is_compliant();
    report
}

fn check_output_intents(pdf: &Pdf, level: PdfXLevel, report: &mut ComplianceReport) {
    let Some(cat) = catalog(pdf) else {
        error(report, "6.1.1", "Missing document catalog");
        return;
    };

    let oi_array: Option<Array<'_>> = cat.get(b"OutputIntents" as &[u8]);
    let Some(oi_array) = oi_array else {
        error(report, "6.2.2", "PDF/X requires at least one OutputIntent");
        return;
    };

    let mut found_gts = false;
    for item in oi_array.iter() {
        if let Object::Dict(ref dict) = item {
            if let Some(subtype) = dict.get::<Name>(b"S" as &[u8]) {
                if subtype.as_ref() == b"GTS_PDFX" {
                    found_gts = true;
                    if level == PdfXLevel::X1a2003
                        && dict
                            .get::<Object<'_>>(b"DestOutputProfile" as &[u8])
                            .is_none()
                    {
                        error(
                            report,
                            "6.2.2",
                            "PDF/X-1a requires DestOutputProfile ICC profile in OutputIntent",
                        );
                    }
                }
            }
        }
    }

    if !found_gts {
        error(report, "6.2.2", "No OutputIntent with /S /GTS_PDFX found");
    }
}

fn check_page_boxes(pdf: &Pdf, report: &mut ComplianceReport) {
    for (i, page) in pdf.pages().iter().enumerate() {
        let raw = page.raw();
        let has_trim = raw.contains_key(b"TrimBox" as &[u8]);
        let has_art = raw.contains_key(b"ArtBox" as &[u8]);

        if !has_trim && !has_art {
            error_at(
                report,
                "6.2.3",
                "Page must have TrimBox or ArtBox",
                format!("Page {}", i + 1),
            );
        }
    }
}

fn check_fonts_embedded(pdf: &Pdf, report: &mut ComplianceReport) {
    for (i, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let Some(res_dict) = page_dict.get::<Dict<'_>>(b"Resources" as &[u8]) else {
            continue;
        };
        let Some(fonts) = res_dict.get::<Dict<'_>>(b"Font" as &[u8]) else {
            continue;
        };

        for (name, obj) in fonts.entries() {
            if let MaybeRef::NotRef(Object::Dict(ref font_dict)) = obj {
                check_single_font(font_dict, name.as_ref(), i + 1, report);
            }
        }
    }
}

fn check_single_font(font: &Dict<'_>, name: &[u8], page: usize, report: &mut ComplianceReport) {
    let font_type = font
        .get::<Name>(b"Subtype" as &[u8])
        .map(|n| n.as_ref().to_vec());

    match font_type.as_deref() {
        Some(b"Type1") | Some(b"TrueType") => {
            if let Some(descriptor) = font.get::<Dict<'_>>(b"FontDescriptor" as &[u8]) {
                let has_file = descriptor.contains_key(b"FontFile" as &[u8])
                    || descriptor.contains_key(b"FontFile2" as &[u8])
                    || descriptor.contains_key(b"FontFile3" as &[u8]);
                if !has_file {
                    error_at(
                        report,
                        "6.3.5",
                        format!("Font /{} not embedded", String::from_utf8_lossy(name)),
                        format!("Page {page}"),
                    );
                }
            } else {
                error_at(
                    report,
                    "6.3.5",
                    format!(
                        "Font /{} has no FontDescriptor",
                        String::from_utf8_lossy(name)
                    ),
                    format!("Page {page}"),
                );
            }
        }
        Some(b"Type0") => {
            if let Some(descendants) = font.get::<Array<'_>>(b"DescendantFonts" as &[u8]) {
                for desc in descendants.iter() {
                    if let Object::Dict(ref cid_font) = desc {
                        if let Some(descriptor) =
                            cid_font.get::<Dict<'_>>(b"FontDescriptor" as &[u8])
                        {
                            let has_file = descriptor.contains_key(b"FontFile" as &[u8])
                                || descriptor.contains_key(b"FontFile2" as &[u8])
                                || descriptor.contains_key(b"FontFile3" as &[u8]);
                            if !has_file {
                                error_at(
                                    report,
                                    "6.3.5",
                                    format!(
                                        "CIDFont descendant of /{} not embedded",
                                        String::from_utf8_lossy(name)
                                    ),
                                    format!("Page {page}"),
                                );
                            }
                        }
                    }
                }
            }
        }
        Some(b"Type3") => {
            error_at(
                report,
                "6.3.5",
                format!(
                    "Type3 font /{} not allowed in PDF/X",
                    String::from_utf8_lossy(name)
                ),
                format!("Page {page}"),
            );
        }
        _ => {}
    }
}

fn check_color_spaces(pdf: &Pdf, level: PdfXLevel, report: &mut ComplianceReport) {
    if level != PdfXLevel::X1a2003 {
        return;
    }

    for (i, page) in pdf.pages().iter().enumerate() {
        let page_dict = page.raw();
        let Some(res_dict) = page_dict.get::<Dict<'_>>(b"Resources" as &[u8]) else {
            continue;
        };
        let Some(cs_dict) = res_dict.get::<Dict<'_>>(b"ColorSpace" as &[u8]) else {
            continue;
        };

        for (name, obj) in cs_dict.entries() {
            if let MaybeRef::NotRef(Object::Array(ref arr)) = obj {
                if let Some(Object::Name(cs_name)) = arr.iter().next() {
                    let cs_bytes = cs_name.as_ref();
                    if cs_bytes == b"DeviceRGB" || cs_bytes == b"CalRGB" || cs_bytes == b"Lab" {
                        error_at(
                            report,
                            "6.2.4.3",
                            format!(
                                "PDF/X-1a forbids RGB/Lab color space /{} ({})",
                                String::from_utf8_lossy(name.as_ref()),
                                String::from_utf8_lossy(cs_bytes),
                            ),
                            format!("Page {}", i + 1),
                        );
                    }
                }
            }
        }
    }
}

fn check_no_transparency(pdf: &Pdf, report: &mut ComplianceReport) {
    for (i, page) in pdf.pages().iter().enumerate() {
        let raw = page.raw();

        if let Some(group) = raw.get::<Dict<'_>>(b"Group" as &[u8]) {
            if let Some(s) = group.get::<Name>(b"S" as &[u8]) {
                if s.as_ref() == b"Transparency" {
                    error_at(
                        report,
                        "6.4",
                        "Transparency group not allowed in PDF/X-1a or PDF/X-3",
                        format!("Page {}", i + 1),
                    );
                }
            }
        }

        let Some(res_dict) = raw.get::<Dict<'_>>(b"Resources" as &[u8]) else {
            continue;
        };
        let Some(gs_dict) = res_dict.get::<Dict<'_>>(b"ExtGState" as &[u8]) else {
            continue;
        };

        for (name, obj) in gs_dict.entries() {
            if let MaybeRef::NotRef(Object::Dict(ref gs)) = obj {
                if gs.contains_key(b"SMask" as &[u8]) {
                    error_at(
                        report,
                        "6.4",
                        format!(
                            "ExtGState /{} has SMask (transparency)",
                            String::from_utf8_lossy(name.as_ref())
                        ),
                        format!("Page {}", i + 1),
                    );
                }
            }
        }
    }
}

fn check_xmp_pdfx(pdf: &Pdf, report: &mut ComplianceReport) {
    let Some(xmp) = check::get_xmp_metadata(pdf) else {
        error(
            report,
            "6.7",
            "PDF/X requires XMP metadata with GTS_PDFXVersion",
        );
        return;
    };

    let text = String::from_utf8_lossy(&xmp);
    if !text.contains("GTS_PDFXVersion") && !text.contains("pdfx:GTS_PDFXVersion") {
        error(
            report,
            "6.7",
            "XMP metadata missing GTS_PDFXVersion identifier",
        );
    }
}

fn check_trapped_key(pdf: &Pdf, report: &mut ComplianceReport) {
    let data = pdf.data().as_ref();
    let has_trapped = data.windows(8).any(|w| w == b"/Trapped");
    if !has_trapped {
        warning(
            report,
            "6.1.12",
            "Info dictionary should contain /Trapped key (True or False)",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdfx_level_properties() {
        assert!(PdfXLevel::X1a2003.forbids_transparency());
        assert!(PdfXLevel::X32003.forbids_transparency());
        assert!(!PdfXLevel::X4.forbids_transparency());
    }

    #[test]
    fn pdfx_level_display() {
        assert_eq!(PdfXLevel::X1a2003.version_string(), "PDF/X-1a:2003");
        assert_eq!(PdfXLevel::X32003.version_string(), "PDF/X-3:2003");
        assert_eq!(PdfXLevel::X4.version_string(), "PDF/X-4");
    }
}
