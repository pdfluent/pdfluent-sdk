//! `EngineError` still has exactly the variants it shipped with at 1.0.0.
//!
//! WHY THIS MATCH HAS NO FALLBACK ARM. `EngineError` is an exhaustive enum,
//! published that way at 1.0.0, so every downstream `match` without a
//! wildcard names all eight variants — and adding a ninth breaks all of
//! them, which is a major-version change, not a minor one. This test is the
//! tripwire for that: it matches `EngineError` exhaustively against the
//! exact variant list of v1.0.0 (`git show
//! v1.0.0:crates/pdf-engine/src/error.rs`), with deliberately no `_` arm, so
//! it FAILS TO COMPILE the day someone adds a variant. New 1.x error cases
//! go to `OptionsError` — `#[non_exhaustive]`, carried inside
//! `EngineError::Io` — never here (#574).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_engine::{EngineError, LimitError};

/// One label per variant. The labels do not matter; that every variant is
/// named, with no wildcard to hide a ninth, is the whole of this test.
fn variant_of(e: &EngineError) -> &'static str {
    match e {
        EngineError::InvalidPdf(_) => "INVALID_PDF",
        EngineError::PageOutOfRange { .. } => "PAGE_OUT_OF_RANGE",
        EngineError::RenderError(_) => "RENDER_ERROR",
        EngineError::Io(_) => "IO_ERROR",
        EngineError::Encrypted(_) => "ENCRYPTED",
        EngineError::InvalidPageGeometry { .. } => "INVALID_PAGE_GEOMETRY",
        EngineError::XfaFlattenFailed(_) => "XFA_FLATTEN_FAILED",
        EngineError::LimitExceeded(_) => "LIMIT_EXCEEDED",
    }
}

#[test]
fn an_engine_error_match_names_every_v1_0_0_variant() {
    let cases: Vec<(EngineError, &'static str)> = vec![
        (EngineError::InvalidPdf("x".into()), "INVALID_PDF"),
        (
            EngineError::PageOutOfRange { index: 9, count: 1 },
            "PAGE_OUT_OF_RANGE",
        ),
        (EngineError::RenderError("x".into()), "RENDER_ERROR"),
        (EngineError::Io(std::io::Error::other("x")), "IO_ERROR"),
        (EngineError::Encrypted("x".into()), "ENCRYPTED"),
        (
            EngineError::InvalidPageGeometry {
                width: 0.0,
                height: 0.0,
                reason: "x".into(),
            },
            "INVALID_PAGE_GEOMETRY",
        ),
        (
            EngineError::XfaFlattenFailed("x".into()),
            "XFA_FLATTEN_FAILED",
        ),
        (
            EngineError::LimitExceeded(LimitError::FileTooLarge {
                actual_bytes: 1,
                limit_bytes: 0,
            }),
            "LIMIT_EXCEEDED",
        ),
    ];
    for (e, expected) in &cases {
        assert_eq!(variant_of(e), *expected);
    }
}
