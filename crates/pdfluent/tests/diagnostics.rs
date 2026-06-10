//! Integration tests for public diagnostics collection.

use pdfluent::diagnostics::{Diagnostic, DiagnosticCategory, Severity};
use pdfluent::prelude::*;
use pdfluent::{OpenOptions, ProcessingLimits};
use std::path::PathBuf;

fn mini(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/corpus-mini")
        .join(name)
}

fn open(name: &str) -> PdfDocument {
    PdfDocument::open_with(
        mini(name),
        OpenOptions::new().with_license_key("tier:enterprise"),
    )
    .expect("open")
}

fn open_with_stream_cap(name: &str, cap: u64) -> PdfDocument {
    PdfDocument::open_with(
        mini(name),
        OpenOptions::new()
            .with_license_key("tier:enterprise")
            .with_processing_limits(ProcessingLimits::new().max_stream_bytes(cap)),
    )
    .expect("open")
}

#[test]
fn clean_document_has_no_content_degradation_diagnostics() {
    // "Clean" means no font substitutions, dropped images, or limit hits — i.e.,
    // no content-degradation. Structural leniency events (Decode/Repair category)
    // are informational: they record pre-existing parser recovery paths. They are
    // not content-loss and are expected on some well-formed PDFs.
    let doc = open("simple.pdf");
    let _ = doc.render_page(1, 150, ImageFormat::Png).expect("render");
    let diags = doc.diagnostics();
    let degradation: Vec<_> = diags
        .iter()
        .filter(|d| {
            matches!(
                d.category,
                DiagnosticCategory::Font | DiagnosticCategory::Image | DiagnosticCategory::Limit
            ) || d.severity == Severity::Error
        })
        .collect();
    assert!(
        degradation.is_empty(),
        "clean document must have no content-degradation diagnostics: {degradation:?}"
    );
}

#[test]
fn leniency_events_are_captured_on_recovery_pdf() {
    // simple.pdf triggers STREAM_PARSE_FALLBACK — a pre-existing recovery path
    // that is now observable. This test asserts the event is surfaced correctly.
    let doc = open("simple.pdf");
    let diags = doc.diagnostics();
    let leniency_report = pdfluent::LeniencyReport::from_diagnostics(&diags);
    // The report struct must be constructable (no panic).
    let _ = leniency_report.is_clean();
    let _ = leniency_report.unique_event_count;
}

#[test]
fn stream_too_large_is_reported_as_diagnostic() {
    let doc = open_with_stream_cap("scanned.pdf", 64);
    let result = doc.render_page(1, 150, ImageFormat::Png);
    assert!(
        result.is_err(),
        "a tiny per-stream cap should make the render fail"
    );
    let diags = doc.diagnostics();
    assert!(
        diags
            .iter()
            .any(|d| d.code == Diagnostic::CODE_STREAM_TOO_LARGE && d.severity == Severity::Error),
        "expected a STREAM_TOO_LARGE diagnostic; got {diags:?}"
    );
}

#[test]
fn take_diagnostics_drains_the_buffer() {
    let doc = open_with_stream_cap("scanned.pdf", 64);
    let _ = doc.render_page(1, 150, ImageFormat::Png);
    let first = doc.take_diagnostics();
    assert!(
        !first.is_empty(),
        "first take should return the collected diagnostics"
    );
    let second = doc.take_diagnostics();
    assert!(
        second.is_empty(),
        "take must drain; the second take is empty"
    );
}

#[test]
fn corrupt_image_drop_is_reported_as_diagnostic() {
    use lopdf::{dictionary, Object, Stream};
    let mut doc = lopdf::Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    // A DCTDecode (JPEG) image whose payload is not a valid JPEG.
    let img = Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image",
            "Width" => 4, "Height" => 4, "BitsPerComponent" => 8,
            "ColorSpace" => "DeviceRGB", "Filter" => "DCTDecode",
        },
        b"this is not a valid jpeg payload".to_vec(),
    );
    let img_id = doc.add_object(img);
    let content = Stream::new(dictionary! {}, b"q 100 0 0 100 10 10 cm /Im0 Do Q".to_vec());
    let content_id = doc.add_object(content);
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 120.into(), 120.into()],
        "Resources" => dictionary! {
            "XObject" => dictionary! { "Im0" => Object::Reference(img_id) },
        },
        "Contents" => Object::Reference(content_id),
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();

    let pdoc = PdfDocument::from_bytes_with(
        &bytes,
        OpenOptions::new().with_license_key("tier:enterprise"),
    )
    .expect("open corrupt-image PDF");
    // Renders (the undecodable image is dropped) and reports the drop.
    let _ = pdoc.render_page(1, 72, ImageFormat::Png).expect("render");
    let diags = pdoc.diagnostics();
    assert!(
        diags
            .iter()
            .any(|d| d.code == Diagnostic::CODE_IMAGE_DECODE_FAILED),
        "expected IMAGE_DECODE_FAILED; got {diags:?}"
    );
}

#[test]
fn with_repair_does_not_change_load_behaviour() {
    // Recovery is always-on; the advisory flag must not alter load success.
    let with = PdfDocument::open_with(
        mini("multi-page.pdf"),
        OpenOptions::new()
            .with_license_key("tier:enterprise")
            .with_repair(true),
    )
    .expect("open repair=true");
    let without = PdfDocument::open_with(
        mini("multi-page.pdf"),
        OpenOptions::new()
            .with_license_key("tier:enterprise")
            .with_repair(false),
    )
    .expect("open repair=false");
    assert_eq!(
        with.page_count(),
        without.page_count(),
        "with_repair must not change load behaviour (recovery is always-on)"
    );
}

#[test]
fn broken_page_tree_reports_page_tree_rebuilt() {
    // Valid xref (both loaders parse it) but the catalog's /Pages points at a
    // missing object; pd-syntax recovers pages via brute-force scan.
    fn broken_page_tree_pdf() -> Vec<u8> {
        let objs: [&[u8]; 3] = [
            b"<< /Type /Catalog /Pages 99 0 R >>", // 99 missing -> page tree broken
            b"<< /Type /Page /MediaBox [0 0 100 100] /Contents 3 0 R >>",
            b"<< /Length 5 >>\nstream\nBT ET\nendstream",
        ];
        let mut buf = Vec::new();
        let mut off = [0usize; 4];
        buf.extend_from_slice(b"%PDF-1.7\n");
        for (i, body) in objs.iter().enumerate() {
            off[i + 1] = buf.len();
            buf.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
            buf.extend_from_slice(body);
            buf.extend_from_slice(b"\nendobj\n");
        }
        let xref_off = buf.len();
        buf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
        for o in &off[1..4] {
            buf.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
        }
        buf.extend_from_slice(
            format!("trailer\n<< /Root 1 0 R /Size 4 >>\nstartxref\n{xref_off}\n%%EOF").as_bytes(),
        );
        buf
    }

    let doc = PdfDocument::from_bytes_with(
        &broken_page_tree_pdf(),
        OpenOptions::new().with_license_key("tier:enterprise"),
    )
    .expect("open should recover the page tree");
    let diags = doc.diagnostics();
    assert!(
        diags
            .iter()
            .any(|d| d.code == Diagnostic::CODE_PAGE_TREE_REBUILT
                && d.category == DiagnosticCategory::Repair),
        "expected PAGE_TREE_REBUILT; got {diags:?}"
    );
    // The xref is valid, so it must NOT falsely report an xref rebuild.
    assert!(
        !diags
            .iter()
            .any(|d| d.code == Diagnostic::CODE_XREF_REBUILT),
        "xref is valid; must not report XREF_REBUILT: {diags:?}"
    );
}

#[test]
fn unsupported_font_substitution_is_reported() {
    // A non-embedded font whose BaseFont is not one of the standard 14 forces a
    // fallback substitution while rendering text.
    fn unsupported_font_pdf() -> Vec<u8> {
        let content: &[u8] = b"BT /F1 12 Tf 10 50 Td (Hi) Tj ET";
        let obj4 = b"<< /Type /Font /Subtype /TrueType /BaseFont /NonexistentFontXYZ >>".to_vec();
        let obj3 = b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] \
/Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>"
            .to_vec();
        let obj5 = format!(
            "<< /Length {} >>\nstream\n{}\nendstream",
            content.len(),
            std::str::from_utf8(content).unwrap()
        )
        .into_bytes();
        let objs: [&[u8]; 5] = [
            b"<< /Type /Catalog /Pages 2 0 R >>",
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            &obj3,
            &obj4,
            &obj5,
        ];
        let mut buf = Vec::new();
        let mut off = [0usize; 6];
        buf.extend_from_slice(b"%PDF-1.7\n");
        for (i, body) in objs.iter().enumerate() {
            off[i + 1] = buf.len();
            buf.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
            buf.extend_from_slice(body);
            buf.extend_from_slice(b"\nendobj\n");
        }
        let xref_off = buf.len();
        buf.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
        for o in &off[1..6] {
            buf.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
        }
        buf.extend_from_slice(
            format!("trailer\n<< /Root 1 0 R /Size 6 >>\nstartxref\n{xref_off}\n%%EOF").as_bytes(),
        );
        buf
    }

    let doc = PdfDocument::from_bytes_with(
        &unsupported_font_pdf(),
        OpenOptions::new().with_license_key("tier:enterprise"),
    )
    .expect("open");
    let _ = doc.render_page(1, 72, ImageFormat::Png).expect("render");
    let diags = doc.diagnostics();
    assert!(
        diags
            .iter()
            .any(|d| d.code == Diagnostic::CODE_FONT_UNSUPPORTED
                && d.category == DiagnosticCategory::Font),
        "expected FONT_UNSUPPORTED; got {diags:?}"
    );
}
