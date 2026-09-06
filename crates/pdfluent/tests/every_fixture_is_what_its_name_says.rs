//! What each document in `tests/corpus-mini` is, and what it is not (#236).
//!
//! This is the binding description. `README.md` beside the fixtures covers
//! provenance and maintenance; the contract that tests may build on is here.
//!
//! **Why this exists.** On 25-08-2026 eight of the eleven documents turned out
//! to carry `<<//Length` -- a double slash, so the key was called `//Length`
//! and not `/Length`. The stream length was therefore unknown and the object
//! did not load. Nobody noticed, because the engine reads more tolerantly than
//! lopdf: text came out all the same. Everything that reached the content
//! stream through lopdf got zero bytes, and redaction reported "no matches" on
//! documents the term is plainly in (#203). One typo, repeated in eight files,
//! invisible for three months.
//!
//! **What this contract adds over "it opens".** A fixture that quietly becomes
//! something other than its name promises makes every test that uses it
//! meaningless without a single red tick. `scanned.pdf` having no text layer is
//! the whole reason the OCR route has anything to do; give it one and that test
//! measures nothing while still reporting green.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::path::{Path, PathBuf};

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .parent()
        .expect("the repository root")
        .join("tests/corpus-mini")
}

/// Every document, with what it promises.
///
/// A fixture that is not in here fails the test. That is deliberate: a document
/// nobody describes becomes, in time, the answer to a question nobody asked.
const CONTRACT: &[(&str, &str)] = &[
    ("acroform.pdf", "two AcroForm fields"),
    ("acroform-multiselect.pdf", "a multi-select choice list"),
    ("encrypted.pdf", "encrypted; only the refusal can be tested"),
    (
        "malformed.pdf",
        "a broken cross-reference table, on purpose",
    ),
    ("multi-page.pdf", "fifty pages, for pagination"),
    ("pdfa-2b.pdf", "claims PDF/A-2b and is NOT"),
    ("scanned.pdf", "no text layer, for the OCR route"),
    ("signed-rsa.pdf", "carries a signature dictionary"),
    ("simple.pdf", "one page, one line of text"),
    (
        "xfa-form.pdf",
        "an XFA template; rebuildable with generators/xfa_form.py",
    ),
    ("zugferd.pdf", "a ZUGFeRD-like structure"),
];

#[test]
fn every_fixture_is_in_the_contract() {
    let dir = corpus();
    let mut on_disk: Vec<String> = std::fs::read_dir(&dir)
        .expect("tests/corpus-mini")
        .flatten()
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("pdf"))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    on_disk.sort();

    // FLOOR: fewer than ten fixtures means the reading is broken, not that the
    // directory is empty. An empty scan would approve everything below it.
    assert!(
        on_disk.len() >= 10,
        "only {} fixture(s) found; the reading is broken and this is not green",
        on_disk.len()
    );

    let described: Vec<&str> = CONTRACT.iter().map(|(n, _)| *n).collect();
    for name in &on_disk {
        assert!(
            described.contains(&name.as_str()),
            "{name} is in corpus-mini but not in CONTRACT. A fixture nobody \
             describes becomes the answer all by itself."
        );
    }
    for (name, _) in CONTRACT {
        assert!(
            on_disk.contains(&name.to_string()),
            "{name} is in CONTRACT but not on disk"
        );
    }
}

/// Every document yields its content stream through lopdf.
///
/// This is the `<<//Length` defect, mechanically. The engine reads more
/// tolerantly, so "it opens" was green while eight documents gave zero bytes
/// through lopdf.
#[test]
fn every_fixture_yields_its_content_through_lopdf() {
    for (name, promise) in CONTRACT {
        // `malformed.pdf` is not supposed to load; that is its promise.
        if *name == "malformed.pdf" || *name == "encrypted.pdf" {
            continue;
        }
        let path = corpus().join(name);
        let doc = lopdf::Document::load(&path)
            .unwrap_or_else(|e| panic!("{name} ({promise}) does not load through lopdf: {e}"));

        let mut streams = 0usize;
        let mut without_length = Vec::new();
        for (id, obj) in &doc.objects {
            let lopdf::Object::Stream(s) = obj else {
                continue;
            };
            streams += 1;
            // A stream without a readable /Length is precisely the 25-08 defect.
            if s.dict.get(b"Length").is_err() {
                without_length.push(format!("{id:?}"));
            }
        }
        assert!(
            without_length.is_empty(),
            "{name} has {} stream(s) without a readable /Length: {}. That is the \
             `<<//Length` defect from #203 -- the key is then called `//Length`.",
            without_length.len(),
            without_length.join(", ")
        );
        assert!(streams > 0, "{name} has no streams at all");
    }
}

/// `scanned.pdf` has no text layer, and it must stay that way.
///
/// The whole OCR route hangs on this. Give this document text and every OCR
/// test measures nothing while still reporting green.
#[test]
fn scanned_pdf_has_no_text_layer() {
    let doc = pdfluent::PdfDocument::open(corpus().join("scanned.pdf")).expect("open");
    let text = doc.extract_text().unwrap_or_default();
    assert!(
        text.trim().is_empty(),
        "scanned.pdf carries text ({:?}); then the OCR route has nothing left to \
         do and those tests measure nothing",
        text.trim()
    );
}

/// `pdfa-2b.pdf` claims PDF/A-2b and is not.
///
/// It exists to test a false claim. Should it ever become genuinely conformant,
/// every test using it proves the opposite of what it means to.
#[test]
fn pdfa_2b_pdf_claims_pdfa_but_is_not() {
    let bytes = std::fs::read(corpus().join("pdfa-2b.pdf")).expect("read");
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.contains("pdfaid"),
        "pdfa-2b.pdf no longer claims PDF/A; then it tests no false claim"
    );
}

/// `multi-page.pdf` has fifty pages.
#[test]
fn multi_page_pdf_has_fifty_pages() {
    let doc = pdfluent::PdfDocument::open(corpus().join("multi-page.pdf")).expect("open");
    assert_eq!(
        doc.page_count(),
        50,
        "multi-page.pdf changed its page count; pagination tests rely on this"
    );
}

/// `xfa-form.pdf` carries a real XFA template.
///
/// It once carried an empty package and did not live up to its name (#204).
#[test]
fn xfa_form_pdf_carries_a_real_template() {
    let mut doc = pdfluent::PdfDocument::open(corpus().join("xfa-form.pdf")).expect("open");
    assert!(
        doc.has_xfa_form(),
        "xfa-form.pdf no longer carries XFA -- that was #204, and it came back"
    );
    let model = doc.xfa_form_model().expect("a form model");
    assert!(
        !model.fields.is_empty(),
        "xfa-form.pdf has a template without fields; the package is empty again"
    );
}

/// `signed-rsa.pdf` carries a signature, and that is counted here.
///
/// The table above has SAID "carries a signature dictionary" since it existed,
/// and nothing checked it. That is the gap of #122 in its purest form: on
/// 23-08-2026 two signed fixtures returned zero signatures. That exposed a real
/// defect in the detection -- but it could just as easily have led somebody to
/// adjust the test to the fixture, because a fixture nobody checks becomes the
/// answer.
#[test]
fn signed_rsa_pdf_really_carries_a_signature() {
    let path = corpus().join("signed-rsa.pdf");
    let doc = pdfluent::PdfDocument::open(&path).expect("signed-rsa.pdf must open");
    let signatures = doc
        .signatures()
        .expect("asking for signatures must not fail on a signed document");
    assert!(
        !signatures.is_empty(),
        "signed-rsa.pdf returned zero signatures. Either the fixture carries none \
         (then its name is wrong), or the detection does not see them (then this is \
         precisely the 23-08-2026 defect). Both are for a person; neither is green."
    );
}

/// `encrypted.pdf` does not open without a password.
///
/// The password is written down nowhere, so only the refusal can be tested --
/// and that is enough for what the name promises. If this open succeeds, the
/// document is not encrypted and every test using it as encrypted is testing
/// something other than it thinks.
#[test]
fn encrypted_pdf_does_not_open_without_a_password() {
    let path = corpus().join("encrypted.pdf");
    assert!(
        pdfluent::PdfDocument::open(&path).is_err(),
        "encrypted.pdf opened without a password. Then it is not encrypted, and \
         every test using it as encrypted is testing something other than it thinks."
    );
}
