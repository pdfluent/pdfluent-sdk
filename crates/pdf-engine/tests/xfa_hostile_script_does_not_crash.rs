//! A hostile FormCalc script must not take the process down through the public
//! API (#305).
//!
//! `render_page` flattens XFA on its own, which runs the form's FormCalc, so
//! this is reachable from every binding that exposes rendering — capi, java,
//! node, python and wasm all do. The bug this pins was not reachable through the
//! parser alone: two depth bounds that each stayed inside their own limit
//! multiplied at run time, and 63 call frames each carrying a 60-term
//! expression descended 3904 levels.
//!
//! It lives at this level deliberately. The same shape was already refused by
//! interpreter unit tests while `render_page` still aborted, because the crash
//! happens on the worker thread `pdf_xfa::flatten` spawns rather than on the
//! caller's — so reachability through the public API is its own property and
//! has to be its own test.

#![cfg(feature = "xfa")]

use pdf_engine::render::RenderOptions;
use pdf_engine::PdfDocument;

/// 63 nested user functions, each body carrying a 60-term expression.
///
/// Both values are legal on their own: 63 is under the frame bound of 64, and
/// 60 is under the parser's expression bound. Before the shared budget this
/// aborted the process at every caller stack size — 256 KB, 2 MB and 8 MB —
/// because the caller cannot size the thread the crash happens on.
const HOSTILE: &[u8] = include_bytes!("../../../fixtures/formcalc/hostile_deep_call_chain.pdf");

/// The same form with the script it shipped with, so a failure here means the
/// fixture stopped rendering rather than the guard doing its job.
const BENIGN: &[u8] = include_bytes!("../../../fixtures/formcalc/fc_01_arithmetic.pdf");

fn renders(bytes: &[u8]) -> bool {
    let doc = match PdfDocument::open(bytes.to_vec()) {
        Ok(doc) => doc,
        Err(_) => return false,
    };
    doc.render_page(0, &RenderOptions::default()).is_ok()
}

#[test]
fn a_hostile_formcalc_script_does_not_abort_render_page() {
    // Any answer is acceptable except not returning one: the script may be
    // refused and the page rendered without it, or the render may report an
    // error. What must not happen is the process dying.
    let _ = renders(HOSTILE);
}

#[test]
fn the_ordinary_form_still_renders() {
    assert!(renders(BENIGN), "the benign control stopped rendering");
}
