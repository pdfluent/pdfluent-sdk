//! Isolating shared font descriptors must produce the same bytes every run.
//!
//! `isolate_font_descriptors` allocates new object ids while it iterates the
//! map of descriptor → owning fonts. That map was a `HashMap`, whose order
//! varies per process, so the ids landed in a different order each run and the
//! output file differed byte for byte from one invocation to the next.
//!
//! It matters because reproducibility is the point of PDF/A conversion for the
//! people who ask for it: you cannot diff a conversion against a baseline, or
//! sign for one, if running it twice gives two answers.
//!
//! This runs the function rather than inspecting any single map. There are over
//! a hundred `HashMap`s in the font code, and auditing them by hand is how you
//! miss the hundred-and-first.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{dictionary, Document, Object};

/// Several descriptors, each shared by several fonts.
///
/// The count matters: the loop that allocates ids iterates the descriptor map,
/// so with a single descriptor there is only one entry and any order gives the
/// same answer. The first version of this fixture had exactly one, and the
/// test passed against the broken code because of it.
fn document_with_shared_descriptors(descriptors: u32, fonts_each: u32) -> Document {
    let mut doc = Document::with_version("1.7");
    for d in 0..descriptors {
        let fd = doc.add_object(dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => format!("Shared{d}"),
            "Flags" => 4,
        });
        for f in 0..fonts_each {
            doc.add_object(dictionary! {
                "Type" => "Font",
                "Subtype" => "Type1",
                "BaseFont" => format!("Font{d}_{f}"),
                "FontDescriptor" => Object::Reference(fd),
            });
        }
    }
    doc
}

fn isolate_and_serialise() -> Vec<u8> {
    let mut doc = document_with_shared_descriptors(8, 3);
    pdf_manip::pdfa_fonts::isolate_font_descriptors(&mut doc);
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("serialise");
    out
}

/// Re-invoke this test binary as a child that prints the bytes, so each run
/// gets its own HashMap seed.
fn bytes_from_a_fresh_process() -> String {
    let exe = std::env::current_exe().expect("test binary path");
    let out = std::process::Command::new(exe)
        .arg("--exact")
        .arg("print_isolated_bytes")
        .arg("--nocapture")
        .arg("--ignored")
        .env("PDFLUENT_PRINT_ISOLATED", "1")
        .output()
        .expect("run child");
    let tekst = String::from_utf8_lossy(&out.stdout);
    tekst
        .lines()
        .find_map(|r| r.strip_prefix("BYTES:"))
        .unwrap_or_else(|| panic!("child printed no BYTES line:\n{tekst}"))
        .to_string()
}

/// Prints the serialised bytes for the parent to compare. Ignored by default so
/// it only runs when the parent asks for it by name.
#[test]
#[ignore]
fn print_isolated_bytes() {
    if std::env::var("PDFLUENT_PRINT_ISOLATED").is_err() {
        eprintln!(
            "SKIPPED (not a pass): PDFLUENT_PRINT_ISOLATED is not set, so nothing was printed"
        );
        return;
    }
    let bytes = isolate_and_serialise();
    let som: u64 = bytes
        .iter()
        .enumerate()
        .map(|(i, b)| (i as u64) ^ (*b as u64) << 3)
        .sum();
    println!("BYTES:{}:{som}", bytes.len());
}

#[test]
fn isolation_produces_identical_bytes_across_runs() {
    // Several owners: with one owner there is nothing to clone and any map
    // order gives the same answer, so the test would pass without the fix.
    let first = bytes_from_a_fresh_process();
    let second = bytes_from_a_fresh_process();

    assert_eq!(
        first, second,
        "isolating shared font descriptors must be byte-deterministic across \
         processes; got {first} then {second}",
    );
}

#[test]
fn isolation_actually_clones_the_shared_descriptor() {
    // Without this the test above passes on a function that does nothing.
    let mut doc = document_with_shared_descriptors(3, 3);
    let before = doc.objects.len();
    pdf_manip::pdfa_fonts::isolate_font_descriptors(&mut doc);
    assert!(
        doc.objects.len() > before,
        "expected clones for the shared descriptor, object count stayed at {before}",
    );
}
