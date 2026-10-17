//! Opaque handle types and C-compatible enums.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/// Opaque document handle. Wraps a `pdf_engine::PdfDocument`.
pub struct PdfDocument(pub(crate) pdf_engine::PdfDocument);

/// Opaque compliance report handle. Wraps a `pdf_compliance::ComplianceReport`.
pub struct PdfComplianceReport(pub(crate) pdf_compliance::ComplianceReport);

/// Status codes returned by all C API functions.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfStatus {
    /// Operation succeeded.
    Ok = 0,
    /// A null pointer or otherwise invalid argument was passed.
    ErrorInvalidArgument = 1,
    /// The file could not be found or read.
    ErrorFileNotFound = 2,
    /// The password is incorrect.
    ErrorInvalidPassword = 3,
    /// The PDF data is corrupt or unparseable.
    ErrorCorruptPdf = 4,
    /// The page index is out of range.
    ErrorPageRange = 5,
    /// A rendering error occurred.
    ErrorRender = 6,
    /// PDF/A conversion failed.
    ErrorConvert = 7,
    /// Redaction failed.
    ErrorRedact = 8,
    /// Document signing failed.
    ErrorSign = 9,
    /// An annotation operation failed.
    ErrorAnnotation = 10,
    /// A document merge operation failed.
    ErrorMerge = 11,
    /// An image or content extraction failed.
    ErrorExtract = 12,
    /// A document split operation failed.
    ErrorSplit = 13,
    /// A watermark operation failed.
    ErrorWatermark = 14,
    /// A compression/optimization operation failed.
    ErrorCompress = 15,
    // 16 to 20 were the licence-activation codes. They are not reused: the
    // numbers stay retired so a caller compiled against an older header can
    // never read a new meaning out of an old constant. (#226)
    /// An unknown error occurred.
    /// A text-edit operation failed (stale match id, unsupported container,
    /// encoding failure, signed document refused, …). The detail is available
    /// via `pdf_last_error_message`.
    ErrorTextEdit = 21,
    // 22 said a tier did not cover the requested operation -- Office export
    // below Business. Nothing withholds anything any more, so nothing can
    // return it; the number stays retired for the reason above. (#226)
    ErrorUnknown = 99,
}

/// PDF/A conformance level for the C API.
///
/// Maps to `pdf_compliance::PdfALevel`. Values are stable across versions.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfALevel {
    /// PDF/A-1B — basic conformance (most common archival target).
    A1b = 0,
    /// PDF/A-1A — accessible conformance (requires tagged PDF).
    A1a = 1,
    /// PDF/A-2B — ISO 19005-2 basic.
    A2b = 2,
    /// PDF/A-2U — ISO 19005-2 with Unicode mapping.
    A2u = 3,
    /// PDF/A-2A — ISO 19005-2 accessible.
    A2a = 4,
    /// PDF/A-3B — ISO 19005-3 basic (allows embedded files).
    A3b = 5,
    /// PDF/A-3U — ISO 19005-3 with Unicode mapping.
    A3u = 6,
    /// PDF/A-3A — ISO 19005-3 accessible.
    A3a = 7,
    /// PDF/A-4 — ISO 19005-4 base level.
    A4 = 8,
    /// PDF/A-4F — allows file attachments.
    A4f = 9,
    /// PDF/A-4E — allows engineering content (3D, rich media).
    A4e = 10,
}

impl PdfALevel {
    /// Convert to the internal `pdf_compliance::PdfALevel` representation.
    pub(crate) fn to_compliance_level(self) -> pdf_compliance::PdfALevel {
        match self {
            Self::A1b => pdf_compliance::PdfALevel::A1b,
            Self::A1a => pdf_compliance::PdfALevel::A1a,
            Self::A2b => pdf_compliance::PdfALevel::A2b,
            Self::A2u => pdf_compliance::PdfALevel::A2u,
            Self::A2a => pdf_compliance::PdfALevel::A2a,
            Self::A3b => pdf_compliance::PdfALevel::A3b,
            Self::A3u => pdf_compliance::PdfALevel::A3u,
            Self::A3a => pdf_compliance::PdfALevel::A3a,
            Self::A4 => pdf_compliance::PdfALevel::A4,
            Self::A4f => pdf_compliance::PdfALevel::A4f,
            Self::A4e => pdf_compliance::PdfALevel::A4e,
        }
    }

    /// The conformance the conversion pipeline writes for this level, or
    /// `None` when the conversion does not deliver it.
    ///
    /// The pipeline writes B-level XMP. The A and U levels add requirements --
    /// a tagged structure tree, a complete Unicode mapping -- that conversion
    /// cannot manufacture from an untagged source, and part 4 has no B level at
    /// all. Until 09-09-2026 this mapped all eleven values onto the B level of
    /// their part: a caller asking for 2u got a file whose XMP said 2b, with no
    /// error and nothing in the header saying so, and Node, Python, Java and
    /// .NET all convert through here. An accepted level that is quietly
    /// something else is worse than a refusal, because the caller archives it.
    ///
    /// Validation is a separate question and still covers all eleven: see
    /// [`Self::to_compliance_level`].
    pub(crate) fn convert_conformance(self) -> Option<pdf_manip::pdfa_xmp::PdfAConformance> {
        use pdf_manip::pdfa_xmp::PdfAConformance;
        match self {
            Self::A1b => Some(PdfAConformance::A1b),
            Self::A2b => Some(PdfAConformance::A2b),
            Self::A3b => Some(PdfAConformance::A3b),
            Self::A1a
            | Self::A2u
            | Self::A2a
            | Self::A3u
            | Self::A3a
            | Self::A4
            | Self::A4f
            | Self::A4e => None,
        }
    }

    /// The levels the conversion delivers, in the words the header uses.
    ///
    /// Named once, so the refusal message and the header guard cannot drift
    /// apart from the mapping above.
    pub const DELIVERED_FOR_CONVERSION: &'static [&'static str] =
        &["PDF_A_LEVEL_1B", "PDF_A_LEVEL_2B", "PDF_A_LEVEL_3B"];
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every level of the enum, so a level added later cannot slip past the
    /// checks below by not being in a list.
    const ALL: &[(PdfALevel, &str)] = &[
        (PdfALevel::A1b, "PDF_A_LEVEL_1B"),
        (PdfALevel::A1a, "PDF_A_LEVEL_1A"),
        (PdfALevel::A2b, "PDF_A_LEVEL_2B"),
        (PdfALevel::A2u, "PDF_A_LEVEL_2U"),
        (PdfALevel::A2a, "PDF_A_LEVEL_2A"),
        (PdfALevel::A3b, "PDF_A_LEVEL_3B"),
        (PdfALevel::A3u, "PDF_A_LEVEL_3U"),
        (PdfALevel::A3a, "PDF_A_LEVEL_3A"),
        (PdfALevel::A4, "PDF_A_LEVEL_4"),
        (PdfALevel::A4f, "PDF_A_LEVEL_4F"),
        (PdfALevel::A4e, "PDF_A_LEVEL_4E"),
    ];

    #[test]
    fn conversion_delivers_the_b_levels_and_claims_no_other() {
        use pdf_manip::pdfa_xmp::PdfAConformance;
        assert_eq!(
            PdfALevel::A1b.convert_conformance(),
            Some(PdfAConformance::A1b)
        );
        assert_eq!(
            PdfALevel::A2b.convert_conformance(),
            Some(PdfAConformance::A2b)
        );
        assert_eq!(
            PdfALevel::A3b.convert_conformance(),
            Some(PdfAConformance::A3b)
        );
        // The eight that used to be written as their part's B level. A2u asked
        // for Unicode mapping and got a file whose XMP said 2b (#447).
        for level in [
            PdfALevel::A1a,
            PdfALevel::A2u,
            PdfALevel::A2a,
            PdfALevel::A3u,
            PdfALevel::A3a,
            PdfALevel::A4,
            PdfALevel::A4f,
            PdfALevel::A4e,
        ] {
            assert_eq!(
                level.convert_conformance(),
                None,
                "{level:?} is not delivered and must not be written as anything else"
            );
        }
    }

    #[test]
    fn the_delivered_list_is_the_mapping() {
        let from_mapping: Vec<&str> = ALL
            .iter()
            .filter(|(level, _)| level.convert_conformance().is_some())
            .map(|(_, name)| *name)
            .collect();
        assert_eq!(
            from_mapping,
            PdfALevel::DELIVERED_FOR_CONVERSION,
            "the refusal message names a different set than the conversion delivers"
        );
    }

    /// The header is what a C caller reads, and it is the half that was wrong:
    /// it documented eleven levels and said nothing about eight of them being
    /// written as something else. So the words in it are checked against the
    /// mapping rather than trusted to be kept up by hand.
    #[test]
    fn the_header_says_of_each_level_what_the_code_does() {
        const HEADER: &str = include_str!("../include/pdfluent.h");
        for (level, name) in ALL {
            let line = HEADER
                .lines()
                .zip(HEADER.lines().skip(1))
                .find(|(_, decl)| decl.trim_start().starts_with(&format!("{name} ")))
                .map(|(doc, _)| doc.trim().to_string())
                .unwrap_or_else(|| panic!("{name} is not declared in pdfluent.h"));
            if level.convert_conformance().is_some() {
                assert!(
                    line.contains("Converts and validates."),
                    "{name} converts; its header line reads {line:?}"
                );
            } else {
                assert!(
                    line.contains("Validates only."),
                    "{name} is refused by conversion; its header line reads {line:?}"
                );
            }
        }
    }
}
