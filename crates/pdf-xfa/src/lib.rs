//! XFA engine — extraction, layout rendering, font resolution.

pub mod appearance_bridge;
pub mod classify;
pub mod dynamic;
pub mod error;
pub mod extract;
pub mod flatten;
pub mod font_bridge;
pub mod image_bridge;
pub mod merger;
pub mod paint_bridge;
pub mod render_bridge;
pub mod template_parser;

pub use classify::{detect_xfa_type, detect_xfa_type_from_packets, XfaType};
pub use extract::{validate_xfa_packets, PacketValidation};
pub use flatten::{flatten_xfa_to_pdf, is_pdf_encrypted};

pub use formcalc_interpreter as formcalc;
pub use xfa_dom_resolver as dom_resolver;
pub use xfa_json as json;
pub use xfa_layout_engine as layout;
