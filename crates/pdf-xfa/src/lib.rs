//! XFA engine — extraction, layout rendering, font resolution.

pub mod appearance_bridge;
pub mod classify;
pub mod dynamic;
pub mod error;
pub mod extract;
pub mod flatten;
pub mod font_bridge;
pub mod image_bridge;
pub mod javascript_policy;
pub mod merger;
pub mod paint_bridge;
pub mod render_bridge;
pub mod template_parser;

pub use classify::{detect_xfa_type, detect_xfa_type_from_packets, XfaType};
pub use dynamic::{DynamicScriptOutcome, JsExecutionMode, OutputQuality};
pub use extract::{validate_xfa_packets, PacketValidation};
pub use flatten::{
    compare_flatten_quality, flatten_xfa_to_pdf, flatten_xfa_to_pdf_with_layout_dump,
    flatten_xfa_to_pdf_with_layout_dump_and_metadata, flatten_xfa_to_pdf_with_metadata,
    is_pdf_encrypted, validate_flattened_pdf, validate_text_completeness, FlattenMetadata,
    FlattenQualityMetrics, FlattenValidation, LayoutDump, LayoutDumpEntry, TextValidation,
};

pub use formcalc_interpreter as formcalc;
pub use xfa_dom_resolver as dom_resolver;
pub use xfa_json as json;
pub use xfa_layout_engine as layout;
