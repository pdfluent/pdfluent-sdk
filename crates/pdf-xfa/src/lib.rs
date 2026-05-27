#![warn(missing_docs)]
//! XFA engine — extraction, layout rendering, font resolution.
//!
//! This crate's public API is panic-free. Errors are returned as `Result<T, XfaError>`.
//!
//! ## JavaScript policy
//!
//! XFA forms that require JavaScript execution produce static-layout output by default.
//! Full JavaScript execution requires the `xfa-js-sandboxed` feature flag.
//!
//! ## Module boundary notes
//!
//! The `*_bridge` modules (`appearance_bridge`, `font_bridge`, `image_bridge`,
//! `paint_bridge`, `render_bridge`) are `pub` because integration tests and the
//! `xfa-cli` debug command access their items directly by module path. These modules
//! are considered **internal** (not part of the stable API surface) and may change
//! without a semver bump. External callers should prefer the stable re-exports at
//! the crate root. Items not listed in the `pub use` block below are not covered
//! by stability guarantees.
//!
//! `javascript_policy` is similarly `pub` for cross-module visibility within the
//! workspace. It is not intended as a public API for downstream crates.

/// Appearance stream generation for XFA form fields (internal bridge).
pub mod appearance_bridge;
pub mod classify;
/// Dynamic XFA script processing — binding, mode selection, and outcome reporting.
pub mod dynamic;
pub mod error;
pub mod extract;
pub mod flatten;
/// Env-gated flatten traceability (parse/bind/script/layout/paint/writer counts).
mod flatten_trace;
/// Font resolution, embedding, and metrics for the XFA flatten pipeline (internal bridge).
pub mod font_bridge;
/// Image embedding utilities for the XFA flatten pipeline (internal bridge).
pub mod image_bridge;
pub mod javascript_policy;
pub mod js_runtime;
pub mod merger;
/// Paint command abstraction layer between layout and PDF rendering (internal bridge).
pub mod paint_bridge;
/// PDF content-stream rendering from layout DOM (internal bridge).
pub mod render_bridge;
pub mod template_parser;

pub use classify::{detect_xfa_type, detect_xfa_type_from_packets, XfaType};
pub use dynamic::{
    DynamicScriptOutcome, FormDomMatchEntry, InstanceWriteEntry, JsExecutionMode, OutputQuality,
    PresenceMutationEntry, ScriptLifecycleEntry, SkippedActivities, SomFailEntry,
};
pub use extract::{extract_embedded_fonts, validate_xfa_packets, PacketValidation};
pub use flatten::{
    compare_flatten_quality, flatten_xfa_to_pdf, flatten_xfa_to_pdf_with_layout_dump,
    flatten_xfa_to_pdf_with_layout_dump_and_metadata, flatten_xfa_to_pdf_with_metadata,
    flatten_xfa_to_pdf_with_policy, flatten_xfa_to_pdf_with_policy_and_metadata, is_pdf_encrypted,
    validate_flattened_pdf, validate_text_completeness, FlattenMetadata, FlattenQualityMetrics,
    FlattenValidation, LayoutDump, LayoutDumpEntry, TextValidation, XfaRenderingPolicy,
};
pub use js_runtime::{
    activity_allowed_for_sandbox, activity_allowed_for_sandbox_with_gate,
    presave_during_flatten_enabled, HostBindings, MutationLogEntry, NullRuntime, RuntimeMetadata,
    RuntimeOutcome, SandboxError, XfaJsRuntime, DEFAULT_MEMORY_BUDGET_BYTES,
    DEFAULT_TIME_BUDGET_MS, ENV_PRESAVE_DURING_FLATTEN, MAX_INSTANCES_PER_SUBFORM,
    MAX_MUTATIONS_PER_DOC, MAX_RESOLVE_CALLS_PER_SCRIPT, MAX_RESOLVE_RESULTS,
    MAX_SCRIPT_BODY_BYTES, MAX_SOM_DEPTH, MAX_VARIABLES_SCRIPT_BODY_BYTES,
    SANDBOX_ACTIVITY_ALLOWLIST,
};

pub use formcalc_interpreter as formcalc;
pub use xfa_dom_resolver as dom_resolver;
pub use xfa_json as json;
pub use xfa_layout_engine as layout;
