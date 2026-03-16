/// Default maximum input size: 256 MiB.
///
/// Chosen so that a single document load can never exceed ~2 GB of RSS on
/// realistic input (the worst-case amplification ratio measured across the
/// corpus is ~20×, so 256 MiB × 20 ≈ 5 GB — still above the default, but
/// the limit is a first-line guard, not a tight memory cap).  Callers that
/// need larger files can pass a higher limit via `LoadOptions::max_file_bytes`.
pub const DEFAULT_MAX_FILE_BYTES: usize = 256 * 1024 * 1024;

/// Options that control how a PDF document is loaded into memory.
///
/// All options have safe defaults:
/// - `max_file_bytes`: `Some(256 MiB)` — rejects enormous inputs before
///   allocating the full object graph.
/// - `lazy_objstm`: `false` — ObjStm streams are decompressed eagerly, but
///   their container streams are dropped immediately after extraction
///   (saves the decompressed container bytes; Phase 2a optimisation).
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
#[derive(Debug, Clone)]
pub struct LoadOptions {
    /// Maximum allowed size of the input buffer in bytes.
    ///
    /// When `Some(limit)`, `load_mem_with_options` / `load_with_options` return
    /// `Err(Error::DocumentTooLarge)` without allocating the object graph if the
    /// input exceeds `limit`.  `None` disables the check entirely.
    pub(crate) max_file_bytes: Option<usize>,

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
    pub(crate) lazy_objstm: bool,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            max_file_bytes: Some(DEFAULT_MAX_FILE_BYTES),
            lazy_objstm: false,
        }
    }
}

impl LoadOptions {
    /// Create options with default values.
    pub fn new() -> Self {
        Self::default()
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
}
