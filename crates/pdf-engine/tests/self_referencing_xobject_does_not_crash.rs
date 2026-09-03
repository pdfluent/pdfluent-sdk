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
use pdf_engine::render::RenderOptions;
use pdf_engine::PdfDocument;

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

    // Reaching the next line is the assertion, as above.
    let _ = doc.render_page(0, &RenderOptions::default());
}
