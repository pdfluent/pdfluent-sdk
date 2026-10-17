// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

// `ModelError`'s seven fields are its public contract, so it is 176 bytes and
// every function returning it trips `result_large_err`. Boxing would change
// the field types the contract pins, for no gain: these are cold paths (init
// once per process, cached sessions), where copying 176 bytes on a failure is
// cheaper than the heap allocation boxing would add to every construction.
#![allow(clippy::result_large_err)]

//! The ONE shared ONNX Runtime layer for the opt-in model tiers.
//!
//! Two model tiers need ONNX Runtime: the layout model (balanced/best headings,
//! lists, regions) and the table-structure model (tables), and the paddle OCR
//! path already runs it. A process loads one `onnxruntime` and one `ort`
//! version per dependency graph, so model-file resolution, checksum
//! verification, one-time runtime initialisation, the version check, session
//! caching and the two failure kinds live here, once. Model-specific
//! pre/post-processing is NOT here: it lives in each model feature on top of
//! `SharedSession::run` (behind the `onnx-runtime` feature).
//!
//! # Pin
//!
//! `ort =2.0.0-rc.13` (a release candidate, pinned exactly), `api-23`, ONNX
//! Runtime [`ORT_VERSION`](crate::runtime::ORT_VERSION), loaded dynamically,
//! never linked, never downloaded.
//! Nothing in this module ever fetches over the network: a missing model file
//! or library is [`ModelErrorKind::ModelNotAvailable`](crate::runtime::ModelErrorKind::ModelNotAvailable),
//! never a download.
//!
//! # Resolution
//!
//! The model file is `<dir>/<spec.name>/<spec.file>`, where `<dir>` is the
//! first of: [`RuntimeConfig::model_dir`](crate::runtime::RuntimeConfig::model_dir),
//! the `PDFLUENT_MODEL_DIR`
//! environment variable (the ONE documented override), or the per-user default
//! `<data dir>/pdfluent/models/`. The runtime library resolves the same way:
//! [`RuntimeConfig::ort_dylib`](crate::runtime::RuntimeConfig::ort_dylib), then
//! `ORT_DYLIB_PATH` (`ort`'s own variable),
//! then `<model dir>/onnxruntime-1.23.2/<platform library>`, where the model
//! dir is `PDFLUENT_MODEL_DIR` or the per-user default -- the same root the
//! install commands put the library under, so an install is found wherever
//! the override points. The explicit `model_dir` is deliberately not read
//! for the library (see [`resolve_ort_dylib`]).
//! The data dir is `$XDG_DATA_HOME` or `~/.local/share` on Linux,
//! `~/Library/Application Support` on macOS, `%LOCALAPPDATA%` on Windows,
//! read with `std::env` only.
//!
//! # Threads
//!
//! `intra_threads` x `sessions_per_model` x the caller's page parallelism
//! should not exceed the cores; one caller-facing thread budget (later, the
//! CLI's `--threads`) drives both numbers. There is no automatic pool sizing
//! here: the pool holds exactly
//! [`RuntimeConfig::sessions_per_model`](crate::runtime::RuntimeConfig::sessions_per_model)
//! sessions
//! and `SharedSession::run` (behind the `onnx-runtime` feature) blocks until
//! one is free.
//!
//! # Errors
//!
//! Failures surface as [`ModelError`](crate::runtime::ModelError), which converts into [`std::io::Error`]
//! via [`std::io::Error::other`] so `pdf-engine` can carry it as
//! `EngineError::Io` without a new variant; callers match that variant and
//! find the typed cause with [`model_error`](crate::runtime::model_error). Bindings and the CLI map
//! [`ModelErrorKind::code`](crate::runtime::ModelErrorKind::code) onto their error codes.

#[cfg(feature = "onnx-runtime")]
use std::collections::HashMap;
#[cfg(feature = "onnx-runtime")]
use std::ffi::CStr;
#[cfg(feature = "onnx-runtime")]
use std::path::Path;
use std::path::PathBuf;
#[cfg(feature = "onnx-runtime")]
use std::sync::{Arc, Condvar, Mutex, OnceLock};

/// The exact ONNX Runtime version this layer runs against.
///
/// [`init`] loads the library and then checks the version string it reports
/// against this constant: anything else is
/// [`ModelErrorKind::VersionMismatch`].
pub const ORT_VERSION: &str = "1.23.2";

/// The ONE documented environment override for the model directory.
const MODEL_DIR_ENV: &str = "PDFLUENT_MODEL_DIR";

/// `ort`'s own variable, honoured as the second source for the library path.
const ORT_DYLIB_ENV: &str = "ORT_DYLIB_PATH";

/// The model name [`ModelError`] carries when the missing file is the ONNX
/// Runtime library itself rather than a model weight.
#[cfg(feature = "onnx-runtime")]
const ORT_MODEL_NAME: &str = "onnxruntime-1.23.2";

/// What a model package must provide.
///
/// Declared as a `const` by each model feature; the file is resolved as
/// `<model dir>/<name>/<file>` and verified against `sha256` before loading.
#[non_exhaustive]
pub struct ModelSpec {
    /// Package directory inside the model dir, e.g. `"layout-heron-ep03"`.
    pub name: &'static str,
    /// File name inside the package dir, e.g. `"heron_ep03.onnx"`.
    pub file: &'static str,
    /// Lowercase hex SHA-256 the file must match before it is loaded.
    pub sha256: &'static str,
    /// Shown verbatim in `MODEL_NOT_AVAILABLE`: where to get the package.
    pub install_command: &'static str,
}

impl ModelSpec {
    /// Declare a model package. `const` so model features can hold it as a
    /// `const` rather than building it per call.
    pub const fn new(
        name: &'static str,
        file: &'static str,
        sha256: &'static str,
        install_command: &'static str,
    ) -> Self {
        Self {
            name,
            file,
            sha256,
            install_command,
        }
    }
}

/// How the shared layer resolves files and sizes its pools.
///
/// # Threads
///
/// `intra_threads` x `sessions_per_model` x the caller's page parallelism
/// should not exceed the cores; one caller-facing thread budget (later, the
/// CLI's `--threads`) drives both numbers. There is no automatic pool sizing:
/// what is set here is what runs.
#[non_exhaustive]
pub struct RuntimeConfig {
    /// Explicit model dir; wins over the env var and the default. Model
    /// files only: the runtime library never reads this field (see
    /// [`resolve_ort_dylib`]).
    pub model_dir: Option<PathBuf>,
    /// Explicit ONNX Runtime library; wins over the env var and the default.
    pub ort_dylib: Option<PathBuf>,
    /// ONNX Runtime intra-op threads per session. Default 4.
    pub intra_threads: usize,
    /// Sessions per model: a pool so parallel pages/tables do not serialise
    /// on one `Mutex`. Default 1. `0` is clamped to 1 (see below).
    pub sessions_per_model: usize,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            model_dir: None,
            ort_dylib: None,
            intra_threads: 4,
            sessions_per_model: 1,
        }
    }
}

impl RuntimeConfig {
    /// A default config: 4 intra threads, a pool of 1, resolved paths.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the explicit model dir; wins over everything.
    pub fn with_model_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.model_dir = Some(dir.into());
        self
    }

    /// Set the explicit ONNX Runtime library; wins over everything.
    pub fn with_ort_dylib(mut self, lib: impl Into<PathBuf>) -> Self {
        self.ort_dylib = Some(lib.into());
        self
    }

    /// Set the ONNX Runtime intra-op threads per session.
    pub fn with_intra_threads(mut self, threads: usize) -> Self {
        self.intra_threads = threads;
        self
    }

    /// Set the sessions per model.
    ///
    /// A pool so parallel pages/tables do not serialise on one `Mutex`;
    /// `SharedSession::run` blocks until a session is free. `0` would block
    /// forever (a pool that can never hold a session), so it is clamped to 1
    /// when the pool is built -- the only automatic sizing in this module, and
    /// a degenerate-input guard rather than a policy.
    pub fn with_sessions_per_model(mut self, sessions: usize) -> Self {
        self.sessions_per_model = sessions;
        self
    }

    /// The pool size that is actually built: the configured value, at least 1.
    #[cfg(feature = "onnx-runtime")]
    fn effective_pool(&self) -> usize {
        self.sessions_per_model.max(1)
    }
}

/// What went wrong, in one of the two failure kinds the layer reports.
///
/// `#[non_exhaustive]`: new 1.x failure cases arrive as new variants, matched
/// with a fallback arm.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelErrorKind {
    /// The model dir/file or the runtime library is missing. Nothing is ever
    /// fetched to make it appear.
    ModelNotAvailable,
    /// The file is present but its SHA-256 differs from the spec.
    ChecksumMismatch,
    /// The library loads but reports a version other than [`ORT_VERSION`].
    VersionMismatch,
    /// A second [`init`] resolving to a different library path. The process
    /// keeps the first library; the call fails rather than switching.
    ConflictingInit,
    /// Anything else: the library would not load, a session would not build,
    /// an inference call failed. The detail string says what.
    Runtime,
}

impl ModelErrorKind {
    /// The stable code bindings and the CLI map onto their error codes.
    pub fn code(self) -> &'static str {
        match self {
            Self::ModelNotAvailable => "MODEL_NOT_AVAILABLE",
            Self::ChecksumMismatch => "MODEL_CHECKSUM_MISMATCH",
            Self::VersionMismatch => "VERSION_MISMATCH",
            Self::ConflictingInit => "RUNTIME_CONFLICTING_INIT",
            Self::Runtime => "MODEL_RUNTIME",
        }
    }
}

/// A shared-layer failure.
///
/// Carries the structured fields callers branch on; converts into
/// [`std::io::Error`] via [`std::io::Error::other`] so `pdf-engine` surfaces
/// it as `EngineError::Io` without a new variant, downcastable through
/// [`model_error`].
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct ModelError {
    /// What went wrong.
    pub kind: ModelErrorKind,
    /// The spec name, or `"onnxruntime-1.23.2"` when the missing or
    /// mismatched file is the runtime library itself.
    pub model: Option<String>,
    /// The digest the file was checked against ([`ModelErrorKind::ChecksumMismatch`]).
    pub expected_sha256: Option<String>,
    /// The path looked at: the model file, or the library.
    pub path: Option<PathBuf>,
    /// The spec's install command, shown verbatim when the model is missing.
    pub install_command: Option<String>,
    /// The runtime version required ([`ModelErrorKind::VersionMismatch`]).
    pub expected_ort: Option<String>,
    /// The runtime version the library reported ([`ModelErrorKind::VersionMismatch`]).
    pub found_ort: Option<String>,
    /// The unstructured remainder: inference failures, load errors, the paths
    /// a conflicting init named.
    detail: String,
}

impl ModelError {
    /// The tier is not compiled into this build: a model tier was asked for on
    /// a build without the cargo feature that runs it (#641).
    ///
    /// [`ModelErrorKind::ModelNotAvailable`], naming the feature to rebuild
    /// with; the [`Display`](std::fmt::Display) appends the spec's install
    /// command, so the message names both the feature and the install.
    pub fn feature_not_compiled(spec: &ModelSpec, feature: &'static str) -> Self {
        Self {
            kind: ModelErrorKind::ModelNotAvailable,
            model: Some(spec.name.to_string()),
            expected_sha256: None,
            path: None,
            install_command: Some(spec.install_command.to_string()),
            expected_ort: None,
            found_ort: None,
            detail: format!(
                "model '{}' is not available: this build lacks the `{feature}` cargo feature \
                 (rebuild with `--features {feature}`)",
                spec.name
            ),
        }
    }

    /// The model file or dir is missing. The display names the path looked at
    /// and the install command, verbatim.
    #[cfg(feature = "onnx-runtime")]
    fn model_missing(spec: &ModelSpec, path: &Path) -> Self {
        Self {
            kind: ModelErrorKind::ModelNotAvailable,
            model: Some(spec.name.to_string()),
            expected_sha256: None,
            path: Some(path.to_path_buf()),
            install_command: Some(spec.install_command.to_string()),
            expected_ort: None,
            found_ort: None,
            detail: format!(
                "model '{}' is not available: {} was not found",
                spec.name,
                path.display()
            ),
        }
    }

    /// The runtime library is missing. `model` names the runtime, not a model.
    #[cfg(feature = "onnx-runtime")]
    fn runtime_missing(path: &Path) -> Self {
        Self {
            kind: ModelErrorKind::ModelNotAvailable,
            model: Some(ORT_MODEL_NAME.to_string()),
            expected_sha256: None,
            path: Some(path.to_path_buf()),
            install_command: None,
            expected_ort: None,
            found_ort: None,
            detail: format!(
                "model '{ORT_MODEL_NAME}' is not available: {} was not found. \
                 Place ONNX Runtime {ORT_VERSION} there, or point {ORT_DYLIB_ENV} \
                 at the library",
                path.display()
            ),
        }
    }

    /// The file is present but hashes differently. Carries the expectation and
    /// the path; the actual digest rides in the detail.
    #[cfg(feature = "onnx-runtime")]
    fn checksum_mismatch(spec: &ModelSpec, path: &Path, actual_sha256: &str) -> Self {
        Self {
            kind: ModelErrorKind::ChecksumMismatch,
            model: Some(spec.name.to_string()),
            expected_sha256: Some(spec.sha256.to_string()),
            path: Some(path.to_path_buf()),
            install_command: Some(spec.install_command.to_string()),
            expected_ort: None,
            found_ort: None,
            detail: actual_sha256.to_string(),
        }
    }

    /// The library loads but reports another version.
    #[cfg(feature = "onnx-runtime")]
    fn version_mismatch(path: &Path, found: &str) -> Self {
        Self {
            kind: ModelErrorKind::VersionMismatch,
            model: Some(ORT_MODEL_NAME.to_string()),
            expected_sha256: None,
            path: Some(path.to_path_buf()),
            install_command: None,
            expected_ort: Some(ORT_VERSION.to_string()),
            found_ort: Some(found.to_string()),
            detail: format!(
                "expected ONNX Runtime {ORT_VERSION}, found '{found}' at {}",
                path.display()
            ),
        }
    }

    /// A second init resolving to a different library. The process keeps the
    /// first; `path` is the rejected one.
    #[cfg(feature = "onnx-runtime")]
    fn conflicting_init(requested: &Path, current: &Path) -> Self {
        Self {
            kind: ModelErrorKind::ConflictingInit,
            model: Some(ORT_MODEL_NAME.to_string()),
            expected_sha256: None,
            path: Some(requested.to_path_buf()),
            install_command: None,
            expected_ort: None,
            found_ort: None,
            detail: format!(
                "ONNX Runtime is already initialised from {} and cannot also use {} \
                 in this process",
                current.display(),
                requested.display()
            ),
        }
    }

    /// An unstructured runtime failure: the detail says what.
    #[cfg(feature = "onnx-runtime")]
    fn runtime(detail: impl Into<String>) -> Self {
        Self {
            kind: ModelErrorKind::Runtime,
            model: None,
            expected_sha256: None,
            path: None,
            install_command: None,
            expected_ort: None,
            found_ort: None,
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            ModelErrorKind::ModelNotAvailable => {
                write!(f, "{}", self.detail)?;
                if let Some(cmd) = &self.install_command {
                    write!(f, ". Install it with: {cmd}")?;
                }
                Ok(())
            }
            ModelErrorKind::ChecksumMismatch => {
                let expected = self.expected_sha256.as_deref().unwrap_or("<unknown>");
                let path = self
                    .path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "<unknown>".to_string());
                write!(
                    f,
                    "model '{}' failed its checksum: {path} has sha256 {}, expected {expected}",
                    self.model.as_deref().unwrap_or("<unknown>"),
                    self.detail,
                )?;
                if let Some(cmd) = &self.install_command {
                    write!(f, ". Reinstall it with: {cmd}")?;
                }
                Ok(())
            }
            ModelErrorKind::VersionMismatch
            | ModelErrorKind::ConflictingInit
            | ModelErrorKind::Runtime => {
                write!(f, "{}", self.detail)
            }
        }
    }
}

impl std::error::Error for ModelError {}

impl From<ModelError> for std::io::Error {
    fn from(e: ModelError) -> Self {
        std::io::Error::other(e)
    }
}

/// Find the [`ModelError`] in an error chain, if there is one.
///
/// `pdf-engine` surfaces these as `EngineError::Io`: match that variant and
/// walk the inner [`std::io::Error`] with this helper, then branch on
/// [`ModelErrorKind::code`]. The walk must start at the `io::Error`, not above
/// it: `EngineError::Io` is `thiserror::transparent`, which forwards
/// `source()` to the inner error's own `source()` -- and that is `None` for
/// an [`std::io::Error::other`] payload, which is exactly how this error
/// travels. So every `io::Error` on the way is also opened through `get_ref()`,
/// the same way `EngineError::ocr_message` reads its OCR causes.
pub fn model_error<'a>(mut e: &'a (dyn std::error::Error + 'static)) -> Option<&'a ModelError> {
    loop {
        if let Some(found) = e.downcast_ref::<ModelError>() {
            return Some(found);
        }
        if let Some(io) = e.downcast_ref::<std::io::Error>() {
            if let Some(inner) = io.get_ref() {
                if let Some(found) = model_error(inner) {
                    return Some(found);
                }
            }
        }
        e = e.source()?;
    }
}

/// The library the process initialised with, once [`init`] has succeeded.
/// Compared by canonical path, so two spellings of the same file agree.
#[cfg(feature = "onnx-runtime")]
static INITIALIZED_DYLIB: OnceLock<PathBuf> = OnceLock::new();

/// Serialises [`init`] bodies: two threads initialising concurrently with
/// different libraries must come out as one `Ok` and one [`ModelErrorKind::ConflictingInit`],
/// never as two `Ok`s with `ort` silently keeping the first load.
#[cfg(feature = "onnx-runtime")]
static INIT_LOCK: Mutex<()> = Mutex::new(());

/// Initialise ONNX Runtime once per process.
///
/// IDEMPOTENT: a later call resolving to the SAME library returns `Ok(())`,
/// so callers need no once-guard of their own; only a call resolving to a
/// DIFFERENT library path fails, with [`ModelErrorKind::ConflictingInit`].
/// Loads the library, then checks the version string it reports against
/// [`ORT_VERSION`] exactly.
#[cfg(feature = "onnx-runtime")]
pub fn init(config: &RuntimeConfig) -> Result<(), ModelError> {
    let _held = match INIT_LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let resolved = resolve_ort_dylib(config);
    if let Some(current) = INITIALIZED_DYLIB.get() {
        if same_library(&resolved, current) {
            return Ok(());
        }
        // A path that is not there is not "a different library": it names
        // nothing to conflict with, so it stays ModelNotAvailable however the
        // process initialised.
        if !resolved.exists() {
            return Err(ModelError::runtime_missing(&resolved));
        }
        return Err(ModelError::conflicting_init(&resolved, current));
    }
    if !resolved.exists() {
        return Err(ModelError::runtime_missing(&resolved));
    }
    let found = probe_ort_version(&resolved)?;
    if check_ort_version(&found).is_err() {
        return Err(ModelError::version_mismatch(&resolved, &found));
    }
    ort::init_from(&resolved)
        .map_err(|e| {
            ModelError::runtime(format!(
                "could not load ONNX Runtime from {}: {e}",
                resolved.display()
            ))
        })?
        .commit();
    match INITIALIZED_DYLIB.set(canonical_or_raw(&resolved)) {
        Ok(()) => Ok(()),
        Err(_) => match INITIALIZED_DYLIB.get() {
            // Unreachable under INIT_LOCK, but a lock is not a proof: answer
            // from the recorded path rather than assuming which thread won.
            Some(current) if same_library(&resolved, current) => Ok(()),
            Some(current) => Err(ModelError::conflicting_init(&resolved, current)),
            None => Ok(()),
        },
    }
}

/// A session builder through the shared init path.
///
/// Calls [`init`], then returns `ort`'s builder with the graph optimisation
/// level and intra threads this layer standardises on (Level3, and the
/// config's `intra_threads`). The paddle backend builds its sessions through
/// this so there is one init path.
#[cfg(feature = "onnx-runtime")]
pub fn builder(
    config: &RuntimeConfig,
) -> Result<ort::session::builder::SessionBuilder, ModelError> {
    init(config)?;
    let builder = ort::session::Session::builder().map_err(|e| {
        ModelError::runtime(format!("could not create an ONNX session builder: {e}"))
    })?;
    let builder = builder
        .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
        .map_err(|e| {
            ModelError::runtime(format!("could not set graph optimisation Level3: {e}"))
        })?;
    builder
        .with_intra_threads(config.intra_threads)
        .map_err(|e| {
            ModelError::runtime(format!(
                "could not set intra_threads={}: {e}",
                config.intra_threads
            ))
        })
}

/// Resolve + verify + load a model, cached process-wide.
///
/// Calls [`init`] if needed. The model file is checked before the library:
/// a missing model is `ModelNotAvailable` naming the model even where no
/// runtime is installed. The SHA-256 is verified by streaming the file, and at
/// most once per process per file: a second `session` for the same key returns
/// the same `Arc` without re-reading the file.
#[cfg(feature = "onnx-runtime")]
pub fn session(spec: &ModelSpec, config: &RuntimeConfig) -> Result<Arc<SharedSession>, ModelError> {
    let model_path = resolve_model_file(spec, config);
    let key = CacheKey::for_request(spec, config, &model_path);
    if let Some(hit) = cache_get(&key) {
        return Ok(hit);
    }
    if !model_path.exists() {
        return Err(ModelError::model_missing(spec, &model_path));
    }
    verify_model_file(spec, &model_path)?;
    init(config)?;
    let pool = build_pool(&model_path, spec, config)?;
    Ok(cache_insert(key, pool))
}

/// Hash the model file by streaming it and compare against the spec.
///
/// The file is read 64 KiB at a time, never held whole: a 300 MB weight must
/// not sit in memory twice (here and in the session).
#[cfg(feature = "onnx-runtime")]
fn verify_model_file(spec: &ModelSpec, path: &Path) -> Result<(), ModelError> {
    let mut file = std::fs::File::open(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ModelError::model_missing(spec, path)
        } else {
            ModelError::runtime(format!("could not read {}: {e}", path.display()))
        }
    })?;
    match crate::integrity::verify_reader_sha256(&mut file, spec.sha256) {
        Ok(()) => Ok(()),
        Err(crate::integrity::StreamIntegrityError::Io(e)) => Err(ModelError::runtime(format!(
            "could not read {}: {e}",
            path.display()
        ))),
        Err(crate::integrity::StreamIntegrityError::Integrity(
            crate::integrity::IntegrityError::DigestMismatch { actual, .. },
        )) => Err(ModelError::checksum_mismatch(spec, path, &actual)),
        Err(crate::integrity::StreamIntegrityError::Integrity(
            crate::integrity::IntegrityError::MalformedDigest(digest),
        )) => Err(ModelError::runtime(format!(
            "model '{}' pins a malformed sha256 {digest:?}: a sha256 is 64 hex characters",
            spec.name
        ))),
    }
}

/// Load `sessions_per_model` sessions for one verified file.
#[cfg(feature = "onnx-runtime")]
fn build_pool(
    model_path: &Path,
    spec: &ModelSpec,
    config: &RuntimeConfig,
) -> Result<SharedSession, ModelError> {
    let mut sessions = Vec::with_capacity(config.effective_pool());
    for _ in 0..config.effective_pool() {
        let mut session_builder = builder(config)?;
        let session = session_builder.commit_from_file(model_path).map_err(|e| {
            ModelError::runtime(format!(
                "could not load model '{}' from {}: {e}",
                spec.name,
                model_path.display()
            ))
        })?;
        sessions.push(session);
    }
    Ok(SharedSession::new(sessions, spec.sha256))
}

/// A loaded model: a pool of [`RuntimeConfig::sessions_per_model`] `ort`
/// sessions behind one handle.
///
/// `Send + Sync`. [`run`](Self::run) takes a FREE session (blocking until one
/// is free -- `ort` 2.0.0-rc.13's `Session::run` takes `&mut self`) and
/// returns it afterwards, including when the closure fails or panics. Results
/// depend only on the inputs: no state is carried between calls.
#[cfg(feature = "onnx-runtime")]
pub struct SharedSession {
    /// Idle sessions. Never empty past construction: [`run`](Self::run) waits
    /// on `available` while it is, and every taken session is returned.
    pool: Mutex<Vec<ort::session::Session>>,
    /// Signalled whenever a session is returned to the pool.
    available: Condvar,
    /// The verified digest of the loaded file, from the spec.
    model_sha256: String,
}

#[cfg(feature = "onnx-runtime")]
impl std::fmt::Debug for SharedSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedSession")
            .field("model_sha256", &self.model_sha256)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "onnx-runtime")]
impl SharedSession {
    /// Hold already-loaded sessions. Private: sessions only enter the pool
    /// through [`session`], after the init, version and checksum checks.
    fn new(sessions: Vec<ort::session::Session>, sha256: &str) -> Self {
        Self {
            pool: Mutex::new(sessions),
            available: Condvar::new(),
            model_sha256: sha256.to_string(),
        }
    }

    /// Run an inference closure on a free session, blocking until one is free.
    ///
    /// The session is returned to the pool afterwards, whether the closure
    /// fails or succeeds; an `ort` failure becomes
    /// [`ModelErrorKind::Runtime`].
    pub fn run<R>(
        &self,
        f: impl FnOnce(&mut ort::session::Session) -> ort::Result<R>,
    ) -> Result<R, ModelError> {
        let session = self.take();
        let mut returned = SessionReturn {
            pool: &self.pool,
            notify: &self.available,
            session: Some(session),
        };
        // Unreachable `None`: the slot is `Some` until `release` below, and
        // this is the only use between construction and release.
        let slot = match returned.session.as_mut() {
            Some(slot) => slot,
            None => unreachable!("the taken session is held until release"),
        };
        let result = f(slot).map_err(|e| ModelError::runtime(e.to_string()));
        returned.release();
        result
    }

    /// The verified SHA-256 of the loaded model file, from the spec.
    pub fn model_sha256(&self) -> &str {
        &self.model_sha256
    }

    /// Take an idle session, blocking while the pool is empty.
    fn take(&self) -> ort::session::Session {
        let mut guard = match self.pool.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        loop {
            if let Some(session) = guard.pop() {
                return session;
            }
            guard = match self.available.wait(guard) {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
        }
    }
}

/// Returns the taken session to the pool on the way out, including when the
/// inference closure panics: without this a panic would shrink the pool by one
/// per unwind, and a pool that has shrunk to nothing blocks forever.
#[cfg(feature = "onnx-runtime")]
struct SessionReturn<'a> {
    pool: &'a Mutex<Vec<ort::session::Session>>,
    notify: &'a Condvar,
    session: Option<ort::session::Session>,
}

#[cfg(feature = "onnx-runtime")]
impl SessionReturn<'_> {
    fn release(&mut self) {
        if let Some(session) = self.session.take() {
            let mut guard = match self.pool.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            guard.push(session);
            self.notify.notify_one();
        }
    }
}

#[cfg(feature = "onnx-runtime")]
impl Drop for SessionReturn<'_> {
    fn drop(&mut self) {
        self.release();
    }
}

/// The process-wide session cache: the verified SHA-256 plus the thread and
/// pool sizes, with the canonical model path so two paths can never alias one
/// entry.
#[cfg(feature = "onnx-runtime")]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    path: PathBuf,
    sha256: String,
    intra_threads: usize,
    sessions_per_model: usize,
}

#[cfg(feature = "onnx-runtime")]
impl CacheKey {
    fn for_request(spec: &ModelSpec, config: &RuntimeConfig, model_path: &Path) -> Self {
        Self {
            path: canonical_or_raw(model_path),
            sha256: spec.sha256.trim().to_ascii_lowercase(),
            intra_threads: config.intra_threads,
            sessions_per_model: config.effective_pool(),
        }
    }
}

#[cfg(feature = "onnx-runtime")]
static SESSION_CACHE: std::sync::LazyLock<Mutex<HashMap<CacheKey, Arc<SharedSession>>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// Look up the cache. A poisoned mutex still answers: its map is intact (the
/// critical sections never panic), so the lock is recovered rather than
/// treated as a failure.
#[cfg(feature = "onnx-runtime")]
fn cache_lock() -> std::sync::MutexGuard<'static, HashMap<CacheKey, Arc<SharedSession>>> {
    match SESSION_CACHE.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[cfg(feature = "onnx-runtime")]
fn cache_get(key: &CacheKey) -> Option<Arc<SharedSession>> {
    cache_lock().get(key).cloned()
}

/// Insert a freshly loaded pool, or return the winner when two threads loaded
/// the same key concurrently: every caller for one key shares one `Arc`.
#[cfg(feature = "onnx-runtime")]
fn cache_insert(key: CacheKey, pool: SharedSession) -> Arc<SharedSession> {
    let mut cache = cache_lock();
    cache.entry(key).or_insert_with(|| Arc::new(pool)).clone()
}

/// Resolve the model root: explicit dir, then `PDFLUENT_MODEL_DIR`, then the
/// per-user default. An empty env var counts as unset, matching `ort`'s own
/// reading of `ORT_DYLIB_PATH`.
///
/// Public so the install commands put every package file -- models and the
/// root-level notices -- under the one root the loader reads.
pub fn resolve_model_root(config: &RuntimeConfig) -> PathBuf {
    if let Some(dir) = &config.model_dir {
        dir.clone()
    } else if let Some(dir) = env_dir(MODEL_DIR_ENV) {
        dir
    } else {
        default_model_root()
    }
}

/// Resolve the model file: [`resolve_model_root`] plus
/// `<spec.name>/<spec.file>`, reported even when it is not there.
///
/// Public so the install commands put the file exactly where the session
/// loader looks for it.
pub fn resolve_model_file(spec: &ModelSpec, config: &RuntimeConfig) -> PathBuf {
    resolve_model_root(config).join(spec.name).join(spec.file)
}

/// Resolve the runtime library: explicit path, then `ORT_DYLIB_PATH`, then
/// `<model dir>/onnxruntime-<version>/<platform library>`, where the model
/// dir is `PDFLUENT_MODEL_DIR` or the per-user default.
///
/// Public for the same reason as [`resolve_model_file`]: the installer and
/// the loader must agree on `<dir>/onnxruntime-<version>/<platform library>`,
/// under `PDFLUENT_MODEL_DIR` as much as under the default -- otherwise an
/// install into a redirected root would leave a library the loader never
/// finds. The explicit `model_dir` is deliberately NOT read here: it is a
/// per-call model override, and letting it move the process-wide runtime
/// would break `extract --model-dir <weights>` wherever the library sits at
/// its installed default.
pub fn resolve_ort_dylib(config: &RuntimeConfig) -> PathBuf {
    if let Some(lib) = &config.ort_dylib {
        return lib.clone();
    }
    if let Ok(path) = std::env::var(ORT_DYLIB_ENV) {
        if !path.is_empty() {
            return PathBuf::from(path);
        }
    }
    let root = env_dir(MODEL_DIR_ENV).unwrap_or_else(default_model_root);
    root.join(format!("onnxruntime-{ORT_VERSION}"))
        .join(platform_lib_name())
}

/// A non-empty directory from the environment, or nothing.
fn env_dir(var: &str) -> Option<PathBuf> {
    match std::env::var(var) {
        Ok(dir) if !dir.is_empty() => Some(PathBuf::from(dir)),
        _ => None,
    }
}

/// The per-user model root: `<data dir>/pdfluent/models/`, where the install
/// commands put packages.
///
/// Public so installers and status commands name the same directory the
/// resolution falls back to. This is the default only: an explicit dir and
/// `PDFLUENT_MODEL_DIR` still win (see [`resolve_model_file`]).
pub fn default_model_root() -> PathBuf {
    user_data_dir().join("pdfluent").join("models")
}

/// The platform data dir via `std::env` only: `$XDG_DATA_HOME` or
/// `~/.local/share` on Linux, `~/Library/Application Support` on macOS,
/// `%LOCALAPPDATA%` on Windows. Without a home to read, the temp dir: a
/// default that cannot be written is worse than a surprising one.
fn user_data_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(dir) = std::env::var("LOCALAPPDATA") {
            if !dir.is_empty() {
                return PathBuf::from(dir);
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            if !home.is_empty() {
                return PathBuf::from(home)
                    .join("Library")
                    .join("Application Support");
            }
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
            if !dir.is_empty() {
                return PathBuf::from(dir);
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            if !home.is_empty() {
                return PathBuf::from(home).join(".local").join("share");
            }
        }
    }
    std::env::temp_dir()
}

/// The ONNX Runtime library file name for this platform.
fn platform_lib_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "onnxruntime.dll"
    }
    #[cfg(target_os = "macos")]
    {
        "libonnxruntime.dylib"
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        "libonnxruntime.so"
    }
}

/// Whether two library paths name the same file. Canonical paths compare, so
/// two spellings of one library agree; when either side cannot canonicalise
/// (a file that is not there), the raw paths compare instead.
#[cfg(feature = "onnx-runtime")]
fn same_library(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Canonicalise for cache keys and init records, falling back to the raw path
/// when the file is not there (yet).
#[cfg(feature = "onnx-runtime")]
fn canonical_or_raw(path: &Path) -> PathBuf {
    match std::fs::canonicalize(path) {
        Ok(canonical) => canonical,
        Err(_) => path.to_path_buf(),
    }
}

/// Read the version string the library reports, through a temporary handle.
///
/// `ort` exposes no plain version of its own, so the layer opens the library
/// with the same dlopen wrapper `ort` loads it through, calls
/// `OrtGetApiBase`/`GetVersionString`, copies the string out, and drops the
/// handle again; [`init`] then hands the path to `ort`, which opens its own
/// handle. The double open is one `dlopen` per process, against a check that
/// must speak before the library is committed to.
#[cfg(feature = "onnx-runtime")]
fn probe_ort_version(dylib: &Path) -> Result<String, ModelError> {
    // SAFETY: mapping the file runs no code; the looked-up symbol is only
    // called below, with its documented signature.
    let lib = unsafe { libloading::Library::new(dylib) }.map_err(|e| {
        ModelError::runtime(format!(
            "could not load ONNX Runtime from {}: {e}",
            dylib.display()
        ))
    })?;
    type ApiBaseGetter = unsafe extern "C" fn() -> *const ort::sys::OrtApiBase;
    // SAFETY: `OrtGetApiBase` is ONNX Runtime's documented entry point with
    // this C signature; a library without it is rejected, not called.
    let getter: libloading::Symbol<ApiBaseGetter> =
        unsafe { lib.get(b"OrtGetApiBase") }.map_err(|e| {
            ModelError::runtime(format!(
                "{} is not an ONNX Runtime library (no OrtGetApiBase): {e}",
                dylib.display()
            ))
        })?;
    // SAFETY: the getter takes no arguments and returns a pointer that is
    // valid while the library is mapped, which outlives this call.
    let base = unsafe { getter() };
    if base.is_null() {
        return Err(ModelError::runtime(format!(
            "{} returned a null OrtApiBase",
            dylib.display()
        )));
    }
    // SAFETY: `GetVersionString` returns a static NUL-terminated UTF-8 string
    // the caller must not free; it is copied into an owned `String` before the
    // temporary handle drops.
    let raw = unsafe { ((*base).GetVersionString)() };
    if raw.is_null() {
        return Err(ModelError::runtime(format!(
            "{} returned a null version string",
            dylib.display()
        )));
    }
    Ok(unsafe { CStr::from_ptr(raw) }
        .to_string_lossy()
        .into_owned())
}

/// The version comparison: the reported string must equal [`ORT_VERSION`]
/// exactly -- no prefix match, no minor-only check.
#[cfg(feature = "onnx-runtime")]
fn check_ort_version(found: &str) -> Result<(), ModelErrorKind> {
    if found == ORT_VERSION {
        Ok(())
    } else {
        Err(ModelErrorKind::VersionMismatch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "onnx-runtime")]
    #[test]
    fn the_pinned_version_passes_and_everything_else_fails() {
        assert_eq!(check_ort_version("1.23.2"), Ok(()));
        for found in [
            "1.23.1", "1.24.0", "1.23.20", "2.0.0", "", "1.23.2\n", " 1.23.2",
        ] {
            assert_eq!(
                check_ort_version(found),
                Err(ModelErrorKind::VersionMismatch),
                "{found:?} must not pass as {ORT_VERSION}"
            );
        }
    }

    #[test]
    fn error_codes_are_the_published_strings() {
        assert_eq!(
            ModelErrorKind::ModelNotAvailable.code(),
            "MODEL_NOT_AVAILABLE"
        );
        assert_eq!(
            ModelErrorKind::ChecksumMismatch.code(),
            "MODEL_CHECKSUM_MISMATCH"
        );
        assert_eq!(ModelErrorKind::VersionMismatch.code(), "VERSION_MISMATCH");
        assert_eq!(
            ModelErrorKind::ConflictingInit.code(),
            "RUNTIME_CONFLICTING_INIT"
        );
        assert_eq!(ModelErrorKind::Runtime.code(), "MODEL_RUNTIME");
    }

    #[cfg(feature = "onnx-runtime")]
    #[test]
    fn a_zero_pool_is_clamped_to_one() {
        let config = RuntimeConfig::new().with_sessions_per_model(0);
        assert_eq!(config.effective_pool(), 1);
        let config = RuntimeConfig::new().with_sessions_per_model(3);
        assert_eq!(config.effective_pool(), 3);
    }

    #[test]
    fn a_build_without_the_tier_names_the_feature_and_the_install() {
        // The `Quality::Balanced` error on a build without `ort-layout`
        // (#641): ModelNotAvailable, naming the cargo feature to rebuild
        // with and the spec's install command. Runs without the feature --
        // that is the build it reports on.
        let spec = ModelSpec::new(
            "layout-heron-ep03",
            "heron_ep03.onnx",
            "b7cc5d31f3223c74cc477a1386b894793618aeb398ca59cf5eb7264ec61d8afa",
            "pdfluent models install layout",
        );
        let error = ModelError::feature_not_compiled(&spec, "ort-layout");
        assert_eq!(error.kind, ModelErrorKind::ModelNotAvailable);
        assert_eq!(error.kind.code(), "MODEL_NOT_AVAILABLE");
        let message = error.to_string();
        assert!(
            message.contains("`ort-layout`"),
            "the message names the cargo feature: {message}"
        );
        assert!(
            message.contains("pdfluent models install layout"),
            "the message names the install command: {message}"
        );
        // And it travels the shared path: io::Error::other, downcastable
        // through model_error.
        let io = std::io::Error::from(error);
        let back = model_error(&io).expect("the chain carries the ModelError");
        assert_eq!(back.kind, ModelErrorKind::ModelNotAvailable);
    }

    #[test]
    fn resolution_honours_the_explicit_dir_over_everything() {
        // The precedence the balanced tier relies on: the explicit model dir
        // wins over the environment and the default, so a caller -- and a
        // test -- can point the tier at a scratch directory.
        let spec = ModelSpec::new("layout-heron-ep03", "heron_ep03.onnx", "0", "install");
        let config = RuntimeConfig::new().with_model_dir("/tmp/ort-explicit-test");
        assert_eq!(
            resolve_model_file(&spec, &config),
            std::path::PathBuf::from("/tmp/ort-explicit-test/layout-heron-ep03/heron_ep03.onnx")
        );
    }

    #[test]
    fn the_dylib_fallback_reads_the_model_dir_env() {
        // The installer and the loader agree on the library path under a
        // redirected root: `PDFLUENT_MODEL_DIR` moves the
        // `onnxruntime-<version>` dir as much as the model packages. The
        // ONLY test in this target that touches the environment: every other
        // test resolves explicitly, so no parallel test observes it.
        struct Restore {
            key: &'static str,
            previous: Option<String>,
        }
        impl Drop for Restore {
            fn drop(&mut self) {
                match &self.previous {
                    Some(value) => std::env::set_var(self.key, value),
                    None => std::env::remove_var(self.key),
                }
            }
        }
        let _model_dir = Restore {
            key: MODEL_DIR_ENV,
            previous: std::env::var(MODEL_DIR_ENV).ok(),
        };
        let _dylib = Restore {
            key: ORT_DYLIB_ENV,
            previous: std::env::var(ORT_DYLIB_ENV).ok(),
        };
        std::env::remove_var(ORT_DYLIB_ENV);
        let from_env = std::env::temp_dir().join("ort-env-dylib-test");
        std::env::set_var(MODEL_DIR_ENV, &from_env);
        let lib = format!("onnxruntime-{ORT_VERSION}");
        assert_eq!(
            resolve_ort_dylib(&RuntimeConfig::new()),
            from_env.join(&lib).join(platform_lib_name()),
            "the env root carries the library dir"
        );
        // `ort`'s own variable still wins over the model root.
        std::env::set_var(ORT_DYLIB_ENV, "/tmp/ort-explicit-test/libonnxruntime.so");
        assert_eq!(
            resolve_ort_dylib(&RuntimeConfig::new()),
            std::path::PathBuf::from("/tmp/ort-explicit-test/libonnxruntime.so"),
            "ORT_DYLIB_PATH wins over the model root"
        );
        // An empty override counts as unset, like everywhere else here.
        std::env::remove_var(ORT_DYLIB_ENV);
        std::env::set_var(MODEL_DIR_ENV, "");
        assert_eq!(
            resolve_ort_dylib(&RuntimeConfig::new()),
            default_model_root().join(&lib).join(platform_lib_name()),
            "an empty model dir falls back to the default"
        );
        // The explicit per-call model dir does NOT move the runtime: it is a
        // model override, and the library stays where it was installed.
        std::env::remove_var(MODEL_DIR_ENV);
        let config = RuntimeConfig::new().with_model_dir("/tmp/ort-explicit-test");
        assert_eq!(
            resolve_ort_dylib(&config),
            default_model_root().join(&lib).join(platform_lib_name()),
            "the explicit model dir leaves the library at its default"
        );
    }
}
