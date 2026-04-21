//! [`PdfDocument`] and lifecycle types.
//!
//! Per RFC 0001 §1, `PdfDocument` is the owning container for a parsed PDF.
//! It is `Send + Sync` and **not** `Clone` (explicit duplication via
//! `from_bytes(doc.to_bytes()?)`). Mutating operations take `&mut self`
//! directly or are exposed via scoped `_mut` accessors.

use std::io::{Read, Write};
use std::path::Path;

use crate::encrypt::EncryptOptions;
use crate::error::Result;
use crate::form::{FormField, PdfFormMut};
use crate::metadata::{Metadata, MetadataMut};
use crate::watermark::{Rotation, WatermarkOptions};

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

/// Options for opening a PDF document.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct OpenOptions {
    pub(crate) password: Option<String>,
    pub(crate) repair: bool,
    pub(crate) memory_limit: Option<usize>,
    pub(crate) license_key: Option<String>,
}

impl OpenOptions {
    /// New default options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the decryption password.
    pub fn with_password(mut self, pw: impl Into<String>) -> Self {
        self.password = Some(pw.into());
        self
    }

    /// Enable repair of malformed PDFs.
    pub fn with_repair(mut self, repair: bool) -> Self {
        self.repair = repair;
        self
    }

    /// Cap peak memory usage during load.
    pub fn strict_memory_limit(mut self, bytes: usize) -> Self {
        self.memory_limit = Some(bytes);
        self
    }

    /// Provide a per-document license key override.
    ///
    /// Overrides the process-global license set via
    /// [`crate::license::set_license_key`] or the `PDFLUENT_LICENSE_KEY`
    /// environment variable.
    pub fn with_license_key(mut self, key: impl Into<String>) -> Self {
        self.license_key = Some(key.into());
        self
    }
}

/// Options for saving a PDF document.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct SaveOptions {
    pub(crate) linearize: bool,
    pub(crate) overwrite: bool,
}

impl SaveOptions {
    /// New default options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Linearize the output for fast web view.
    pub fn with_linearize(mut self, v: bool) -> Self {
        self.linearize = v;
        self
    }

    /// Allow overwriting the source file when saving to the same path.
    pub fn with_overwrite(mut self, v: bool) -> Self {
        self.overwrite = v;
        self
    }
}

// ---------------------------------------------------------------------------
// PdfDocument
// ---------------------------------------------------------------------------

/// The primary entry point for a PDF document.
///
/// See RFC 0001 §1 for the full lifecycle contract. Key properties:
///
/// - `Send + Sync`
/// - Not `Clone`. Duplicate via `PdfDocument::from_bytes(doc.to_bytes()?)`.
/// - All mutation is explicit via `&mut self` methods or `_mut` accessors.
///
/// # Memory
///
/// A `PdfDocument` holds the fully-parsed PDF in memory. Peak extra
/// allocation per operation is documented on each method.
#[derive(Debug)]
pub struct PdfDocument {
    // Placeholder; replaced during Epic 2 wiring. Private field keeps the
    // type opaque from outside the crate.
    _placeholder: (),
}

impl PdfDocument {
    // ---------- Constructors ----------

    /// Open a PDF from a filesystem path.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::open_with(path, OpenOptions::new())
    }

    /// Open a PDF from a filesystem path with explicit options.
    pub fn open_with<P: AsRef<Path>>(_path: P, _opts: OpenOptions) -> Result<Self> {
        unimplemented!("Epic 2 #1242 wires this against lopdf::Document::load_from_path");
    }

    /// Construct a document from an in-memory byte buffer.
    pub fn from_bytes(_bytes: &[u8]) -> Result<Self> {
        unimplemented!("Epic 2 #1242 wires this against lopdf::Document::load_from");
    }

    /// Construct a document from bytes with explicit options.
    pub fn from_bytes_with(_bytes: &[u8], _opts: OpenOptions) -> Result<Self> {
        unimplemented!("Epic 2 #1242 wires this against lopdf::Document::load_from");
    }

    /// Construct a document from a `Read` stream.
    pub fn from_reader<R: Read>(_reader: R) -> Result<Self> {
        unimplemented!("Epic 2 #1242 wires this against lopdf::Document::load_from");
    }

    /// Create a new empty document.
    pub fn create() -> Self {
        unimplemented!("Epic 2 #1242 wires this against lopdf::Document::with_version");
    }

    // ---------- Read-only content access ----------

    /// Total number of pages.
    pub fn page_count(&self) -> usize {
        unimplemented!("Epic 2 #1242");
    }

    /// PDF version declared in the document header.
    pub fn version(&self) -> PdfVersion {
        unimplemented!("Epic 2 #1242");
    }

    /// Extract all plain text from the document.
    pub fn text(&self) -> Result<String> {
        unimplemented!("Epic 2 #1242");
    }

    /// Extract text grouped into structured blocks with coordinates.
    ///
    /// Matches the `Capability::TextExtractWithLayout` capability. Prefer
    /// [`text`] if you only need plain text.
    pub fn text_with_layout(&self) -> Result<Vec<TextBlock>> {
        unimplemented!("Epic 2 #1242");
    }

    /// Borrow a specific page (1-based).
    pub fn page(&self, _page_number: usize) -> Result<Page<'_>> {
        unimplemented!("Epic 2 #1242");
    }

    /// Iterate over all pages.
    pub fn pages(&self) -> Pages<'_> {
        unimplemented!("Epic 2 #1242");
    }

    /// Mutate pages in-place.
    pub fn pages_mut(&mut self) -> PagesMut<'_> {
        unimplemented!("Epic 2 #1242");
    }

    // ---------- Metadata ----------

    /// Read document metadata (Info dict + XMP).
    pub fn metadata(&self) -> Metadata {
        unimplemented!("Epic 2 #1245");
    }

    /// Mutate document metadata. Changes are flushed on
    /// [`MetadataMut::commit`] or when the handle is dropped.
    pub fn metadata_mut(&mut self) -> MetadataMut<'_> {
        unimplemented!("Epic 2 #1245");
    }

    // ---------- Forms ----------

    /// Read-only list of form fields. Returns an empty `Vec` if the document
    /// has no form.
    pub fn form_fields(&self) -> Vec<FormField> {
        unimplemented!("Epic 2 #1245");
    }

    /// Mutable form handle. Returns a handle even if the document has no
    /// form; errors surface on the individual `set_*` calls.
    pub fn form_mut(&mut self) -> PdfFormMut<'_> {
        unimplemented!("Epic 2 #1245");
    }

    /// Flatten all AcroForm fields to static content.
    pub fn flatten_forms(&mut self) -> Result<()> {
        unimplemented!("Epic 2 #1223 / #1245");
    }

    // ---------- Decoration ----------

    /// Add a text watermark to pages.
    pub fn add_watermark(&mut self, _text: &str, _opts: WatermarkOptions) -> Result<()> {
        unimplemented!("Epic 2 #1223");
    }

    // ---------- Page operations ----------

    /// Rotate a specific page.
    pub fn rotate_page(&mut self, _page: usize, _rotation: Rotation) -> Result<()> {
        unimplemented!("Epic 2 #1223 / #1243");
    }

    // ---------- Security ----------

    /// Encrypt the document with the given options.
    ///
    /// On an already-encrypted document, this re-encrypts with the new
    /// options (requires the owner password to be set via [`decrypt`] first).
    pub fn encrypt(&mut self, _opts: EncryptOptions) -> Result<()> {
        unimplemented!("Epic 2 #1244");
    }

    /// Decrypt the document using the provided password.
    pub fn decrypt(&mut self, _password: &str) -> Result<()> {
        unimplemented!("Epic 2 #1244");
    }

    /// Sign the document using the given signer.
    pub fn sign(
        &mut self,
        _signer: &dyn crate::signer::PdfSigner,
        _opts: crate::signer::SignOptions,
    ) -> Result<()> {
        unimplemented!("Epic 2 #1244");
    }

    /// Lightweight list of signatures present in the document.
    ///
    /// Does **not** cryptographically validate signatures. Use
    /// [`verify_signatures`] for full validation.
    pub fn signatures(&self) -> Result<Vec<crate::signer::SignatureInfo>> {
        unimplemented!("Epic 2 #1244");
    }

    /// Cryptographically validate all signatures and return a structured
    /// report with per-signature status, certificate-chain result, and
    /// timestamp verification.
    pub fn verify_signatures(&self) -> Result<crate::signer::SignatureValidationReport> {
        unimplemented!("Epic 2 #1244");
    }

    // ---------- Persistence ----------

    /// Save the document to a filesystem path.
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        self.save_with(path, SaveOptions::new())
    }

    /// Save with explicit options.
    pub fn save_with<P: AsRef<Path>>(&self, _path: P, _opts: SaveOptions) -> Result<()> {
        unimplemented!("Epic 2 #1242");
    }

    /// Serialise the document to a byte vector.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        unimplemented!("Epic 2 #1242");
    }

    /// Write the document to a `Write` sink.
    pub fn write_to<W: Write>(&self, _writer: W) -> Result<()> {
        unimplemented!("Epic 2 #1242");
    }
}

// ---------------------------------------------------------------------------
// Supporting types
// ---------------------------------------------------------------------------

/// PDF version tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdfVersion {
    /// Major version (1 or 2 for ISO 32000).
    pub major: u8,
    /// Minor version.
    pub minor: u8,
}

/// A structured block of text with bounding-box coordinates.
#[derive(Debug, Clone)]
pub struct TextBlock {
    /// Text content.
    pub text: String,
    /// Bounding box in PDF points: `[x_min, y_min, x_max, y_max]`.
    pub bbox: [f64; 4],
    /// Page number (1-based).
    pub page: usize,
}

/// Borrowed handle to a single page.
pub struct Page<'a> {
    _doc: std::marker::PhantomData<&'a PdfDocument>,
}

impl Page<'_> {
    /// Extract text from this page.
    pub fn text(&self) -> Result<String> {
        unimplemented!("Epic 2 #1242");
    }

    /// Page dimensions in points `(width, height)`.
    pub fn dimensions(&self) -> (f64, f64) {
        unimplemented!("Epic 2 #1242");
    }
}

/// Iterator over all pages.
pub struct Pages<'a> {
    _doc: std::marker::PhantomData<&'a PdfDocument>,
}

impl<'a> Iterator for Pages<'a> {
    type Item = Page<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        unimplemented!("Epic 2 #1242");
    }
}

/// Mutating iterator over all pages.
pub struct PagesMut<'a> {
    _doc: std::marker::PhantomData<&'a mut PdfDocument>,
}
