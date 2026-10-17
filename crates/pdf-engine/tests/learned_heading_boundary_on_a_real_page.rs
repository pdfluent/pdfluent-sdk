// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A heading swallowed by the block under it, on a real page (#602, #574).
//!
//! This page was the operator test for `heading_ends`, the boundary model the
//! line model replaced: the council's name is set as a heading over its postal
//! address, and our line grouping reads the two as one paragraph. The boundary
//! model cut it loose; the line model does not, and this file says so.

use pdf_engine::PdfDocument;

const PAGE: &str = "corpus/PDFBOX-4322-3.pdf";

/// And as shipped, the line graph network cuts it again (#638).
///
/// `heading_lines` alone left this page uncut -- it read the council's name
/// over the address at 0.26 -- where the boundary model it replaced had cut
/// it. The graph network's link head sees that the name and the address are
/// two regions and cuts between them; it does not make the name a heading,
/// which DocLayNet would, and the test says only what it does.
#[test]
fn the_shipped_graph_network_cuts_the_name_off_the_address() {
    const { assert!(pdf_engine::learned::SWITCHES.line_gnn) };
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/PDFBOX-4322-3.pdf"
    );
    let doc = PdfDocument::open(std::fs::read(path).expect(PAGE)).expect("open");
    let blocks = doc
        .extract_blocks(&pdf_engine::ExtractOptions::default())
        .expect("blocks");
    let at = blocks
        .iter()
        .position(|b| b.text.trim_start().starts_with("Unit 31301 Box 25"))
        .expect("the address is a block of its own: the graph network did not cut it loose");
    assert!(at > 0, "nothing above the address");
    assert!(
        blocks[at - 1].text.contains("Transatlantic Council"),
        "the part above the cut is the council's name: {:?}",
        blocks[at - 1].text
    );
}
