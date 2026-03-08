//! PDF compliance checking (PDF/A, PDF/UA, PDF/X).
//!
//! Validates PDF documents against conformance profiles
//! (ISO 19005 for PDF/A, ISO 14289 for PDF/UA, ISO 15930 for PDF/X).

pub mod pdfa;
pub mod pdfua;
pub mod pdfx;
pub mod pdfx_gen;
pub mod tagged;

pub mod check;
mod xmp;

use pdf_syntax::Pdf;

/// PDF/A conformance level (ISO 19005 parts 1–4).
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
    /// PDF/A-4 base (ISO 19005-4, no conformance letter).
    A4,
    /// PDF/A-4f — allows file attachments.
    A4f,
    /// PDF/A-4e — allows engineering content (3D, rich media).
    A4e,
}

impl PdfALevel {
    /// ISO 19005 part number.
    pub fn part(self) -> u8 {
        match self {
            Self::A1a | Self::A1b => 1,
            Self::A2a | Self::A2b | Self::A2u => 2,
            Self::A3a | Self::A3b | Self::A3u => 3,
            Self::A4 | Self::A4f | Self::A4e => 4,
        }
    }

    /// Conformance letter (a, b, u, f, e, or empty for PDF/A-4 base).
    pub fn conformance(self) -> &'static str {
        match self {
            Self::A1a | Self::A2a | Self::A3a => "A",
            Self::A1b | Self::A2b | Self::A3b => "B",
            Self::A2u | Self::A3u => "U",
            Self::A4 => "",
            Self::A4f => "F",
            Self::A4e => "E",
        }
    }

    /// Whether this level requires tagged PDF (level "a").
    pub fn requires_tagged(self) -> bool {
        matches!(self, Self::A1a | Self::A2a | Self::A3a)
    }

    /// Detect PDF/A level from part number and conformance letter.
    pub fn from_parts(part: u8, conformance: &str) -> Option<Self> {
        match (part, conformance.to_ascii_uppercase().as_str()) {
            (1, "A") => Some(Self::A1a),
            (1, "B") | (1, _) => Some(Self::A1b),
            (2, "A") => Some(Self::A2a),
            (2, "U") => Some(Self::A2u),
            (2, "B") | (2, _) => Some(Self::A2b),
            (3, "A") => Some(Self::A3a),
            (3, "U") => Some(Self::A3u),
            (3, "B") | (3, _) => Some(Self::A3b),
            (4, "F") => Some(Self::A4f),
            (4, "E") => Some(Self::A4e),
            (4, _) => Some(Self::A4),
            _ => None,
        }
    }
}

/// PDF/X conformance level (ISO 15930).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfXLevel {
    /// PDF/X-1a:2003 — CMYK-only, no transparency.
    X1a2003,
    /// PDF/X-3:2003 — allows color-managed workflows, no transparency.
    X32003,
    /// PDF/X-4 — allows transparency and ICC-based colors.
    X4,
}

impl PdfXLevel {
    /// Whether this level forbids transparency.
    pub fn forbids_transparency(self) -> bool {
        matches!(self, Self::X1a2003 | Self::X32003)
    }

    /// Human-readable version string.
    pub fn version_string(self) -> &'static str {
        match self {
            Self::X1a2003 => "PDF/X-1a:2003",
            Self::X32003 => "PDF/X-3:2003",
            Self::X4 => "PDF/X-4",
        }
    }

    /// GTS version identifier for XMP metadata.
    pub fn gts_version(self) -> &'static str {
        match self {
            Self::X1a2003 => "PDF/X-1a:2003",
            Self::X32003 => "PDF/X-3:2003",
            Self::X4 => "PDF/X-4",
        }
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

/// Detect the PDF/A level declared in XMP metadata.
pub fn detect_pdfa_level(pdf: &Pdf) -> Option<PdfALevel> {
    let xmp = check::get_xmp_metadata(pdf)?;
    let (part, conformance) = check::parse_xmp_pdfa(&xmp)?;
    PdfALevel::from_parts(part, &conformance)
}

/// Validate a PDF against PDF/UA-1 (ISO 14289-1).
pub fn validate_pdfua(pdf: &Pdf) -> ComplianceReport {
    pdfua::validate(pdf)
}

/// Validate a PDF against a PDF/X conformance level.
pub fn validate_pdfx(pdf: &Pdf, level: PdfXLevel) -> ComplianceReport {
    pdfx::validate(pdf, level)
}

/// Parse the structure tree from a PDF.
pub fn parse_structure_tree(pdf: &Pdf) -> Option<tagged::StructureTree> {
    tagged::parse(pdf)
}
