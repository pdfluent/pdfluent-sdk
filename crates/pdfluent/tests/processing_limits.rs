//! Integration tests for issue #1429 — `ProcessingLimits` enforcement
//! at the public open-path boundary.
//!
//! Today only `max_file_bytes` is enforced (the file-size cap fires
//! before any allocation). The remaining caps in `ProcessingLimits`
//! (stream-size, image-pixels, object-depth, operator-count, XFA /
//! FormCalc nesting) require parser-internal hooks tracked in the
//! same issue. These tests pin the wired path so the contract is
//! explicit; tests for the deeper hooks land alongside their wiring.

use pdfluent::{Error, OpenOptions, PdfDocument, ProcessingLimits, ResourceLimitKind};

const FIXTURE_PATH: &str = "tests/fixtures/sample.pdf";

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
