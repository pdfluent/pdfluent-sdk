//! Integration tests for issue #1429 — `ProcessingLimits` enforcement
//! at the public open-path boundary.
//!
//! Today only `max_file_bytes` is enforced (the file-size cap fires
//! before any allocation). The remaining caps in `ProcessingLimits`
//! (stream-size, image-pixels, object-depth, operator-count, XFA /
//! FormCalc nesting) require parser-internal hooks tracked in the
//! same issue. These tests pin the wired path so the contract is
//! explicit; tests for the deeper hooks land alongside their wiring.

use pdfluent::{
    Error, OpenOptions, PdfDocument, ProcessingLimits, ResourceLimitKind, ToImagesOptions,
};
use std::fmt::Write as _;

const FIXTURE_PATH: &str = "tests/fixtures/sample.pdf";

fn pdf_from_objects(objects: &[String]) -> Vec<u8> {
    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = Vec::with_capacity(objects.len() + 1);
    offsets.push(0usize);

    for (idx, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        writeln!(&mut out, "{} 0 obj", idx + 1).unwrap();
        out.push_str(object);
        out.push_str("\nendobj\n");
    }

    let xref_offset = out.len();
    writeln!(&mut out, "xref\n0 {}", objects.len() + 1).unwrap();
    out.push_str("0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        writeln!(&mut out, "{offset:010} 00000 n ").unwrap();
    }
    write!(
        &mut out,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
        objects.len() + 1,
        xref_offset
    )
    .unwrap();

    out.into_bytes()
}

fn deeply_nested_page_tree_pdf(page_tree_depth: usize) -> Vec<u8> {
    let mut objects = vec!["<< /Type /Catalog /Pages 2 0 R >>".to_string()];

    for idx in 0..page_tree_depth {
        let next_id = idx + 3;
        objects.push(format!("<< /Type /Pages /Kids [{next_id} 0 R] /Count 1 >>"));
    }

    let parent_id = page_tree_depth + 1;
    objects.push(format!(
        "<< /Type /Page /Parent {parent_id} 0 R /MediaBox [0 0 72 72] >>"
    ));

    pdf_from_objects(&objects)
}

fn image_xobject_pdf(width: i64, height: i64) -> Vec<u8> {
    let image_obj = format!(
        "<< /Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length 0 >>\nstream\nendstream"
    );
    let content = "q /Im1 Do Q\n";
    let content_obj = format!(
        "<< /Length {} >>\nstream\n{}endstream",
        content.len(),
        content
    );
    let objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 72] /Resources << /XObject << /Im1 4 0 R >> >> /Contents 5 0 R >>"),
        image_obj,
        content_obj,
    ];
    pdf_from_objects(&objects)
}

fn operator_runaway_pdf(save_restore_pairs: usize) -> Vec<u8> {
    let mut content = String::new();
    for _ in 0..save_restore_pairs {
        content.push_str("q Q\n");
    }
    content.push_str("BT /F1 12 Tf 10 10 Td (late text) Tj ET\n");

    let objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 72] /Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >> /Contents 4 0 R >>".to_string(),
        format!(
            "<< /Length {} >>\nstream\n{}endstream",
            content.len(),
            content
        ),
    ];

    pdf_from_objects(&objects)
}

// ---------------------------------------------------------------------------
// File-size limit — bytes path
// ---------------------------------------------------------------------------

/// Below the cap: open succeeds.
#[test]
fn processing_limits_file_size_under_cap_opens() {
    let bytes = std::fs::read(FIXTURE_PATH).expect("fixture");
    let limits = ProcessingLimits::default().max_file_bytes(10 * 1024 * 1024);
    let opts = OpenOptions::new().with_processing_limits(limits);
    let doc = PdfDocument::from_bytes_with(&bytes, opts).expect("open under cap");
    assert!(doc.page_count() >= 1);
}

/// Above the cap: returns ResourceLimitExceeded with FileTooLarge kind.
#[test]
fn processing_limits_file_size_over_cap_rejects() {
    let bytes = std::fs::read(FIXTURE_PATH).expect("fixture");
    // Set the cap below the fixture's size so the check fires.
    let cap = (bytes.len() as u64).saturating_sub(1);
    let limits = ProcessingLimits::default().max_file_bytes(cap);
    let opts = OpenOptions::new().with_processing_limits(limits);

    let err = PdfDocument::from_bytes_with(&bytes, opts).expect_err("over cap should reject");
    match err {
        Error::ResourceLimitExceeded {
            kind,
            observed,
            limit,
        } => {
            assert_eq!(kind, ResourceLimitKind::FileTooLarge);
            assert_eq!(observed, bytes.len() as u64);
            assert_eq!(limit, cap);
        }
        other => panic!("expected ResourceLimitExceeded, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// File-size limit — file path
// ---------------------------------------------------------------------------

/// File path with a tight cap: the metadata check happens before the
/// file is read into memory, so a real read never occurs on rejection.
#[test]
fn processing_limits_file_size_path_over_cap_rejects() {
    let bytes = std::fs::read(FIXTURE_PATH).expect("fixture");
    let cap = (bytes.len() as u64).saturating_sub(1);
    let limits = ProcessingLimits::default().max_file_bytes(cap);
    let opts = OpenOptions::new().with_processing_limits(limits);

    let err = PdfDocument::open_with(FIXTURE_PATH, opts).expect_err("over cap should reject");
    match err {
        Error::ResourceLimitExceeded {
            kind: ResourceLimitKind::FileTooLarge,
            observed,
            limit,
        } => {
            assert_eq!(observed, bytes.len() as u64);
            assert_eq!(limit, cap);
        }
        other => panic!("expected ResourceLimitExceeded(FileTooLarge), got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Object-depth limit — page tree
// ---------------------------------------------------------------------------

#[test]
fn processing_limits_object_depth_rejects_deeply_nested_page_tree() {
    let bytes = deeply_nested_page_tree_pdf(4);

    let permissive =
        OpenOptions::new().with_processing_limits(ProcessingLimits::default().max_object_depth(16));
    let doc = PdfDocument::from_bytes_with(&bytes, permissive).expect("open under cap");
    assert_eq!(doc.page_count(), 1);

    let strict_cap = 2;
    let strict = OpenOptions::new()
        .with_processing_limits(ProcessingLimits::default().max_object_depth(strict_cap));

    match PdfDocument::from_bytes_with(&bytes, strict) {
        Ok(doc) => assert_eq!(
            doc.page_count(),
            0,
            "strict cap should stop traversal before collecting the nested page"
        ),
        Err(Error::ResourceLimitExceeded {
            kind: ResourceLimitKind::ObjectDepthExceeded,
            observed,
            limit,
        }) => {
            assert!(observed > limit);
            assert_eq!(limit, strict_cap as u64);
        }
        Err(Error::InvalidPdf { .. }) => {}
        Err(other) => panic!("expected object-depth rejection or partial traversal, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Operator-count limit — content stream
// ---------------------------------------------------------------------------

#[test]
fn processing_limits_operator_count_aborts_runaway_stream() {
    let bytes = operator_runaway_pdf(100);

    let permissive = OpenOptions::new()
        .with_processing_limits(ProcessingLimits::default().max_operator_count(512));
    let doc = PdfDocument::from_bytes_with(&bytes, permissive).expect("open under cap");
    let text = doc.extract_text().expect("extract under cap");
    assert!(text.contains("late text"));

    let strict = OpenOptions::new()
        .with_processing_limits(ProcessingLimits::default().max_operator_count(50));
    let doc = PdfDocument::from_bytes_with(&bytes, strict).expect("open strict cap");
    let text = doc.extract_text().expect("strict cap should not panic");
    assert!(
        !text.contains("late text"),
        "operator cap should stop interpretation before late text operators"
    );
}

// ---------------------------------------------------------------------------
// Image pixel limit — pre-render check in to_images
// ---------------------------------------------------------------------------

#[test]
fn processing_limits_image_pixels_rejects_oversized() {
    let bytes = image_xobject_pdf(10_000, 10_000); // 100 MP

    let strict = OpenOptions::new()
        .with_license_key("tier:business")
        .with_processing_limits(ProcessingLimits::default().max_image_pixels(1_000));
    let doc = PdfDocument::from_bytes_with(&bytes, strict).expect("open strict cap");

    let dir = std::env::temp_dir().join("pdfluent-processing-limits-image");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let pattern = dir.join("page_{page}.png");

    let err = doc
        .to_images(&pattern, ToImagesOptions::new().with_dpi(72))
        .expect_err("oversized image should reject");

    match err {
        Error::ResourceLimitExceeded {
            kind: ResourceLimitKind::ImageTooLarge,
            observed,
            limit,
        } => {
            assert_eq!(observed, 100_000_000);
            assert_eq!(limit, 1_000);
        }
        other => panic!("expected ImageTooLarge, got {other:?}"),
    }

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// Backward compatibility — strict_memory_limit still works
// ---------------------------------------------------------------------------

/// `strict_memory_limit` (the legacy entry point) keeps producing the
/// historical `MemoryBudgetExceeded` variant — not the new typed one.
/// This pins the back-compat boundary so callers on the old API don't
/// silently start seeing a different error.
#[test]
fn legacy_strict_memory_limit_still_returns_memory_budget_error() {
    let bytes = std::fs::read(FIXTURE_PATH).expect("fixture");
    let opts = OpenOptions::new().strict_memory_limit(bytes.len() - 1);
    let err = PdfDocument::from_bytes_with(&bytes, opts).expect_err("over cap should reject");
    assert!(
        matches!(err, Error::MemoryBudgetExceeded { .. }),
        "legacy strict_memory_limit must keep returning MemoryBudgetExceeded; got {err:?}"
    );
}

/// When BOTH limits are set, ProcessingLimits is checked first (the
/// stricter typed path). This ordering is documented on the
/// `with_processing_limits` builder.
#[test]
fn processing_limits_takes_precedence_when_both_set() {
    let bytes = std::fs::read(FIXTURE_PATH).expect("fixture");
    // Tight file_bytes cap; strict_memory_limit is set to a generous
    // value that would not fire on its own.
    let cap = (bytes.len() as u64).saturating_sub(1);
    let limits = ProcessingLimits::default().max_file_bytes(cap);
    let opts = OpenOptions::new()
        .with_processing_limits(limits)
        .strict_memory_limit(bytes.len() * 100);

    let err = PdfDocument::from_bytes_with(&bytes, opts).expect_err("typed cap should fire first");
    assert!(
        matches!(
            err,
            Error::ResourceLimitExceeded {
                kind: ResourceLimitKind::FileTooLarge,
                ..
            }
        ),
        "ProcessingLimits typed variant should win over the legacy one; got {err:?}"
    );
}

// ---------------------------------------------------------------------------
// Display + error code stability
// ---------------------------------------------------------------------------

#[test]
fn resource_limit_error_has_stable_code_and_url() {
    let err = Error::ResourceLimitExceeded {
        kind: ResourceLimitKind::ImageTooLarge,
        observed: 1_000_000,
        limit: 100_000,
    };
    assert_eq!(err.code(), "E-BUDGET-RESOURCE-LIMIT");
    assert_eq!(
        err.docs_url(),
        "https://pdfluent.com/errors/E-BUDGET-RESOURCE-LIMIT"
    );
    let msg = format!("{err}");
    assert!(msg.contains("Resource limit exceeded"));
    assert!(msg.contains("image too large"));
    assert!(msg.contains("1000000"));
    assert!(msg.contains("100000"));
}
