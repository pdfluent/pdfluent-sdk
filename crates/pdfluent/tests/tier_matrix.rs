//! Exhaustive tier × capability matrix tests for M5-TIER-01 (#1292).
//!
//! Each test asserts every capability for a single tier — present or absent.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::Capability as C;
use pdfluent::Tier;

// ---------------------------------------------------------------------------
// Trial
// ---------------------------------------------------------------------------

#[test]
fn trial_grants_core_only() {
    let caps = Tier::Trial.capabilities();
    // present
    assert!(caps.contains(C::PdfParse));
    assert!(caps.contains(C::PdfWrite));
    assert!(caps.contains(C::PageOps));
    assert!(caps.contains(C::TextExtract));
    assert!(caps.contains(C::AcroFormRead));
    assert!(caps.contains(C::PdfaValidate));
    // absent
    assert!(!caps.contains(C::TextExtractWithLayout));
    assert!(!caps.contains(C::ImageExtract));
    assert!(!caps.contains(C::TableExtract));
    assert!(!caps.contains(C::RenderRaster));
    assert!(!caps.contains(C::RenderThumbnail));
    assert!(!caps.contains(C::AcroFormFill));
    assert!(!caps.contains(C::AcroFormFlatten));
    assert!(!caps.contains(C::XfaParse));
    assert!(!caps.contains(C::XfaFill));
    assert!(!caps.contains(C::XfaFlatten));
    assert!(!caps.contains(C::EncryptionRead));
    assert!(!caps.contains(C::EncryptionWrite));
    assert!(!caps.contains(C::DigitalSignatureSign));
    assert!(!caps.contains(C::DigitalSignatureVerify));
    assert!(!caps.contains(C::PadesLongTerm));
    assert!(!caps.contains(C::PadesLongTermArchive));
    assert!(!caps.contains(C::PdfaConvertA1b));
    assert!(!caps.contains(C::PdfaConvertA2b));
    assert!(!caps.contains(C::PdfaConvertA3b));
    assert!(!caps.contains(C::PdfuaValidate));
    assert!(!caps.contains(C::PdfuaConvert));
    assert!(!caps.contains(C::EInvoiceZugferd));
    assert!(!caps.contains(C::EInvoiceFacturX));
    assert!(!caps.contains(C::EInvoiceXRechnung));
    assert!(!caps.contains(C::Redaction));
    // TextEdit is deliberately in Trial: trial edits are usable but stamped
    // with a trial notice (enforced in PdfDocument::after_text_edit_commit).
    assert!(caps.contains(C::TextEdit));
    assert!(!caps.contains(C::OcrTesseract));
    assert!(!caps.contains(C::OcrPaddle));
    assert!(!caps.contains(C::Html2Pdf));
    assert!(!caps.contains(C::DocxExport));
    assert!(!caps.contains(C::XlsxExport));
    assert!(!caps.contains(C::PptxExport));
    assert!(!caps.contains(C::PdfDiff));
    assert!(!caps.contains(C::WasmRuntime));
    assert!(!caps.contains(C::AirGapped));
    assert!(!caps.contains(C::OemRedistribution));
}

// ---------------------------------------------------------------------------
// Developer
// ---------------------------------------------------------------------------

#[test]
fn developer_grants_trial_plus_dev_tier() {
    let caps = Tier::Developer.capabilities();
    // Trial set still present
    assert!(caps.contains(C::PdfParse));
    assert!(caps.contains(C::PdfWrite));
    assert!(caps.contains(C::PageOps));
    assert!(caps.contains(C::TextExtract));
    assert!(caps.contains(C::AcroFormRead));
    assert!(caps.contains(C::PdfaValidate));
    // Developer additions
    assert!(caps.contains(C::TextExtractWithLayout));
    assert!(caps.contains(C::ImageExtract));
    assert!(caps.contains(C::AcroFormFill));
    assert!(caps.contains(C::AcroFormFlatten));
    assert!(caps.contains(C::PdfaConvertA2b));
    assert!(caps.contains(C::Redaction));
    assert!(caps.contains(C::TextEdit));
    assert!(caps.contains(C::XfaParse));
    assert!(caps.contains(C::XfaFill));
    assert!(caps.contains(C::EncryptionRead));
    // Team+ absent
    assert!(!caps.contains(C::DigitalSignatureSign));
    assert!(!caps.contains(C::DigitalSignatureVerify));
    assert!(!caps.contains(C::XfaFlatten));
    assert!(!caps.contains(C::WasmRuntime));
    assert!(!caps.contains(C::RenderRaster));
    assert!(!caps.contains(C::RenderThumbnail));
    assert!(!caps.contains(C::EncryptionWrite));
    assert!(!caps.contains(C::PdfuaValidate));
    assert!(!caps.contains(C::PdfuaConvert));
    assert!(!caps.contains(C::EInvoiceZugferd));
    assert!(!caps.contains(C::EInvoiceFacturX));
    assert!(!caps.contains(C::EInvoiceXRechnung));
    // Business+ absent
    assert!(!caps.contains(C::PadesLongTerm));
    assert!(!caps.contains(C::PadesLongTermArchive));
    assert!(!caps.contains(C::PdfaConvertA1b));
    assert!(!caps.contains(C::PdfaConvertA3b));
    assert!(!caps.contains(C::OcrTesseract));
    assert!(!caps.contains(C::TableExtract));
    assert!(!caps.contains(C::PdfDiff));
    assert!(!caps.contains(C::DocxExport));
    assert!(!caps.contains(C::XlsxExport));
    assert!(!caps.contains(C::PptxExport));
    assert!(!caps.contains(C::Html2Pdf));
    // Enterprise absent
    assert!(!caps.contains(C::OcrPaddle));
    assert!(!caps.contains(C::AirGapped));
    assert!(!caps.contains(C::OemRedistribution));
}

// ---------------------------------------------------------------------------
// Team
// ---------------------------------------------------------------------------

#[test]
fn team_grants_developer_plus_team_tier() {
    let caps = Tier::Team.capabilities();
    // Developer set still present
    assert!(caps.contains(C::PdfParse));
    assert!(caps.contains(C::TextExtract));
    assert!(caps.contains(C::TextExtractWithLayout));
    assert!(caps.contains(C::Redaction));
    assert!(caps.contains(C::TextEdit));
    assert!(caps.contains(C::XfaParse));
    assert!(caps.contains(C::XfaFill));
    assert!(caps.contains(C::EncryptionRead));
    assert!(caps.contains(C::PdfaConvertA2b));
    // Team additions
    assert!(caps.contains(C::DigitalSignatureSign));
    assert!(caps.contains(C::DigitalSignatureVerify));
    assert!(caps.contains(C::XfaFlatten));
    assert!(caps.contains(C::WasmRuntime));
    assert!(caps.contains(C::RenderRaster));
    assert!(caps.contains(C::RenderThumbnail));
    assert!(caps.contains(C::EncryptionWrite));
    assert!(caps.contains(C::PdfuaValidate));
    assert!(caps.contains(C::PdfuaConvert));
    assert!(caps.contains(C::EInvoiceZugferd));
    assert!(caps.contains(C::EInvoiceFacturX));
    assert!(caps.contains(C::EInvoiceXRechnung));
    // Business+ absent
    assert!(!caps.contains(C::PadesLongTerm));
    assert!(!caps.contains(C::PadesLongTermArchive));
    assert!(!caps.contains(C::PdfaConvertA1b));
    assert!(!caps.contains(C::PdfaConvertA3b));
    assert!(!caps.contains(C::OcrTesseract));
    assert!(!caps.contains(C::TableExtract));
    assert!(!caps.contains(C::PdfDiff));
    assert!(!caps.contains(C::DocxExport));
    assert!(!caps.contains(C::XlsxExport));
    assert!(!caps.contains(C::PptxExport));
    assert!(!caps.contains(C::Html2Pdf));
    // Enterprise absent
    assert!(!caps.contains(C::OcrPaddle));
    assert!(!caps.contains(C::AirGapped));
    assert!(!caps.contains(C::OemRedistribution));
}

// ---------------------------------------------------------------------------
// Business
// ---------------------------------------------------------------------------

#[test]
fn business_grants_team_plus_business_tier() {
    let caps = Tier::Business.capabilities();
    // Team set still present
    assert!(caps.contains(C::PdfParse));
    assert!(caps.contains(C::DigitalSignatureSign));
    assert!(caps.contains(C::DigitalSignatureVerify));
    assert!(caps.contains(C::XfaFlatten));
    assert!(caps.contains(C::WasmRuntime));
    assert!(caps.contains(C::RenderRaster));
    assert!(caps.contains(C::EncryptionWrite));
    assert!(caps.contains(C::PdfuaValidate));
    assert!(caps.contains(C::EInvoiceZugferd));
    // Business additions
    assert!(caps.contains(C::PadesLongTerm));
    assert!(caps.contains(C::PadesLongTermArchive));
    assert!(caps.contains(C::PdfaConvertA1b));
    assert!(caps.contains(C::PdfaConvertA3b));
    assert!(caps.contains(C::OcrTesseract));
    assert!(caps.contains(C::TableExtract));
    assert!(caps.contains(C::PdfDiff));
    assert!(caps.contains(C::DocxExport));
    assert!(caps.contains(C::XlsxExport));
    assert!(caps.contains(C::PptxExport));
    assert!(caps.contains(C::Html2Pdf));
    // Enterprise absent
    assert!(!caps.contains(C::OcrPaddle));
    assert!(!caps.contains(C::AirGapped));
    assert!(!caps.contains(C::OemRedistribution));
}

// ---------------------------------------------------------------------------
// Enterprise
// ---------------------------------------------------------------------------

#[test]
fn enterprise_grants_all_capabilities() {
    let caps = Tier::Enterprise.capabilities();
    for cap in [
        C::PdfParse,
        C::PdfWrite,
        C::PageOps,
        C::TextExtract,
        C::TextExtractWithLayout,
        C::ImageExtract,
        C::TableExtract,
        C::RenderRaster,
        C::RenderThumbnail,
        C::AcroFormRead,
        C::AcroFormFill,
        C::AcroFormFlatten,
        C::XfaParse,
        C::XfaFill,
        C::XfaFlatten,
        C::EncryptionRead,
        C::EncryptionWrite,
        C::DigitalSignatureSign,
        C::DigitalSignatureVerify,
        C::PadesLongTerm,
        C::PadesLongTermArchive,
        C::PdfaValidate,
        C::PdfaConvertA1b,
        C::PdfaConvertA2b,
        C::PdfaConvertA3b,
        C::PdfuaValidate,
        C::PdfuaConvert,
        C::EInvoiceZugferd,
        C::EInvoiceFacturX,
        C::EInvoiceXRechnung,
        C::Redaction,
        C::TextEdit,
        C::OcrTesseract,
        C::OcrPaddle,
        C::Html2Pdf,
        C::DocxExport,
        C::XlsxExport,
        C::PptxExport,
        C::PdfDiff,
        C::WasmRuntime,
        C::AirGapped,
        C::OemRedistribution,
    ] {
        assert!(caps.contains(cap), "Enterprise must grant {cap:?}");
    }
}
