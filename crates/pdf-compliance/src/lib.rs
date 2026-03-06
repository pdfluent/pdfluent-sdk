//! PDF compliance checking (PDF/A, PDF/UA).
//!
//! Validates PDF documents against conformance profiles
//! (ISO 19005 for PDF/A, ISO 14289 for PDF/UA).

pub mod pdfa;
pub mod pdfua;
pub mod tagged;

mod check;

use pdf_syntax::Pdf;

/// PDF/A conformance level (ISO 19005 parts 1–3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfALevel {
    A1a,
    A1b,
    A2a,
    A2b,
    A2u,
    A3a,
    A3b,
    A3u,
}

impl PdfALevel {
    /// ISO 19005 part number.
    pub fn part(self) -> u8 {
        match self {
            Self::A1a | Self::A1b => 1,
            Self::A2a | Self::A2b | Self::A2u => 2,
            Self::A3a | Self::A3b | Self::A3u => 3,
        }
    }

    /// Conformance letter (a, b, u).
    pub fn conformance(self) -> &'static str {
        match self {
            Self::A1a | Self::A2a | Self::A3a => "A",
            Self::A1b | Self::A2b | Self::A3b => "B",
            Self::A2u | Self::A3u => "U",
        }
    }

    /// Whether this level requires tagged PDF (level "a").
    pub fn requires_tagged(self) -> bool {
        matches!(self, Self::A1a | Self::A2a | Self::A3a)
    }
}

/// A single compliance issue found during checking.
#[derive(Debug, Clone)]
pub struct ComplianceIssue {
    /// Rule identifier (e.g., "6.1.2" for PDF/A clause).
    pub rule: String,
    /// Issue severity.
    pub severity: Severity,
    /// Human-readable description.
    pub message: String,
    /// Location in the document (object number, page, etc.).
    pub location: Option<String>,
}

/// Severity of a compliance issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Conformance violation — document is non-compliant.
    Error,
    /// Potential issue that may affect compliance.
    Warning,
    /// Informational observation.
    Info,
}

/// A complete compliance report.
#[derive(Debug, Clone, Default)]
pub struct ComplianceReport {
    /// All issues found during the check.
    pub issues: Vec<ComplianceIssue>,
    /// The checked conformance level (if PDF/A).
    pub pdfa_level: Option<PdfALevel>,
    /// Whether the document is compliant.
    pub compliant: bool,
}

impl ComplianceReport {
    /// Returns `true` if no errors were found (warnings/info are allowed).
    pub fn is_compliant(&self) -> bool {
        !self.issues.iter().any(|i| i.severity == Severity::Error)
    }

    /// Number of errors.
    pub fn error_count(&self) -> usize {
        self.issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .count()
    }

    /// Number of warnings.
    pub fn warning_count(&self) -> usize {
        self.issues
            .iter()
            .filter(|i| i.severity == Severity::Warning)
            .count()
    }
}

/// Validate a PDF against a PDF/A conformance level.
pub fn validate_pdfa(pdf: &Pdf, level: PdfALevel) -> ComplianceReport {
    pdfa::validate(pdf, level)
}

/// Validate a PDF against PDF/UA-1 (ISO 14289-1).
pub fn validate_pdfua(pdf: &Pdf) -> ComplianceReport {
    pdfua::validate(pdf)
}

/// Parse the structure tree from a PDF.
pub fn parse_structure_tree(pdf: &Pdf) -> Option<tagged::StructureTree> {
    tagged::parse(pdf)
}
