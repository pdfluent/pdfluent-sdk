//! Error types for the rendering engine.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::api_error::PdfError;

/// Errors that can occur in the rendering engine.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// The PDF data is invalid or could not be parsed.
    #[error("invalid PDF: {0}")]
    InvalidPdf(String),

    /// A page index is out of range.
    #[error("page {index} out of range (document has {count} pages)")]
    PageOutOfRange {
        /// The requested page index.
        index: usize,
        /// Total number of pages.
        count: usize,
    },

    /// A rendering error occurred.
    #[error("render error: {0}")]
    RenderError(String),

    /// An I/O error occurred.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// The PDF is encrypted and requires a password to open.
    #[error("PDF is encrypted: {0}")]
    Encrypted(String),

    /// The page has invalid or degenerate dimensions.
    #[error("invalid page geometry (w={width:.2}pt, h={height:.2}pt): {reason}")]
    InvalidPageGeometry {
        /// Page width in points.
        width: f32,
        /// Page height in points.
        height: f32,
        /// Human-readable reason for the failure.
        reason: String,
    },

    /// XFA flattening failed.
    #[error("XFA flatten failed: {0}")]
    XfaFlattenFailed(String),

    /// A configured resource limit was exceeded during rendering or
    /// text extraction.  Carries the [`LimitError`] so callers can
    /// branch on the specific cap that fired.
    ///
    /// [`LimitError`]: crate::limits::LimitError
    #[error("resource limit exceeded: {0}")]
    LimitExceeded(crate::limits::LimitError),
}

impl EngineError {
    /// If this error is an OCR failure carried as [`EngineError::Io`], the
    /// failure's message.
    ///
    /// OCR failures ride as I/O errors because this enum is exhaustive in
    /// 1.0.0 and a new variant would break semver: engine-construction
    /// failures carry the typed `pdf_ocr::TesseractCliError`, everything else
    /// an [`OcrError`], both via [`std::io::Error::other`]. Callers that need
    /// the typed cause downcast [`std::io::Error::get_ref`]; callers that only
    /// need the message read it here. An OS error, and any other variant,
    /// reads as `None`.
    pub fn ocr_message(&self) -> Option<String> {
        let EngineError::Io(e) = self else {
            return None;
        };
        let cause = e.get_ref()?;
        #[cfg(feature = "ocr-cli")]
        if let Some(cli) = cause.downcast_ref::<pdf_ocr::TesseractCliError>() {
            return Some(cli.to_string());
        }
        cause
            .downcast_ref::<OcrError>()
            .map(|ocr| ocr.message().to_string())
    }

    /// An engine-side OCR failure: a malformed request, a build without an
    /// engine, or an internal step that failed. See [`EngineError::ocr_message`]
    /// for why it rides as [`EngineError::Io`].
    pub(crate) fn ocr_failure(message: impl Into<String>) -> Self {
        EngineError::Io(std::io::Error::other(OcrError::new(message)))
    }

    /// An OCR engine that could not be constructed, typed so callers can
    /// downcast to it. See [`EngineError::ocr_message`].
    #[cfg(feature = "ocr-cli")]
    pub(crate) fn ocr_engine_failure(e: pdf_ocr::TesseractCliError) -> Self {
        EngineError::Io(std::io::Error::other(e))
    }
}

/// An OCR failure on the engine side of the recognition: a malformed request
/// (`searchable_copy` with `Never`, a zero dpi), a build without an engine,
/// or an internal step that failed (reopening the copy, the pipeline, ...).
///
/// Carried inside [`EngineError::Io`], where engine-construction failures
/// carry the typed `pdf_ocr::TesseractCliError` instead; see
/// [`EngineError::ocr_message`].
///
/// This is not [`crate::ocr::OcrError`]: that one is what an [`OcrBackend`]
/// reports, this one is what the extraction path reports around it.
///
/// [`OcrBackend`]: crate::ocr::OcrBackend
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct OcrError {
    message: String,
}

impl OcrError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// The failure, as it reads after the `OCR error: ` prefix the mappings
    /// put in front of it.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for OcrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for OcrError {}

impl PdfError for EngineError {
    fn code(&self) -> &str {
        match self {
            EngineError::InvalidPdf(_) => "INVALID_PDF",
            EngineError::PageOutOfRange { .. } => "PAGE_OUT_OF_RANGE",
            EngineError::RenderError(_) => "RENDER_ERROR",
            // An OptionsError rides inside Io, but it is a mistake in the
            // call, not an OS failure, so it reports its own code (#574).
            EngineError::Io(_) if self.options().is_some() => "INVALID_OPTIONS",
            EngineError::Io(_) => "IO_ERROR",
            EngineError::Encrypted(_) => "ENCRYPTED",
            EngineError::InvalidPageGeometry { .. } => "INVALID_PAGE_GEOMETRY",
            EngineError::XfaFlattenFailed(_) => "XFA_FLATTEN_FAILED",
            EngineError::LimitExceeded(_) => "LIMIT_EXCEEDED",
        }
    }

    fn help(&self) -> Option<String> {
        match self {
            EngineError::InvalidPdf(_) => {
                Some("The PDF file is corrupt or invalid. Try using a PDF repair tool or re-save the document.".to_string())
            }
            EngineError::PageOutOfRange { index, count } => {
                Some(format!("Page {} does not exist. This document has {} pages (0-{}).", index, count, count - 1))
            }
            EngineError::RenderError(_) => {
                Some("Rendering failed. Check that the page exists and the document is not corrupted.".to_string())
            }
            EngineError::Io(_) => {
                Some("An I/O error occurred. Check file permissions and disk space.".to_string())
            }
            EngineError::Encrypted(_) => {
                Some("The PDF is encrypted. Provide the correct password to open it.".to_string())
            }
            EngineError::InvalidPageGeometry { .. } => {
                Some("The page has invalid or degenerate dimensions and cannot be rendered.".to_string())
            }
            EngineError::XfaFlattenFailed(_) => {
                Some("XFA flattening produced an invalid PDF. The document may use unsupported XFA features.".to_string())
            }
            EngineError::LimitExceeded(_) => {
                Some("A processing resource limit was exceeded. Raise the relevant cap in ProcessingLimits or check that the input is not adversarial.".to_string())
            }
        }
    }
}

/// Option and selection errors: mistakes in the call, not faults in the
/// document or the engine.
///
/// `#[non_exhaustive]`, and that is the semver contract: new 1.x error cases
/// arrive as new variants of THIS enum, never as new variants of
/// [`EngineError`]. `EngineError` is an exhaustive enum published at 1.0.0,
/// so adding a variant to it breaks every downstream exhaustive `match` — a
/// major-version change. These cases travel inside [`EngineError::Io`] with
/// [`std::io::ErrorKind::InvalidInput`] instead, readable through
/// [`EngineError::options`], and matching on this enum always keeps a
/// fallback arm.
#[non_exhaustive]
#[derive(Debug, Clone, thiserror::Error)]
pub enum OptionsError {
    /// `pages` and the `first_page`/`page_count` window were combined.
    #[error("cannot combine pages with first_page/page_count: pass one or the other")]
    ConflictingPageSelection,
    /// A clip margin was negative or not finite.
    #[error("invalid clip margins: {side} must be finite and at least 0, got {value}")]
    InvalidClipMargins {
        /// Which side: `"left"`, `"top"`, `"right"` or `"bottom"`.
        side: &'static str,
        /// What was asked for: negative, or not finite.
        value: f64,
    },
    /// `quality` was [`Quality::Balanced`](crate::Quality) on a call that
    /// returns blocks: the balanced tier assembles markdown, so it is read by
    /// `extract_markdown` only (#641).
    #[error("quality `balanced` is markdown-only: it is read by extract_markdown, not by extract_blocks")]
    BalancedIsMarkdownOnly,
    /// TATR table structure was asked for on a build without it. The model
    /// runs only behind the `ort-tables` feature; without it the request
    /// is refused, never run as fast.
    #[error("tatr table structure needs the `ort-tables` build; this build has no ONNX Runtime")]
    TatrNeedsOrtTables,
}

impl PartialEq for OptionsError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::ConflictingPageSelection, Self::ConflictingPageSelection)
            | (Self::BalancedIsMarkdownOnly, Self::BalancedIsMarkdownOnly)
            | (Self::TatrNeedsOrtTables, Self::TatrNeedsOrtTables) => true,
            (
                Self::InvalidClipMargins { side, value },
                Self::InvalidClipMargins {
                    side: other_side,
                    value: other_value,
                },
            ) => side == other_side && value.to_bits() == other_value.to_bits(),
            _ => false,
        }
    }
}

impl Eq for OptionsError {}

impl EngineError {
    /// The [`OptionsError`] carried by this error, if any.
    ///
    /// An additive inherent method, so adding it is semver-safe. Callers
    /// that used to match `EngineError::ConflictingPageSelection` check this
    /// first, before whatever they do with a plain [`EngineError::Io`].
    pub fn options(&self) -> Option<&OptionsError> {
        match self {
            EngineError::Io(e) if e.kind() == std::io::ErrorKind::InvalidInput => {
                e.get_ref()?.downcast_ref::<OptionsError>()
            }
            _ => None,
        }
    }

    /// The [`ModelError`](crate::model::ModelError) carried by this error, if
    /// any (#641).
    ///
    /// Model failures ride as [`EngineError::Io`] because this enum is
    /// exhaustive in 1.0.0 and a new variant would break semver; this walks
    /// the chain back to the typed cause, on every build. Branch on
    /// [`ModelErrorKind::code`](crate::model::ModelErrorKind::code).
    pub fn model_error(&self) -> Option<&crate::model::ModelError> {
        match self {
            EngineError::Io(e) => crate::model::model_error(e),
            _ => None,
        }
    }
}

/// Convenience type alias.
pub type Result<T> = std::result::Result<T, EngineError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypted_error_code() {
        let e = EngineError::Encrypted("bad password".into());
        assert_eq!(e.code(), "ENCRYPTED");
    }

    #[test]
    fn invalid_page_geometry_error_code() {
        let e = EngineError::InvalidPageGeometry {
            width: 0.5,
            height: 0.5,
            reason: "test".into(),
        };
        assert_eq!(e.code(), "INVALID_PAGE_GEOMETRY");
    }

    #[test]
    fn xfa_flatten_failed_error_code() {
        let e = EngineError::XfaFlattenFailed("corrupt output".into());
        assert_eq!(e.code(), "XFA_FLATTEN_FAILED");
    }

    #[test]
    fn options_ignores_a_non_invalid_input_kind() {
        // options() only reads an OptionsError off InvalidInput: the same
        // payload under any other kind is not an OptionsError (#574).
        let e = EngineError::Io(std::io::Error::other(
            OptionsError::ConflictingPageSelection,
        ));
        assert_eq!(e.options(), None);
    }

    #[test]
    fn options_ignores_an_invalid_input_without_a_payload() {
        // The mirror case: InvalidInput kind, but a plain message rather
        // than an OptionsError payload, is still not an OptionsError (#574).
        let e = EngineError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "plain",
        ));
        assert_eq!(e.options(), None);
    }

    #[test]
    fn an_engine_side_ocr_failure_reads_through_ocr_message() {
        let e = EngineError::ocr_failure("ocr_dpi must be positive");
        assert!(matches!(e, EngineError::Io(_)));
        assert_eq!(e.ocr_message().as_deref(), Some("ocr_dpi must be positive"));
        // And the typed cause is there for callers that downcast.
        let EngineError::Io(io) = &e else {
            panic!("an OCR failure rides as Io");
        };
        let cause = io.get_ref().expect("io::Error::other carries its cause");
        assert_eq!(
            cause.downcast_ref::<OcrError>().map(OcrError::message),
            Some("ocr_dpi must be positive")
        );
    }

    #[cfg(feature = "ocr-cli")]
    #[test]
    fn an_engine_construction_failure_reads_typed_through_ocr_message() {
        let e = EngineError::ocr_engine_failure(pdf_ocr::TesseractCliError::Other(
            "tesseract language data for 'xx' is not installed".to_string(),
        ));
        assert!(matches!(e, EngineError::Io(_)));
        assert_eq!(
            e.ocr_message().as_deref(),
            Some("tesseract language data for 'xx' is not installed")
        );
        let EngineError::Io(io) = &e else {
            panic!("an OCR failure rides as Io");
        };
        assert!(
            io.get_ref()
                .is_some_and(|cause| cause.is::<pdf_ocr::TesseractCliError>()),
            "callers downcast through io::Error::get_ref()"
        );
    }

    #[test]
    fn an_os_error_and_other_variants_are_not_ocr_failures() {
        let os = EngineError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "gone"));
        assert_eq!(os.ocr_message(), None);
        let render = EngineError::RenderError("x".to_string());
        assert_eq!(render.ocr_message(), None);
    }
    #[test]
    fn clip_margins_errors_compare_by_side_and_bits() {
        // `PartialEq` is manual because `value` is an `f64`: the bits
        // compare, so even NaN is reflexive and `Eq` holds.
        let nan = || OptionsError::InvalidClipMargins {
            side: "left",
            value: f64::NAN,
        };
        assert_eq!(nan(), nan());
        assert_eq!(
            OptionsError::InvalidClipMargins {
                side: "top",
                value: -1.0
            },
            OptionsError::InvalidClipMargins {
                side: "top",
                value: -1.0
            },
        );
        assert_ne!(
            OptionsError::InvalidClipMargins {
                side: "left",
                value: -1.0
            },
            OptionsError::InvalidClipMargins {
                side: "right",
                value: -1.0
            },
        );
        assert_ne!(
            OptionsError::InvalidClipMargins {
                side: "left",
                value: -1.0
            },
            OptionsError::InvalidClipMargins {
                side: "left",
                value: -2.0
            },
        );
        assert_ne!(
            OptionsError::InvalidClipMargins {
                side: "left",
                value: -1.0
            },
            OptionsError::ConflictingPageSelection,
        );
    }

    #[cfg(feature = "ort")]
    #[test]
    fn a_model_error_round_trips_through_io_to_engine_error_and_back() {
        // The shared ONNX layer reports without a new EngineError variant
        // (#641): ModelError -> io::Error::other -> EngineError::Io, and
        // `model_error` walks the chain back to the same kind.
        let spec = pdf_ocr::runtime::ModelSpec::new(
            "roundtrip",
            "roundtrip.onnx",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "install roundtrip",
        );
        let config = pdf_ocr::runtime::RuntimeConfig::new()
            .with_model_dir("ort-roundtrip-test-no-such-dir")
            .with_ort_dylib("ort-roundtrip-test-no-such-lib");
        let model =
            pdf_ocr::runtime::session(&spec, &config).expect_err("a missing model dir must fail");
        assert_eq!(
            model.kind,
            pdf_ocr::runtime::ModelErrorKind::ModelNotAvailable
        );
        let engine = EngineError::from(std::io::Error::from(model));
        // The walk starts at the io::Error: EngineError::Io is transparent,
        // which forwards source() past the payload rather than to it, so a
        // walk from &engine cannot see it (see model_error's docs).
        let EngineError::Io(io) = &engine else {
            panic!("a model failure rides as Io");
        };
        let back = pdf_ocr::runtime::model_error(io).expect("the chain carries the ModelError");
        assert_eq!(
            back.kind,
            pdf_ocr::runtime::ModelErrorKind::ModelNotAvailable
        );
        assert_eq!(back.kind.code(), "MODEL_NOT_AVAILABLE");
    }
}
