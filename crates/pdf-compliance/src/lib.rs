#![deny(missing_docs)]
//! PDF compliance checking: PDF/A, PDF/UA, and PDF/X.
//!
//! Validates PDF documents against conformance profiles defined by:
//! - **ISO 19005** — PDF/A archival format (parts 1–4)
//! - **ISO 14289** — PDF/UA accessibility
//! - **ISO 15930** — PDF/X prepress exchange
//!
//! # Quick Start
//!
//! ```no_run
//! use std::sync::Arc;
//! use pdf_syntax::Pdf;
//! use pdf_compliance::{preferred_pdfa_level, validate_pdfa, Severity};
//!
//! let data = Arc::new(std::fs::read("document.pdf").unwrap());
//! let pdf = Pdf::new(data).unwrap();
//!
//! // Prefer the declared level, but promote PDF/A-1 inputs to PDF/A-2B when
//! // the source uses features like xref streams, transparency, or JPEG2000.
//! let level = preferred_pdfa_level(&pdf);
//! let report = validate_pdfa(&pdf, level);
//!
//! if report.is_compliant() {
//!     println!("PDF/A-{}{} compliant", level.part(), level.conformance());
//! } else {
//!     println!("{} error(s), {} warning(s)", report.error_count(), report.warning_count());
//!     for issue in &report.issues {
//!         if issue.severity == Severity::Error {
//!             println!("  [{}] {:?}: {}", issue.rule, issue.severity, issue.message);
//!         }
//!     }
//! }
//! ```
//!
//! # Key Types
//!
//! | Type | Description |
//! |---|---|
//! | [`PdfALevel`] | PDF/A conformance level: `A1b`, `A2b`, `A2u`, `A3b`, `A4`, … |
//! | [`PdfXLevel`] | PDF/X level: `X1a2003`, `X32003`, `X4` |
//! | [`ComplianceReport`] | Validation outcome with issue list and pass/fail flag |
//! | [`ComplianceIssue`] | Rule ID, severity, message, and optional location |
//! | [`Severity`] | `Error`, `Warning`, `Info` |

pub(crate) mod pdfa;
pub(crate) mod pdfua;
pub(crate) mod pdfx;
pub mod pdfx_gen;
pub mod tagged;
pub mod tagged_gen;

pub(crate) mod check;
mod xmp;

use pdf_syntax::Pdf;

/// PDF/A conformance level (ISO 19005 parts 1–4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfALevel {
    /// PDF/A-1a — conformance level A (tagged, accessible).
    A1a,
    /// PDF/A-1b — conformance level B (basic).
    A1b,
    /// PDF/A-2a — conformance level A (tagged, accessible).
    A2a,
    /// PDF/A-2b — conformance level B (basic).
    A2b,
    /// PDF/A-2u — conformance level U (Unicode).
    A2u,
    /// PDF/A-3a — conformance level A (tagged, accessible).
    A3a,
    /// PDF/A-3b — conformance level B (basic).
    A3b,
    /// PDF/A-3u — conformance level U (Unicode).
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
        // PDF/A-4 and its variants (4, 4e, 4f) have no "A" conformance level and
        // never require tagged content. Only PDF/A-1A, 2A, 3A require tagged. (#FP-6.6.1)
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
#[must_use]
pub fn validate_pdfa(pdf: &Pdf, level: PdfALevel) -> ComplianceReport {
    pdfa::validate(pdf, level)
}

/// Like `validate_pdfa` but prints per-check timing to stderr.
#[must_use]
pub fn validate_pdfa_timed(pdf: &Pdf, level: PdfALevel) -> ComplianceReport {
    pdfa::validate_timed(pdf, level)
}

/// Like `validate_pdfa` but updates a progress tracker with the name of the
/// current check.  Useful for diagnosing timeouts — the caller can read the
/// tracker to see which check was last running.
#[must_use]
pub fn validate_pdfa_with_progress(
    pdf: &Pdf,
    level: PdfALevel,
    progress: &std::sync::Mutex<String>,
) -> ComplianceReport {
    pdfa::validate_with_progress(pdf, level, progress)
}

/// Detect the PDF/A level declared in XMP metadata.
///
/// Uses lenient parsing: extracts part/conformance even when the pdfaid namespace URI is
/// wrong, matching veraPDF's profile-selection behaviour. Compliance violations (§6.7.9,
/// §6.7.11) are still reported by the strict checks in `validate_pdfa`.
#[must_use]
pub fn detect_pdfa_level(pdf: &Pdf) -> Option<PdfALevel> {
    let xmp = check::get_xmp_metadata(pdf)?;
    let (part, conformance) = check::parse_xmp_pdfa_lenient(&xmp)?;
    PdfALevel::from_parts(part, &conformance)
}

/// Choose the preferred PDF/A level for validating or converting a source PDF.
///
/// Policy:
/// - keep the declared XMP level when it is already PDF/A-2 or later;
/// - promote declared PDF/A-1 documents to `A2b` when the source uses
///   cross-reference streams, transparency, or JPEG2000;
/// - default to `A2b` when no PDF/A level is declared.
#[must_use]
pub fn preferred_pdfa_level(pdf: &Pdf) -> PdfALevel {
    match detect_pdfa_level(pdf) {
        Some(level) if level.part() >= 2 => level,
        Some(_level)
            if check::has_xref_streams(pdf)
                || check::uses_transparency(pdf)
                || check::uses_jpeg2000(pdf) =>
        {
            PdfALevel::A2b
        }
        Some(level) => level,
        None => PdfALevel::A2b,
    }
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

#[cfg(test)]
mod tests {
    use super::{detect_pdfa_level, preferred_pdfa_level, PdfALevel};
    use lopdf::{dictionary, xref::XrefType, Document, Object, Stream};
    use pdf_syntax::Pdf;

    #[test]
    fn preferred_level_defaults_to_a2b_without_xmp() {
        let pdf = parse_pdf(base_doc_bytes(false, |_, _| {}));
        assert_eq!(detect_pdfa_level(&pdf), None);
        assert_eq!(preferred_pdfa_level(&pdf), PdfALevel::A2b);
    }

    #[test]
    fn preferred_level_keeps_declared_a1b_when_source_is_part1_compatible() {
        let pdf = parse_pdf(base_doc_bytes(true, |_, _| {}));
        assert_eq!(detect_pdfa_level(&pdf), Some(PdfALevel::A1b));
        assert!(!crate::check::has_xref_streams(&pdf));
        assert!(!crate::check::uses_transparency(&pdf));
        assert!(!crate::check::uses_jpeg2000(&pdf));
        assert_eq!(preferred_pdfa_level(&pdf), PdfALevel::A1b);
    }

    #[test]
    fn preferred_level_promotes_declared_a1b_for_xref_streams() {
        let mut doc = build_base_doc(true);
        let mut bytes = Vec::new();
        doc.save_modern(&mut bytes).unwrap();
        let pdf = parse_pdf(bytes);

        assert_eq!(detect_pdfa_level(&pdf), Some(PdfALevel::A1b));
        assert!(crate::check::has_xref_streams(&pdf));
        assert_eq!(preferred_pdfa_level(&pdf), PdfALevel::A2b);
    }

    #[test]
    fn preferred_level_promotes_declared_a1b_for_transparency() {
        let pdf = parse_pdf(base_doc_bytes(true, |page_id, doc| {
            let page = doc.get_object_mut(page_id).unwrap().as_dict_mut().unwrap();
            page.set(
                "Resources",
                dictionary! {
                    "ExtGState" => dictionary! {
                        "GS1" => dictionary! {
                            "Type" => "ExtGState",
                            "ca" => 0.5,
                        }
                    }
                },
            );
        }));

        assert_eq!(detect_pdfa_level(&pdf), Some(PdfALevel::A1b));
        assert!(crate::check::uses_transparency(&pdf));
        assert_eq!(preferred_pdfa_level(&pdf), PdfALevel::A2b);
    }

    #[test]
    fn preferred_level_promotes_declared_a1b_for_jpeg2000() {
        let pdf = parse_pdf(base_doc_bytes(true, |_, doc| {
            doc.add_object(Stream::new(
                dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Image",
                    "Width" => 1,
                    "Height" => 1,
                    "BitsPerComponent" => 8,
                    "ColorSpace" => "DeviceGray",
                    "Filter" => "JPXDecode",
                },
                vec![0u8; 8],
            ));
        }));

        assert_eq!(detect_pdfa_level(&pdf), Some(PdfALevel::A1b));
        assert!(crate::check::uses_jpeg2000(&pdf));
        assert_eq!(preferred_pdfa_level(&pdf), PdfALevel::A2b);
    }

    #[test]
    fn cyclic_acroform_aa_validation_terminates() {
        // AcroForm fields whose /Kids reference each other form a cycle, and a
        // field carries /AA so the §6.6.2 check recurses. Without the depth
        // guard this overflows the stack during validation.
        fn cyclic_aa_pdf() -> Vec<u8> {
            let objs: [&[u8]; 6] = [
                b"<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>",
                b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
                b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
                b"<< /Fields [5 0 R] >>",
                b"<< /T (A) /FT /Tx /AA << >> /Kids [6 0 R] >>",
                b"<< /T (B) /Kids [5 0 R] >>", // /Kids back to A -> cycle
            ];
            let mut buf = Vec::new();
            let mut offsets = [0usize; 7];
            buf.extend_from_slice(b"%PDF-1.7\n");
            for (i, body) in objs.iter().enumerate() {
                offsets[i + 1] = buf.len();
                buf.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
                buf.extend_from_slice(body);
                buf.extend_from_slice(b"\nendobj\n");
            }
            let xref_off = buf.len();
            buf.extend_from_slice(b"xref\n0 7\n0000000000 65535 f \n");
            for o in &offsets[1..7] {
                buf.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
            }
            buf.extend_from_slice(
                format!("trailer\n<< /Size 7 /Root 1 0 R >>\nstartxref\n{xref_off}\n%%EOF")
                    .as_bytes(),
            );
            buf
        }

        let pdf = parse_pdf(cyclic_aa_pdf());
        // Termination is the assertion: the depth guard bounds the cyclic /Kids
        // recursion so validation returns instead of overflowing the stack.
        let _report = crate::validate_pdfa(&pdf, PdfALevel::A2b);
    }

    fn parse_pdf(bytes: Vec<u8>) -> Pdf {
        Pdf::new(bytes).unwrap()
    }

    fn base_doc_bytes(
        with_a1_xmp: bool,
        mutate: impl FnOnce((u32, u16), &mut Document),
    ) -> Vec<u8> {
        let mut doc = build_base_doc(with_a1_xmp);
        let page_id = doc.get_pages().into_values().next().unwrap();
        mutate(page_id, &mut doc);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    fn build_base_doc(with_a1_xmp: bool) -> Document {
        let mut doc = Document::with_version("1.4");
        doc.reference_table.cross_reference_type = XrefType::CrossReferenceTable;

        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        let catalog_id = doc.new_object_id();
        let content_id = doc.add_object(Stream::new(dictionary! {}, Vec::new()));

        let mut catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        if with_a1_xmp {
            let metadata_id = doc.add_object(Stream::new(
                dictionary! {
                    "Type" => "Metadata",
                    "Subtype" => "XML",
                },
                pdfa_xmp(1, "B").into_bytes(),
            ));
            catalog.set("Metadata", Object::Reference(metadata_id));
        }

        doc.objects.insert(
            (pages_id.0, pages_id.1),
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page_id)],
                "Count" => 1,
            }),
        );
        doc.objects.insert(
            (page_id.0, page_id.1),
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
                "Contents" => Object::Reference(content_id),
            }),
        );
        doc.objects
            .insert((catalog_id.0, catalog_id.1), Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));
        doc
    }

    fn pdfa_xmp(part: u8, conformance: &str) -> String {
        format!(
            r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
  <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
    <rdf:Description rdf:about="" xmlns:pdfaid="http://www.aiim.org/pdfa/ns/id/">
      <pdfaid:part>{part}</pdfaid:part>
      <pdfaid:conformance>{conformance}</pdfaid:conformance>
    </rdf:Description>
  </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#
        )
    }
}
