//! A PDF whose Form XObject draws itself must not take the process down (#262).
//!
//! The same class as #305 — stack exhaustion through nesting — but reached with
//! a document rather than a script, so it needs no XFA, no FormCalc and no
//! feature flag. `render_page` is exported by capi, java, node, python and wasm,
//! and this file is 639 bytes.
//!
//! Before the bound in `pdf_interpret::context`, this aborted the process:
//!
//! ```text
//! thread 'main' has overflowed its stack
//! fatal runtime error: stack overflow, aborting     (rc=134)
//! ```
//!
//! The test asserts the process SURVIVES, not that the page is blank: a
//! self-referencing XObject has no defined appearance, and what matters to a
//! caller embedding the SDK is that a malformed file returns rather than
//! killing the host. Asserting the pixels would pin an arbitrary choice.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.
use pdf_engine::render::RenderOptions;
use pdf_engine::PdfDocument;
use pdf_render::pdf_interpret::InterpreterWarning;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// `/X1` is a Form XObject whose content stream is `q /X1 Do Q`.
const SELF_REFERENCING: &[u8] =
    include_bytes!("../../../fixtures/pdf/self_referencing_xobject.pdf");

#[test]
fn a_self_referencing_xobject_returns_instead_of_aborting() {
    let doc = PdfDocument::open(SELF_REFERENCING.to_vec())
        .expect("the file is structurally valid; only its XObject is circular");

    assert_eq!(doc.page_count(), 1, "the fixture has one page");

    // The call that used to abort. Reaching the next line IS the assertion.
    let _ = doc.render_page(0, &RenderOptions::default());
}

/// The bound must not refuse ordinary nesting.
///
/// Without this, a limit of 1 would pass the test above and break every real
/// document — the failure mode of a depth bound is not "too permissive".
#[test]
fn an_ordinary_document_still_renders() {
    const ORDINARY: &[u8] = include_bytes!("../../pdf-java/src/test/resources/binding-fixture.pdf");

    let doc = PdfDocument::open(ORDINARY.to_vec()).expect("a one-page generated PDF");
    let rendered = doc.render_page(0, &RenderOptions::default());

    assert!(
        rendered.is_ok(),
        "the depth bound refused a document that nests nothing: {:?}",
        rendered.err()
    );
}

/// The cycle that alternates BETWEEN constructs, which the XObject bound alone
/// does not catch (#318).
///
/// A tiling pattern whose cell sets a soft mask, whose group paints with that
/// same pattern. Each construct builds a fresh `Context`, so a depth counter
/// that starts at zero for each one never reaches its limit however deep the
/// alternation goes.
///
/// Measured with only the XObject bound in place: rc=134, stack overflow. The
/// identical file with the cycle broken — one stream painting grey instead of
/// the pattern — renders normally, which is how we know it is the cycle and not
/// the file.
const CYCLE_ACROSS_CONSTRUCTS: &[u8] =
    include_bytes!("../../../fixtures/pdf/cycle_pattern_softmask.pdf");

#[test]
fn a_pattern_soft_mask_cycle_returns_instead_of_aborting() {
    let doc = PdfDocument::open(CYCLE_ACROSS_CONSTRUCTS.to_vec())
        .expect("the file is structurally valid; only its pattern/mask pair is circular");

    assert_eq!(doc.page_count(), 1, "the fixture has one page");
    assert_bound_fired(doc);
}

/// The same alternation reached through a different pair of constructs (#318).
///
/// A Form XObject whose `ExtGState` carries a luminosity `/SMask` whose group
/// redraws that same XObject: XObject -> soft mask -> XObject -> soft mask.
/// The test above alternates pattern/soft-mask; this one alternates
/// XObject/soft-mask, so a fix that threads the depth through one pair and not
/// the other passes one test and fails this one.
///
/// Contributed by the reviewer of the narrower fix, and measured on three
/// binaries rather than assumed:
///
/// ```text
///                                   master   narrow fix   this branch
///   self_referencing_xobject.pdf     rc=134     rc=0         rc=0
///   cycle_pattern_softmask.pdf       rc=134       -          rc=0
///   alternating XObject <-> SMask    rc=134     rc=134       rc=0
/// ```
///
/// The middle column is why the narrow fix lands as "narrows the class" rather
/// than "fixes it". That every fixture aborts on master is what makes the zeros
/// meaningful: the method demonstrably detects the crash it reports absent.
const ALTERNATING_XOBJECT_SOFT_MASK: &[u8] =
    include_bytes!("../../../fixtures/pdf/alternating_smask_xobject.pdf");

#[test]
fn an_xobject_soft_mask_cycle_returns_instead_of_aborting() {
    let doc = PdfDocument::open(ALTERNATING_XOBJECT_SOFT_MASK.to_vec())
        .expect("the file is structurally valid; only its XObject/mask pair is circular");

    assert_eq!(doc.page_count(), 1, "the fixture has one page");
    assert_bound_fired(doc);
}

/// Render page 0 and require that the depth bound actually refused something.
///
/// WHY THE SURVIVAL IS NOT ENOUGH
///
/// A test whose only assertion is "we reached the next line" cannot tell a
/// survived cycle from a fixture that stopped being a cycle. Asserting
/// `page_count() == 1` does not fix that either, and this was measured rather
/// than reasoned: a valid 456-byte one-page PDF drawing a grey box -- no
/// XObject, no cycle, nothing to recurse on -- passes both the open and the
/// page count, and all four tests stayed green with it in place. The same
/// shape as an empty glob satisfying a loop.
///
/// So assert the thing only a real cycle can produce: `NestingTooDeep` in the
/// warning sink. The inert fixture cannot emit it, because there is nothing to
/// nest. That is also why the bound reports to the sink at all -- reaching the
/// limit means paint is missing from the page, and a `warn!` line is invisible
/// to a library caller. (Reviewer finding, #318.)
fn assert_bound_fired(mut doc: PdfDocument) {
    let fired = Arc::new(AtomicBool::new(false));
    let zag = Arc::clone(&fired);
    doc.set_warning_sink(Arc::new(move |w: InterpreterWarning| {
        if matches!(w, InterpreterWarning::NestingTooDeep { .. }) {
            zag.store(true, Ordering::SeqCst);
        }
    }));

    // Reaching the next line is still half the assertion: before the bound, it
    // aborted the process.
    let _ = doc.render_page(0, &RenderOptions::default());

    assert!(
        fired.load(Ordering::SeqCst),
        "the depth bound never fired, so this fixture nests nothing and the \
         test would pass with any valid one-page PDF in its place"
    );
}
