//! Public diagnostics model.
//!
//! A stable, engine-agnostic view of the warnings, fallbacks and degradations
//! that occurred while processing a document — font substitutions, dropped
//! images, exceeded resource limits. Diagnostics let an enterprise caller tell
//! a clean render apart from a silently-degraded one.
//!
//! Collected diagnostics are read via [`PdfDocument::diagnostics`] and
//! [`PdfDocument::take_diagnostics`]. The model never exposes internal engine
//! types, so it stays stable across releases.
//!
//! [`PdfDocument::diagnostics`]: crate::PdfDocument::diagnostics
//! [`PdfDocument::take_diagnostics`]: crate::PdfDocument::take_diagnostics

use pdf_render::pdf_interpret::InterpreterWarning;

/// How serious a [`Diagnostic`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Severity {
    /// Informational: a recovery or fallback succeeded; output may differ
    /// slightly from the source.
    Info,
    /// A degradation occurred — content was substituted or dropped — but
    /// processing continued and produced output.
    Warning,
    /// A hard limit or failure stopped the operation; the result is incomplete
    /// or the call returned an error.
    Error,
}

/// What area a [`Diagnostic`] concerns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DiagnosticCategory {
    /// Font loading or substitution.
    Font,
    /// Image decoding or dropping.
    Image,
    /// A processing-limit / resource cap was hit.
    Limit,
    /// Structural repair or recovery (e.g. xref / page-tree reconstruction).
    Repair,
    /// Stream or content decoding.
    Decode,
    /// Anything not covered by the categories above.
    Other,
}

/// A single diagnostic: something the engine recovered from, substituted, or
/// dropped while processing a document.
///
/// Constructed only by the SDK; callers inspect the fields. The struct is
/// `#[non_exhaustive]` so future fields do not break callers.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Diagnostic {
    /// Severity.
    pub severity: Severity,
    /// Category.
    pub category: DiagnosticCategory,
    /// Stable, machine-readable code (e.g. `"IMAGE_DECODE_FAILED"`). Stable
    /// across releases; safe to match on programmatically.
    pub code: &'static str,
    /// Human-readable description.
    pub message: String,
    /// 0-based page index this diagnostic relates to, if known.
    pub page: Option<usize>,
    /// PDF object number this diagnostic relates to, if known.
    pub object: Option<u32>,
    /// Optional free-form source/context hint.
    pub source: Option<String>,
}

impl Diagnostic {
    /// Stable code for an unsupported font that was substituted with a fallback.
    pub const CODE_FONT_UNSUPPORTED: &'static str = "FONT_UNSUPPORTED";
    /// Stable code for an image that could not be decoded and was omitted.
    pub const CODE_IMAGE_DECODE_FAILED: &'static str = "IMAGE_DECODE_FAILED";
    /// Stable code for a stream whose decompressed size exceeded the limit.
    pub const CODE_STREAM_TOO_LARGE: &'static str = "STREAM_TOO_LARGE";
    /// Stable code for an invalid cross-reference table that was rebuilt.
    pub const CODE_XREF_REBUILT: &'static str = "XREF_REBUILT";
    /// Stable code for an invalid page tree recovered by brute-force scan.
    pub const CODE_PAGE_TREE_REBUILT: &'static str = "PAGE_TREE_REBUILT";

    /// Diagnostic for a cross-reference table rebuilt during load.
    pub(crate) fn xref_rebuilt() -> Self {
        Diagnostic {
            severity: Severity::Warning,
            category: DiagnosticCategory::Repair,
            code: Self::CODE_XREF_REBUILT,
            message: "The cross-reference table was invalid and was rebuilt by \
                      scanning the file for objects; recovery may be incomplete."
                .to_string(),
            page: None,
            object: None,
            source: None,
        }
    }

    /// Diagnostic for a page tree recovered by brute-force scan during load.
    pub(crate) fn page_tree_rebuilt() -> Self {
        Diagnostic {
            severity: Severity::Warning,
            category: DiagnosticCategory::Repair,
            code: Self::CODE_PAGE_TREE_REBUILT,
            message: "The page tree was invalid and pages were recovered by a \
                      brute-force scan; page order may differ from the source."
                .to_string(),
            page: None,
            object: None,
            source: None,
        }
    }

    /// Translate an internal interpreter warning into a stable public
    /// diagnostic. Internal: the fork type never escapes the public surface.
    pub(crate) fn from_interpreter_warning(warning: InterpreterWarning) -> Self {
        match warning {
            InterpreterWarning::UnsupportedFont => Diagnostic {
                severity: Severity::Warning,
                category: DiagnosticCategory::Font,
                code: Self::CODE_FONT_UNSUPPORTED,
                message: "An unsupported font was encountered; a fallback font \
                          was substituted, so text may render differently from \
                          the source."
                    .to_string(),
                page: None,
                object: None,
                source: None,
            },
            InterpreterWarning::ImageDecodeFailure => Diagnostic {
                severity: Severity::Warning,
                category: DiagnosticCategory::Image,
                code: Self::CODE_IMAGE_DECODE_FAILED,
                message: "An image could not be decoded and was omitted from the \
                          output."
                    .to_string(),
                page: None,
                object: None,
                source: None,
            },
            InterpreterWarning::StreamTooLarge { observed, limit } => Diagnostic {
                severity: Severity::Error,
                category: DiagnosticCategory::Limit,
                code: Self::CODE_STREAM_TOO_LARGE,
                message: format!(
                    "A stream's decompressed size ({observed} bytes) exceeded \
                     the configured limit ({limit} bytes); the operation was \
                     stopped."
                ),
                page: None,
                object: None,
                source: Some(format!("observed={observed};limit={limit}")),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translation_maps_each_warning_to_a_stable_code() {
        let f = Diagnostic::from_interpreter_warning(InterpreterWarning::UnsupportedFont);
        assert_eq!(f.code, Diagnostic::CODE_FONT_UNSUPPORTED);
        assert_eq!(f.category, DiagnosticCategory::Font);
        assert_eq!(f.severity, Severity::Warning);

        let i = Diagnostic::from_interpreter_warning(InterpreterWarning::ImageDecodeFailure);
        assert_eq!(i.code, Diagnostic::CODE_IMAGE_DECODE_FAILED);
        assert_eq!(i.category, DiagnosticCategory::Image);

        let s = Diagnostic::from_interpreter_warning(InterpreterWarning::StreamTooLarge {
            observed: 9000,
            limit: 100,
        });
        assert_eq!(s.code, Diagnostic::CODE_STREAM_TOO_LARGE);
        assert_eq!(s.category, DiagnosticCategory::Limit);
        assert_eq!(s.severity, Severity::Error);
        assert!(s.message.contains("9000"));
        assert!(s.message.contains("100"));
    }
}
