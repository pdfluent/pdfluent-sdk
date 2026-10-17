//! `extract-text` hands over the page boundaries (#449).
//!
//! Measured in #390 on the pinned `gate` set: our output carried 0 form feeds
//! on a 14-page document where `pdftotext`, `mutool`, PyMuPDF and pdfminer.six
//! each carried 14. Page is the unit a RAG pipeline chunks on and the unit a
//! citation points at, and a consumer could not recover either without parsing
//! the PDF a second time with another library.
//!
//! The fixture is built in-memory via `lopdf`, with different text on each
//! page: a fixture whose pages read alike cannot catch a reader that hands
//! back page one twice.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::path::{Path, PathBuf};
use std::process::Command;

use lopdf::{dictionary, Document, Object, Stream};

const PAGE_ONE_TEXT: &str = "Alpha on page one";
const PAGE_TWO_TEXT: &str = "Beta on page two";
const FORM_FEED: char = '\u{000C}';

/// A two-page PDF whose pages carry different, known text.
fn build_two_page_pdf() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();

    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });

    let mut page_ids = Vec::new();
    for text in [PAGE_ONE_TEXT, PAGE_TWO_TEXT] {
        let content = format!("BT\n/F1 12 Tf\n72 720 Td\n({text}) Tj\nET\n");
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => resources_id,
        });
        page_ids.push(page_id);
    }

    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => page_ids.iter().map(|id| Object::Reference(*id)).collect::<Vec<_>>(),
            "Count" => page_ids.len() as i64,
        }),
    );

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("serialise fixture");
    buf
}

/// Write the fixture under a name unique to the calling test, so tests that
/// run in parallel do not read each other's file.
fn fixture_path(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("pdfluent_cli_{name}.pdf"));
    std::fs::write(&path, build_two_page_pdf()).expect("write fixture");
    path
}

fn run(args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_pdfluent-cli"))
        .args(args)
        .output()
        .expect("spawn pdfluent-cli");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

fn cleanup(path: &Path) {
    let _ = std::fs::remove_file(path);
}

#[test]
fn plain_output_ends_every_page_with_a_form_feed() {
    let pdf = fixture_path("page_boundaries_plain");
    let (ok, stdout) = run(&["extract-text", pdf.to_str().unwrap()]);
    cleanup(&pdf);
    assert!(ok, "extract-text on a valid PDF should succeed");

    // One form feed per page, the count every other reader emits, and the
    // trailing empty field a form-feed consumer already drops.
    assert_eq!(
        stdout.matches(FORM_FEED).count(),
        2,
        "two pages carry two form feeds, got: {stdout:?}"
    );
    let mut pages: Vec<&str> = stdout.split(FORM_FEED).collect();
    assert_eq!(
        pages.pop().map(str::trim),
        Some(""),
        "nothing after the last page"
    );
    assert_eq!(pages.len(), 2);
    assert!(
        pages[0].contains(PAGE_ONE_TEXT) && !pages[0].contains(PAGE_TWO_TEXT),
        "the first field is page one, got: {:?}",
        pages[0]
    );
    assert!(
        pages[1].contains(PAGE_TWO_TEXT),
        "the second field is page two, got: {:?}",
        pages[1]
    );
}

#[test]
fn no_page_breaks_restores_the_blank_line_join() {
    let pdf = fixture_path("page_boundaries_no_breaks");
    let (ok, stdout) = run(&["extract-text", pdf.to_str().unwrap(), "--no-page-breaks"]);
    cleanup(&pdf);
    assert!(ok);
    assert_eq!(
        stdout.matches(FORM_FEED).count(),
        0,
        "the escape hatch hands back the pre-1.0 bytes"
    );
    assert!(stdout.contains(PAGE_ONE_TEXT) && stdout.contains(PAGE_TWO_TEXT));
}

#[test]
fn json_carries_the_text_of_every_page() {
    let pdf = fixture_path("page_boundaries_json");
    let (ok, stdout) = run(&["extract-text", pdf.to_str().unwrap(), "--json"]);
    cleanup(&pdf);
    assert!(ok);

    let envelope: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON envelope");
    let data = &envelope["data"];
    assert_eq!(data["page_count"], 2);

    let pages = data["pages"].as_array().expect("`pages` array present");
    assert_eq!(pages.len(), 2, "one entry per page");
    assert_eq!(pages[0]["page"], 1, "pages are numbered from one");
    assert_eq!(pages[1]["page"], 2);
    assert!(
        pages[0]["text"]
            .as_str()
            .expect("page text present")
            .contains(PAGE_ONE_TEXT),
        "page one carries its own text: {:?}",
        pages[0]
    );
    assert!(
        pages[1]["text"]
            .as_str()
            .expect("page text present")
            .contains(PAGE_TWO_TEXT),
        "page two carries its own text: {:?}",
        pages[1]
    );
    assert!(
        pages[0]["char_count"].as_u64().unwrap_or(0) > 0,
        "a page states how much text it holds"
    );
}

#[test]
fn the_file_written_by_out_matches_stdout() {
    let pdf = fixture_path("page_boundaries_out");
    let out = std::env::temp_dir().join("pdfluent_cli_page_boundaries_out.txt");
    let _ = std::fs::remove_file(&out);

    let (ok, stdout) = run(&["extract-text", pdf.to_str().unwrap()]);
    assert!(ok);
    let (ok_out, _) = run(&[
        "extract-text",
        pdf.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(ok_out);

    let written = std::fs::read_to_string(&out).expect("read written file");
    cleanup(&pdf);
    let _ = std::fs::remove_file(&out);
    assert_eq!(
        written, stdout,
        "--out writes the same bytes the terminal sees, separators included"
    );
}
