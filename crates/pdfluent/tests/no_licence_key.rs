// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The facade asks nothing of the caller, and the environment cannot change it.
//!
//! Until #226 every one of these calls sat behind `require_capability`, and the
//! tier that answered it came from `PDFLUENT_LICENSE_KEY` when nothing else had
//! set one. Sixty-five call sites; a document opened without a key ran as
//! `Tier::Trial` and was refused Office export, redaction, signing, PDF/A
//! conversion and layout-aware extraction. The decision (#199) is that no such
//! check exists, and `LICENSE-COMMERCIAL` §5 states that as a fact about this
//! binary.
//!
//! Two claims, and both are needed. That every call succeeds unlicensed is the
//! obvious one. That the answer does not depend on the environment is the one
//! that catches a check coming back quietly: a gate reintroduced with a key in
//! CI would leave every other test in this repository green.
//!
//! The visual-regression baseline carries the other half of this: the
//! `pdfa-converted` fixture page changed when the free-tier watermark went, and
//! the 1361 pixels that moved are the mark itself.
//!
//! A separate test binary, because the environment is process-wide state. A
//! test that shared a process with one that set a key would prove nothing about
//! either -- the first to run would decide the answer for both, which is
//! exactly the hole the old C ABI refusal suite had.

use pdfluent::text_edit::{ReplaceOptions, TextQuery};
use pdfluent::{OpenOptions, PdfDocument};

const SAMPLE_PDF: &[u8] = include_bytes!("../../../fixtures/sample.pdf");

/// Everything the tier table used to withhold, run end to end, reduced to a
/// value that changes when the output changes.
fn what_the_sdk_produces() -> Vec<(&'static str, usize)> {
    let mut doc =
        PdfDocument::from_bytes_with(SAMPLE_PDF, OpenOptions::new()).expect("open sample.pdf");

    let mut out = vec![
        // Trial granted this one, so it stands for the baseline rather than for
        // a gate: if it moves, the fixture moved, not the licensing.
        ("text", doc.text().expect("text").len()),
        // Developer and up.
        (
            "text_with_layout",
            doc.text_with_layout().expect("layout").len(),
        ),
        // Business and up: the three the C ABI, Node, Python and WASM all
        // refused with a licence error of their own.
        ("docx", doc.to_docx_bytes().expect("docx").len()),
        ("xlsx", doc.to_xlsx_bytes().expect("xlsx").len()),
        ("pptx", doc.to_pptx_bytes().expect("pptx").len()),
        // Trial-granted, but the write path carried its own gate.
        ("bytes", doc.to_bytes().expect("bytes").len()),
    ];

    // Team and up, and the one that stamped the page rather than refusing.
    let report = doc
        .replace_text(TextQuery::exact("PDF"), "PDF", ReplaceOptions::default())
        .expect("replace_text");
    out.push(("replacements", report.replacements_applied));
    let edited = doc.to_bytes().expect("bytes after edit");
    let marked = String::from_utf8_lossy(&edited).contains("PDFluent trial");
    assert!(!marked, "an edit was stamped with a trial notice");
    out.push(("edited", edited.len()));

    out
}

#[test]
fn every_gated_call_works_with_no_licence_and_ignores_the_environment() {
    // This binary holds one test, so nothing else in the process reads the
    // environment while it is being changed.
    std::env::remove_var("PDFLUENT_LICENSE_KEY");
    let unlicensed = what_the_sdk_produces();

    std::env::set_var("PDFLUENT_LICENSE_KEY", "tier:enterprise");
    let with_a_key = what_the_sdk_produces();
    std::env::remove_var("PDFLUENT_LICENSE_KEY");

    assert_eq!(
        unlicensed, with_a_key,
        "a key in the environment changed what the SDK produced"
    );
}
