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

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

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

/// The stacks `render_page` is called from. 256 KiB is the least an
/// embedder's worker thread might have; 1 MiB is the wasm32 default; 2 MiB is
/// Rust's default for a spawned thread and what most binding runtimes give a
/// worker; 8 MiB is the main thread on macOS and Linux.
const CALLER_STACKS: [usize; 4] = [256 * 1024, 1 << 20, 2 << 20, 8 << 20];

fn renders(bytes: &[u8]) -> bool {
    let doc = match PdfDocument::open(bytes.to_vec()) {
        Ok(doc) => doc,
        Err(_) => return false,
    };
    doc.render_page(0, &RenderOptions::default()).is_ok()
}

/// Run `f` on a thread with exactly `bytes` of stack. A stack overflow does
/// not come back through `join`: it aborts the process, and the test run
/// with it.
fn on_a_stack_of(bytes: usize, f: impl FnOnce() -> bool + Send + 'static) -> bool {
    std::thread::Builder::new()
        .stack_size(bytes)
        .spawn(f)
        .expect("spawn")
        .join()
        .expect("render_page must return, not abort")
}

#[test]
fn a_hostile_formcalc_script_does_not_abort_render_page() {
    // Any answer is acceptable except not returning one: the script may be
    // refused and the page rendered without it, or the render may report an
    // error. What must not happen is the process dying.
    let _ = renders(HOSTILE);
}

/// The issue's table: before the fix, SIGABRT at 256 KB, 2 MB and 8 MB alike,
/// because the crash was on the thread `flatten` spawns and not the caller's.
/// Now the answer is a `Result` on every caller stack -- and the caller's
/// stack still does not matter, which is the point: the evaluator's budget is
/// counted and measured inside the pipeline, not inherited from whoever calls.
#[test]
fn a_hostile_formcalc_script_returns_on_every_caller_stack() {
    // Discarding each answer left an abort as the only detectable failure, and
    // an abort takes the test binary with it rather than failing an assertion.
    // The claim worth asserting is the one the fix makes: the budget is counted
    // and measured inside the pipeline, so the caller's stack does not change
    // the verdict. Every size must therefore give the SAME answer -- if 256 KiB
    // quietly took a different path than 8 MiB, that is the bug this guards
    // against, and now it is a failure rather than a silence. (review of #1677)
    let answers: Vec<(usize, bool)> = CALLER_STACKS
        .iter()
        .map(|&stack| (stack, on_a_stack_of(stack, || renders(HOSTILE))))
        .collect();
    let first = answers[0].1;
    assert!(
        answers.iter().all(|&(_, a)| a == first),
        "the hostile form answered differently depending on the caller's stack: {answers:?}"
    );
}

#[test]
fn the_ordinary_form_still_renders() {
    assert!(renders(BENIGN), "the benign control stopped rendering");
}

/// The acceptance side on the same stacks: a refusal that also stopped
/// ordinary forms from rendering would be the other failure direction.
#[test]
fn the_ordinary_form_still_renders_on_every_caller_stack() {
    for stack in CALLER_STACKS {
        assert!(
            on_a_stack_of(stack, || renders(BENIGN)),
            "the benign control stopped rendering on a {} KiB caller stack",
            stack / 1024
        );
    }
}
