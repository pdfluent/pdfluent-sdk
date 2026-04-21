//! [`PdfDocument`] and lifecycle types.
//!
//! Per RFC 0001 §1, `PdfDocument` is the owning container for a parsed PDF.
//! It is `Send + Sync` and **not** `Clone` (explicit duplication via
//! `from_bytes(doc.to_bytes()?)`). Mutating operations take `&mut self`
//! directly or are exposed via scoped `_mut` accessors.
//!
//! # Implementation
//!
//! Internally `PdfDocument` holds two parallel representations of the same
//! PDF:
//!
//! - [`pdf_engine::PdfDocument`] — used for read-side operations (text
//!   extraction, page count, structured text, page geometry).
//! - [`lopdf::Document`] — used for write-side operations (save, save_to,
//!   mutations in later epics).
//!
//! Both are built from the same source bytes at construction time. Read-only
//! operations are routed to the engine; save operations flush the lopdf
//! representation. In later epics (Epic 2 #1244 security, etc.) mutations
//! will modify the lopdf representation and re-sync the engine on demand.

use std::fs;
use std::io::{Read, Write};
use std::path::Path;

use crate::capability::Capability;
use crate::encrypt::EncryptOptions;
use crate::error::{internal_error, Error, Result};
use crate::form::{FormField, PdfFormMut};
use crate::license;
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

    /// Cap the **input size** accepted during load.
    ///
    /// If the source file (for [`PdfDocument::open_with`]) or byte buffer
    /// (for [`PdfDocument::from_bytes_with`] / [`PdfDocument::from_reader`])
    /// exceeds `bytes`, load fails with
    /// [`crate::Error::MemoryBudgetExceeded`] before any parsing begins.
    ///
    /// # Scope — read carefully
    ///
    /// This is **not** a guaranteed cap on peak process memory. The PDF
    /// parser (`pdf-engine` + `lopdf`) builds internal tree structures
    /// whose size is bounded by the input but typically 1×–3× the raw
    /// byte count. During load, peak RSS can therefore reach ~3× `bytes`
    /// for pathological inputs.
    ///
    /// What it *does* guarantee:
    ///
    /// - A file larger than `bytes` is refused before any allocation.
    /// - This protects against the most common DoS vector (load a 10 GB
    ///   PDF to exhaust memory) without needing an OS-level cgroup.
    ///
    /// For hard peak-memory guarantees use OS-level sandboxing
    /// (cgroups, ulimit, jails) — the SDK cannot enforce them from
    /// inside the process.
    ///
    /// A stricter in-process peak-memory limiter is tracked as a
    /// post-1.0 improvement.
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
///
/// **Default behaviour** (per RFC 0001 §1.2): `linearize = false`,
/// `overwrite = false`. `save` / `save_with` therefore refuse to
/// clobber existing files unless you opt in via
/// [`with_overwrite(true)`](Self::with_overwrite).
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
    ///
    /// # 1.0 behaviour
    ///
    /// In 1.0 this flag is **accepted but currently a no-op**. A true
    /// linearizer lands in 1.1. Use it to make your code forward-compatible
    /// without waiting for the feature.
    pub fn with_linearize(mut self, v: bool) -> Self {
        self.linearize = v;
        self
    }

    /// Permit overwriting an existing file at the target path.
    ///
    /// Default is `false` per RFC 0001 §1.2: `save` / `save_with` refuse
    /// to clobber an existing file unless you opt in here. This protects
    /// against accidental overwrites of either the source file or
    /// unrelated files.
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
pub struct PdfDocument {
    engine: pdf_engine::PdfDocument,
    lopdf: lopdf::Document,
}

impl std::fmt::Debug for PdfDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PdfDocument")
            .field("page_count", &self.engine.page_count())
            .finish_non_exhaustive()
    }
}

impl PdfDocument {
    // ---------- Constructors ----------

    /// Open a PDF from a filesystem path.
    ///
    /// # Errors
    ///
    /// - [`Error::FileNotFound`] if the path does not exist.
    /// - [`Error::InvalidPdf`] if the file is not a valid PDF.
    /// - [`Error::DecryptionFailed`] if the file is encrypted and no
    ///   password was provided via [`OpenOptions::with_password`].
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::open_with(path, OpenOptions::new())
    }

    /// Open a PDF from a filesystem path with explicit options.
    pub fn open_with<P: AsRef<Path>>(path: P, opts: OpenOptions) -> Result<Self> {
        license::require_capability(Capability::PdfParse)?;
        let path_ref = path.as_ref();

        // Enforce memory budget BEFORE reading the file into memory. Without
        // this, a malicious PDF could exhaust RAM before the limit check ever
        // ran (file already in `bytes` by then).
        if let Some(limit) = opts.memory_limit {
            let metadata = fs::metadata(path_ref).map_err(|source| match source.kind() {
                std::io::ErrorKind::NotFound => Error::FileNotFound {
                    path: path_ref.to_path_buf(),
                },
                _ => Error::Io {
                    source,
                    path: Some(path_ref.to_path_buf()),
                },
            })?;
            let size = metadata.len() as usize;
            if size > limit {
                return Err(Error::MemoryBudgetExceeded {
                    requested: size,
                    limit,
                });
            }
        }

        let bytes = fs::read(path_ref).map_err(|source| match source.kind() {
            std::io::ErrorKind::NotFound => Error::FileNotFound {
                path: path_ref.to_path_buf(),
            },
            _ => Error::Io {
                source,
                path: Some(path_ref.to_path_buf()),
            },
        })?;
        Self::from_bytes_with(&bytes, opts)
    }

    /// Construct a document from an in-memory byte buffer.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Self::from_bytes_with(bytes, OpenOptions::new())
    }

    /// Construct a document from an in-memory byte buffer with explicit options.
    pub fn from_bytes_with(bytes: &[u8], opts: OpenOptions) -> Result<Self> {
        license::require_capability(Capability::PdfParse)?;

        if let Some(limit) = opts.memory_limit {
            if bytes.len() > limit {
                return Err(Error::MemoryBudgetExceeded {
                    requested: bytes.len(),
                    limit,
                });
            }
        }

        let owned = bytes.to_vec();
        let engine = match &opts.password {
            Some(pw) => pdf_engine::PdfDocument::open_with_password(owned.clone(), pw.as_str())
                .map_err(map_engine_error)?,
            None => pdf_engine::PdfDocument::open(owned.clone()).map_err(map_engine_error)?,
        };

        let lopdf = match &opts.password {
            Some(pw) => {
                let mut doc = lopdf::Document::load_mem(&owned).map_err(map_lopdf_error)?;
                // lopdf separates load from decrypt: decrypt in place if possible.
                let _ = doc.decrypt(pw.as_str());
                doc
            }
            None => lopdf::Document::load_mem(&owned).map_err(map_lopdf_error)?,
        };

        Ok(Self { engine, lopdf })
    }

    /// Construct a document from a [`std::io::Read`] stream.
    ///
    /// Reads the stream to completion into an in-memory buffer; streaming
    /// incremental parsing is tracked as a post-1.0 improvement.
    pub fn from_reader<R: Read>(mut reader: R) -> Result<Self> {
        license::require_capability(Capability::PdfParse)?;
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .map_err(|source| Error::Io { source, path: None })?;
        Self::from_bytes(&bytes)
    }

    /// Create a new empty document with a single blank page (US Letter).
    ///
    /// # 1.0 behaviour
    ///
    /// The document contains one blank 612×792-point page. Further
    /// page-level construction helpers land in Epic 2 #1243 (merge/combine)
    /// and will be added as non-breaking additions.
    pub fn create() -> Self {
        use lopdf::{dictionary, Document as LoDoc, Object};

        // Construct a valid minimal PDF via lopdf so byte offsets and
        // xref tables are guaranteed-correct. Re-parse the serialised
        // output so both the engine and lopdf representations are in
        // sync.
        let mut doc = LoDoc::with_version("1.7");
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => dictionary! {},
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);

        let mut buf: Vec<u8> = Vec::new();
        doc.save_to(&mut buf)
            .expect("serialising an in-memory lopdf::Document is infallible");
        Self::from_bytes(&buf).expect("freshly-built PDF must re-parse")
    }

    // ---------- Read-only content access ----------

    /// Total number of pages.
    pub fn page_count(&self) -> usize {
        self.engine.page_count()
    }

    /// PDF version declared in the document header.
    pub fn version(&self) -> PdfVersion {
        // lopdf exposes version as a string like "1.7". Parse defensively.
        let s = self.lopdf.version.as_str();
        PdfVersion::parse(s).unwrap_or(PdfVersion { major: 1, minor: 7 })
    }

    /// Extract all plain text from the document.
    ///
    /// Uses the underlying engine's `extract_all_text`, which concatenates
    /// per-page content streams with `\f` page separators (pdftotext
    /// convention) and appends any AcroForm field values.
    pub fn text(&self) -> Result<String> {
        license::require_capability(Capability::TextExtract)?;
        Ok(self.engine.extract_all_text())
    }

    /// Extract text grouped into structured blocks with coordinates.
    ///
    /// Matches the [`Capability::TextExtractWithLayout`] capability. Prefer
    /// [`text`](Self::text) if you only need plain text.
    pub fn text_with_layout(&self) -> Result<Vec<TextBlock>> {
        license::require_capability(Capability::TextExtractWithLayout)?;
        let mut out = Vec::new();
        let count = self.engine.page_count();
        for idx in 0..count {
            let blocks = self
                .engine
                .extract_text_blocks(idx)
                .map_err(map_engine_error)?;
            for block in blocks {
                out.push(TextBlock::from_engine(block, idx + 1));
            }
        }
        Ok(out)
    }

    /// Borrow a specific page (1-based).
    pub fn page(&self, page_number: usize) -> Result<Page<'_>> {
        let total = self.engine.page_count();
        if page_number == 0 || page_number > total {
            return Err(internal_error(format!(
                "page index {page_number} out of range (document has {total} pages)",
            )));
        }
        Ok(Page {
            doc: self,
            index: page_number - 1,
        })
    }

    /// Iterate over all pages.
    pub fn pages(&self) -> Pages<'_> {
        Pages { doc: self, next: 0 }
    }

    // ---------- Metadata (Epic 2 #1245) ----------

    /// Read document metadata (Info dict + XMP-derived date fields).
    ///
    /// Combines [`pdf_engine::DocumentInfo`] (title/author/subject/keywords/
    /// creator/producer) with the `/CreationDate` + `/ModDate` strings
    /// extracted directly from the Info dict.
    pub fn metadata(&self) -> Metadata {
        let info = self.engine.info();
        let (creation, modification) = crate::metadata::read_info_dates(&self.lopdf);
        Metadata {
            title: info.title,
            author: info.author,
            subject: info.subject,
            keywords: crate::metadata::parse_keywords(info.keywords),
            producer: info.producer,
            creator: info.creator,
            creation_date: creation,
            modification_date: modification,
        }
    }

    /// Mutate document metadata. The returned handle flushes changes on
    /// [`MetadataMut::commit`] or when dropped.
    ///
    /// The handle operates on the internal lopdf `/Info` dictionary.
    /// After commit, a subsequent `save` / `to_bytes` serialises the
    /// updated metadata. Note that the engine-side representation used
    /// by [`metadata`](Self::metadata) is only refreshed when the document
    /// is re-opened; call `metadata_mut(...).commit()?.to_bytes()?` + reopen
    /// if you need the engine-side to reflect new values in the same
    /// process.
    pub fn metadata_mut(&mut self) -> MetadataMut<'_> {
        MetadataMut::new(self)
    }

    // ---------- Internal accessors ----------

    /// Crate-private read access to the lopdf representation. Used by the
    /// merger and future read-side mutating facades.
    pub(crate) fn lopdf(&self) -> &lopdf::Document {
        &self.lopdf
    }

    /// Crate-private mutable access to the lopdf representation. Used by
    /// `MetadataMut::commit` and future mutating facade methods.
    pub(crate) fn lopdf_mut(&mut self) -> &mut lopdf::Document {
        &mut self.lopdf
    }

    /// Crate-private constructor from a freshly-built lopdf document.
    ///
    /// Used by merge/split/extract paths that produce a new `lopdf::Document`
    /// from existing inputs. Re-parses via `to_bytes` round-trip so the
    /// parallel pdf-engine representation is consistent with the lopdf one.
    pub(crate) fn from_lopdf(mut lopdf_doc: lopdf::Document) -> Result<Self> {
        let mut buf = Vec::with_capacity(64 * 1024);
        lopdf_doc
            .save_to(&mut buf)
            .map_err(|source| Error::Io { source, path: None })?;
        Self::from_bytes(&buf)
    }

    // ---------- Forms (Epic 2 #1245 / #1223) ----------

    /// Read-only list of AcroForm fields.
    ///
    /// Returns an empty `Vec` if the document has no AcroForm dictionary.
    /// XFA-only documents also return an empty Vec; use XFA-specific APIs
    /// (tracked separately) for XFA field enumeration.
    ///
    /// # 1.0 scope
    ///
    /// The read surface covers field name, field type, current value, and
    /// required/read-only flags. Richer field introspection (kid hierarchy,
    /// widget appearances, javascript actions) lands with the form-mutation
    /// wiring in follow-up issues.
    pub fn form_fields(&self) -> Result<Vec<FormField>> {
        license::require_capability(Capability::AcroFormRead)?;
        Ok(crate::form::read_acroform_fields(&self.lopdf))
    }

    /// Mutable form handle.
    pub fn form_mut(&mut self) -> PdfFormMut<'_> {
        unimplemented!("Epic 2 #1245");
    }

    /// Flatten all AcroForm fields to static content.
    pub fn flatten_forms(&mut self) -> Result<()> {
        unimplemented!("Epic 2 #1223 / #1245");
    }

    // ---------- Decoration (Epic 2 #1223) ----------

    /// Add a text watermark to pages.
    pub fn add_watermark(&mut self, _text: &str, _opts: WatermarkOptions) -> Result<()> {
        unimplemented!("Epic 2 #1223");
    }

    // ---------- Page operations (Epic 2 #1223 / #1243) ----------

    /// Rotate a specific page by a quarter-turn.
    ///
    /// Pages are 1-based. Routes to `pdf_manip::pages::rotate_page`.
    ///
    /// # Errors
    ///
    /// - [`Error::Internal`] wrapping "page out of range" if `page` is 0 or
    ///   exceeds [`page_count`](Self::page_count).
    pub fn rotate_page(&mut self, page: usize, rotation: Rotation) -> Result<()> {
        license::require_capability(Capability::PageOps)?;
        let total = self.engine.page_count();
        if page == 0 || page > total {
            return Err(internal_error(format!(
                "page index {page} out of range (document has {total} pages)",
            )));
        }
        let degrees: i64 = match rotation {
            Rotation::Clockwise90 => 90,
            Rotation::Clockwise180 => 180,
            Rotation::Clockwise270 => 270,
        };
        pdf_manip::pages::rotate_page(&mut self.lopdf, page as u32, degrees)
            .map_err(map_manip_error)
    }

    // ---------- Security (Epic 2 #1244) ----------

    /// Encrypt the document with the given options.
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
    pub fn signatures(&self) -> Result<Vec<crate::signer::SignatureInfo>> {
        unimplemented!("Epic 2 #1244");
    }

    /// Cryptographically validate all signatures.
    pub fn verify_signatures(&self) -> Result<crate::signer::SignatureValidationReport> {
        unimplemented!("Epic 2 #1244");
    }

    // ---------- Redaction (Epic 2 #1244) ----------

    /// Redact every occurrence of the given text.
    pub fn redact(&mut self, _text: &str, _opts: crate::redact::RedactOptions) -> Result<()> {
        unimplemented!("Epic 2 #1244");
    }

    /// Redact a specific rectangular region on the given page.
    pub fn redact_region(&mut self, _page: usize, _rect: [f64; 4]) -> Result<()> {
        unimplemented!("Epic 2 #1244");
    }

    // ---------- Split / extract (Epic 2 #1243) ----------

    /// Split the document into individual one-page documents.
    ///
    /// The source document is unchanged. Returns a new [`PdfDocument`] per
    /// input page, in order. Bookmarks on the source are **not** propagated
    /// per-page in 1.0; this is a best-effort split that only preserves
    /// page content.
    pub fn split_pages(&self) -> Result<Vec<PdfDocument>> {
        license::require_capability(Capability::PageOps)?;
        let split = pdf_manip::pages::split_per_page(&self.lopdf).map_err(map_manip_error)?;
        let mut out = Vec::with_capacity(split.len());
        for lopdf_doc in split {
            out.push(Self::from_lopdf(lopdf_doc)?);
        }
        Ok(out)
    }

    /// Extract a page range into a new document.
    ///
    /// Accepts any range expression (inclusive or exclusive). Pages are
    /// 1-based. The source document is unchanged.
    ///
    /// ```no_run
    /// # use pdfluent::prelude::*;
    /// # fn run() -> Result<()> {
    /// let doc = PdfDocument::open("full.pdf")?;
    /// let first_chapter = doc.extract_pages(1..=10)?;
    /// first_chapter.save("chapter1.pdf")?;
    /// # Ok(()) }
    /// ```
    ///
    /// # Errors
    ///
    /// - [`Error::Internal`] if the normalised range is empty or points
    ///   past the end of the document.
    pub fn extract_pages<R: std::ops::RangeBounds<usize>>(&self, range: R) -> Result<PdfDocument> {
        license::require_capability(Capability::PageOps)?;
        let total = self.engine.page_count();
        let (start, end) = normalise_page_range(&range, total)?;
        let pages: Vec<u32> = (start..=end).map(|p| p as u32).collect();
        let lopdf_doc =
            pdf_manip::pages::extract_pages(&self.lopdf, &pages).map_err(map_manip_error)?;
        Self::from_lopdf(lopdf_doc)
    }

    // ---------- Persistence ----------

    /// Save the document to a filesystem path.
    ///
    /// **Refuses to clobber existing files by default** (per RFC §1.2). If
    /// `path` already points at an existing file, returns
    /// [`Error::Io`] with `ErrorKind::AlreadyExists`. To overwrite, use
    /// [`save_with`](Self::save_with) with
    /// `SaveOptions::new().with_overwrite(true)`.
    ///
    /// ```no_run
    /// # use pdfluent::prelude::*;
    /// # fn run(doc: PdfDocument) -> Result<()> {
    /// // New output file: works.
    /// doc.save("output-new.pdf")?;
    ///
    /// // Existing path: refused unless you opt in.
    /// doc.save_with(
    ///     "output-new.pdf",
    ///     SaveOptions::new().with_overwrite(true),
    /// )?;
    /// # Ok(()) }
    /// ```
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        self.save_with(path, SaveOptions::new())
    }

    /// Save with explicit options.
    ///
    /// When `opts.overwrite` is `false` (the default — see
    /// [`SaveOptions::with_overwrite`]) and the target file already exists,
    /// returns [`Error::Io`] with `ErrorKind::AlreadyExists` rather than
    /// clobbering the file. This honours RFC §1.2: *"save writes to a new
    /// path or overwrites only when explicitly requested"*.
    ///
    /// See [`SaveOptions::with_linearize`] for the 1.0 linearize-is-no-op
    /// caveat.
    pub fn save_with<P: AsRef<Path>>(&self, path: P, opts: SaveOptions) -> Result<()> {
        license::require_capability(Capability::PdfWrite)?;
        let path_ref = path.as_ref();
        if !opts.overwrite && path_ref.exists() {
            return Err(Error::Io {
                source: std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "target file exists; pass `SaveOptions::new().with_overwrite(true)` to clobber",
                ),
                path: Some(path_ref.to_path_buf()),
            });
        }
        let bytes = self.to_bytes()?;
        fs::write(path_ref, bytes).map_err(|source| Error::Io {
            source,
            path: Some(path_ref.to_path_buf()),
        })?;
        Ok(())
    }

    /// Serialise the document to a byte vector.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        license::require_capability(Capability::PdfWrite)?;
        let mut buf = Vec::with_capacity(64 * 1024);
        // lopdf::Document::save_to takes &mut self; clone so `to_bytes`
        // stays &self. For documents up to ~200 MB this is acceptable;
        // streaming serialisation is a post-1.0 optimisation.
        let mut clone = self.lopdf.clone();
        clone
            .save_to(&mut buf)
            .map_err(|source| Error::Io { source, path: None })?;
        Ok(buf)
    }

    /// Write the document to a [`std::io::Write`] sink.
    pub fn write_to<W: Write>(&self, mut writer: W) -> Result<()> {
        license::require_capability(Capability::PdfWrite)?;
        let bytes = self.to_bytes()?;
        writer
            .write_all(&bytes)
            .map_err(|source| Error::Io { source, path: None })?;
        Ok(())
    }
}

// `PdfDocument` is `Send + Sync` as long as its fields are.
// `pdf_engine::PdfDocument` and `lopdf::Document` are both Send + Sync at time of writing.

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

impl PdfVersion {
    /// Parse a PDF version string of the form `"1.7"` or `"2.0"`.
    pub fn parse(s: &str) -> Option<Self> {
        let mut parts = s.trim_start_matches("PDF-").split('.');
        let major: u8 = parts.next()?.parse().ok()?;
        let minor: u8 = parts.next()?.parse().ok()?;
        Some(Self { major, minor })
    }
}

impl std::fmt::Display for PdfVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
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

impl TextBlock {
    fn from_engine(engine_block: pdf_engine::TextBlock, page_1_based: usize) -> Self {
        // Compute an enclosing bbox from the block's spans. `pdf_engine`
        // stores per-span (x, y, width, height); we aggregate to the
        // `[x_min, y_min, x_max, y_max]` representation promised by our
        // public `TextBlock`.
        let mut x_min = f64::INFINITY;
        let mut y_min = f64::INFINITY;
        let mut x_max = f64::NEG_INFINITY;
        let mut y_max = f64::NEG_INFINITY;
        for span in &engine_block.spans {
            x_min = x_min.min(span.x);
            y_min = y_min.min(span.y);
            x_max = x_max.max(span.x + span.width);
            y_max = y_max.max(span.y + span.height);
        }
        if !x_min.is_finite() {
            x_min = 0.0;
            y_min = 0.0;
            x_max = 0.0;
            y_max = 0.0;
        }
        let text = engine_block.text();
        Self {
            text,
            bbox: [x_min, y_min, x_max, y_max],
            page: page_1_based,
        }
    }
}

/// Borrowed handle to a single page.
pub struct Page<'a> {
    doc: &'a PdfDocument,
    /// 0-based index into the document's page list.
    index: usize,
}

impl Page<'_> {
    /// Extract text from this page.
    pub fn text(&self) -> Result<String> {
        license::require_capability(Capability::TextExtract)?;
        self.doc
            .engine
            .extract_text(self.index)
            .map_err(map_engine_error)
    }

    /// Page dimensions in points `(width, height)`.
    pub fn dimensions(&self) -> (f64, f64) {
        match self.doc.engine.page_geometry(self.index) {
            Ok(geom) => (geom.crop_box.width(), geom.crop_box.height()),
            Err(_) => (0.0, 0.0),
        }
    }

    /// 1-based page number.
    pub fn number(&self) -> usize {
        self.index + 1
    }
}

/// Iterator over all pages.
pub struct Pages<'a> {
    doc: &'a PdfDocument,
    next: usize,
}

impl<'a> Iterator for Pages<'a> {
    type Item = Page<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.doc.engine.page_count() {
            return None;
        }
        let page = Page {
            doc: self.doc,
            index: self.next,
        };
        self.next += 1;
        Some(page)
    }
}

// ---------------------------------------------------------------------------
// Internal error mapping
// ---------------------------------------------------------------------------

fn map_engine_error(e: pdf_engine::EngineError) -> Error {
    // Flatten pdf-engine's error variants into our public `Error`. Epic 4
    // #1231 will tighten this with richer per-variant mapping and binding
    // integration.
    use pdf_engine::EngineError as E;
    match e {
        E::Encrypted(_reason) => Error::DecryptionFailed {
            reason: crate::error::DecryptionFailureReason::WrongPassword,
        },
        E::InvalidPdf(reason) => Error::InvalidPdf {
            byte_offset: None,
            reason,
        },
        other => Error::InvalidPdf {
            byte_offset: None,
            reason: format!("{other:?}"),
        },
    }
}

fn map_lopdf_error(e: lopdf::Error) -> Error {
    Error::InvalidPdf {
        byte_offset: None,
        reason: e.to_string(),
    }
}

fn map_manip_error(e: pdf_manip::ManipError) -> Error {
    // Placeholder mapping — Epic 4 #1231 will tighten per-variant. For now
    // route everything through InvalidPdf, which matches the existing
    // pdf-engine/lopdf error-pattern in this crate.
    Error::InvalidPdf {
        byte_offset: None,
        reason: e.to_string(),
    }
}

/// Convert a user-provided `RangeBounds<usize>` into an inclusive 1-based
/// `(start, end)` page range, validating against the document size.
fn normalise_page_range<R: std::ops::RangeBounds<usize>>(
    range: &R,
    total_pages: usize,
) -> Result<(usize, usize)> {
    use std::ops::Bound;
    let start = match range.start_bound() {
        Bound::Included(&n) => n,
        Bound::Excluded(&n) => n + 1,
        Bound::Unbounded => 1,
    };
    let end = match range.end_bound() {
        Bound::Included(&n) => n,
        Bound::Excluded(&n) => n.saturating_sub(1),
        Bound::Unbounded => total_pages,
    };
    if start == 0 || start > total_pages || end < start || end > total_pages {
        return Err(internal_error(format!(
            "page range {start}..={end} is out of bounds (document has {total_pages} pages)",
        )));
    }
    Ok((start, end))
}
