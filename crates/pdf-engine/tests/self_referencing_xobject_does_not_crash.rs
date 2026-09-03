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
