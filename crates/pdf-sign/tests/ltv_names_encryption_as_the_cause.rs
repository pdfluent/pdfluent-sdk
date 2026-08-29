//! An encrypted document must be refused by name, not by symptom (#270).
//!
//! `embed_dss_incremental` loads the document, and lopdf takes its decryption
//! route when the trailer carries `/Encrypt`. When that route fails the object
//! table stays empty, so the first lookup returns `object ID ... not found` —
//! a message that points at the cross-reference table while the cause is
//! encryption. Somebody reading that goes looking for a corrupt xref.
//!
//! The check deliberately reads the trailer rather than calling
//! `is_encrypted()`: that one requires `/Encrypt` to be an indirect reference,
//! and a direct dictionary is legal and does occur — including in this very
//! fixture — so the check would compile, run, and do nothing.

use std::path::Path;

fn fixture() -> Option<Vec<u8>> {
    // Walk up: tests run from the crate directory, the corpus sits at the root.
    for op in [
        "tests/corpus-mini/encrypted.pdf",
        "../../tests/corpus-mini/encrypted.pdf",
    ] {
        if Path::new(op).exists() {
            return std::fs::read(op).ok();
        }
    }
    None
}

#[test]
fn encrypted_input_says_so() {
    let Some(bytes) = fixture() else {
        eprintln!(
            "SKIPPED (not a pass): tests/corpus-mini/encrypted.pdf is missing, so the \
             encrypted path could not be exercised."
        );
        return;
    };

    let err = pdfluent_sign::embed_dss_incremental(
        &bytes,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .expect_err("embedding into an encrypted document must fail");

    assert!(
        err.contains("encrypted"),
        "the error must name encryption as the cause; got {err:?}",
    );
    assert!(
        !err.contains("not found"),
        "the error must not point at the cross-reference table; got {err:?}",
    );
}
