// Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
//
// This software is proprietary. The PDFluent application is free to use,
// including for commercial purposes. Redistribution, or extraction or reuse
// of its components (including the embedded PDF engine), requires a licence.
// See https://pdfluent.com/license for terms.

//! A PDF/A conversion may not take the spaces out of a page.
//!
//! `fix_notdef_glyph_refs` used to delete character code 32 from the content
//! streams whenever the embedded Type1 program's encoding vector had no entry
//! at that code. A subset that never had to draw a visible space legitimately
//! leaves 32 out, while the PDF still uses it with a declared width to move the
//! cursor along. So the word separator was cut out of documents that were
//! otherwise intact: `To improve cooperation` came back as
//! `Toimprovecooperation`.
//!
//! Nothing we measured could see it, and that is the part worth keeping in
//! mind. The page renders the same, because a deleted space and a blank space
//! both draw nothing and the widths were never touched. veraPDF calls the
//! result conformant -- the violation it was raised against really is gone.
//! Character retention stays at 100%, because no letter is lost, and the
//! neighbouring guard in `conversion_preserves_characters.rs` skips whitespace
//! on purpose. Only word retention moves, and on the six worst govdocs
//! documents it read a median of 4.0% (#182, #210).
//!
//! So this test asks the one question none of the others do: did the spaces
//! survive?
//!
//! It runs over a corpus directory rather than a fixture, because no document
//! small enough to commit reaches the branch -- the shape needs an embedded
//! Type1 subset with its own encoding vector. It names no document: it reports
//! how many failed and what their numbers were, so the file says nothing about
//! which corpus it was pointed at.

use std::path::PathBuf;

/// How many documents in the CI corpus still lose their separators.
///
/// Two of the six known cases are repaired: the deletion of code 32 in the
/// Type1 path. Four remain, and they take a different route -- the CID path in
/// `fix_cid_font_notdef`, which drops a two-byte code when it has no valid
/// value to substitute (#210).
///
/// This is a ceiling that may only come down. The test fails when the number
/// grows, and it also fails when it shrinks, so a repair cannot land without
/// this line moving with it. Calibrated against the CI corpus; pointed at a
/// different set of documents it will complain, which is the intended failure
/// mode -- loudly wrong beats quietly meaningless.
const STILL_BROKEN: usize = 4;

/// Spaces and non-space characters the document's text layer yields.
fn spaces_and_characters(pdf: &[u8]) -> Option<(usize, usize)> {
    let doc = pdf_engine::PdfDocument::open(pdf.to_vec()).ok()?;
    let (mut spaces, mut characters) = (0usize, 0usize);
    for page in 0..doc.page_count() {
        let Ok(text) = doc.extract_text(page) else {
            continue;
        };
        for ch in text.chars() {
            if ch == ' ' {
                spaces += 1;
            } else if !ch.is_whitespace() {
                characters += 1;
            }
        }
    }
    Some((spaces, characters))
}

fn corpus() -> Option<PathBuf> {
    for sleutel in [
        "PDFA_WORD_SEPARATOR_CORPUS",
        "CI_CORPUS_DIR",
        "PDFLUENT_CORPUS",
    ] {
        if let Ok(pad) = std::env::var(sleutel) {
            let pad = PathBuf::from(pad);
            if pad.is_dir() {
                return Some(pad);
            }
        }
    }
    None
}

#[test]
fn no_document_loses_its_spaces_while_keeping_its_letters() {
    let Some(dir) = corpus() else {
        eprintln!(
            "SKIPPED (not a pass): no corpus directory. Set PDFA_WORD_SEPARATOR_CORPUS, \
             CI_CORPUS_DIR or PDFLUENT_CORPUS to a directory of PDFs."
        );
        return;
    };

    let mut paden: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("corpus directory is not readable")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")))
        .collect();
    paden.sort();

    let opts = pdf_manip::pdfa::PdfAConvertOptions::default();
    let mut gemeten = 0usize;
    let mut kapot: Vec<String> = Vec::new();

    for pad in &paden {
        let Ok(bron) = std::fs::read(pad) else {
            continue;
        };
        let Some((spaces_before, chars_before)) = spaces_and_characters(&bron) else {
            continue;
        };
        // A document whose text layer has no spaces to begin with says nothing
        // about whether we remove them.
        if spaces_before < 20 || chars_before < 100 {
            continue;
        }
        let Ok(uit) = pdf_manip::pdfa::convert_bytes(&bron, &opts) else {
            continue;
        };
        let Some((spaces_after, chars_after)) = spaces_and_characters(&uit) else {
            continue;
        };
        gemeten += 1;

        let space_share = spaces_after as f64 / spaces_before as f64;
        let char_share = chars_after as f64 / chars_before.max(1) as f64;
        // Losing text is a different failure, measured by the retention gate.
        // This one is about keeping every letter and dropping the separators.
        if char_share >= 0.95 && space_share < 0.5 {
            kapot.push(format!(
                "{:.1}% of the spaces left while {:.1}% of the characters stayed \
                 ({spaces_before} -> {spaces_after} spaces)",
                100.0 * space_share,
                100.0 * char_share
            ));
        }
    }

    assert!(
        gemeten >= 5,
        "only {gemeten} document(s) had enough text to measure; this test then \
         proves nothing about the corpus it was pointed at"
    );
    assert!(
        kapot.len() <= STILL_BROKEN,
        "{} of {gemeten} document(s) came out of the conversion with their letters \
         intact and their word boundaries gone, and STILL_BROKEN allows {STILL_BROKEN}:\n  {}",
        kapot.len(),
        kapot.join("\n  ")
    );
    assert!(
        kapot.len() >= STILL_BROKEN,
        "only {} document(s) still lose their separators, and STILL_BROKEN says \
         {STILL_BROKEN}. Something was repaired -- bring the constant down with it, \
         otherwise the next regression has {} documents of room to hide in.",
        kapot.len(),
        STILL_BROKEN - kapot.len()
    );
}
