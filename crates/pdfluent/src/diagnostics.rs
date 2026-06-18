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

use pdf_interpret::InterpreterWarning;
use pdf_syntax::leniency::{LeniencyEvent, LeniencySeverity};

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
    /// Stable code for a flate stream decoded via the pure-Rust fallback.
    pub const CODE_FLATE_BROKEN_FALLBACK: &'static str = "FLATE_BROKEN_FALLBACK";
    /// Stable code for a bad block header encountered in a flate stream.
    pub const CODE_FLATE_BAD_BLOCK: &'static str = "FLATE_BAD_BLOCK";
    /// Stable code for premature EOF in an LZW stream.
    pub const CODE_LZW_PREMATURE_EOF: &'static str = "LZW_PREMATURE_EOF";
    /// Stable code for an invalid code in an LZW stream.
    pub const CODE_LZW_INVALID_CODE: &'static str = "LZW_INVALID_CODE";
    /// Stable code for an ASCII-85 1-character terminal group accepted leniently.
    pub const CODE_ASCII85_LENIENT_PARTIAL: &'static str = "ASCII85_LENIENT_PARTIAL";
    /// Stable code for partial CCITT row decode.
    pub const CODE_CCITT_PARTIAL_DECODE: &'static str = "CCITT_PARTIAL_DECODE";
    /// Stable code for the stream manual fallback parser being used.
    pub const CODE_STREAM_PARSE_FALLBACK: &'static str = "STREAM_PARSE_FALLBACK";
    /// Stable code for a cycle detected in indirect object references.
    pub const CODE_INDIRECT_CYCLE: &'static str = "INDIRECT_CYCLE";
    /// Stable code for indirect object resolution depth exceeding 512.
    pub const CODE_INDIRECT_DEPTH_EXCEEDED: &'static str = "INDIRECT_DEPTH_EXCEEDED";

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

    /// Translate a low-level [`LeniencyEvent`] into a stable public diagnostic.
    ///
    /// Called by the document façade after draining the thread-local leniency
    /// collector following a load or decode operation.
    pub(crate) fn from_leniency_event(event: LeniencyEvent) -> Self {
        let severity = match event.severity {
            LeniencySeverity::Info => Severity::Info,
            LeniencySeverity::Warning => Severity::Warning,
            LeniencySeverity::Critical => Severity::Error,
        };
        let category = match event.code {
            "FLATE_BROKEN_FALLBACK"
            | "FLATE_BAD_BLOCK"
            | "LZW_PREMATURE_EOF"
            | "LZW_INVALID_CODE"
            | "ASCII85_LENIENT_PARTIAL"
            | "CCITT_PARTIAL_DECODE"
            | "STREAM_PARSE_FALLBACK" => DiagnosticCategory::Decode,
            "INDIRECT_CYCLE" | "INDIRECT_DEPTH_EXCEEDED" => DiagnosticCategory::Repair,
            _ => DiagnosticCategory::Other,
        };
        Diagnostic {
            severity,
            category,
            code: event.code,
            message: event.message.to_string(),
            page: None,
            object: None,
            source: None,
        }
    }
}

/// An aggregated summary of decode-leniency diagnostics for a document.
///
/// Computed from the diagnostics buffer via [`LeniencyReport::from_diagnostics`].
/// Provides quick access to counts without iterating the full list.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct LeniencyReport {
    /// All leniency-related diagnostics (Repair + Decode categories).
    pub events: Vec<Diagnostic>,
    /// Number of distinct event codes present.
    pub unique_event_count: usize,
    /// Number of events at [`Severity::Warning`] level.
    pub warning_count: usize,
    /// Number of events at [`Severity::Error`] level (Critical in the parser layer).
    pub critical_count: usize,
}

impl LeniencyReport {
    /// Build a report from the diagnostics collected on a document.
    ///
    /// Filters to Repair and Decode categories (the leniency-relevant subset).
    pub fn from_diagnostics(diagnostics: &[Diagnostic]) -> Self {
        let events: Vec<Diagnostic> = diagnostics
            .iter()
            .filter(|d| {
                matches!(
                    d.category,
                    DiagnosticCategory::Repair | DiagnosticCategory::Decode
                )
            })
            .cloned()
            .collect();
        // Count distinct codes — `events` may contain one entry per render/extract
        // call for the same code (each operation has its own activate/drain session).
        let mut seen_codes = std::collections::HashSet::new();
        for e in &events {
            seen_codes.insert(e.code);
        }
        let unique_event_count = seen_codes.len();
        let warning_count = events
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .count();
        let critical_count = events
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count();
        LeniencyReport {
            events,
            unique_event_count,
            warning_count,
            critical_count,
        }
    }

    /// Returns `true` if no leniency events were recorded (clean document).
    pub fn is_clean(&self) -> bool {
        self.events.is_empty()
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

    #[test]
    fn from_leniency_event_maps_severity_and_category() {
        use pdf_syntax::leniency::{LeniencyEvent, LeniencySeverity};

        let warn_event = LeniencyEvent {
            code: "FLATE_BROKEN_FALLBACK",
            severity: LeniencySeverity::Warning,
            message: "test warn",
        };
        let d = Diagnostic::from_leniency_event(warn_event);
        assert_eq!(d.code, "FLATE_BROKEN_FALLBACK");
        assert_eq!(d.severity, Severity::Warning);
        assert_eq!(d.category, DiagnosticCategory::Decode);

        let critical_event = LeniencyEvent {
            code: "INDIRECT_CYCLE",
            severity: LeniencySeverity::Critical,
            message: "test critical",
        };
        let d = Diagnostic::from_leniency_event(critical_event);
        assert_eq!(d.code, "INDIRECT_CYCLE");
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.category, DiagnosticCategory::Repair);

        let info_event = LeniencyEvent {
            code: "ASCII85_LENIENT_PARTIAL",
            severity: LeniencySeverity::Info,
            message: "test info",
        };
        let d = Diagnostic::from_leniency_event(info_event);
        assert_eq!(d.severity, Severity::Info);
        assert_eq!(d.category, DiagnosticCategory::Decode);
    }

    #[test]
    fn leniency_report_filters_repair_and_decode_categories() {
        let diagnostics = vec![
            Diagnostic {
                severity: Severity::Warning,
                category: DiagnosticCategory::Font,
                code: Diagnostic::CODE_FONT_UNSUPPORTED,
                message: "font sub".to_string(),
                page: None,
                object: None,
                source: None,
            },
            Diagnostic::xref_rebuilt(),
            Diagnostic {
                severity: Severity::Warning,
                category: DiagnosticCategory::Decode,
                code: Diagnostic::CODE_FLATE_BROKEN_FALLBACK,
                message: "flate fallback".to_string(),
                page: None,
                object: None,
                source: None,
            },
        ];

        let report = LeniencyReport::from_diagnostics(&diagnostics);
        // Font diag excluded; xref_rebuilt (Repair) + flate (Decode) included.
        assert_eq!(report.events.len(), 2);
        assert_eq!(report.unique_event_count, 2);
        assert_eq!(report.warning_count, 2);
        assert_eq!(report.critical_count, 0);
        assert!(!report.is_clean());
    }

    #[test]
    fn leniency_report_is_clean_when_no_repair_or_decode() {
        let diagnostics = vec![Diagnostic {
            severity: Severity::Warning,
            category: DiagnosticCategory::Font,
            code: Diagnostic::CODE_FONT_UNSUPPORTED,
            message: "font sub".to_string(),
            page: None,
            object: None,
            source: None,
        }];
        let report = LeniencyReport::from_diagnostics(&diagnostics);
        assert!(report.is_clean());
    }

    #[test]
    fn leniency_report_counts_severities_correctly() {
        let diagnostics = vec![
            Diagnostic {
                severity: Severity::Warning,
                category: DiagnosticCategory::Decode,
                code: "CODE_A",
                message: String::new(),
                page: None,
                object: None,
                source: None,
            },
            Diagnostic {
                severity: Severity::Error,
                category: DiagnosticCategory::Repair,
                code: "CODE_B",
                message: String::new(),
                page: None,
                object: None,
                source: None,
            },
            Diagnostic {
                severity: Severity::Info,
                category: DiagnosticCategory::Decode,
                code: "CODE_C",
                message: String::new(),
                page: None,
                object: None,
                source: None,
            },
        ];
        let report = LeniencyReport::from_diagnostics(&diagnostics);
        assert_eq!(report.warning_count, 1);
        assert_eq!(report.critical_count, 1);
        assert_eq!(report.unique_event_count, 3);
    }
}
