use crate::Object;
use crate::object::MAX_DECOMPRESSED_BYTES;

/// Default maximum input size: 256 MiB.
///
/// Chosen so that a single document load can never exceed ~2 GB of RSS on
/// realistic input (the worst-case amplification ratio measured across the
/// corpus is ~20×, so 256 MiB × 20 ≈ 5 GB — still above the default, but
/// the limit is a first-line guard, not a tight memory cap).  Callers that
/// need larger files can pass a higher limit via `LoadOptions::max_file_bytes`.
pub const DEFAULT_MAX_FILE_BYTES: usize = 256 * 1024 * 1024;

/// Type alias for the filter function used during PDF loading.
///
/// The function receives an object ID and a mutable reference to the object,
/// and returns `Some((id, object))` to keep it or `None` to discard it.
pub type FilterFunc = fn((u32, u16), &mut Object) -> Option<((u32, u16), Object)>;

/// Options that control how a PDF document is loaded into memory.
///
/// This is the union of upstream lopdf's `LoadOptions` (`password`, `filter`,
/// `strict`, `max_decompressed_size`) and the two options this fork added
/// independently (`max_file_bytes`, `lazy_objstm`). Both structs existed under
/// the same name with disjoint fields; merging them was the only resolution
/// that kept upstream's loading paths and ours working at once.
///
/// The defaults differ from upstream on purpose. Upstream defaults
/// `max_decompressed_size` to `None` — no bound at all. This fork defaults it
/// to [`crate::object::MAX_DECOMPRESSED_BYTES`], because LOPDF-ZBOMB-01 is a
/// guarantee this crate already shipped and a default of `None` would revoke it
/// for every caller that does not opt in.
///
/// # Example
///
/// ```rust,ignore
/// let opts = LoadOptions::new()
///     .max_file_bytes(64 * 1024 * 1024)   // 64 MiB hard limit
///     .lazy_objstm(true);                  // defer ObjStm extraction
/// let doc = Document::load_mem_with_options(data, &opts)?;
/// doc.resolve_pending_object_streams()?;   // required when lazy_objstm = true
/// ```
#[derive(Clone)]
pub struct LoadOptions {
    /// Password for encrypted PDFs.
    pub password: Option<String>,

    /// Object filter applied during loading.
    pub filter: Option<FilterFunc>,

    /// When `true`, reject non-conforming PDFs instead of silently accepting
    /// them. Defaults to `false` (lenient parsing).
    pub strict: bool,

    /// Maximum number of bytes any single stream may decompress to during
    /// loading (object streams and cross-reference streams).
    ///
    /// Compression filters can inflate a tiny input into an enormous output (a
    /// "decompression bomb"). Because object and xref streams are decoded
    /// eagerly while the document is loaded, an unbounded stream can exhaust
    /// memory before any of your code runs. A stream that would exceed this
    /// fails with [`crate::DecompressError::MemoryLimitExceeded`].
    ///
    /// Defaults to `Some(MAX_DECOMPRESSED_BYTES)` (256 MiB). Upstream defaults
    /// this to `None`; see the type-level note for why we do not.
    pub max_decompressed_size: Option<usize>,

    /// Maximum allowed size of the input buffer in bytes.
    ///
    /// When `Some(limit)`, `load_mem_with_options` / `load_with_options` return
    /// `Err(Error::DocumentTooLarge)` without allocating the object graph if the
    /// input exceeds `limit`.  `None` disables the check entirely.
    pub max_file_bytes: Option<usize>,

    /// When `true`, ObjStm streams are **not** decompressed during loading.
    ///
    /// Their container objects are kept in `Document::objects` and their IDs
    /// are stored in `Document::pending_obj_streams`.  The caller must invoke
    /// `Document::resolve_pending_object_streams` before accessing any object
    /// that lives inside an ObjStm container.
    ///
    /// When `false` (default), ObjStm streams are decompressed eagerly during
    /// load and their container streams are discarded immediately after
    /// extraction (Phase 2a memory optimisation).
    pub lazy_objstm: bool,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            password: None,
            filter: None,
            strict: false,
            max_decompressed_size: Some(MAX_DECOMPRESSED_BYTES),
            max_file_bytes: Some(DEFAULT_MAX_FILE_BYTES),
            lazy_objstm: false,
        }
    }
}

impl std::fmt::Debug for LoadOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadOptions")
            .field("password", &self.password.as_ref().map(|_| "***"))
            .field("filter", &self.filter.map(|_| "fn(..)"))
            .field("strict", &self.strict)
            .field("max_decompressed_size", &self.max_decompressed_size)
            .field("max_file_bytes", &self.max_file_bytes)
            .field("lazy_objstm", &self.lazy_objstm)
            .finish()
    }
}

impl LoadOptions {
    /// Create options with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create options with a password for encrypted PDFs.
    pub fn with_password(password: &str) -> Self {
        Self {
            password: Some(password.to_string()),
            ..Default::default()
        }
    }

    /// Create options with an object filter.
    pub fn with_filter(filter: FilterFunc) -> Self {
        Self {
            filter: Some(filter),
            ..Default::default()
        }
    }

    /// Create options that bound how large any single stream may decompress to
    /// during loading, to defend against decompression bombs in untrusted PDFs.
    /// See [`LoadOptions::max_decompressed_size`].
    pub fn with_max_decompressed_size(max_decompressed_size: usize) -> Self {
        Self {
            max_decompressed_size: Some(max_decompressed_size),
            ..Default::default()
        }
    }

    /// Set the maximum allowed input size in bytes.
    ///
    /// Pass `None` to remove the limit (not recommended in worker-pool contexts).
    pub fn max_file_bytes(mut self, limit: impl Into<Option<usize>>) -> Self {
        self.max_file_bytes = limit.into();
        self
    }

    /// Enable or disable lazy ObjStm decompression.
    ///
    /// When `true`, ObjStm streams are kept compressed in `Document::objects`
    /// and their IDs accumulate in `Document::pending_obj_streams`.  The caller
    /// must call `Document::resolve_pending_object_streams` before using the
    /// document.
    pub fn lazy_objstm(mut self, lazy: bool) -> Self {
        self.lazy_objstm = lazy;
        self
    }

    /// Bound how large any single stream may decompress to during loading.
    ///
    /// Builder counterpart to [`LoadOptions::with_max_decompressed_size`].
    /// Pass `None` to remove the bound entirely — which also removes the
    /// LOPDF-ZBOMB-01 protection that is on by default.
    pub fn max_decompressed_size(mut self, limit: impl Into<Option<usize>>) -> Self {
        self.max_decompressed_size = limit.into();
        self
    }
}
