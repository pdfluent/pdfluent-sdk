//! License tiers and the tier → capability mapping.
//!
//! Tiers match the pricing page on <https://pdfluent.com/pricing>. Old tier
//! names from 0.x releases are accepted as serde aliases during the 1.0.x
//! compatibility window.

use crate::capability::{Capability, CapabilitySet};

/// Commercial tier level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Tier {
    /// Free evaluation. All capabilities available; output is marked via
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
    /// This mapping is the source of truth. It is snapshot-tested against the
    /// public pricing page under `tests/tier_matches_pricing_page.rs`.
    pub fn capabilities(self) -> CapabilitySet {
        use Capability::*;

        // Core — available in every tier including Trial.
        let core = CapabilitySet::empty()
            .with(PdfParse)
            .with(PdfWrite)
            .with(PageOps)
            .with(TextExtract)
            .with(TextExtractWithLayout)
            .with(ImageExtract)
            .with(RenderRaster)
            .with(RenderThumbnail)
            .with(AcroFormRead)
            .with(AcroFormFill)
            .with(AcroFormFlatten)
            .with(EncryptionRead)
            .with(EncryptionWrite)
            .with(WasmRuntime);

        match self {
            Tier::Trial => {
                // All capabilities in Trial; output is marked.
                core.with(PdfaValidate)
                    .with(PdfaConvertA1b)
                    .with(PdfaConvertA2b)
                    .with(PdfaConvertA3b)
                    .with(DigitalSignatureSign)
                    .with(DigitalSignatureVerify)
                    .with(PadesBLT)
                    .with(PadesBLTA)
                    .with(Redaction)
                    .with(PdfuaValidate)
                    .with(PdfuaConvert)
                    .with(EInvoiceZugferd)
                    .with(EInvoiceFacturX)
                    .with(EInvoiceXRechnung)
                    .with(XfaParse)
                    .with(XfaFill)
                    .with(XfaFlatten)
                    .with(OcrTesseract)
                    .with(OcrPaddle)
                    .with(Html2Pdf)
                    .with(DocxExport)
                    .with(XlsxExport)
                    .with(PptxExport)
                    .with(PdfDiff)
                    .with(TableExtract)
            }
            Tier::Developer => core.with(XfaParse).with(XfaFill),
            Tier::Team => core
                .with(XfaParse)
                .with(XfaFill)
                .with(PdfaValidate)
                .with(PdfaConvertA1b)
                .with(PdfaConvertA2b)
                .with(PdfaConvertA3b)
                .with(DigitalSignatureSign)
                .with(DigitalSignatureVerify)
                .with(PadesBLT)
                .with(PadesBLTA)
                .with(Redaction)
                .with(PdfuaValidate)
                .with(PdfuaConvert)
                .with(EInvoiceZugferd)
                .with(EInvoiceFacturX)
                .with(EInvoiceXRechnung)
                .with(TableExtract),
            Tier::Business => Tier::Team
                .capabilities()
                .with(XfaFlatten)
                .with(OcrTesseract)
                .with(OcrPaddle)
                .with(Html2Pdf)
                .with(DocxExport)
                .with(XlsxExport)
                .with(PptxExport)
                .with(PdfDiff),
            Tier::Enterprise => Tier::Business
                .capabilities()
                .with(AirGapped)
                .with(OemRedistribution),
        }
    }
}
