//! PDF/A compliance types.

/// PDF/A conformance profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PdfAProfile {
    /// PDF/A-1b — visual conformance, PDF 1.4 baseline.
    A1b,
    /// PDF/A-2b — visual conformance, PDF 1.7 baseline.
    A2b,
    /// PDF/A-3b — visual conformance with embedded files allowed.
    A3b,
}

/// Severity of a PDF/A validation finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Severity {
    /// Blocks compliance.
    Error,
    /// Non-blocking concern.
    Warning,
}

/// A single PDF/A validation finding.
#[derive(Debug, Clone)]
pub struct Violation {
    /// Rule identifier (e.g. `"6.2.3.3-1"` from PDF/A-1b spec).
    pub rule: String,
    /// Short description.
    pub message: String,
    /// Severity of the finding.
    pub severity: Severity,
    /// Page number where the violation was detected, if page-scoped.
    pub page: Option<usize>,
}

/// Aggregate report of a PDF/A validation.
#[derive(Debug, Clone)]
pub struct PdfAValidationReport {
    /// Profile that was validated against.
    pub profile: PdfAProfile,
    /// All findings.
    pub violations: Vec<Violation>,
}

impl PdfAValidationReport {
    /// True if no error-severity violations were found.
    pub fn is_compliant(&self) -> bool {
        !self
            .violations
            .iter()
            .any(|v| matches!(v.severity, Severity::Error))
    }
}

#[cfg(feature = "pdfa")]
impl From<PdfAProfile> for pdf_compliance::PdfALevel {
    fn from(p: PdfAProfile) -> Self {
        match p {
            PdfAProfile::A1b => pdf_compliance::PdfALevel::A1b,
            PdfAProfile::A2b => pdf_compliance::PdfALevel::A2b,
            PdfAProfile::A3b => pdf_compliance::PdfALevel::A3b,
        }
    }
}

#[cfg(feature = "pdfa")]
pub(crate) fn report_from_compliance(
    raw: pdf_compliance::ComplianceReport,
    profile: PdfAProfile,
) -> PdfAValidationReport {
    let violations = raw
        .issues
        .into_iter()
        .filter(|i| !matches!(i.severity, pdf_compliance::Severity::Info))
        .map(|i| Violation {
            rule: i.rule,
            message: i.message,
            severity: match i.severity {
                pdf_compliance::Severity::Error => Severity::Error,
                _ => Severity::Warning,
            },
            page: None,
        })
        .collect();
    PdfAValidationReport {
        profile,
        violations,
    }
}
