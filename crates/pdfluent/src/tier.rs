//! License tiers and the tier → capability mapping.
//!
//! Tiers match the pricing page on <https://pdfluent.com/pricing>.

use crate::capability::{Capability, CapabilitySet};

/// Commercial tier level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Tier {
    /// Free evaluation with minimum evaluation surface; output is marked via
    /// `/Producer` metadata.
    Trial,
    /// €499/yr — single developer.
    Developer,
    /// €1,499/yr — up to 5 developers, adds PDF/A, signatures, redaction,
    /// PDF/UA, e-invoicing, Python/Node bindings.
    Team,
    /// €2,999/yr — up to 15 developers, adds XFA flatten, OCR, DOCX/XLSX/PPTX
    /// export, Java bindings, Docker image.
    Business,
    /// €5,999/yr — unlimited, adds OEM/SaaS redistribution, air-gapped,
    /// 24h support, dedicated Slack.
    Enterprise,
}

impl Tier {
    /// Whether this tier marks output via `/Producer` metadata.
    pub const fn is_marked(self) -> bool {
        matches!(self, Tier::Trial)
    }

    /// Canonical set of capabilities granted by this tier.
    ///
    /// Each tier is a strict superset of the tier below it. Trial grants
    /// the minimum surface needed to evaluate the SDK; full capabilities
    /// unlock progressively from Developer through Enterprise.
    ///
    /// Snapshot-tested in `tests/tier_matrix.rs`.
    pub fn capabilities(self) -> CapabilitySet {
        use Capability::*;

        let trial = CapabilitySet::empty()
            .with(PdfParse)
            .with(PdfWrite)
            .with(PageOps)
            .with(TextExtract)
            .with(AcroFormRead)
            .with(PdfaValidate)
            .with(TextEdit);

        match self {
            Tier::Trial => trial,
            Tier::Developer => trial
                .with(TextExtractWithLayout)
                .with(ImageExtract)
                .with(AcroFormFill)
                .with(AcroFormFlatten)
                .with(PdfaConvertA2b)
                .with(Redaction)
                .with(XfaParse)
                .with(XfaFill)
                .with(EncryptionRead),
            Tier::Team => Tier::Developer
                .capabilities()
                .with(DigitalSignatureSign)
                .with(DigitalSignatureVerify)
                .with(XfaFlatten)
                .with(WasmRuntime)
                .with(RenderRaster)
                .with(RenderThumbnail)
                .with(EncryptionWrite)
                .with(PdfuaValidate)
                .with(PdfuaConvert)
                .with(EInvoiceZugferd)
                .with(EInvoiceFacturX)
                .with(EInvoiceXRechnung),
            Tier::Business => Tier::Team
                .capabilities()
                .with(PadesLongTerm)
                .with(PadesLongTermArchive)
                .with(PdfaConvertA1b)
                .with(PdfaConvertA3b)
                .with(OcrTesseract)
                .with(TableExtract)
                .with(PdfDiff)
                .with(DocxExport)
                .with(XlsxExport)
                .with(PptxExport)
                .with(Html2Pdf),
            Tier::Enterprise => Tier::Business
                .capabilities()
                .with(OcrPaddle)
                .with(AirGapped)
                .with(OemRedistribution),
        }
    }
}
