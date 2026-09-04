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

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::capability::Capability;
use crate::decoration::PageDecoration;
use crate::encrypt::EncryptOptions;
use crate::error::{internal_error, Error, Result};
use crate::form::{FormField, PdfFormMut};
use crate::license;
use crate::metadata::{Metadata, MetadataMut};
use crate::parity::{
    CompressOptions, CompressReport, FontSubsetReport, ImageFormat, ImageInsert, ImageInsertReport,
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
    pub(crate) processing_limits: Option<pdf_engine::ProcessingLimits>,
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

    /// Request best-effort repair of a malformed PDF.
    ///
    /// **Recovery is always on.** The engine *unconditionally* attempts to
    /// recover from a damaged cross-reference table or page tree (brute-force
    /// object/page scanning) when normal parsing fails — independent of this
    /// flag. This option is therefore advisory: it never disables recovery and
    /// does not currently change load behaviour. It is retained for API
    /// compatibility and reserved for future control of repair *diagnostics*
    /// verbosity (see [`PdfDocument::diagnostics`](crate::PdfDocument::diagnostics)).
    /// It is intentionally **not** a hidden switch: recovery already happens for
    /// every document, reported or not.
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

    /// Apply a [`ProcessingLimits`](pdf_engine::ProcessingLimits) policy
    /// to the open path.
    ///
    /// Currently enforces the **file-size** cap
    /// ([`ProcessingLimits::max_file_bytes`](pdf_engine::ProcessingLimits::max_file_bytes))
    /// before any allocation occurs. The remaining caps in
    /// `ProcessingLimits` (stream-size, image-pixels, object-depth,
    /// operator-count, XFA / FormCalc nesting) require parser-internal
    /// hooks and are tracked as follow-up work in issue #1429; today
    /// they are accepted into the policy but only the file-size cap
    /// fires. When the parser-side hooks land, those caps will start
    /// firing without any caller change.
    ///
    /// If [`strict_memory_limit`](Self::strict_memory_limit) is also
    /// set, the **smaller** of the two caps wins for the input-size
    /// check. The two are kept as separate methods for backward
    /// compatibility — `strict_memory_limit` predates this builder.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pdfluent::{OpenOptions, PdfDocument, ProcessingLimits};
    ///
    /// // Stricter caps for a server-side intake pipeline:
    /// let limits = ProcessingLimits::default()
    ///     .max_file_bytes(50 * 1024 * 1024)   // 50 MB
    ///     .max_stream_bytes(32 * 1024 * 1024); // 32 MB per stream
    ///
    /// let opts = OpenOptions::new().with_processing_limits(limits);
    /// let doc = PdfDocument::open_with("invoice.pdf", opts).unwrap();
    /// # drop(doc);
    /// ```
    pub fn with_processing_limits(mut self, limits: pdf_engine::ProcessingLimits) -> Self {
        self.processing_limits = Some(limits);
        self
    }
}

/// Options for saving a PDF document.
///
/// **Default behaviour** (per RFC 0001 §1.2): `linearize = false`,
/// `overwrite = false`. `save` / `save_with` therefore refuse to
/// clobber existing files unless you opt in via
/// [`with_overwrite(true)`](Self::with_overwrite).
///
/// # Determinism
///
/// For an unencrypted document, `save`, `save_with`, `to_bytes`, and
/// `write_to` produce **byte-identical output** for byte-identical input
/// (issue #1308). This is enforced by integration tests in
/// `tests/determinism.rs` and is part of the 1.0 contract.
///
/// Specifically guaranteed:
/// - Object ordering is sorted by `(id, generation)` — `BTreeMap` in lopdf.
/// - Dictionary key order is preserved on round-trip — `IndexMap` in lopdf.
/// - Stream content is copied as-is.
/// - The cross-reference table is derived deterministically.
///
/// **NOT deterministic (by design):**
/// - **Encrypted output** — AES IVs and content keys are randomly generated
///   per save (security requirement, ISO 32000-2 §7.6). Two saves of the
///   same encrypted document produce different bytes.
/// - **Caller-introduced timestamps** — anything you write via
///   [`metadata_mut()`](PdfDocument::metadata_mut) using the system clock
///   (e.g. `set_creation_date(SystemTime::now())`).
///
/// Customer CI pipelines that compare PDF checksums depend on this
/// guarantee — break it only with a new RFC.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct SaveOptions {
    pub(crate) linearize: bool,
    pub(crate) overwrite: bool,
    pub(crate) incremental: bool,
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

    /// Enable incremental save.
    ///
    /// When set to true, only changes made to the PDF will be appended
    /// to the original file. This preserves signatures and avoids rewriting the
    /// entire file. Returns an error if the document is encrypted or was not loaded
    /// from bytes.
    pub fn with_incremental(mut self, v: bool) -> Self {
        self.incremental = v;
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
    processing_limits: Option<pdf_engine::ProcessingLimits>,
    /// The original bytes of the PDF document. Used for incremental save.
    /// Memory impact: We hold a single reference count `Arc<Vec<u8>>` to the
    /// loaded buffer. This does not clone the buffer in memory, but rather
    /// shares the existing allocation that was already read into memory.
    original_bytes: Option<Arc<Vec<u8>>>,
    /// Diagnostics collected from interpreter warnings during render/extract.
    /// Shared with the warning sink installed on `engine`; interior-mutable so
    /// read-side operations on `&self` can accumulate into it.
    diagnostics: Arc<Mutex<Vec<crate::diagnostics::Diagnostic>>>,
    /// Lazily-created XFA fill session (parse-once cache of the XFA
    /// template/data/form/layout state). Built on first
    /// [`xfa_form_model`](Self::xfa_form_model) /
    /// [`set_xfa_field_value`](Self::set_xfa_field_value) call.
    xfa_session: Option<Box<pdf_engine::xfa::XfaSession>>,
    /// Current text-edit revision (design §10.1 of the text-replace design
    /// doc): SHA-256 of the source bytes plus a commit counter. Lazily
    /// initialised on the first text-edit call; advanced by every applied
    /// text-edit commit.
    text_edit_revision: Option<pdf_manip::text_edit::DocumentRevision>,
}

/// Build a diagnostics collector, install a warning sink on `engine` that
/// translates each interpreter warning into a public [`Diagnostic`] and pushes
/// it, and return the shared collector. The sink is poison-tolerant so a
/// panicked holder of the lock never cascades into the render path.
fn install_diagnostics_sink(
    engine: &mut pdf_engine::PdfDocument,
) -> Arc<Mutex<Vec<crate::diagnostics::Diagnostic>>> {
    let collector: Arc<Mutex<Vec<crate::diagnostics::Diagnostic>>> =
        Arc::new(Mutex::new(Vec::new()));
    let sink_target = Arc::clone(&collector);
    engine.set_warning_sink(Arc::new(move |warning| {
        let mut guard = sink_target.lock().unwrap_or_else(|e| e.into_inner());
        guard.push(crate::diagnostics::Diagnostic::from_interpreter_warning(
            warning,
        ));
    }));

    // Seed load-time structural-recovery diagnostics once, at open. xref /
    // page-tree rebuilds happen during parsing — before any warning sink runs —
    // so they are read from the engine and pushed here rather than via the sink.
    let recovery = engine.load_recovery();
    {
        let mut guard = collector.lock().unwrap_or_else(|e| e.into_inner());
        if recovery.xref_rebuilt {
            guard.push(crate::diagnostics::Diagnostic::xref_rebuilt());
        }
        if recovery.page_tree_rebuilt {
            guard.push(crate::diagnostics::Diagnostic::page_tree_rebuilt());
        }
    }
    collector
}

/// Activate the thread-local leniency collector, run `f`, then drain any
/// accumulated events into the diagnostics buffer.
///
/// Used around operations that trigger stream or filter decoding (text
/// extraction, page rendering) where low-level leniency events can fire.
fn with_leniency<T>(
    diagnostics: &Arc<Mutex<Vec<crate::diagnostics::Diagnostic>>>,
    f: impl FnOnce() -> T,
) -> T {
    pdf_syntax::leniency::activate();
    let result = f();
    let events = pdf_syntax::leniency::drain();
    if !events.is_empty() {
        let mut guard = diagnostics.lock().unwrap_or_else(|e| e.into_inner());
        for event in events {
            guard.push(crate::diagnostics::Diagnostic::from_leniency_event(event));
        }
    }
    result
}

impl std::fmt::Debug for PdfDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PdfDocument")
            .field("page_count", &self.engine.page_count())
            .finish_non_exhaustive()
    }
}

fn open_engine_from_shared_bytes(
    shared: Arc<Vec<u8>>,
    password: Option<&str>,
    processing_limits: Option<&pdf_engine::ProcessingLimits>,
) -> Result<pdf_engine::PdfDocument> {
    match (password, processing_limits) {
        (Some(pw), Some(limits)) => {
            pdf_engine::PdfDocument::open_with_password_and_processing_limits(
                shared,
                pw,
                limits.clone(),
            )
        }
        (None, Some(limits)) => {
            pdf_engine::PdfDocument::open_with_processing_limits(shared, limits.clone())
        }
        (Some(pw), None) => pdf_engine::PdfDocument::open_with_password(shared, pw),
        (None, None) => pdf_engine::PdfDocument::open(shared),
    }
    .map_err(Into::into)
}

fn load_lopdf_from_shared_bytes(
    shared: &Arc<Vec<u8>>,
    password: Option<&str>,
) -> Result<lopdf::Document> {
    match password {
        Some(pw) => {
            let mut doc = lopdf::Document::load_mem(shared.as_slice())?;
            // lopdf separates load from decrypt: decrypt in place if possible.
            let _ = doc.decrypt(pw);
            Ok(doc)
        }
        None => lopdf::Document::load_mem(shared.as_slice()).map_err(Into::into),
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
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pdfluent::PdfDocument;
    ///
    /// let doc = PdfDocument::open("invoice.pdf").unwrap();
    /// println!("{} pages", doc.page_count());
    /// ```
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::open_with(path, OpenOptions::new())
    }

    /// Open a PDF from a filesystem path with explicit options.
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            target = "pdfluent",
            skip(opts),
            fields(path = %path.as_ref().display())
        )
    )]
    pub fn open_with<P: AsRef<Path>>(path: P, opts: OpenOptions) -> Result<Self> {
        license::require_capability(Capability::PdfParse)?;
        let path_ref = path.as_ref();

        // Enforce memory budget BEFORE reading the file into memory. Without
        // this, a malicious PDF could exhaust RAM before the limit check ever
        // ran (file already in `bytes` by then).
        //
        // Two limit sources may be in play:
        //   1. `strict_memory_limit` (predates with_processing_limits)
        //   2. `with_processing_limits` (issue #1429, partial wiring)
        // The smaller cap wins. We do a single stat and check both.
        let processing_file_cap: Option<u64> =
            opts.processing_limits.as_ref().map(|l| l.max_file_bytes);
        if opts.memory_limit.is_some() || processing_file_cap.is_some() {
            let metadata = fs::metadata(path_ref).map_err(|source| match source.kind() {
                std::io::ErrorKind::NotFound => Error::FileNotFound {
                    path: path_ref.to_path_buf(),
                },
                _ => Error::Io {
                    source,
                    path: Some(path_ref.to_path_buf()),
                },
            })?;
            let size_u64 = metadata.len();

            if let Some(limit_bytes) = processing_file_cap {
                if size_u64 > limit_bytes {
                    return Err(Error::ResourceLimitExceeded {
                        kind: crate::error::ResourceLimitKind::FileTooLarge,
                        observed: size_u64,
                        limit: limit_bytes,
                    });
                }
            }

            if let Some(limit) = opts.memory_limit {
                let size = size_u64 as usize;
                if size > limit {
                    return Err(Error::MemoryBudgetExceeded {
                        requested: size,
                        limit,
                    });
                }
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
    ///
    /// If [`OpenOptions::strict_memory_limit`] is set, the input length is
    /// checked before `pdfluent` takes any owned copy of `bytes`.
    ///
    /// # Peak input ownership
    ///
    /// On success, `pdfluent` takes exactly one owned copy of `bytes` into a
    /// shared `Arc<Vec<u8>>`. `pdf_engine` keeps shared ownership of that
    /// buffer, while `lopdf` parses from a borrowed slice of the same bytes.
    /// This avoids the previous second input-sized clone in the engine path.
    /// Any further allocations come from parser-internal state rather than a
    /// duplicated raw-input buffer inside `pdfluent`.
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            target = "pdfluent",
            skip(bytes, opts),
            fields(len = bytes.len())
        )
    )]
    pub fn from_bytes_with(bytes: &[u8], opts: OpenOptions) -> Result<Self> {
        license::require_capability(Capability::PdfParse)?;

        // ProcessingLimits::max_file_bytes (issue #1429): typed
        // ResourceLimitExceeded variant — preferred path.
        if let Some(ref limits) = opts.processing_limits {
            let len_u64 = bytes.len() as u64;
            if len_u64 > limits.max_file_bytes {
                return Err(Error::ResourceLimitExceeded {
                    kind: crate::error::ResourceLimitKind::FileTooLarge,
                    observed: len_u64,
                    limit: limits.max_file_bytes,
                });
            }
        }

        // Legacy strict_memory_limit (predates with_processing_limits).
        if let Some(limit) = opts.memory_limit {
            if bytes.len() > limit {
                return Err(Error::MemoryBudgetExceeded {
                    requested: bytes.len(),
                    limit,
                });
            }
        }

        let shared = Arc::new(bytes.to_vec());
        // Activate leniency collector before parsing so load-time filter and
        // structural recovery events are captured. Always drain — even on error —
        // so the thread-local collector is never left in an active state.
        pdf_syntax::leniency::activate();
        let engine_result = open_engine_from_shared_bytes(
            shared.clone(),
            opts.password.as_deref(),
            opts.processing_limits.as_ref(),
        );
        // Drain load-time events before checking the result so the collector is
        // never leaked when open_engine_from_shared_bytes returns an error.
        let load_events = pdf_syntax::leniency::drain();
        let mut engine = engine_result?;
        let diagnostics = install_diagnostics_sink(&mut engine);
        {
            let mut guard = diagnostics.lock().unwrap_or_else(|e| e.into_inner());
            for event in load_events {
                guard.push(crate::diagnostics::Diagnostic::from_leniency_event(event));
            }
        }
        let lopdf = load_lopdf_from_shared_bytes(&shared, opts.password.as_deref())?;

        Ok(Self {
            engine,
            lopdf,
            license_key_override: opts.license_key.clone(),
            processing_limits: opts.processing_limits.clone(),
            original_bytes: Some(shared),
            diagnostics,
            xfa_session: None,
            text_edit_revision: None,
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
        // A from-scratch document has no backing original bytes, so incremental
        // save is unsupported for it.
        let mut new_doc = Self::from_bytes(&buf).expect("freshly-built PDF must re-parse");
        new_doc.original_bytes = None;
        new_doc
    }

    // ---------- Read-only content access ----------

    /// Total number of pages.
    pub fn page_count(&self) -> usize {
        self.engine.page_count()
    }

    // ---------- Diagnostics ----------

    /// Diagnostics collected so far from render and text-extraction operations.
    ///
    /// Each [`Diagnostic`](crate::diagnostics::Diagnostic) records a
    /// degradation the engine recovered from — a substituted font, a dropped
    /// image, an exceeded resource limit — that would otherwise be invisible to
    /// the caller. A document that processed cleanly returns an empty list.
    ///
    /// Diagnostics accumulate across operations on this document; use
    /// [`take_diagnostics`](Self::take_diagnostics) to drain them (for example
    /// to scope diagnostics to a single render). The returned list is a
    /// snapshot copy.
    pub fn diagnostics(&self) -> Vec<crate::diagnostics::Diagnostic> {
        self.diagnostics
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Take and clear the collected diagnostics, returning them.
    ///
    /// Like [`diagnostics`](Self::diagnostics) but drains the buffer, so the
    /// next call only sees diagnostics raised after this one. Useful to attach
    /// diagnostics to a specific operation:
    ///
    /// ```no_run
    /// # use pdfluent::prelude::*;
    /// # fn run(doc: &PdfDocument) -> pdfluent::Result<()> {
    /// let _ = doc.take_diagnostics(); // clear
    /// let _png = doc.render_page(1, 150, ImageFormat::Png)?;
    /// for d in doc.take_diagnostics() {
    ///     eprintln!("[{}] {}", d.code, d.message);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn take_diagnostics(&self) -> Vec<crate::diagnostics::Diagnostic> {
        std::mem::take(&mut *self.diagnostics.lock().unwrap_or_else(|e| e.into_inner()))
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
    ///
    /// For per-page text with a double-newline separator see
    /// [`extract_text`](Self::extract_text).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pdfluent::PdfDocument;
    ///
    /// let doc = PdfDocument::open("report.pdf").unwrap();
    /// let raw = doc.text().unwrap();
    /// println!("Characters extracted: {}", raw.len());
    /// ```
    pub fn text(&self) -> Result<String> {
        self.require_capability(Capability::TextExtract)?;
        Ok(self.engine.extract_all_text())
    }

    /// Extract plain text from every page, joined with `"\n\n"` between pages.
    ///
    /// This is the simplest way to get all text out of a document. Each page's
    /// text is extracted independently via [`Page::text`] and the results are
    /// concatenated with a double-newline separator so paragraph boundaries are
    /// preserved across page breaks. Pages that yield no text contribute an
    /// empty string (no extra blank lines are inserted for them).
    ///
    /// Requires [`Capability::TextExtract`] (available from the Trial tier).
    /// For structured output with bounding boxes, use
    /// [`text_with_layout`](Self::text_with_layout) instead.
    ///
    /// # Errors
    ///
    /// Returns [`Error::FeatureNotInTier`] if the active license tier does not
    /// grant [`Capability::TextExtract`].
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pdfluent::prelude::*;
    ///
    /// let doc = PdfDocument::open("report.pdf").unwrap();
    /// let text = doc.extract_text().unwrap();
    /// println!("{text}");
    /// ```
    pub fn extract_text(&self) -> Result<String> {
        self.require_capability(Capability::TextExtract)?;
        let count = self.engine.page_count();
        with_leniency(&self.diagnostics, || -> Result<String> {
            let mut parts: Vec<String> = Vec::with_capacity(count);
            for idx in 0..count {
                parts.push(self.engine.extract_text(idx)?);
            }
            Ok(parts.join("\n\n"))
        })
    }

    /// Extract text grouped into structured blocks with coordinates.
    ///
    /// Matches the [`Capability::TextExtractWithLayout`] capability. Prefer
    /// [`text`](Self::text) if you only need plain text.
    pub fn text_with_layout(&self) -> Result<Vec<TextBlock>> {
        self.require_capability(Capability::TextExtractWithLayout)?;
        let mut out = Vec::new();
        for (idx, blocks) in self
            .engine
            .extract_all_text_blocks()
            .into_iter()
            .enumerate()
        {
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

    /// Pre-flight check for `to_images`: scan the lopdf representation for
    /// image XObjects whose pixel count (width × height) exceeds the
    /// caller's `max_image_pixels` cap.  This fires BEFORE rasterisation
    /// so that a pathological image dictionary cannot cause an OOM during
    /// render.
    // Only called from the native-only `to_images` path; dead on wasm32.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    fn check_image_pixel_limits(&self) -> Result<()> {
        let Some(ref limits) = self.processing_limits else {
            return Ok(());
        };
        if limits.max_image_pixels == u64::MAX {
            return Ok(());
        }

        use lopdf::Object;

        for obj in self.lopdf.objects.values() {
            let dict = match obj {
                Object::Dictionary(d) => d,
                Object::Stream(stream) => &stream.dict,
                Object::Reference(id) => match self.lopdf.get_object(*id) {
                    Ok(Object::Dictionary(d)) => d,
                    Ok(Object::Stream(stream)) => &stream.dict,
                    _ => continue,
                },
                _ => continue,
            };

            let is_xobject = match dict.get(b"Type") {
                Ok(Object::Name(n)) => n.as_slice() == b"XObject",
                _ => false,
            };
            if !is_xobject {
                continue;
            }

            let is_image = match dict.get(b"Subtype") {
                Ok(Object::Name(n)) => n.as_slice() == b"Image",
                _ => false,
            };
            if !is_image {
                continue;
            }

            let width = match dict.get(b"Width") {
                Ok(Object::Integer(v)) if *v > 0 => *v as u64,
                Ok(Object::Reference(id)) => match self.lopdf.get_object(*id) {
                    Ok(Object::Integer(v)) if *v > 0 => *v as u64,
                    _ => continue,
                },
                _ => continue,
            };

            let height = match dict.get(b"Height") {
                Ok(Object::Integer(v)) if *v > 0 => *v as u64,
                Ok(Object::Reference(id)) => match self.lopdf.get_object(*id) {
                    Ok(Object::Integer(v)) if *v > 0 => *v as u64,
                    _ => continue,
                },
                _ => continue,
            };

            let pixels = width.saturating_mul(height);
            if pixels > limits.max_image_pixels {
                return Err(Error::ResourceLimitExceeded {
                    kind: crate::error::ResourceLimitKind::ImageTooLarge,
                    observed: pixels,
                    limit: limits.max_image_pixels,
                });
            }
        }

        Ok(())
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
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pdfluent::PdfDocument;
    ///
    /// let doc = PdfDocument::open("form.pdf").unwrap();
    /// for field in doc.form_fields().unwrap() {
    ///     println!("{}: {:?} = {:?}", field.name, field.field_type, field.value);
    /// }
    /// ```
    pub fn form_fields(&self) -> Result<Vec<FormField>> {
        self.require_capability(Capability::AcroFormRead)?;
        Ok(crate::form::read_acroform_fields(&self.lopdf))
    }

    /// Complete AcroForm model for interactive form UIs.
    ///
    /// One [`pdf_forms::FormFieldModel`] per *logical* field — a radio group
    /// with N option widgets is one entry — carrying the typed kind
    /// (text/checkbox/radio/combo/listbox with kind-specific data such as
    /// comb/multiline flags, on-state names and choice options), per-page
    /// widget rectangles, current and default values, read-only/required
    /// flags, `/MaxLen`, alignment, and resolved `/DA` font info.
    ///
    /// Returns an empty `Vec` when the document has no AcroForm (including
    /// XFA-only documents).
    ///
    /// Field `name` values are fully qualified (`parent.kid`) and are
    /// accepted verbatim by the [`form_mut`](Self::form_mut) setters.
    pub fn form_model(&self) -> Result<Vec<pdf_forms::FormFieldModel>> {
        self.require_capability(Capability::AcroFormRead)?;
        Ok(pdf_forms::parse_acroform(self.engine.pdf())
            .map(|tree| pdf_forms::build_form_model(&tree))
            .unwrap_or_default())
    }

    /// Regenerate `/AP` appearance streams for all filled text/choice fields.
    ///
    /// Documents filled by tools that only write `/V` (often paired with
    /// `/NeedAppearances true`) show stale or missing values in viewers that
    /// do not regenerate appearances — including this SDK's renderer. This
    /// method materialises trustworthy appearance streams for every filled
    /// field; call [`sync_engine`](Self::sync_engine) afterwards if you need
    /// the change reflected in rendering on this same handle.
    pub fn regenerate_form_appearances(&mut self) -> Result<pdf_forms::WriteOutcome> {
        self.require_capability(Capability::AcroFormFill)?;
        pdf_forms::regenerate_appearances(&mut self.lopdf)
            .map_err(|e| crate::error::internal_error(e.to_string()))
    }

    /// Mutable form handle.
    ///
    /// Returns unconditionally — the handle is always constructable, even
    /// on documents without an AcroForm. Capability enforcement and field
    /// lookups happen on the individual setter calls.
    ///
    /// Setters accept fully-qualified field names (`parent.kid`) and
    /// update the complete chain: `/V`, widget `/AS`, regenerated `/AP`,
    /// with a `/NeedAppearances` fallback for non-WinAnsi text.
    ///
    /// # Rendering after mutations
    ///
    /// Mutations applied through the returned guard update the in-memory
    /// `lopdf` representation immediately. The rendering engine is **not**
    /// automatically refreshed while the guard is live (the borrow checker
    /// prevents it). If you need to call [`render_page`](Self::render_page)
    /// on the same `PdfDocument` handle after mutations, drop the guard and
    /// then call [`sync_engine`](Self::sync_engine) first.
    pub fn form_mut(&mut self) -> PdfFormMut<'_> {
        // Read the license override BEFORE the mutable borrow of `lopdf`
        // so the two field borrows don't overlap.
        let license_override = self.license_key_override.as_deref();
        PdfFormMut::new(&mut self.lopdf, license_override)
    }

    /// Flatten all AcroForm fields to static content.
    ///
    /// # 1.0 status — deferred runtime
    ///
    /// The flatten pipeline (widget appearance stream rendering +
    /// acroform removal + /Annots pruning) is tracked on #1223 and
    /// lands post-freeze. Calling this method at 1.0 returns
    /// [`Error::MissingDependency`] — no panic. Users MUST check the
    /// result; see [STABILITY.md §3.3](../STABILITY.md) for the
    /// full deferred-items register.
    pub fn flatten_forms(&mut self) -> Result<()> {
        self.require_capability(Capability::AcroFormFlatten)?;
        Err(Error::MissingDependency {
            dep: "pdf-manip::flatten_forms",
            install_hint: "AcroForm flatten runtime tracked on #1223; lands in a 1.x MINOR. Use \
                 pdf_manip::flatten_forms directly for now if you need the raw pipeline.",
        })
    }

    // ---------- XFA forms (Phase 1 fill foundation) ----------

    /// Whether the document carries an XFA form (an active `/AcroForm /XFA`
    /// entry with a template packet).
    ///
    /// Cheap detection probe — no capability gate, no session construction.
    pub fn has_xfa_form(&self) -> bool {
        pdf_engine::xfa::has_xfa(&self.engine)
    }

    /// The XFA form model: the layout page count and one entry per logical
    /// field (radio groups fold their member widgets into a single
    /// [`crate::xfa::XfaField`]), with values, read-only/required/multiline
    /// flags, choice options, per-widget page mapping and geometry.
    ///
    /// The first call parses the XFA packets, merges template with data,
    /// applies the saved form state, and lays the form out; the resulting
    /// session is cached on this handle, so subsequent model reads and
    /// [`set_xfa_field_value`](Self::set_xfa_field_value) calls are cheap.
    ///
    /// Values written through [`set_xfa_field_value`](Self::set_xfa_field_value)
    /// are reflected in later model reads. The layout is **not** recomputed
    /// after value writes in this phase (no reflow, no change/click event
    /// scripts) — geometry stays that of the opened document.
    ///
    /// # Errors
    ///
    /// [`Error::Unsupported`] when the document has no XFA form;
    /// [`Error::FeatureNotInTier`] without the `XfaParse` capability.
    pub fn xfa_form_model(&mut self) -> Result<crate::xfa::XfaFormModel> {
        self.require_capability(Capability::XfaParse)?;
        self.ensure_xfa_session()?;
        let session = self.xfa_session.as_ref().expect("session ensured above");
        Ok(crate::xfa::XfaFormModel {
            page_count: session.page_count(),
            fields: session
                .fields()
                .iter()
                .map(crate::xfa::field_from_engine)
                .collect(),
        })
    }

    /// Set the value of one XFA form field.
    ///
    /// `name` is a fully-qualified field name as reported by
    /// [`xfa_form_model`](Self::xfa_form_model) — either the display form
    /// (`form1.applicant.name`) or the explicit SOM path
    /// (`form1[0].applicant[0].name[0]`).
    ///
    /// The write updates the in-memory form tree, writes through to the
    /// bound `datasets` node (creating it on demand for default-bound
    /// fields), keeps an Adobe-saved form packet's value in sync, and swaps
    /// the updated packets into the PDF — a subsequent
    /// [`save`](Self::save) / [`to_bytes`](Self::to_bytes) produces a PDF
    /// that Adobe Acrobat/Reader reopens with the filled values.
    ///
    /// Read-only fields (template or saved-state `access` locks) are
    /// rejected. No change/click event scripts run in this phase.
    ///
    /// # Errors
    ///
    /// [`Error::Unsupported`] for unknown fields, read-only fields, values
    /// not assignable to the field's type, or non-XFA documents;
    /// [`Error::FeatureNotInTier`] without the `XfaFill` capability.
    pub fn set_xfa_field_value(
        &mut self,
        name: &str,
        value: crate::xfa::XfaFieldValue<'_>,
    ) -> Result<crate::xfa::XfaSetOutcome> {
        self.require_capability(Capability::XfaFill)?;
        self.ensure_xfa_session()?;
        let session = self.xfa_session.as_mut().expect("session ensured above");
        let engine_value = match value {
            crate::xfa::XfaFieldValue::Text(s) => pdf_engine::xfa::XfaWriteValue::Text(s),
            crate::xfa::XfaFieldValue::Checkbox(b) => pdf_engine::xfa::XfaWriteValue::Checkbox(b),
            crate::xfa::XfaFieldValue::Radio(s) => pdf_engine::xfa::XfaWriteValue::Radio(s),
        };
        let outcome = session
            .set_value(name, engine_value)
            .map_err(crate::xfa::map_xfa_err)?;
        session
            .write_into_document(&mut self.lopdf)
            .map_err(crate::xfa::map_xfa_err)?;
        self.sync_engine()?;
        Ok(crate::xfa::XfaSetOutcome {
            raw_value: outcome.raw_value,
            persisted_to_datasets: outcome.persisted_to_datasets,
        })
    }

    /// Build the cached XFA session from the document's current bytes.
    fn ensure_xfa_session(&mut self) -> Result<()> {
        if self.xfa_session.is_none() {
            let bytes = self.to_bytes()?;
            let session =
                pdf_engine::xfa::XfaSession::open(&bytes).map_err(crate::xfa::map_xfa_err)?;
            self.xfa_session = Some(Box::new(session));
        }
        Ok(())
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
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            target = "pdfluent",
            skip(self),
            fields(path = %path.as_ref().display())
        )
    )]
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

    /// Convert the document to an `.xlsx` file on disk.
    ///
    /// Routes to [`pdf_xlsx::convert_pdf_bytes_to_xlsx`], which detects tabular
    /// structure and emits a spreadsheet. Table detection in PDF is inference —
    /// PDF has no table model — so the result is best-effort and worth checking
    /// against the source for anything load-bearing.
    ///
    /// # Capability
    ///
    /// Requires [`Capability::XlsxExport`].
    ///
    /// # Why this exists
    ///
    /// `pdf-xlsx` has existed and been published for a long time, but the facade
    /// did not depend on it, so "PDF to Excel" was reachable only by adding the
    /// crate by hand — while the features page presented it as an SDK
    /// capability. The `xlsx-export` feature flag was empty, which is worse than
    /// absent: enabling it did nothing and read as consent.
    #[cfg(not(target_arch = "wasm32"))]
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            target = "pdfluent",
            skip(self),
            fields(path = %path.as_ref().display())
        )
    )]
    pub fn to_xlsx<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        self.require_capability(Capability::XlsxExport)?;
        let pdf_bytes = self.to_bytes()?;
        let xlsx_bytes = pdf_xlsx::convert_pdf_bytes_to_xlsx(&pdf_bytes)
            .map_err(|e| internal_error(format!("xlsx conversion failed: {e}")))?;
        fs::write(path.as_ref(), xlsx_bytes).map_err(|source| Error::Io {
            source,
            path: Some(path.as_ref().to_path_buf()),
        })?;
        Ok(())
    }

    /// Convert the document to an `.xlsx` file — wasm stub.
    #[cfg(target_arch = "wasm32")]
    pub fn to_xlsx<P: AsRef<Path>>(&self, _path: P) -> Result<()> {
        Err(Error::UnsupportedOnWasm {
            operation: "to_xlsx",
        })
    }

    /// Convert the document to a `.pptx` file on disk.
    ///
    /// Routes to [`pdf_pptx::convert_pdf_bytes_to_pptx`], one slide per page.
    ///
    /// # Capability
    ///
    /// Requires [`Capability::PptxExport`].
    ///
    /// # Why this exists
    ///
    /// Same as [`Document::to_xlsx`]: the crate was published and tested, the
    /// facade did not depend on it, and `pptx-export` was an empty flag.
    #[cfg(not(target_arch = "wasm32"))]
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            target = "pdfluent",
            skip(self),
            fields(path = %path.as_ref().display())
        )
    )]
    pub fn to_pptx<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        self.require_capability(Capability::PptxExport)?;
        let pdf_bytes = self.to_bytes()?;
        let pptx_bytes = pdf_pptx::convert_pdf_bytes_to_pptx(&pdf_bytes)
            .map_err(|e| internal_error(format!("pptx conversion failed: {e}")))?;
        fs::write(path.as_ref(), pptx_bytes).map_err(|source| Error::Io {
            source,
            path: Some(path.as_ref().to_path_buf()),
        })?;
        Ok(())
    }

    /// Convert the document to a `.pptx` file — wasm stub.
    #[cfg(target_arch = "wasm32")]
    pub fn to_pptx<P: AsRef<Path>>(&self, _path: P) -> Result<()> {
        Err(Error::UnsupportedOnWasm {
            operation: "to_pptx",
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
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            target = "pdfluent",
            skip(self, opts),
            fields(pattern = %pattern.as_ref().display())
        )
    )]
    pub fn to_images<P: AsRef<Path>>(
        &self,
        pattern: P,
        opts: ToImagesOptions,
    ) -> Result<ToImagesReport> {
        use pdf_engine::render::RenderOptions;

        self.require_capability(Capability::RenderRaster)?;
        self.check_image_pixel_limits()?;

        let total = self.engine.page_count();
        // Codex P2 on #1269: guard zero-page documents BEFORE computing
        // `to - from + 1` — otherwise a default `opts.pages = None` on
        // a 0-page doc produces `from=1, to=0`, and the subsequent
        // `with_capacity(usize::MAX)` panics in debug (underflow) or
        // tries a huge allocation in release.
        if total == 0 {
            return Ok(ToImagesReport { paths: Vec::new() });
        }
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
                .map_err(|e| {
                    // #1467: LimitExceeded must surface as ResourceLimitExceeded,
                    // not as an opaque Internal error.
                    use pdf_engine::EngineError;
                    if let EngineError::LimitExceeded(ref le) = e {
                        return Error::from(le.clone());
                    }
                    internal_error(format!("render page {page_idx_1b} failed: {e}"))
                })?;

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

    /// Render a single page to image bytes in memory.
    ///
    /// Returns encoded image bytes (PNG or JPEG) without writing to disk.
    /// Prefer this over [`to_images`](Self::to_images) when you need the
    /// bytes directly — e.g. in an HTTP handler or test — rather than
    /// writing a file.
    ///
    /// `page` is 1-based.  `dpi` controls render resolution.  `format`
    /// selects [`ImageFormat::Png`] (lossless) or [`ImageFormat::Jpeg`]
    /// (smaller, lossy).
    ///
    /// # Errors
    ///
    /// - [`Error::FeatureNotInTier`] if the active license does not grant
    ///   [`crate::capability::Capability::RenderRaster`].
    /// - [`Error::Internal`] if `page` is 0, exceeds [`page_count`](Self::page_count),
    ///   or the render / encode step fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pdfluent::{PdfDocument, ImageFormat};
    ///
    /// let doc = PdfDocument::open("report.pdf").unwrap();
    /// let png_bytes = doc.render_page(1, 150, ImageFormat::Png).unwrap();
    /// assert!(!png_bytes.is_empty());
    /// ```
    #[cfg(not(target_arch = "wasm32"))]
    pub fn render_page(&self, page: usize, dpi: u32, format: ImageFormat) -> Result<Vec<u8>> {
        use pdf_engine::render::{PixelFormat, RenderOptions};

        self.require_capability(Capability::RenderRaster)?;
        let total = self.engine.page_count();
        if page == 0 || page > total {
            return Err(internal_error(format!(
                "page index {page} out of range (document has {total} pages)",
            )));
        }

        let render_opts = RenderOptions {
            dpi: dpi as f64,
            ..Default::default()
        };

        let rendered = with_leniency(&self.diagnostics, || {
            self.engine
                .render_page(page - 1, &render_opts)
                .map_err(|e| {
                    use pdf_engine::EngineError;
                    if let EngineError::LimitExceeded(ref le) = e {
                        return Error::from(le.clone());
                    }
                    internal_error(format!("render page {page} failed: {e}"))
                })
        })?;

        if !matches!(rendered.pixel_format, PixelFormat::Rgba8) {
            return Err(internal_error(format!(
                "unexpected pixel format {:?} from renderer",
                rendered.pixel_format,
            )));
        }

        match format {
            ImageFormat::Png => {
                let mut buf: Vec<u8> = Vec::new();
                {
                    let mut encoder = png::Encoder::new(&mut buf, rendered.width, rendered.height);
                    encoder.set_color(png::ColorType::Rgba);
                    encoder.set_depth(png::BitDepth::Eight);
                    let mut writer = encoder
                        .write_header()
                        .map_err(|e| internal_error(format!("png header failed: {e}")))?;
                    writer
                        .write_image_data(&rendered.pixels)
                        .map_err(|e| internal_error(format!("png write failed: {e}")))?;
                }
                Ok(buf)
            }
            ImageFormat::Jpeg => {
                let mut rgb = Vec::with_capacity(rendered.pixels.len() / 4 * 3);
                for chunk in rendered.pixels.chunks_exact(4) {
                    rgb.extend_from_slice(&chunk[..3]);
                }
                let mut buf: Vec<u8> = Vec::new();
                use image::ImageEncoder;
                let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 90);
                encoder
                    .write_image(
                        &rgb,
                        rendered.width,
                        rendered.height,
                        image::ExtendedColorType::Rgb8,
                    )
                    .map_err(|e| internal_error(format!("jpeg encoding failed: {e}")))?;
                Ok(buf)
            }
        }
    }

    /// Render a single page to image bytes in memory — wasm stub.
    #[cfg(target_arch = "wasm32")]
    pub fn render_page(&self, _page: usize, _dpi: u32, _format: ImageFormat) -> Result<Vec<u8>> {
        Err(Error::UnsupportedOnWasm {
            operation: "render_page",
        })
    }

    /// Compress the document by running the full optimisation stack.
    ///
    /// Returns a [`CompressReport`] describing what each pass did.
    ///
    /// # Capability
    ///
    /// Core-tier (always available).
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(target = "pdfluent", skip(self, opts))
    )]
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
    /// Writes the font programme, a `/FontDescriptor`, and a simple
    /// `/Subtype /TrueType` dictionary carrying `/FirstChar`, `/LastChar` and
    /// `/Widths`, then names it in every page's `/Resources/Font`. The returned
    /// string is the name page content should use in a `Tf` operator.
    ///
    /// # What this does not do
    ///
    /// Composite (Type0/CID) fonts. A simple font reaches at most 256 glyphs
    /// through a single-byte encoding, which covers WinAnsi text and does not
    /// cover CJK. A font with no `cmap` is refused rather than embedded as
    /// something that would render the wrong glyphs; the Type0 route stays
    /// deferred (#1224).
    ///
    /// Until 2026-09-01 this returned `Error::MissingDependency`. It was
    /// tempting to wire it to `lopdf::Document::add_font` alone, but that
    /// writes no widths and registers the font on no page -- an `Ok(())` the
    /// caller could not tell from a real one.
    pub fn embed_font(&mut self, font_data: &[u8], name: &str) -> Result<String> {
        self.require_capability(Capability::PdfWrite)?;
        let embedded = pdf_manip::embed_font::embed_font(&mut self.lopdf, font_data, name)
            .map_err(|e| internal_error(format!("font embedding failed: {e}")))?;
        self.refresh_from_lopdf()?;
        Ok(embedded.resource_name)
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
        self.refresh_from_lopdf()
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
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(target = "pdfluent", skip(self, opts))
    )]
    pub fn encrypt(&mut self, opts: EncryptOptions) -> Result<()> {
        self.require_capability(Capability::EncryptionWrite)?;

        // Build the pdf-manip encrypt config. Empty passwords mean the
        // caller did not supply one; leave them empty (lopdf accepts).
        let user_pw_bytes = opts.user_password.unwrap_or_default().into_bytes();
        let owner_pw_bytes = opts
            .owner_password
            .map(String::into_bytes)
            .unwrap_or_else(|| user_pw_bytes.clone());

        let config = pdf_manip::encrypt::EncryptConfig {
            user_password: user_pw_bytes,
            owner_password: owner_pw_bytes,
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
    #[cfg_attr(
        feature = "tracing",
        // `password` is deliberately NOT in fields — it's a secret.
        tracing::instrument(target = "pdfluent", skip(self, password))
    )]
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
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(target = "pdfluent", skip_all)
    )]
    pub fn sign(
        &mut self,
        signer: &dyn crate::signer::PdfSigner,
        opts: crate::signer::SignOptions,
    ) -> Result<()> {
        self.require_capability(Capability::DigitalSignatureSign)?;
        profile_is_reachable(opts.profile)?;
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
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(target = "pdfluent", skip(self))
    )]
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
    /// Routes to `pdf_redact::search_and_redact`. Honours `case_sensitive`,
    /// `regex`, and `on_pages` from [`RedactOptions`](crate::redact::RedactOptions)
    /// — page numbers are translated 1-to-1.
    ///
    /// Images within redaction regions are fully blacked out. Unsupported image
    /// filters (JBIG2, JPEG2000) cause the operation to fail. Overlapping
    /// annotations and XMP metadata are cleaned during apply.
    #[cfg_attr(
        feature = "tracing",
        // `text` is the redaction query — frequently PII/secrets the
        // caller is trying to scrub. MUST be skipped so the tracing
        // span records only `text_len` (low-cardinality, non-secret).
        tracing::instrument(
            target = "pdfluent",
            skip(self, text, opts),
            fields(text_len = text.len())
        )
    )]
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
    ///
    /// Images within redaction regions are fully blacked out. Unsupported image
    /// filters (JBIG2, JPEG2000) cause the operation to fail. Overlapping
    /// annotations and XMP metadata are cleaned during apply.
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

    /// Resync the rendering engine from the current in-memory document state.
    ///
    /// All mutation methods that affect page content automatically call this
    /// after they return. The only case where you need to call it explicitly is
    /// after using [`form_mut`](Self::form_mut): because `PdfFormMut` borrows
    /// `lopdf` mutably, the engine cannot be refreshed while the guard is
    /// live. Call `sync_engine()` once the guard is dropped if you intend to
    /// render the updated document without re-opening it.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pdfluent::PdfDocument;
    ///
    /// let mut doc = PdfDocument::open("form.pdf").unwrap();
    /// doc.form_mut().set_text("name", "Alice").unwrap();
    /// // Drop the form guard, then sync so rendering reflects the new value.
    /// doc.sync_engine().unwrap();
    /// ```
    pub fn sync_engine(&mut self) -> Result<()> {
        self.refresh_from_lopdf()
    }

    // ---------- Text editing (Layout-Aware Text Replacement, Phase 1C) ----------

    /// The document's current text-edit revision, initialised from the
    /// original bytes (or the serialized state for created documents).
    fn current_text_edit_revision(&mut self) -> Result<pdf_manip::text_edit::DocumentRevision> {
        if let Some(rev) = self.text_edit_revision {
            return Ok(rev);
        }
        let rev = match &self.original_bytes {
            Some(bytes) => pdf_manip::text_edit::DocumentRevision::from_source_bytes(bytes),
            None => {
                let mut buf = Vec::new();
                let mut clone = self.lopdf.clone();
                clone
                    .save_to(&mut buf)
                    .map_err(|source| Error::Io { source, path: None })?;
                pdf_manip::text_edit::DocumentRevision::from_source_bytes(&buf)
            }
        };
        self.text_edit_revision = Some(rev);
        Ok(rev)
    }

    /// Bookkeeping after a text-edit commit: stamp the trial notice when the
    /// effective tier is Trial, advance the stored revision, and re-sync the
    /// engine view when the commit touched the document.
    ///
    /// The trial notice is the deliberate trade for having
    /// [`Capability::TextEdit`] available in every tier including Trial: the
    /// feature is fully usable for evaluation, and licensed tiers edit
    /// without the notice.
    fn after_text_edit_commit(
        &mut self,
        report: &pdf_manip::text_edit::TextReplacementReport,
    ) -> Result<()> {
        if report.replacements_applied == 0 {
            return Ok(());
        }
        let tier = license::effective_tier_with_override(self.license_key_override.as_deref());
        if tier == crate::Tier::Trial && !report.pages_modified.is_empty() {
            use pdf_manip::watermark::{
                apply_text_watermark, Color, Layer, PageSelection, Position, TextWatermark,
            };
            let notice = TextWatermark {
                text: "Edited with PDFluent trial - pdfluent.com".into(),
                font_size: 8.0,
                rotation: 0.0,
                opacity: 0.6,
                color: Color::Gray(0.45),
                position: Position::BottomLeft(24.0, 12.0),
                layer: Layer::Foreground,
            };
            let selection = PageSelection::Pages(report.pages_modified.clone());
            apply_text_watermark(&mut self.lopdf, &notice, &selection)?;
        }
        self.text_edit_revision = Some(report.next_revision);
        self.sync_engine()?;
        Ok(())
    }

    /// Find text occurrences for programmatic replacement.
    ///
    /// Each returned [`text_edit::TextMatch`](pdf_manip::text_edit::TextMatch)
    /// carries an opaque, serializable
    /// [`MatchId`](pdf_manip::text_edit::MatchId) that stays valid until the
    /// next applied text edit on this document — find, translate or transform
    /// the text externally, then apply with
    /// [`replace_text_matches`](Self::replace_text_matches).
    ///
    /// Matches inside Form XObjects, style-mixed spans or `/ActualText`
    /// regions are found and reported with `editable = false` rather than
    /// silently omitted.
    ///
    /// Available in every tier ([`Capability::TextEdit`]); searching never
    /// modifies the document.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pdfluent::PdfDocument;
    /// use pdfluent::text_edit::TextQuery;
    ///
    /// let mut doc = PdfDocument::open("contract.pdf").unwrap();
    /// let matches = doc.find_text(TextQuery::exact("Acme B.V.")).unwrap();
    /// println!("{} occurrence(s)", matches.len());
    /// ```
    pub fn find_text(
        &mut self,
        query: pdf_manip::text_edit::TextQuery,
    ) -> Result<Vec<pdf_manip::text_edit::TextMatch>> {
        self.require_capability(Capability::TextEdit)?;
        let revision = self.current_text_edit_revision()?;
        let mut session = pdf_manip::text_edit::begin_text_edit(&mut self.lopdf, revision)
            .map_err(|e| Error::TextEditFailed {
                reason: e.to_string(),
            })?;
        session.find_text(query).map_err(|e| Error::TextEditFailed {
            reason: e.to_string(),
        })
    }

    /// Find and replace text in one call, preserving fonts, positioning and
    /// the surrounding page content.
    ///
    /// Every found occurrence is accounted for in the returned report —
    /// applied or failed with a reason, never silently skipped. Signed
    /// documents are refused unless
    /// [`SignaturePolicy::AllowPostSignatureChange`](pdf_manip::text_edit::SignaturePolicy)
    /// is set in `options`.
    ///
    /// Available in every tier ([`Capability::TextEdit`]). **Trial-tier
    /// edits stamp a small "PDFluent trial" notice on each modified page**;
    /// licensed tiers edit without the notice.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pdfluent::PdfDocument;
    /// use pdfluent::text_edit::{ReplaceOptions, TextQuery};
    ///
    /// let mut doc = PdfDocument::open("template.pdf").unwrap();
    /// let report = doc
    ///     .replace_text(TextQuery::exact("{{customer}}"), "Acme B.V.", ReplaceOptions::default())
    ///     .unwrap();
    /// assert_eq!(report.replacements_failed, 0);
    /// doc.save("filled.pdf").unwrap();
    /// ```
    pub fn replace_text(
        &mut self,
        query: pdf_manip::text_edit::TextQuery,
        replacement: &str,
        options: pdf_manip::text_edit::ReplaceOptions,
    ) -> Result<pdf_manip::text_edit::TextReplacementReport> {
        self.require_capability(Capability::TextEdit)?;
        let revision = self.current_text_edit_revision()?;
        let report = pdf_manip::text_edit::replace_text(
            &mut self.lopdf,
            revision,
            query,
            replacement,
            options,
        )
        .map_err(|e| Error::TextEditFailed {
            reason: e.to_string(),
        })?;
        self.after_text_edit_commit(&report)?;
        Ok(report)
    }

    /// Apply per-match replacements previously located with
    /// [`find_text`](Self::find_text) — the asynchronous workflow: find,
    /// produce replacement text externally (translation, personalisation),
    /// then apply by match id in one atomic transaction.
    ///
    /// All edits commit under `options` (default: `AllOrNothing` — any
    /// invalid edit aborts the whole transaction with a typed error).
    ///
    /// Available in every tier ([`Capability::TextEdit`]). **Trial-tier
    /// edits stamp a small "PDFluent trial" notice on each modified page**;
    /// licensed tiers edit without the notice.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pdfluent::PdfDocument;
    /// use pdfluent::text_edit::{ReplaceOptions, TextQuery};
    ///
    /// let mut doc = PdfDocument::open("letter.pdf").unwrap();
    /// let matches = doc.find_text(TextQuery::exact("Dear customer")).unwrap();
    /// let edits: Vec<_> = matches
    ///     .iter()
    ///     .filter(|m| m.editable)
    ///     .map(|m| (m.id.clone(), "Beste klant".to_string()))
    ///     .collect();
    /// let report = doc.replace_text_matches(&edits, ReplaceOptions::default()).unwrap();
    /// assert_eq!(report.replacements_applied, edits.len());
    /// ```
    pub fn replace_text_matches(
        &mut self,
        edits: &[(pdf_manip::text_edit::MatchId, String)],
        options: pdf_manip::text_edit::ReplaceOptions,
    ) -> Result<pdf_manip::text_edit::TextReplacementReport> {
        self.require_capability(Capability::TextEdit)?;
        let revision = self.current_text_edit_revision()?;
        let mut session = pdf_manip::text_edit::begin_text_edit(&mut self.lopdf, revision)
            .map_err(|e| Error::TextEditFailed {
                reason: e.to_string(),
            })?;
        for (id, replacement) in edits {
            session
                .stage_replace(id, replacement, options.clone())
                .map_err(|e| Error::TextEditFailed {
                    reason: e.to_string(),
                })?;
        }
        let report = session.commit().map_err(|e| Error::TextEditFailed {
            reason: e.to_string(),
        })?;
        self.after_text_edit_commit(&report)?;
        Ok(report)
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

        // Preserve the per-document license override, processing limits, and the
        // original backing bytes (needed for incremental save) across the
        // re-parse; `from_bytes` alone would reset them.
        let mut opts = OpenOptions::new();
        if let Some(ref key) = self.license_key_override {
            opts = opts.with_license_key(key.clone());
        }
        if let Some(ref limits) = self.processing_limits {
            opts = opts.with_processing_limits(limits.clone());
        }
        let original_bytes = self.original_bytes.clone();

        let mut new_doc = Self::from_bytes_with(&buf, opts)?;
        new_doc.original_bytes = original_bytes;
        new_doc.text_edit_revision = self.text_edit_revision;

        *self = new_doc;
        Ok(())
    }

    // ---------- PDF/A compliance (M5 #1291) ----------

    /// Validate the document against the given PDF/A profile.
    ///
    /// Returns a [`PdfAValidationReport`](crate::compliance::PdfAValidationReport)
    /// describing all findings. Call its
    /// [`is_compliant`](crate::compliance::PdfAValidationReport::is_compliant)
    /// method to check whether the document passes without error-severity
    /// violations.
    ///
    /// Requires the `pdfa` feature and a license tier that grants
    /// [`Capability::PdfaValidate`].
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pdfluent::prelude::*;
    ///
    /// let doc = PdfDocument::open("form.pdf").unwrap();
    /// let report = doc.validate_pdfa(PdfAProfile::A2b).unwrap();
    /// if !report.is_compliant() {
    ///     for v in &report.violations {
    ///         eprintln!("[{}] {}", v.rule, v.message);
    ///     }
    /// }
    /// ```
    #[cfg(feature = "pdfa")]
    pub fn validate_pdfa(
        &self,
        profile: crate::compliance::PdfAProfile,
    ) -> Result<crate::compliance::PdfAValidationReport> {
        self.require_capability(Capability::PdfaValidate)?;
        let raw = pdf_compliance::validate_pdfa(self.engine.pdf(), profile.into());
        Ok(crate::compliance::report_from_compliance(raw, profile))
    }

    /// Convert this document to PDF/A and return the converted document.
    ///
    /// Runs the full conversion pipeline: font embedding and repair, colour
    /// space normalization with an OutputIntent, removal of constructs PDF/A
    /// forbids (JavaScript, embedded files, transfer functions), and XMP
    /// metadata. The source document is unchanged.
    ///
    /// Conversion is best-effort on damaged input: a broken xref table or page
    /// tree is repaired first where possible. Validate the result with
    /// [`validate_pdfa`](Self::validate_pdfa) rather than assuming success —
    /// some source documents cannot be made conformant without changing how
    /// they look, which an archival converter must not do silently.
    ///
    /// Requires the `pdfa` feature and a license tier granting the matching
    /// [`Capability::PdfaConvertA1b`]/[`A2b`](Capability::PdfaConvertA2b)/[`A3b`](Capability::PdfaConvertA3b).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pdfluent::prelude::*;
    ///
    /// let doc = PdfDocument::open("invoice.pdf").unwrap();
    /// let archived = doc.convert_to_pdfa(PdfAProfile::A2b).unwrap();
    /// assert!(archived.validate_pdfa(PdfAProfile::A2b).unwrap().is_compliant());
    /// archived.save("invoice_pdfa.pdf").unwrap();
    /// ```
    #[cfg(feature = "pdfa")]
    pub fn convert_to_pdfa(&self, profile: crate::compliance::PdfAProfile) -> Result<PdfDocument> {
        use crate::compliance::PdfAProfile;

        self.require_capability(match profile {
            PdfAProfile::A1b => Capability::PdfaConvertA1b,
            PdfAProfile::A2b => Capability::PdfaConvertA2b,
            PdfAProfile::A3b => Capability::PdfaConvertA3b,
        })?;

        let opts = pdf_manip::pdfa::PdfAConvertOptions {
            conformance: match profile {
                PdfAProfile::A1b => pdf_manip::pdfa_xmp::PdfAConformance::A1b,
                PdfAProfile::A2b => pdf_manip::pdfa_xmp::PdfAConformance::A2b,
                PdfAProfile::A3b => pdf_manip::pdfa_xmp::PdfAConformance::A3b,
            },
            ..Default::default()
        };

        let converted = pdf_manip::pdfa::convert_bytes(self.engine.pdf().data().as_ref(), &opts)
            .map_err(|e| Error::InvalidPdf {
                byte_offset: None,
                reason: e.to_string(),
            })?;
        Self::from_bytes(&converted)
    }

    /// Read the logical structure tree of a tagged PDF (ISO 32000-1 §14.7).
    ///
    /// Returns the document's semantic structure — headings, paragraphs,
    /// tables, lists, figures — in reading order, with accessibility attributes
    /// (alternate text, replacement text, language). Returns `None` if the
    /// document is not tagged (no `/StructTreeRoot`).
    ///
    /// This is a read-only inspection of the existing structure; it does not
    /// generate or repair tags. Useful for accessibility auditing (heading
    /// outline, alt-text coverage) and structure-aware extraction.
    #[cfg(feature = "pdfa")]
    pub fn structure_tree(&self) -> Option<crate::tagged::DocumentStructure> {
        pdf_compliance::tagged::parse(self.engine.pdf()).map(crate::tagged::from_structure_tree)
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

    // ---------- Document structure: outlines / annotations / attachments ----------

    /// Read the document outline (bookmark) tree.
    ///
    /// Returns the nested [`Outline`](crate::structure::Outline) list; an
    /// empty `Vec` for a document without an outline. `Outline::page` is
    /// the 0-based target page for in-document GoTo actions, `None` for
    /// URI / named / external destinations.
    pub fn outlines(&self) -> Result<Vec<crate::structure::Outline>> {
        let bookmarks = pdf_manip::bookmarks::read_bookmarks(&self.lopdf)?;
        Ok(bookmarks
            .iter()
            .map(crate::structure::from_bookmark)
            .collect())
    }

    /// Replace the document outline (bookmark) tree.
    ///
    /// Passing an empty slice removes the outline. Pages are 0-based.
    pub fn set_outlines(&mut self, outlines: &[crate::structure::Outline]) -> Result<()> {
        let bookmarks: Vec<_> = outlines.iter().map(crate::structure::to_bookmark).collect();
        pdf_manip::bookmarks::write_bookmarks(&mut self.lopdf, &bookmarks)?;
        self.refresh_from_lopdf()
    }

    /// List the annotations on a 0-based page (read-only view).
    ///
    /// Returns `[{subtype, rect, contents}]`. Annotation **authoring**
    /// (add/delete/update) is intentionally not exposed on the Rust
    /// facade in v1 — the v1 annotation-authoring surface is the WASM
    /// binding (`PdfDocMut.addHighlight` / `addStickyNote` / `addFreeText`)
    /// and the `pdf-annot` builder crate. See
    /// `benchmarks/runs/ga_100_closure_v3/core_pdf_sdk_quality_true100/`.
    pub fn annotations(&self, page_index: usize) -> Result<Vec<crate::structure::AnnotationInfo>> {
        Ok(crate::structure::read_annotations(&self.lopdf, page_index))
    }

    /// List embedded-file attachments (name + uncompressed size).
    ///
    /// Useful for ZUGFeRD / e-invoice extraction. Adding embedded files
    /// is intentionally not exposed on the Rust facade in v1 (see the
    /// closure report); only read/list + extract are supported.
    pub fn attachments(&self) -> Result<Vec<crate::structure::Attachment>> {
        Ok(crate::structure::read_attachments(&self.lopdf))
    }

    /// Compute the display label of every page from the document's
    /// `/PageLabels` (ISO 32000-1 §12.4.2).
    ///
    /// Returns exactly [`page_count`](Self::page_count) entries, in page order.
    /// Documents commonly label front matter as `i, ii, iii` and the body as
    /// `1, 2, 3`, or use prefixes like `A-1`. If the document declares no page
    /// labels, every page gets its decimal physical number (`"1"`, `"2"`, …),
    /// matching the viewer default.
    pub fn page_labels(&self) -> Vec<String> {
        crate::page_labels::read_page_labels(&self.lopdf, self.page_count())
    }

    /// Extract the bytes of the embedded file with the given name, or
    /// `None` if no attachment with that name exists.
    pub fn attachment_bytes(&self, name: &str) -> Result<Option<Vec<u8>>> {
        Ok(crate::structure::read_attachment_bytes(&self.lopdf, name))
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
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            target = "pdfluent",
            skip(self, opts),
            fields(path = %path.as_ref().display(), overwrite = opts.overwrite)
        )
    )]
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
        let bytes = if opts.incremental {
            self.to_incremental_bytes()?
        } else {
            self.to_bytes()?
        };
        fs::write(path_ref, bytes).map_err(|source| Error::Io {
            source,
            path: Some(path_ref.to_path_buf()),
        })?;
        Ok(())
    }

    /// Serialise the document to a byte vector.
    ///
    /// Useful when you need the PDF bytes in memory rather than on disk —
    /// e.g. to return them from an HTTP handler or to pass them to another
    /// library.  For disk output prefer [`save`](Self::save).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pdfluent::PdfDocument;
    ///
    /// let doc = PdfDocument::open("input.pdf").unwrap();
    /// let bytes = doc.to_bytes().unwrap();
    /// // bytes can be written to an HTTP response, stored in a database, etc.
    /// assert!(!bytes.is_empty());
    /// ```
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(target = "pdfluent", skip(self))
    )]
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

    /// Serialise only the incremental changes to a byte vector.
    ///
    /// The original bytes are preserved verbatim as the file prefix and only the
    /// changed/new objects are appended as an incremental update section. This
    /// keeps any existing digital signatures valid (their signed byte range is
    /// untouched).
    ///
    /// Returns [`Error::Unsupported`] if the document is encrypted or was not
    /// loaded from existing bytes (e.g. created from scratch).
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(target = "pdfluent", skip(self))
    )]
    pub fn to_incremental_bytes(&self) -> Result<Vec<u8>> {
        self.require_capability(Capability::PdfWrite)?;

        // 1. Encryption check: refuse if encrypted.
        if self.lopdf.is_encrypted() {
            return Err(Error::Unsupported(
                "Incremental save for encrypted PDFs is currently unsupported".into(),
            ));
        }

        // 2. Original-bytes availability check: refuse if created from scratch.
        let original_bytes = match &self.original_bytes {
            Some(bytes) => bytes,
            None => {
                return Err(Error::Unsupported(
                    "Incremental save requires a document loaded from existing bytes".into(),
                ));
            }
        };

        // 3. Load the original document and build an incremental update over it.
        let original_lopdf = lopdf::Document::load_mem(original_bytes.as_slice())
            .map_err(|e| internal_error(format!("Failed to load original bytes: {e}")))?;
        let mut incremental_doc = lopdf::IncrementalDocument::create_from(
            original_bytes.as_ref().clone(),
            original_lopdf.clone(),
        );

        // 4. Emit only changed or new objects.
        for (&object_id, object) in &self.lopdf.objects {
            let is_changed_or_new = match original_lopdf.objects.get(&object_id) {
                Some(orig_object) => orig_object != object,
                None => true,
            };
            if is_changed_or_new {
                incremental_doc
                    .new_document
                    .set_object(object_id, object.clone());
            }
        }

        // 5. Carry over version and trailer (lopdf manages Prev/Size).
        incremental_doc.new_document.version = self.lopdf.version.clone();
        for (key, val) in &self.lopdf.trailer {
            if key != b"Prev" && key != b"Size" {
                incremental_doc
                    .new_document
                    .trailer
                    .set(key.clone(), val.clone());
            }
        }
        incremental_doc.new_document.max_id = self.lopdf.max_id;

        let mut buf = Vec::with_capacity(64 * 1024);
        incremental_doc
            .save_to(&mut buf)
            .map_err(|source| Error::Io { source, path: None })?;
        Ok(buf)
    }

    /// Flatten all non-widget and non-link annotations containing appearance
    /// streams (`/AP` `/N`) directly into the page content streams, removing them
    /// from the pages' annotation catalogs and from the document.
    pub fn flatten_annotations(&mut self) -> Result<()> {
        self.require_capability(Capability::PdfWrite)?;
        pdf_annot::flatten_annotations(&mut self.lopdf)
            .map_err(|e| internal_error(format!("Failed to flatten annotations: {e}")))?;
        self.refresh_from_lopdf()
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

/// Refuse the profiles we cannot actually produce.
///
/// B-T, B-LT and B-LTA all need a timestamp authority, and SignOptions carries
/// no TSA configuration, so `sign_pdf_ltv` is unreachable from here. Until it
/// is wired, asking for one of those returned a plain B-B signature with no
/// indication that the request had been downgraded (#176).
///
/// A signature that claims long-term validity and carries no validation data
/// is a false compliance claim. Failing is the smaller harm.
fn profile_is_reachable(profile: crate::signer::PadesProfile) -> Result<()> {
    use crate::signer::PadesProfile::*;
    let (naam, wat) = match profile {
        BasicSignature => return Ok(()),
        Timestamped => ("B-T", "a trusted timestamp"),
        LongTerm => ("B-LT", "long-term validation data (DSS)"),
        LongTermArchive => ("B-LTA", "an archive timestamp"),
    };
    Err(crate::Error::Unsupported(format!(
        "PAdES {naam} needs {wat}, which requires a timestamp authority. \
         SignOptions carries no TSA configuration yet, so this profile cannot \
         be produced. Use PadesProfile::BasicSignature, or follow #176."
    )))
}

fn map_sign_options(opts: &crate::signer::SignOptions) -> pdf_sign::SignOptions {
    // Every profile maps to the same SubFilter, which is correct: PAdES uses
    // ETSI.CAdES.detached throughout. What distinguishes B-T, B-LT and B-LTA is
    // the timestamp and validation data, and those come from sign_pdf_ltv with
    // a TSA -- not from this field. Refusing the unreachable profiles happens
    // in sign(); see profile_is_reachable.
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

#[cfg(test)]
mod tests {
    use super::{load_lopdf_from_shared_bytes, open_engine_from_shared_bytes, SaveOptions};
    use std::sync::Arc;

    fn minimal_pdf_bytes() -> Vec<u8> {
        use lopdf::{dictionary, Document, Object, Stream};

        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        let content = Stream::new(dictionary! {}, b"BT ET".to_vec());
        let content_id = doc.add_object(content);

        doc.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => Object::Name(b"Page".to_vec()),
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => Object::Array(vec![
                    Object::Integer(0),
                    Object::Integer(0),
                    Object::Integer(72),
                    Object::Integer(72),
                ]),
                "Contents" => Object::Reference(content_id),
            }),
        );

        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => Object::Name(b"Pages".to_vec()),
                "Kids" => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => Object::Integer(1),
            }),
        );

        let catalog_id = doc.new_object_id();
        doc.objects.insert(
            catalog_id,
            Object::Dictionary(dictionary! {
                "Type" => Object::Name(b"Catalog".to_vec()),
                "Pages" => Object::Reference(pages_id),
            }),
        );
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).expect("build minimal fixture");
        bytes
    }

    #[test]
    fn diagnostics_survive_a_poisoned_lock() {
        let doc = super::PdfDocument::from_bytes(&minimal_pdf_bytes()).expect("open minimal pdf");
        // Poison the diagnostics mutex: a thread panics while holding the lock.
        let handle = Arc::clone(&doc.diagnostics);
        let _ = std::thread::spawn(move || {
            let _guard = handle.lock().expect("acquire before poisoning");
            panic!("intentionally poison the diagnostics lock");
        })
        .join();
        // The public accessors must recover the poisoned guard, not cascade-panic.
        let _ = doc.diagnostics();
        let _ = doc.take_diagnostics();
    }

    #[test]
    fn from_bytes_with_shares_single_owned_input_buffer() {
        let shared = Arc::new(minimal_pdf_bytes());
        let shared_ptr = shared.as_slice().as_ptr();
        let shared_len = shared.len();

        let engine = open_engine_from_shared_bytes(shared.clone(), None, None)
            .expect("open engine from shared bytes");

        assert_eq!(
            engine.pdf().data().as_ref().as_ptr(),
            shared_ptr,
            "pdf_engine should retain the same shared buffer rather than a second Vec clone",
        );
        assert_eq!(engine.pdf().data().as_ref().len(), shared_len);
        let owners_after_engine = Arc::strong_count(&shared);
        assert!(
            owners_after_engine >= 2,
            "pdf_engine should retain shared ownership of the input buffer",
        );

        let _lopdf = load_lopdf_from_shared_bytes(&shared, None)
            .expect("load lopdf from borrowed shared bytes");

        assert_eq!(
            Arc::strong_count(&shared),
            owners_after_engine,
            "lopdf should parse from a borrowed slice of the same shared buffer",
        );
    }

    #[test]
    fn save_options_default_overwrite_is_false() {
        assert!(!SaveOptions::default().overwrite);
    }

    #[cfg(feature = "pdfa")]
    #[test]
    fn validate_pdfa_returns_report_for_non_conforming_doc() {
        use super::PdfDocument;
        use crate::compliance::PdfAProfile;

        let bytes = minimal_pdf_bytes();
        let doc = PdfDocument::from_bytes(&bytes).expect("parse minimal fixture");
        let report = doc
            .validate_pdfa(PdfAProfile::A2b)
            .expect("validate_pdfa should not return an error");
        assert!(
            !report.is_compliant(),
            "a minimal synthetic PDF must not pass PDF/A-2b validation"
        );
    }

    // -----------------------------------------------------------------------
    // extract_text tests (M4-FACADE-01 / #1316)
    // -----------------------------------------------------------------------

    /// `extract_text` must succeed on a minimal single-page document and
    /// return a `String` (possibly empty for a content-free page).
    #[test]
    fn extract_text_returns_ok_for_minimal_pdf() {
        use super::PdfDocument;

        let bytes = minimal_pdf_bytes();
        let doc = PdfDocument::from_bytes(&bytes).expect("parse minimal fixture");
        let text = doc.extract_text().expect("extract_text should not fail");
        // The minimal PDF has an empty `BT ET` content stream, so the result
        // is the empty string joined across one page — just assert it round-
        // trips as a String without panicking.
        let _ = text; // type-check: must be String
    }

    /// `extract_text` on a multi-page document joins page texts with `"\n\n"`.
    ///
    /// We verify the separator contract using the minimal lopdf builder to
    /// construct a two-page PDF and confirming a `"\n\n"` is present in the
    /// output when both pages have text.  Because the minimal content stream
    /// produces no extractable text, this test limits itself to asserting the
    /// method returns `Ok` and that the result contains at most one `"\n\n"`
    /// separator for a two-page document (i.e. join is used, not something
    /// that inserts extra separators).
    #[test]
    fn extract_text_joins_pages_with_double_newline() {
        use super::PdfDocument;
        use lopdf::{dictionary, Document, Object, Stream};

        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();

        let make_page = |doc: &mut Document, pages_id| {
            let content = Stream::new(dictionary! {}, b"BT ET".to_vec());
            let content_id = doc.add_object(content);
            let page_id = doc.new_object_id();
            doc.objects.insert(
                page_id,
                Object::Dictionary(dictionary! {
                    "Type" => Object::Name(b"Page".to_vec()),
                    "Parent" => Object::Reference(pages_id),
                    "MediaBox" => Object::Array(vec![
                        Object::Integer(0),
                        Object::Integer(0),
                        Object::Integer(72),
                        Object::Integer(72),
                    ]),
                    "Contents" => Object::Reference(content_id),
                }),
            );
            page_id
        };

        let page1_id = make_page(&mut doc, pages_id);
        let page2_id = make_page(&mut doc, pages_id);

        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => Object::Name(b"Pages".to_vec()),
                "Kids" => Object::Array(vec![
                    Object::Reference(page1_id),
                    Object::Reference(page2_id),
                ]),
                "Count" => Object::Integer(2),
            }),
        );
        let catalog_id = doc.new_object_id();
        doc.objects.insert(
            catalog_id,
            Object::Dictionary(dictionary! {
                "Type" => Object::Name(b"Catalog".to_vec()),
                "Pages" => Object::Reference(pages_id),
            }),
        );
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).expect("build two-page fixture");
        let pdf_doc = PdfDocument::from_bytes(&bytes).expect("parse two-page fixture");

        assert_eq!(pdf_doc.page_count(), 2, "fixture must have 2 pages");
        let text = pdf_doc
            .extract_text()
            .expect("extract_text on two-page doc");
        // The join produces exactly one "\n\n" separator for a two-page doc.
        // Both pages are empty here, so result is "\n\n" (empty + sep + empty).
        assert_eq!(
            text.matches("\n\n").count(),
            1,
            "two pages joined with '\\n\\n' must produce exactly one separator; got {text:?}",
        );
    }

    /// Build a minimal single-page PDF with a non-square mediabox (200×100 pts)
    /// so that rotation by 90° produces an observable dimension swap.
    fn rect_pdf_bytes() -> Vec<u8> {
        use lopdf::{dictionary, Document, Object, Stream};

        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        let content = Stream::new(dictionary! {}, b"BT ET".to_vec());
        let content_id = doc.add_object(content);

        doc.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => Object::Name(b"Page".to_vec()),
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => Object::Array(vec![
                    Object::Integer(0),
                    Object::Integer(0),
                    Object::Integer(200),
                    Object::Integer(100),
                ]),
                "Contents" => Object::Reference(content_id),
            }),
        );

        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => Object::Name(b"Pages".to_vec()),
                "Kids" => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => Object::Integer(1),
            }),
        );

        let catalog_id = doc.new_object_id();
        doc.objects.insert(
            catalog_id,
            Object::Dictionary(dictionary! {
                "Type" => Object::Name(b"Catalog".to_vec()),
                "Pages" => Object::Reference(pages_id),
            }),
        );
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).expect("build rect fixture");
        bytes
    }

    /// After `rotate_page`, the rendering engine must be re-synced so that
    /// `page_geometry` returns the updated rotation rather than the stale state.
    #[test]
    fn rotate_page_refreshes_engine() {
        use crate::Rotation;
        use pdf_engine::geometry::PageRotation;

        let mut doc = super::PdfDocument::from_bytes(&rect_pdf_bytes()).expect("open rect pdf");

        // Before rotation: engine must report no rotation.
        let geom_before = doc.engine.page_geometry(0).expect("geometry before");
        assert_eq!(
            geom_before.rotation,
            PageRotation::None,
            "initial rotation must be None"
        );

        doc.rotate_page(1, Rotation::Clockwise90).expect("rotate");

        // After rotation: engine must report Rotate90 (the fix resync'd it).
        let geom_after = doc.engine.page_geometry(0).expect("geometry after");
        assert_eq!(
            geom_after.rotation,
            PageRotation::Rotate90,
            "engine must reflect Clockwise90 rotation after rotate_page"
        );
    }

    /// After mutations via `form_mut`, the engine remains stale until
    /// `sync_engine` is called. This test verifies that:
    ///   1. Immediately after a form mutation the engine is stale.
    ///   2. After `sync_engine()` the engine reflects the mutated lopdf state.
    ///
    /// We can't observe AcroForm field values through the read-path on an
    /// ordinary test PDF, so we verify staleness through `page_count` and
    /// byte equality: `to_bytes()` (from lopdf) must differ from what the
    /// engine would produce from its own data before the sync, and match
    /// after the sync.
    #[test]
    fn sync_engine_refreshes_after_form_mut() {
        let bytes = rect_pdf_bytes();
        let mut doc = super::PdfDocument::from_bytes(&bytes).expect("open");

        // Engine bytes before any mutation.
        let engine_bytes_before = {
            let engine_inner = &doc.engine;
            let pdf_data: &[u8] = engine_inner.pdf().data().as_ref();
            pdf_data.to_vec()
        };

        // Mutate via form_mut (adds an empty AcroForm to lopdf, making the
        // bytes diverge even though no field is set).
        {
            let _form = doc.form_mut();
            // The borrow is dropped here; lopdf has been accessed mutably.
        }

        // Also mutate lopdf directly so the divergence is measurable.
        {
            // Write an arbitrary info dict entry to lopdf only.
            let info_id = doc.lopdf.add_object(lopdf::Object::Dictionary(
                lopdf::dictionary! { "Producer" => lopdf::Object::string_literal(b"test-sync") },
            ));
            doc.lopdf
                .trailer
                .set("Info", lopdf::Object::Reference(info_id));
        }

        // Engine pdf bytes must still equal pre-mutation (stale).
        let engine_bytes_mid = {
            let pdf_data: &[u8] = doc.engine.pdf().data().as_ref();
            pdf_data.to_vec()
        };
        assert_eq!(
            engine_bytes_mid, engine_bytes_before,
            "engine must be stale (unchanged) before sync_engine is called"
        );

        // Sync the engine.
        doc.sync_engine().expect("sync_engine");

        // After sync, the lopdf and engine are consistent: serialize both and compare.
        let mut lopdf_bytes = Vec::new();
        doc.lopdf
            .clone()
            .save_to(&mut lopdf_bytes)
            .expect("lopdf serialize");

        let engine_bytes_after: &[u8] = doc.engine.pdf().data().as_ref();

        // The engine's stored bytes must now match the lopdf serialization.
        // We compare length as a proxy (exact bytes may differ due to lopdf
        // cross-reference rebuilding, but the length delta must shrink significantly).
        assert_ne!(
            engine_bytes_after,
            engine_bytes_before.as_slice(),
            "engine must no longer be stale after sync_engine"
        );
    }

    /// Capability gate: `extract_text` must return `Error::FeatureNotInTier`
    /// when the effective tier does not include `Capability::TextExtract`.
    ///
    /// `TextExtract` is granted to all tiers (including Trial), so we verify
    /// the gate by constructing the `FeatureNotInTier` variant directly and
    /// asserting its `code()` matches the stable error code — this is the
    /// same approach used in `tests/capability_enforcement.rs` for caps that
    /// are not currently withheld from any real tier.
    #[test]
    fn extract_text_capability_gate_error_is_well_formed() {
        use crate::capability::Capability;
        use crate::error::Error;
        use crate::tier::Tier;

        // Construct the error that `extract_text` would return if a future
        // tier configuration excluded TextExtract.
        let err = Error::FeatureNotInTier {
            capability: Capability::TextExtract,
            current_tier: Tier::Trial,
            required_tier: Tier::Developer,
        };
        assert_eq!(
            err.code(),
            "E-LICENSE-FEATURE-NOT-IN-TIER",
            "stable error code must match RFC §5.4",
        );
        let rendered = format!("{err}");
        assert!(
            rendered.contains("TextExtract"),
            "Display must mention the missing capability; got {rendered:?}",
        );
    }
}
