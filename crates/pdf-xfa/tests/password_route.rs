// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A protected XFA form can be opened with a password.
//!
//! Until 24-08-2026 the XFA route tried the empty password and nothing else.
//! That works for owner-only encryption and no further: on a business corpus it
//! drops 3 to 5% of the documents, and the caller cannot tell whether that is
//! the document's fault or ours. See #149.
//!
//! The tests encrypt a real XFA form and probe both directions: without a
//! password it must fail, with the right one it must succeed. Asserting only
//! the second would leave an implementation that ignores the password green --
//! as long as the document also opens on the empty one.

use lopdf::Document;

const PASSWORD: &str = "secret-for-the-test";

/// The XFA form from the mini corpus, encrypted with a user password.
fn encrypted_xfa_form() -> Vec<u8> {
    const XFA: &[u8] = include_bytes!("../../../tests/corpus-mini/xfa-form.pdf");
    let mut doc = Document::load_mem(XFA).expect("the XFA fixture does not load");

    let state = lopdf::aes256_encryption_state(PASSWORD, PASSWORD, lopdf::Permissions::all())
        .expect("building the encryption state failed");
    doc.encrypt(&state).expect("encrypt failed");

    let mut out = Vec::new();
    doc.save_to(&mut out).expect("saving failed");
    assert!(
        out.windows(8).any(|w| w == b"/Encrypt"),
        "the fixture is not encrypted; then this test asserts nothing"
    );
    out
}

#[test]
fn without_a_password_it_does_not_open() {
    // The counter-proof. Without it, an implementation that ignores the
    // password entirely would pass as well.
    let bytes = encrypted_xfa_form();
    let out = pdf_xfa::flatten::flatten_xfa_to_pdf(&bytes);
    assert!(
        out.is_err(),
        "a password-protected document opened without a password"
    );
}

#[test]
fn with_the_right_password_it_opens() {
    let bytes = encrypted_xfa_form();
    let out = pdf_xfa::flatten::flatten_xfa_to_pdf_with_password(&bytes, PASSWORD)
        .expect("the right password did not open the document");
    assert!(!out.is_empty(), "no bytes came back");
    assert!(
        out.starts_with(b"%PDF-"),
        "the output is not a PDF: {:?}",
        &out[..out.len().min(8)]
    );
}

#[test]
fn a_wrong_password_does_not_open_it() {
    // Otherwise "with a password it opens" would also hold for an
    // implementation that accepts the password and never uses it.
    let bytes = encrypted_xfa_form();
    let out = pdf_xfa::flatten::flatten_xfa_to_pdf_with_password(&bytes, "something else");
    assert!(out.is_err(), "a wrong password opened the document");
}

#[test]
fn an_unencrypted_document_still_opens_with_a_password_supplied() {
    // Passing a password to something that is not encrypted must not be an
    // error -- otherwise every caller has to find out first whether it is
    // needed.
    const XFA: &[u8] = include_bytes!("../../../tests/corpus-mini/xfa-form.pdf");
    let out = pdf_xfa::flatten::flatten_xfa_to_pdf_with_password(XFA, PASSWORD)
        .expect("an unencrypted document was refused with a password supplied");
    assert!(out.starts_with(b"%PDF-"));
}
