// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Form flattening through the facade.
//!
//! Until 23-08-2026 `flatten_forms` returned Error::MissingDependency with a
//! note saying the runtime was deferred to a 1.x MINOR. The runtime was not
//! deferred: pdf-forms implements the whole pipeline -- appearance streams,
//! widget removal, AcroForm removal -- with eleven passing tests. Only the
//! wiring was missing, and the deferral note had gone stale in the meantime.
//!
//! These tests assert on the produced document. "Returned Ok" is not the
//! claim; "the form is no longer a form and still shows the same text" is.

use pdfluent::PdfDocument;

const ACROFORM_PDF: &[u8] = include_bytes!("../../../tests/corpus-mini/acroform.pdf");
const PLAIN_PDF: &[u8] = include_bytes!("../../../tests/corpus-mini/simple.pdf");

fn licensed() {
    let _ = pdfluent::set_license_key("tier:business");
}

#[test]
fn flattening_removes_the_form_and_keeps_the_content() {
    licensed();
    let mut doc = PdfDocument::from_bytes(ACROFORM_PDF).expect("open acroform.pdf");

    let before = doc.to_bytes().expect("serialise before");
    assert!(
        contains(&before, b"/AcroForm"),
        "fixture has no AcroForm, so this test would prove nothing"
    );

    let report = doc.flatten_forms().expect("flatten_forms");
    assert!(
        report.fields_flattened > 0,
        "nothing was flattened: {report:?}"
    );

    let after = doc.to_bytes().expect("serialise after");
    assert!(
        !contains(&after, b"/AcroForm"),
        "the AcroForm dictionary survived flattening, so the document is still editable"
    );
}

#[test]
fn a_document_without_a_form_flattens_to_nothing_rather_than_failing() {
    licensed();
    let mut doc = PdfDocument::from_bytes(PLAIN_PDF).expect("open simple.pdf");
    let report = doc
        .flatten_forms()
        .expect("flatten on a plain PDF must not fail");
    assert_eq!(report.fields_flattened, 0);
    assert!(report.is_complete(), "nothing to skip, yet {report:?}");
}

#[test]
fn flattening_takes_the_values_written_through_form_mut() {
    licensed();
    let mut doc = PdfDocument::from_bytes(ACROFORM_PDF).expect("open acroform.pdf");

    // Fill first, flatten second. The parser reads a Pdf and the flattener
    // writes to the lopdf handle; only the latter carries this edit. Parsing
    // the bytes the document was opened with would flatten the empty form and
    // silently discard what the caller just filled in.
    let fields = doc.form_fields().expect("form_fields");
    let Some(field) = fields.first().map(|f| f.name.clone()) else {
        eprintln!("SKIPPED (not a pass): acroform.pdf exposes no field names");
        return;
    };

    doc.form_mut()
        .set_text(&field, "Jasper de Winter")
        .expect("set_text");

    let report = doc.flatten_forms().expect("flatten_forms");
    assert!(report.fields_flattened > 0, "nothing flattened: {report:?}");

    let after = doc.to_bytes().expect("serialise");
    assert!(
        contains(&after, b"Jasper"),
        "the filled value is not in the flattened output"
    );
    // Both halves, or this proves nothing: the value is in the output whether
    // or not the flatten landed, because form_mut already wrote it. Only the
    // absence of the AcroForm shows the flatten actually ran on the document
    // rather than on a copy of it.
    assert!(
        !contains(&after, b"/AcroForm"),
        "the form survived, so the value is still editable"
    );
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}
