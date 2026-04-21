//! Capability enumeration.
//!
//! Each method on the public API that is subject to licensing requires a
//! [`Capability`]. The runtime license checks the tier's capability set
//! before executing the method body and returns
//! [`crate::Error::FeatureNotInTier`] if the capability is unavailable.
//!
//! See RFC 0001 §6 for the full mapping to tiers.

/// A capability represents a discrete SDK feature subject to licensing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Capability {
    // ---------- Core (always available) ----------
    /// Parse PDF documents.
    PdfParse,
    /// Serialise PDF documents.
    PdfWrite,
    /// Page-level operations (rotate, split, merge).
    PageOps,

    // ---------- Extraction ----------
    /// Extract plain text.
    TextExtract,
    /// Extract text with layout (bounding boxes).
    TextExtractWithLayout,
    /// Extract embedded images.
    ImageExtract,
    /// Extract tables.
    TableExtract,

    // ---------- Rendering ----------
    /// Rasterise pages to PNG/JPEG/WebP.
    RenderRaster,
    /// Generate page thumbnails.
    RenderThumbnail,

    // ---------- Forms ----------
    /// Read AcroForm fields.
    AcroFormRead,
    /// Fill AcroForm fields.
    AcroFormFill,
    /// Flatten AcroForms to static content.
    AcroFormFlatten,
    /// Parse XFA forms.
    XfaParse,
    /// Fill XFA form data.
    XfaFill,
    /// Flatten XFA forms to AcroForm or static content.
    XfaFlatten,

    // ---------- Security ----------
    /// Open encrypted PDFs.
    EncryptionRead,
    /// Encrypt PDFs (AES-256).
    EncryptionWrite,
    /// Sign documents with digital signatures.
    DigitalSignatureSign,
    /// Verify digital signatures.
    DigitalSignatureVerify,
    /// PAdES B-LT (long-term validation) signing.
    PadesLongTerm,
    /// PAdES B-LTA (long-term with archive timestamp) signing.
    PadesLongTermArchive,

    // ---------- Compliance ----------
    /// Validate PDF/A conformance.
    PdfaValidate,
    /// Convert to PDF/A-1b.
    PdfaConvertA1b,
    /// Convert to PDF/A-2b.
    PdfaConvertA2b,
    /// Convert to PDF/A-3b.
    PdfaConvertA3b,
    /// Validate PDF/UA accessibility.
    PdfuaValidate,
    /// Convert to PDF/UA.
    PdfuaConvert,
    /// ZUGFeRD e-invoicing support.
    EInvoiceZugferd,
    /// Factur-X e-invoicing support.
    EInvoiceFacturX,
    /// XRechnung e-invoicing support.
    EInvoiceXRechnung,

    // ---------- Advanced ----------
    /// Redaction of text and content.
    Redaction,
    /// OCR using Tesseract.
    OcrTesseract,
    /// OCR using PaddleOCR.
    OcrPaddle,
    /// HTML-to-PDF conversion via headless Chromium.
    Html2Pdf,
    /// Export to DOCX.
    DocxExport,
    /// Export to XLSX.
    XlsxExport,
    /// Export to PPTX.
    PptxExport,
    /// Document diff/compare.
    PdfDiff,

    // ---------- Deployment ----------
    /// Run in WebAssembly environments.
    WasmRuntime,
    /// Run in air-gapped/offline deployments.
    AirGapped,
    /// OEM/SaaS redistribution rights.
    OemRedistribution,
}

/// A compact set of capabilities granted by a tier.
///
/// Internally backed by a bitset for O(1) membership tests. Construction is
/// typically via [`crate::Tier::capabilities`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CapabilitySet {
    bits: u64,
}

impl CapabilitySet {
    /// An empty capability set.
    pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    /// Whether the set contains the given capability.
    pub fn contains(&self, cap: Capability) -> bool {
        let bit = 1u64 << (cap as u32);
        self.bits & bit != 0
    }

    /// Add a capability to the set.
    pub fn with(mut self, cap: Capability) -> Self {
        self.bits |= 1u64 << (cap as u32);
        self
    }
}

impl std::fmt::Display for CapabilitySet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CapabilitySet(0b{:b})", self.bits)
    }
}
