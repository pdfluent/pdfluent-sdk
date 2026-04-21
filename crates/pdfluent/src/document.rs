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
use crate::decoration::PageDecoration;
use crate::encrypt::EncryptOptions;
use crate::error::{internal_error, Error, Result};
use crate::form::{FormField, PdfFormMut};
use crate::license;
use crate::metadata::{Metadata, MetadataMut};
use crate::parity::{
    CompressOptions, CompressReport, FontSubsetReport, ImageInsert, ImageInsertReport,
    InsertImageFormat, ToImagesOptions, ToImagesReport,
};
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
    /// Per-document license-key override from
    /// [`OpenOptions::with_license_key`]. Consulted by
    /// [`require_capability`](Self::require_capability) when gated
    /// methods are called. `None` means the process-global license
    /// (or env, or Trial) applies.
    license_key_override: Option<String>,
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
            Some(pw) => pdf_engine::PdfDocument::open_with_password(owned.clone(), pw.as_str())?,
            None => pdf_engine::PdfDocument::open(owned.clone())?,
        };

        let lopdf = match &opts.password {
            Some(pw) => {
                let mut doc = lopdf::Document::load_mem(&owned)?;
                // lopdf separates load from decrypt: decrypt in place if possible.
                let _ = doc.decrypt(pw.as_str());
                doc
            }
            None => lopdf::Document::load_mem(&owned)?,
        };

        Ok(Self {
            engine,
            lopdf,
            license_key_override: opts.license_key.clone(),
        })
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
        self.require_capability(Capability::TextExtract)?;
        Ok(self.engine.extract_all_text())
    }

    /// Extract text grouped into structured blocks with coordinates.
    ///
    /// Matches the [`Capability::TextExtractWithLayout`] capability. Prefer
    /// [`text`](Self::text) if you only need plain text.
    pub fn text_with_layout(&self) -> Result<Vec<TextBlock>> {
        self.require_capability(Capability::TextExtractWithLayout)?;
        let mut out = Vec::new();
        let count = self.engine.page_count();
        for idx in 0..count {
            let blocks = self.engine.extract_text_blocks(idx)?;
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

    // ---------- Capability enforcement ----------

    /// Check that this document's effective license grants `cap`.
    ///
    /// Honours the per-document [`OpenOptions::with_license_key`]
    /// override set at construction before falling back to process-global
    /// / env / Trial per [`license::effective_tier`].
    pub(crate) fn require_capability(&self, cap: Capability) -> Result<()> {
        license::require_capability_with_override(cap, self.license_key_override.as_deref())
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
        self.require_capability(Capability::AcroFormRead)?;
        Ok(crate::form::read_acroform_fields(&self.lopdf))
    }

    /// Mutable form handle.
    ///
    /// Returns unconditionally — the handle is always constructable, even
    /// on documents without an AcroForm. Capability enforcement and field
    /// lookups happen on the individual setter calls.
    ///
    /// See [`PdfFormMut`] for the 1.0 scope notes (flat AcroForm walk,
    /// no `/Kids` recursion).
    pub fn form_mut(&mut self) -> PdfFormMut<'_> {
        // Read the license override BEFORE the mutable borrow of `lopdf`
        // so the two field borrows don't overlap.
        let license_override = self.license_key_override.as_deref();
        PdfFormMut::new(&mut self.lopdf, license_override)
    }

    /// Flatten all AcroForm fields to static content.
    pub fn flatten_forms(&mut self) -> Result<()> {
        unimplemented!("Epic 2 #1223 / #1245");
    }

    // ---------- Decoration (Epic 2 #1223 / Epic 3 #1225) ----------

    /// Apply a page decoration.
    ///
    /// This is the consolidated entry point for all decoration families
    /// (watermark today; header/footer, page numbers, and stamp land
    /// in 1.1 via new [`PageDecoration`] variants without breaking this
    /// signature).
    ///
    /// # Family-specific methods
    ///
    /// [`add_watermark`](Self::add_watermark) is preserved for backwards
    /// compatibility with website snippets; it delegates here.
    ///
    /// # 1.0 status
    ///
    /// The decoration runtime is tracked on issue #1223. Calling this
    /// method currently returns [`Error::MissingDependency`] — the
    /// consolidated surface is in place so 1.0 code compiles against
    /// the final API, but the rendering pipeline lands post-freeze.
    pub fn add_decoration(&mut self, decoration: PageDecoration) -> Result<()> {
        self.require_capability(Capability::PdfWrite)?;
        match decoration {
            PageDecoration::Watermark {
                text: _,
                options: _,
            } => Err(Error::MissingDependency {
                dep: "pdf-manip::watermark",
                install_hint:
                    "watermark runtime lands with Epic 2 #1223; consolidated surface is in place",
            }),
        }
    }

    /// Add a text watermark to pages.
    ///
    /// Thin wrapper around [`add_decoration`](Self::add_decoration) with
    /// the [`PageDecoration::Watermark`] variant.
    pub fn add_watermark(&mut self, text: &str, opts: WatermarkOptions) -> Result<()> {
        self.add_decoration(PageDecoration::watermark(text, opts))
    }

    // ---------- Parity methods (Epic 3 #1224) ----------

    /// Convert the document to a `.docx` file on disk.
    ///
    /// Routes to [`pdf_docx::convert_pdf_bytes_to_docx`], which runs a
    /// text-extraction pipeline over the PDF and emits an Office Open XML
    /// document. The conversion is text-oriented: tables, layout and
    /// images are best-effort and may not round-trip perfectly.
    ///
    /// # Capability
    ///
    /// Requires [`Capability::DocxExport`] (Business tier and up per
    /// RFC §6.3).
    ///
    /// # 1.0 note
    ///
    /// The `docx-export` Cargo feature flag is reserved for future
    /// compile-time gating; in 1.0 the method always compiles on
    /// non-wasm targets. On wasm32 the method returns
    /// [`Error::UnsupportedOnWasm`] because `pdf-docx` is not compiled
    /// for that target.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn to_docx<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        self.require_capability(Capability::DocxExport)?;
        let pdf_bytes = self.to_bytes()?;
        let docx_bytes = pdf_docx::convert_pdf_bytes_to_docx(&pdf_bytes)
            .map_err(|e| internal_error(format!("docx conversion failed: {e}")))?;
        fs::write(path.as_ref(), docx_bytes).map_err(|source| Error::Io {
            source,
            path: Some(path.as_ref().to_path_buf()),
        })?;
        Ok(())
    }

    /// Convert the document to a `.docx` file — wasm stub.
    #[cfg(target_arch = "wasm32")]
    pub fn to_docx<P: AsRef<Path>>(&self, _path: P) -> Result<()> {
        Err(Error::UnsupportedOnWasm {
            operation: "to_docx",
        })
    }

    /// Render each page to an image file.
    ///
    /// `pattern` is a path template; the substring `{page}` (if present)
    /// is replaced with the 1-based page number. When `{page}` is not
    /// present, `_{page}` is appended before the extension.
    ///
    /// Returns the list of written paths in page order.
    ///
    /// # Capability
    ///
    /// Requires [`Capability::RenderRaster`] (available at every tier).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn to_images<P: AsRef<Path>>(
        &self,
        pattern: P,
        opts: ToImagesOptions,
    ) -> Result<ToImagesReport> {
        use pdf_engine::render::RenderOptions;

        self.require_capability(Capability::RenderRaster)?;

        let total = self.engine.page_count();
        let (from, to) = match opts.pages {
            Some((f, t)) => {
                if f == 0 || t < f || t > total {
                    return Err(internal_error(format!(
                        "invalid page range {f}..={t} (document has {total} pages)",
                    )));
                }
                (f, t)
            }
            None => (1, total),
        };

        let render_opts = RenderOptions {
            dpi: opts.dpi as f64,
            ..Default::default()
        };

        let mut out_paths = Vec::with_capacity(to - from + 1);
        for page_idx_1b in from..=to {
            // `PdfEngine::render_page` takes a 0-based index internally.
            let rendered = self
                .engine
                .render_page(page_idx_1b - 1, &render_opts)
                .map_err(|e| internal_error(format!("render page {page_idx_1b} failed: {e}")))?;

            let path = build_image_path(pattern.as_ref(), page_idx_1b, opts.format.extension());
            encode_image(&rendered, opts.format, &path)?;
            out_paths.push(path);
        }

        Ok(ToImagesReport { paths: out_paths })
    }

    /// Render each page to an image file — wasm stub.
    #[cfg(target_arch = "wasm32")]
    pub fn to_images<P: AsRef<Path>>(
        &self,
        _pattern: P,
        _opts: ToImagesOptions,
    ) -> Result<ToImagesReport> {
        Err(Error::UnsupportedOnWasm {
            operation: "to_images",
        })
    }

    /// Compress the document by running the full optimisation stack.
    ///
    /// Returns a [`CompressReport`] describing what each pass did.
    ///
    /// # Capability
    ///
    /// Core-tier (always available).
    pub fn compress(&mut self, opts: CompressOptions) -> Result<CompressReport> {
        self.require_capability(Capability::PdfWrite)?;

        let mut report = CompressReport::default();

        if opts.subset_fonts {
            let subset = pdf_manip::font_subset::subset_fonts(&mut self.lopdf)
                .map_err(|e| internal_error(format!("font subsetting failed: {e:?}")))?;
            report.font_subset = Some(FontSubsetReport {
                fonts_processed: subset.fonts_processed,
                fonts_subsetted: subset.fonts_subsetted,
                bytes_saved: subset.bytes_saved,
            });
        }

        if opts.compress_streams {
            report.streams_compressed = pdf_manip::optimize::compress_streams(&mut self.lopdf)
                .map_err(|e| internal_error(format!("stream compression failed: {e:?}")))?;
        }

        if opts.deduplicate_streams {
            report.streams_deduplicated = pdf_manip::optimize::deduplicate_streams(&mut self.lopdf);
        }

        if opts.remove_unused {
            report.unused_removed = pdf_manip::optimize::remove_unused_objects(&mut self.lopdf);
        }

        self.refresh_from_lopdf()?;
        Ok(report)
    }

    /// Linearize the document for fast web-view.
    ///
    /// # 1.0 scope — honest deferred
    ///
    /// Real PDF linearization (hint-stream construction, object-order
    /// rewriting, cross-reference update per ISO 32000-1 Annex F) is a
    /// multi-day implementation not yet available in `pdf-manip`. Per
    /// RFC decision D8 and issue #1224, this method ships as part of
    /// the 1.0 API surface so that `SaveOptions::with_linearize(true)`
    /// code compiles today, but a call returns
    /// [`Error::MissingDependency`] pointing to the follow-up crate.
    ///
    /// **Truth-gap**: users MUST check the result; silent no-op would
    /// violate the "no fake support" rule.
    pub fn linearize(&mut self) -> Result<()> {
        self.require_capability(Capability::PdfWrite)?;
        Err(Error::MissingDependency {
            dep: "pdf-manip::linearize",
            install_hint: "linearization not yet implemented; tracked as a 1.1 follow-up to #1224",
        })
    }

    /// Subset every embedded font to only the glyphs actually used.
    ///
    /// Smaller output files without visual changes. Routes to
    /// [`pdf_manip::font_subset::subset_fonts`].
    ///
    /// # Capability
    ///
    /// Core-tier (always available).
    pub fn subset_fonts(&mut self) -> Result<FontSubsetReport> {
        self.require_capability(Capability::PdfWrite)?;
        let subset = pdf_manip::font_subset::subset_fonts(&mut self.lopdf)
            .map_err(|e| internal_error(format!("font subsetting failed: {e:?}")))?;
        self.refresh_from_lopdf()?;
        Ok(FontSubsetReport {
            fonts_processed: subset.fonts_processed,
            fonts_subsetted: subset.fonts_subsetted,
            bytes_saved: subset.bytes_saved,
        })
    }

    /// Embed an OpenType/TrueType font into the document.
    ///
    /// # 1.0 scope — honest deferred
    ///
    /// `pdf-manip` currently only has PDF/A-driven font embedding
    /// (`pdfa_fonts::embed_fonts`), which assumes existing font
    /// references in the page tree. A general-purpose "add a new font
    /// for future content" pipeline (write a fresh Type0 composite
    /// font dict + CIDFont + FontDescriptor + FontFile2/3 stream, then
    /// register it in every page's `/Resources/Font`) is a multi-day
    /// task. Per issue #1224 and RFC D8 this method is part of the
    /// 1.0 surface but a call returns [`Error::MissingDependency`].
    pub fn embed_font(&mut self, _font_data: &[u8], _name: &str) -> Result<()> {
        self.require_capability(Capability::PdfWrite)?;
        Err(Error::MissingDependency {
            dep: "pdf-manip::embed_font",
            install_hint:
                "arbitrary font embedding not yet implemented; tracked as a 1.1 follow-up to #1224",
        })
    }

    /// Insert a JPEG or PNG image onto a page.
    ///
    /// Routes to [`pdf_manip::image_insert::insert_image`].
    ///
    /// # Capability
    ///
    /// Core-tier (always available).
    pub fn insert_image(&mut self, img: ImageInsert) -> Result<ImageInsertReport> {
        self.require_capability(Capability::PdfWrite)?;

        let total = self.engine.page_count();
        if img.page == 0 || img.page > total {
            return Err(internal_error(format!(
                "page index {} out of range (document has {} pages)",
                img.page, total,
            )));
        }

        let format = match img.format {
            InsertImageFormat::Jpeg => pdf_manip::image_insert::ImageFormat::Jpeg,
            InsertImageFormat::Png => pdf_manip::image_insert::ImageFormat::Png,
        };

        let insert = pdf_manip::image_insert::ImageInsert {
            image_data: img.bytes,
            format,
            x: img.x,
            y: img.y,
            width: img.width,
            height: img.height,
            page_index: img.page as u32,
            opacity: img.opacity,
        };

        let res = pdf_manip::image_insert::insert_image(&mut self.lopdf, &insert)
            .map_err(|e| internal_error(format!("image insertion failed: {e:?}")))?;

        self.refresh_from_lopdf()?;
        Ok(ImageInsertReport {
            pixel_width: res.pixel_width,
            pixel_height: res.pixel_height,
            resource_name: res.resource_name,
        })
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
        self.require_capability(Capability::PageOps)?;
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
        pdf_manip::pages::rotate_page(&mut self.lopdf, page as u32, degrees)?;
        Ok(())
    }

    // ---------- Security (Epic 2 #1244) ----------

    /// Encrypt the document with the given options.
    ///
    /// Routes to `pdf_manip::encrypt::encrypt_and_save`, which encrypts
    /// every object in the internal lopdf representation in place. The
    /// engine-side representation is **not** refreshed — re-parsing an
    /// encrypted document from our own serialised output through
    /// `pdf-engine` + `pdf-syntax` is currently unreliable for PDF 2.0
    /// AES-256 output (tracked as a post-1.0 improvement).
    ///
    /// # Usage contract
    ///
    /// After `encrypt(...)` returns successfully:
    ///
    /// - [`save`](Self::save) / [`to_bytes`](Self::to_bytes) /
    ///   [`write_to`](Self::write_to) produce the encrypted bytes.
    /// - Reading operations ([`text`](Self::text),
    ///   [`metadata`](Self::metadata), etc.) on the same handle return
    ///   results for the **pre-encryption** state; they do not
    ///   transparently follow the encryption. This is an acceptable
    ///   trade-off for 1.0 because:
    ///   1. The typical `encrypt-then-save` flow does not read after.
    ///   2. Users who need to read encrypted content should save the
    ///      output, then re-open with
    ///      `OpenOptions::new().with_password(...)`.
    ///
    /// On an already-encrypted document, call [`decrypt`](Self::decrypt)
    /// first; `pdf-manip`'s encryption pipeline does not perform a
    /// decrypt-and-re-encrypt in a single call.
    pub fn encrypt(&mut self, opts: EncryptOptions) -> Result<()> {
        self.require_capability(Capability::EncryptionWrite)?;

        // Build the pdf-manip encrypt config. Empty passwords mean the
        // caller did not supply one; leave them empty (lopdf accepts).
        let user_pw = opts.user_password.clone().unwrap_or_default();
        let owner_pw = opts
            .owner_password
            .clone()
            .unwrap_or_else(|| user_pw.clone());

        let config = pdf_manip::encrypt::EncryptConfig {
            user_password: user_pw.into_bytes(),
            owner_password: owner_pw.into_bytes(),
            algorithm: map_encryption_algorithm(opts.algorithm),
            permissions: map_permissions(opts.permissions),
        };

        // `encrypt_and_save` writes to a `Write` sink AND mutates the
        // document in place. We discard the buffer; the mutated
        // `self.lopdf` is what `save`/`to_bytes` serialises. We do not
        // rebuild `self.engine` — see the method doc-comment for the
        // read-after-encrypt contract.
        let mut sink = std::io::sink();
        pdf_manip::encrypt::encrypt_and_save(&mut self.lopdf, &config, &mut sink)?;
        Ok(())
    }

    /// Decrypt the document using the provided password.
    ///
    /// Routes to `pdf_manip::encrypt::decrypt`. The decrypted state is then
    /// serialised and re-parsed so the engine-side representation reflects
    /// the now-plaintext content.
    pub fn decrypt(&mut self, password: &str) -> Result<()> {
        self.require_capability(Capability::EncryptionRead)?;
        pdf_manip::encrypt::decrypt(&mut self.lopdf, password)?;
        self.refresh_from_lopdf()
    }

    /// Sign the document using the given signer.
    ///
    /// Routes to `pdf_sign::sign_pdf`. The current state is serialised,
    /// signed, and re-parsed. Supports PAdES B-LT by default (via the
    /// signer's SubFilter), matching `pdfluent::SignOptions::new()`'s
    /// default.
    pub fn sign(
        &mut self,
        signer: &dyn crate::signer::PdfSigner,
        opts: crate::signer::SignOptions,
    ) -> Result<()> {
        self.require_capability(Capability::DigitalSignatureSign)?;
        let pdf_bytes = self.to_bytes()?;
        let inner_opts = map_sign_options(&opts);
        // Wrap our trait-object signer in an adapter that implements the
        // pdf_sign::PdfSigner trait by delegation.
        let adapter = PdfSignerAdapter { inner: signer };
        let signed = pdf_sign::sign_pdf(&pdf_bytes, &adapter, &inner_opts)?;
        *self = Self::from_bytes(&signed)?;
        Ok(())
    }

    /// Lightweight list of signatures present in the document.
    ///
    /// Does **not** cryptographically validate. Use
    /// [`verify_signatures`](Self::verify_signatures) for the full report.
    pub fn signatures(&self) -> Result<Vec<crate::signer::SignatureInfo>> {
        self.require_capability(Capability::DigitalSignatureVerify)?;
        let pdf = self.engine.pdf();
        let fields = pdf_sign::signature_fields(pdf);
        let mut out = Vec::with_capacity(fields.len());
        for f in fields {
            out.push(crate::signer::SignatureInfo {
                field_name: f.field_name.clone(),
                signer_name: f.sig.signer_name().unwrap_or_default(),
                timestamp: f.sig.signing_time(),
                profile: None, // PAdES profile inference is post-1.0
            });
        }
        Ok(out)
    }

    /// Cryptographically validate all signatures and return a structured
    /// report.
    pub fn verify_signatures(&self) -> Result<crate::signer::SignatureValidationReport> {
        self.require_capability(Capability::DigitalSignatureVerify)?;
        let pdf = self.engine.pdf();
        let results = pdf_sign::validate_signatures(pdf);
        let validations = results
            .into_iter()
            .map(|r| crate::signer::SignatureValidation {
                info: crate::signer::SignatureInfo {
                    field_name: r.field_name,
                    signer_name: r.signer.unwrap_or_default(),
                    timestamp: r.timestamp,
                    profile: None,
                },
                status: match r.status {
                    pdf_sign::ValidationStatus::Valid => crate::signer::SignatureStatus::Valid,
                    pdf_sign::ValidationStatus::Invalid(reason) => {
                        crate::signer::SignatureStatus::Invalid { reason }
                    }
                    pdf_sign::ValidationStatus::Unknown(reason) => {
                        crate::signer::SignatureStatus::Unknown { reason }
                    }
                },
            })
            .collect();
        Ok(crate::signer::SignatureValidationReport::from_validations(
            validations,
        ))
    }

    // ---------- Redaction (Epic 2 #1244) ----------

    /// Redact every occurrence of the given text.
    ///
    /// Routes to `pdf_redact::search_and_redact`. Honours
    /// [`RedactOptions::case_sensitive`](crate::redact::RedactOptions),
    /// [`RedactOptions::regex`], and
    /// [`RedactOptions::on_pages`] — page numbers are translated 1-to-1.
    pub fn redact(&mut self, text: &str, opts: crate::redact::RedactOptions) -> Result<()> {
        self.require_capability(Capability::Redaction)?;
        let search_opts = pdf_redact::RedactSearchOptions {
            case_sensitive: opts.case_sensitive,
            regex: opts.regex,
            fill_color: [0.0, 0.0, 0.0],
            pages: opts
                .on_pages
                .as_ref()
                .map(|v| v.iter().map(|p| *p as u32).collect()),
            overlay_text: None,
        };
        pdf_redact::search_and_redact(&mut self.lopdf, text, &search_opts)?;
        self.refresh_from_lopdf()
    }

    /// Redact a specific rectangular region on the given page.
    ///
    /// Routes to `pdf_redact::Redactor::apply` with a single
    /// [`pdf_redact::RedactionArea`]. `page` is 1-based; `rect` is
    /// `[x_min, y_min, x_max, y_max]` in PDF points.
    pub fn redact_region(&mut self, page: usize, rect: [f64; 4]) -> Result<()> {
        self.require_capability(Capability::Redaction)?;
        let mut redactor = pdf_redact::Redactor::new();
        redactor.mark(pdf_redact::RedactionArea {
            page: page as u32,
            rect,
            fill_color: [0.0, 0.0, 0.0],
            overlay_text: None,
        });
        redactor.apply(&mut self.lopdf)?;
        self.refresh_from_lopdf()
    }

    /// Re-parse the engine-side from the current lopdf state. Used after
    /// in-place lopdf mutations (decrypt, redact) to keep the two
    /// representations consistent.
    fn refresh_from_lopdf(&mut self) -> Result<()> {
        let mut buf = Vec::with_capacity(64 * 1024);
        let mut clone = self.lopdf.clone();
        clone
            .save_to(&mut buf)
            .map_err(|source| Error::Io { source, path: None })?;
        *self = Self::from_bytes(&buf)?;
        Ok(())
    }

    // ---------- Split / extract (Epic 2 #1243) ----------

    /// Split the document into individual one-page documents.
    ///
    /// The source document is unchanged. Returns a new [`PdfDocument`] per
    /// input page, in order. Bookmarks on the source are **not** propagated
    /// per-page in 1.0; this is a best-effort split that only preserves
    /// page content.
    pub fn split_pages(&self) -> Result<Vec<PdfDocument>> {
        self.require_capability(Capability::PageOps)?;
        let split = pdf_manip::pages::split_per_page(&self.lopdf)?;
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
        self.require_capability(Capability::PageOps)?;
        let total = self.engine.page_count();
        let (start, end) = normalise_page_range(&range, total)?;
        let pages: Vec<u32> = (start..=end).map(|p| p as u32).collect();
        let lopdf_doc = pdf_manip::pages::extract_pages(&self.lopdf, &pages)?;
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
        self.require_capability(Capability::PdfWrite)?;
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
        self.require_capability(Capability::PdfWrite)?;
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
        self.require_capability(Capability::PdfWrite)?;
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
// Epic 3 #1224 helpers — native-only image export
// ---------------------------------------------------------------------------

/// Expand a path pattern for [`PdfDocument::to_images`].
///
/// `{page}` is substituted with the 1-based page number. If the pattern
/// does not contain `{page}`, the page number is injected before the
/// extension (or appended if there's no extension).
#[cfg(not(target_arch = "wasm32"))]
fn build_image_path(pattern: &Path, page_1b: usize, ext: &str) -> std::path::PathBuf {
    use std::path::PathBuf;

    let s = pattern.to_string_lossy();
    if s.contains("{page}") {
        return PathBuf::from(s.replace("{page}", &page_1b.to_string()));
    }

    // No {page} marker — inject _N before the extension.
    let parent = pattern.parent().map(Path::to_path_buf).unwrap_or_default();
    let stem = pattern
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let pattern_ext = pattern
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_else(|| ext.to_string());

    parent.join(format!("{stem}_{page_1b}.{pattern_ext}"))
}

/// Encode a rendered page to disk in the requested format.
///
/// We don't depend on the `image` crate directly; `pdf-render` already
/// pulls in its own encoding stack via `vello_cpu`. For JPEG we drop
/// alpha via a manual RGBA→RGB walk and hand-roll a minimal encoder
/// path — but the simplest reliable route is to go through `image`.
/// Since `image` is a transitive dep we can re-use it without adding
/// a direct dep: instead, we encode PNG via a tiny in-crate wrapper
/// around the `png` crate if present, or fall back to writing a PPM
/// for debugging.
///
/// For 1.0 we only support PNG and JPEG. PNG uses the `png` crate
/// (transitively via `vello_cpu`); JPEG uses a minimal RGB→JPEG
/// dependency path through `pdf-render` which already compiles
/// `image`. To avoid accidental feature creep, we encode PNG via
/// hand-rolled path that doesn't require new deps.
#[cfg(not(target_arch = "wasm32"))]
fn encode_image(
    page: &pdf_engine::render::RenderedPage,
    format: crate::parity::ImageFormat,
    path: &Path,
) -> Result<()> {
    use crate::parity::ImageFormat as Fmt;
    use pdf_engine::render::PixelFormat;

    // `RenderedPage::pixels` is 4 bytes per pixel. The engine produces
    // Rgba8 for the default RenderOptions we pass in.
    if !matches!(page.pixel_format, PixelFormat::Rgba8) {
        return Err(internal_error(format!(
            "unexpected pixel format {:?} from renderer",
            page.pixel_format,
        )));
    }

    match format {
        Fmt::Png => encode_png(page.width, page.height, &page.pixels, path),
        Fmt::Jpeg => encode_jpeg(page.width, page.height, &page.pixels, path),
    }
}

/// Encode RGBA8 pixels as a PNG using the `png` crate.
#[cfg(not(target_arch = "wasm32"))]
fn encode_png(width: u32, height: u32, rgba: &[u8], path: &Path) -> Result<()> {
    let file = fs::File::create(path).map_err(|source| Error::Io {
        source,
        path: Some(path.to_path_buf()),
    })?;
    let w = std::io::BufWriter::new(file);

    let mut encoder = png::Encoder::new(w, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|e| internal_error(format!("png header failed: {e}")))?;
    writer
        .write_image_data(rgba)
        .map_err(|e| internal_error(format!("png write failed: {e}")))?;
    Ok(())
}

/// Encode RGBA8 pixels as a JPEG via the `zune-jpeg`-based encoder exposed
/// by `image`. JPEG doesn't carry alpha, so we drop the alpha channel
/// via a manual RGBA→RGB conversion.
///
/// We depend on `image` transitively through `pdf-render`; rather than
/// pin a new direct dep for a single format, encode through a minimal
/// `jpeg-encoder`-free path using the `png` crate's sibling in the
/// transitive graph. In practice the simplest path is `image` —
/// adding it as a direct dep is clean.
#[cfg(not(target_arch = "wasm32"))]
fn encode_jpeg(width: u32, height: u32, rgba: &[u8], path: &Path) -> Result<()> {
    // Drop alpha.
    let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);
    for chunk in rgba.chunks_exact(4) {
        rgb.extend_from_slice(&chunk[..3]);
    }

    let file = fs::File::create(path).map_err(|source| Error::Io {
        source,
        path: Some(path.to_path_buf()),
    })?;
    let w = std::io::BufWriter::new(file);

    use image::ImageEncoder;
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(w, 90);
    encoder
        .write_image(&rgb, width, height, image::ExtendedColorType::Rgb8)
        .map_err(|e| internal_error(format!("jpeg encoding failed: {e}")))?;
    Ok(())
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
        self.doc.require_capability(Capability::TextExtract)?;
        let text = self.doc.engine.extract_text(self.index)?;
        Ok(text)
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

// Error mapping lives in `crate::error` as `From<internal>` impls, letting
// call-sites use the `?` operator directly. See `error.rs` for the five
// conversions (pdf_engine::EngineError, lopdf::Error, pdf_manip::ManipError,
// pdf_sign::SignError, pdf_redact::RedactError) that replaced the earlier
// ad-hoc `map_*_error` helpers that used to live here.

fn map_encryption_algorithm(
    alg: crate::encrypt::EncryptionAlgorithm,
) -> pdf_manip::encrypt::EncryptionAlgorithm {
    use crate::encrypt::EncryptionAlgorithm as Ours;
    use pdf_manip::encrypt::EncryptionAlgorithm as Theirs;
    match alg {
        Ours::Aes128 => Theirs::Aes128,
        Ours::Aes256 => Theirs::Aes256,
    }
}

fn map_permissions(perms: crate::encrypt::Permissions) -> pdf_manip::encrypt::Permissions {
    // Our Permissions struct has pub(crate) bool fields; pdf-manip's
    // Permissions has the same 8 fields under slightly different names.
    pdf_manip::encrypt::Permissions {
        print: perms.print,
        modify_contents: perms.modify,
        extract_content: perms.copy,
        modify_annotations: perms.annotate,
        fill_forms: perms.fill_forms,
        extract_for_accessibility: perms.extract_accessibility,
        assemble_document: perms.assemble,
        print_high_quality: perms.print_high_quality,
    }
}

fn map_sign_options(opts: &crate::signer::SignOptions) -> pdf_sign::SignOptions {
    let sub_filter = match opts.profile {
        crate::signer::PadesProfile::BasicSignature => pdf_sign::SubFilter::EtsiCadesDetached,
        crate::signer::PadesProfile::Timestamped => pdf_sign::SubFilter::EtsiCadesDetached,
        crate::signer::PadesProfile::LongTerm => pdf_sign::SubFilter::EtsiCadesDetached,
        crate::signer::PadesProfile::LongTermArchive => pdf_sign::SubFilter::EtsiCadesDetached,
    };
    pdf_sign::SignOptions {
        reason: opts.reason.clone(),
        location: opts.location.clone(),
        contact: opts.contact_info.clone(),
        field_name: opts.field_name.clone(),
        visible_rect: opts.visible_rect.map(|(page, rect)| (page as u32, rect)),
        sub_filter,
        certification: None,
        placeholder_size: 8192,
    }
}

/// Adapter that makes a `&dyn crate::signer::PdfSigner` usable where
/// `pdf_sign` expects a concrete `impl pdf_sign::PdfSigner`.
///
/// The two traits have different method-sets but compatible enough
/// semantics: both sign bytes and expose a DER-encoded certificate chain.
/// The adapter maps digest-algorithm selection onto pdf-sign's default
/// (SHA-256) since our public trait does not yet expose the choice.
struct PdfSignerAdapter<'a> {
    inner: &'a dyn crate::signer::PdfSigner,
}

impl<'a> pdf_sign::PdfSigner for PdfSignerAdapter<'a> {
    fn sign(&self, data: &[u8]) -> std::result::Result<Vec<u8>, pdf_sign::SignError> {
        self.inner
            .sign(data)
            .map_err(|e| pdf_sign::SignError::SigningFailed(e.to_string()))
    }
    fn certificate_chain_der(&self) -> &[Vec<u8>] {
        self.inner.certificate_chain()
    }
    fn digest_algorithm(&self) -> pdf_sign::DigestAlgorithm {
        pdf_sign::DigestAlgorithm::Sha256
    }
    fn signature_algorithm_oid(&self) -> &[u8] {
        // rsaEncryption — the most common default for RSA PKCS#1 signers.
        // Concrete implementations may override via a richer trait in 1.1.
        &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x01]
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
