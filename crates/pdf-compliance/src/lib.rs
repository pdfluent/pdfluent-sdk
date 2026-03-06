//! PDF compliance checking (PDF/A, PDF/UA).
//!
//! Validates PDF documents against conformance profiles
//! (ISO 19005 for PDF/A, ISO 14289 for PDF/UA).

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
}

impl ComplianceReport {
    /// Returns `true` if no errors were found (warnings/info are allowed).
    pub fn is_compliant(&self) -> bool {
        !self.issues.iter().any(|i| i.severity == Severity::Error)
    }
}

/// Core trait for PDF compliance checking.
pub trait ComplianceChecker {
    /// Checks the document against a PDF/A conformance level.
    fn check_pdfa(&self, level: PdfALevel) -> ComplianceReport;

    /// Checks the document against PDF/UA (ISO 14289) requirements.
    fn check_pdfua(&self) -> ComplianceReport;
}
