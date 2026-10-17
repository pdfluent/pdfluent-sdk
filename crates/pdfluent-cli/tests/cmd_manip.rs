//! Behaviour tests for `pdfluent merge`, `split`, and `pages`.
//!
//! Page order and membership are asserted through extracted text, so a
//! merge that concatenates in the wrong order fails here, not in review.

// CLI covers: merge
// CLI covers: split
// CLI covers: pages

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

mod common;

use common::{page_count_of, run, run_with_stdin, stdout_json, tempdir, text_of};

fn two(n: usize) -> Vec<u8> {
    let texts: Vec<String> = (1..=n).map(|i| format!("content of page {i}")).collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    common::pdf_with_texts(&refs)
}

#[test]
fn merge_concatenates_in_order() {
    let dir = tempdir("merge");
    let a = dir.join("a.pdf");
    let b = dir.join("b.pdf");
    std::fs::write(&a, common::pdf_with_texts(&["first document"])).unwrap();
    std::fs::write(&b, common::pdf_with_texts(&["second document"])).unwrap();
    let merged = dir.join("merged.pdf");
    let out = run(&[
        "merge",
        &a.to_string_lossy(),
        &b.to_string_lossy(),
        "-o",
        &merged.to_string_lossy(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(page_count_of(&merged), 2);
    let text = text_of(&merged);
    let first = text.find("first document").expect("first kept");
    let second = text.find("second document").expect("second kept");
    assert!(first < second, "merge order: {text}");
}

#[test]
fn merge_reports_json_and_reads_stdin() {
    let dir = tempdir("merge-stdin");
    let a = dir.join("a.pdf");
    std::fs::write(&a, two(1)).unwrap();
    let bytes = std::fs::read(&a).unwrap();
    let merged = dir.join("merged.pdf");
    let arg = merged.to_string_lossy().to_string();
    let a_arg = a.to_string_lossy().to_string();
    let out = run_with_stdin(&["--json", "merge", "-", &a_arg, "-o", &arg], &bytes);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value = stdout_json(&out);
    assert_eq!(value["data"]["page_count"], 2);
    assert_eq!(page_count_of(&merged), 2);
}

#[test]
fn merge_bookmarks_flatten_and_discard_are_refused() {
    // The facade accepts FlattenAll/Discard but renders them concat-like,
    // so the CLI refuses them instead of silently mis-merging bookmarks.
    let dir = tempdir("merge-bookmarks");
    let a = dir.join("a.pdf");
    let b = dir.join("b.pdf");
    std::fs::write(&a, common::pdf_with_texts(&["first"])).unwrap();
    std::fs::write(&b, common::pdf_with_texts(&["second"])).unwrap();
    for value in ["flatten", "discard"] {
        let merged = dir.join(format!("m-{value}.pdf"));
        let out = run(&[
            "merge",
            &a.to_string_lossy(),
            &b.to_string_lossy(),
            "--bookmarks",
            value,
            "-o",
            &merged.to_string_lossy(),
        ]);
        assert_eq!(out.status.code(), Some(3), "{value}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("BOOKMARKS_NOT_SUPPORTED"), "{value}: {err}");
        assert!(err.contains("facade"), "{value}: {err}");
        assert!(!merged.exists(), "{value}: no output may be written");
    }
}

#[test]
fn merge_rejects_two_stdin_inputs() {
    let out = run(&["merge", "-", "-", "-o", "-"]);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn merge_refuses_output_equal_to_input() {
    let dir = tempdir("merge-self");
    let a = dir.join("a.pdf");
    let bytes = common::pdf_with_texts(&["stays a PDF"]);
    std::fs::write(&a, &bytes).unwrap();
    let arg = a.to_string_lossy().to_string();
    let out = run(&["merge", &arg, "-o", &arg]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("is the input file"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(std::fs::read(&a).unwrap(), bytes);
}

#[test]
fn merge_refuses_existing_output_without_force() {
    let dir = tempdir("merge-clobber");
    let a = dir.join("a.pdf");
    std::fs::write(&a, common::pdf_with_texts(&["A"])).unwrap();
    let merged = dir.join("merged.pdf");
    std::fs::write(&merged, "previous run").unwrap();
    let out = run(&[
        "merge",
        &a.to_string_lossy(),
        "-o",
        &merged.to_string_lossy(),
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("--force"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(std::fs::read(&merged).unwrap(), b"previous run");
    let out = run(&[
        "merge",
        &a.to_string_lossy(),
        "--force",
        "-o",
        &merged.to_string_lossy(),
    ]);
    assert!(out.status.success());
    assert!(std::fs::read(&merged).unwrap().starts_with(b"%PDF-"));
}

#[test]
fn merge_fails_on_missing_or_corrupt_input() {
    let dir = tempdir("merge-fail");
    let bad = dir.join("bad.pdf");
    std::fs::write(&bad, common::corrupt_bytes()).unwrap();
    let out = run(&["merge", "no-such.pdf", "-o", "x.pdf"]);
    assert_eq!(out.status.code(), Some(2));
    let out = run(&["merge", &bad.to_string_lossy(), "-o", "x.pdf"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn split_every_chunks_evenly() {
    let dir = tempdir("split-every");
    let doc = dir.join("doc.pdf");
    std::fs::write(&doc, two(5)).unwrap();
    let parts = dir.join("parts");
    let out = run(&[
        "split",
        &doc.to_string_lossy(),
        "--every",
        "2",
        "-o",
        &parts.to_string_lossy(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(page_count_of(&parts.join("pages-1-2.pdf")), 2);
    assert_eq!(page_count_of(&parts.join("pages-3-4.pdf")), 2);
    assert_eq!(page_count_of(&parts.join("page-5.pdf")), 1);
    assert!(text_of(&parts.join("page-5.pdf")).contains("content of page 5"));
}

#[test]
fn split_pages_writes_one_file_per_item() {
    let dir = tempdir("split-pages");
    let doc = dir.join("doc.pdf");
    std::fs::write(&doc, two(5)).unwrap();
    let parts = dir.join("parts");
    let out = run(&[
        "split",
        &doc.to_string_lossy(),
        "--pages",
        "1-3,5",
        "-o",
        &parts.to_string_lossy(),
    ]);
    assert!(out.status.success());
    assert_eq!(page_count_of(&parts.join("pages-1-3.pdf")), 3);
    assert_eq!(page_count_of(&parts.join("page-5.pdf")), 1);
}

#[test]
fn split_rejects_zero_every_and_out_of_range() {
    let dir = tempdir("split-fail");
    let doc = dir.join("doc.pdf");
    std::fs::write(&doc, two(2)).unwrap();
    let out = run(&["split", &doc.to_string_lossy(), "--every", "0", "-o", "x"]);
    assert_eq!(out.status.code(), Some(1));
    let out = run(&["split", &doc.to_string_lossy(), "--pages", "9", "-o", "x"]);
    assert_eq!(out.status.code(), Some(2));
    let out = run(&["split", &doc.to_string_lossy(), "-o", "x"]);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn pages_rotate_keeps_content() {
    let dir = tempdir("pages-rotate");
    let doc = dir.join("doc.pdf");
    std::fs::write(&doc, two(2)).unwrap();
    let rotated = dir.join("rotated.pdf");
    let out = run(&[
        "pages",
        &doc.to_string_lossy(),
        "--rotate",
        "90",
        "-o",
        &rotated.to_string_lossy(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(page_count_of(&rotated), 2);
    assert!(text_of(&rotated).contains("content of page 1"));
    // Unknown angles are usage errors, not guesses.
    let out = run(&[
        "pages",
        &doc.to_string_lossy(),
        "--rotate",
        "45",
        "-o",
        "x.pdf",
    ]);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn pages_delete_removes_only_named_pages() {
    let dir = tempdir("pages-delete");
    let doc = dir.join("doc.pdf");
    std::fs::write(&doc, two(3)).unwrap();
    let kept = dir.join("kept.pdf");
    let out = run(&[
        "pages",
        &doc.to_string_lossy(),
        "--delete",
        "2",
        "-o",
        &kept.to_string_lossy(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(page_count_of(&kept), 2);
    let text = text_of(&kept);
    assert!(text.contains("content of page 1"), "{text}");
    assert!(!text.contains("content of page 2"), "{text}");
    assert!(text.contains("content of page 3"), "{text}");
}

#[test]
fn pages_delete_refuses_an_empty_result() {
    let dir = tempdir("pages-delete-all");
    let doc = dir.join("doc.pdf");
    std::fs::write(&doc, two(2)).unwrap();
    let out = run(&[
        "pages",
        &doc.to_string_lossy(),
        "--delete",
        "1-2",
        "-o",
        "x.pdf",
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("EMPTY_RESULT"));
}

#[test]
fn pages_reorder_applies_the_new_order() {
    let dir = tempdir("pages-reorder");
    let doc = dir.join("doc.pdf");
    std::fs::write(&doc, two(3)).unwrap();
    let reordered = dir.join("reordered.pdf");
    let out = run(&[
        "pages",
        &doc.to_string_lossy(),
        "--reorder",
        "3,1,2",
        "-o",
        &reordered.to_string_lossy(),
    ]);
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = text_of(&reordered);
    let p3 = text.find("content of page 3").unwrap();
    let p1 = text.find("content of page 1").unwrap();
    let p2 = text.find("content of page 2").unwrap();
    assert!(p3 < p1 && p1 < p2, "reorder: {text}");
    // Not a permutation: usage against the document fails as input error.
    let out = run(&[
        "pages",
        &doc.to_string_lossy(),
        "--reorder",
        "1,1",
        "-o",
        "x.pdf",
    ]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn pages_extract_keeps_a_window() {
    let dir = tempdir("pages-extract");
    let doc = dir.join("doc.pdf");
    std::fs::write(&doc, two(4)).unwrap();
    let kept = dir.join("kept.pdf");
    let out = run(&[
        "pages",
        &doc.to_string_lossy(),
        "--extract",
        "2-3",
        "-o",
        &kept.to_string_lossy(),
    ]);
    assert!(out.status.success());
    assert_eq!(page_count_of(&kept), 2);
    assert!(text_of(&kept).contains("content of page 2"));
}

#[test]
fn pages_rejects_pages_without_rotate() {
    let dir = tempdir("pages-misuse");
    let doc = dir.join("doc.pdf");
    std::fs::write(&doc, two(2)).unwrap();
    let out = run(&[
        "pages",
        &doc.to_string_lossy(),
        "--delete",
        "1",
        "--pages",
        "1",
        "-o",
        "x.pdf",
    ]);
    assert_eq!(out.status.code(), Some(1));
}
