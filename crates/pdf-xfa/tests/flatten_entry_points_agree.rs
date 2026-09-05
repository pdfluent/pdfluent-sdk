// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The combined flatten entry point returns what the separate ones return.
//!
//! `flatten_xfa_to_pdf_with_layout_dump_and_metadata` exists so a caller who
//! wants both does not run the pipeline twice. Its whole body is one call into
//! `flatten_xfa_to_pdf_internal` with a boolean that decides whether the layout
//! dump gets collected at all.
//!
//! Pass `false` for that boolean and the function still returns three values,
//! still compiles, still produces a correct PDF, and still reports correct
//! metadata. The only thing wrong is a `LayoutDump` with no pages in it — the
//! one thing the caller asked for over the cheaper entry point. Nothing else in
//! the tree would notice, which is why this test exists rather than a page-count
//! assertion.

use pdf_xfa::flatten::{
    flatten_xfa_to_pdf_with_layout_dump, flatten_xfa_to_pdf_with_layout_dump_and_metadata,
    flatten_xfa_to_pdf_with_metadata,
};
use std::path::PathBuf;

/// The dynamic fixture paginates to three pages (see
/// `dynamic_xfa_fixture_paginates.rs`), so the dump has three entries to have.
/// A one-page document would let an empty-dump bug through on a technicality.
const VERWACHTE_PAGINAS: usize = 3;

fn fixture() -> Vec<u8> {
    let pad = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("fixtures/xfa-layout/xl_31_dynamic_multipage_overflow.pdf");
    std::fs::read(&pad).unwrap_or_else(|e| {
        panic!(
            "{}: {e}. Regenerate with: cargo run -p xfa-test-runner \
             --example generate_xfa_layout_fixtures",
            pad.display()
        )
    })
}

#[test]
fn combined_entry_point_collects_the_layout_dump() {
    let bytes = fixture();
    let (_, dump, _) =
        flatten_xfa_to_pdf_with_layout_dump_and_metadata(&bytes).expect("fixture must flatten");

    assert_eq!(
        dump.pages.len(),
        VERWACHTE_PAGINAS,
        "the dump must carry one entry per rendered page; an empty dump is what \
         you get when the combined entry point forgets to ask for one, and \
         nothing else in the output changes"
    );
    let nummers: Vec<u32> = dump.pages.iter().map(|p| p.page_num).collect();
    assert_eq!(
        nummers,
        (1..=VERWACHTE_PAGINAS as u32).collect::<Vec<_>>(),
        "pages are numbered 1..n after renumbering"
    );
    assert!(
        dump.pages.iter().all(|p| p.page_height > 0.0),
        "a dump entry without a page height is a placeholder, not a measurement"
    );
}

#[test]
fn combined_entry_point_agrees_with_the_separate_ones() {
    let bytes = fixture();

    let (bytes_a, dump_a) =
        flatten_xfa_to_pdf_with_layout_dump(&bytes).expect("dump entry point must flatten");
    let (bytes_b, meta_b) =
        flatten_xfa_to_pdf_with_metadata(&bytes).expect("metadata entry point must flatten");
    let (bytes_c, dump_c, meta_c) =
        flatten_xfa_to_pdf_with_layout_dump_and_metadata(&bytes).expect("combined must flatten");

    assert_eq!(
        bytes_c.len(),
        bytes_a.len(),
        "the combined call must produce the same document as the dump-only call"
    );
    assert_eq!(
        bytes_c.len(),
        bytes_b.len(),
        "and the same one as the metadata-only call"
    );

    assert_eq!(
        dump_c.pages.len(),
        dump_a.pages.len(),
        "the dump must be the dump the dedicated entry point returns, not an \
         empty stand-in"
    );
    assert_eq!(
        format!("{:?}", dump_c.output_quality),
        format!("{:?}", dump_a.output_quality),
    );

    assert_eq!(
        format!("{:?}", meta_c.rendering_policy),
        format!("{:?}", meta_b.rendering_policy),
        "the combined call uses the same default policy as the others"
    );
    assert_eq!(
        format!("{:?}", meta_c.dynamic_scripts),
        format!("{:?}", meta_b.dynamic_scripts),
    );
}
