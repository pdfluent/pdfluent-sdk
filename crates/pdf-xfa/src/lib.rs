//! XFA (XML Forms Architecture) engine wrapper.
//!
//! Re-exports the XFA DOM resolver, FormCalc interpreter, layout engine,
//! and JSON serialization as a unified crate.

pub use formcalc_interpreter as formcalc;
pub use xfa_dom_resolver as dom_resolver;
pub use xfa_json as json;
pub use xfa_layout_engine as layout;
