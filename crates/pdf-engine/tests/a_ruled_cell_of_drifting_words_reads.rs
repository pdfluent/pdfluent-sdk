//! A ruled table whose cell holds words at drifting heights reads, and does
//! not abort the page (#574, 28-09-2026).
//!
//! The lattice reader ordered a cell's words with a comparator that called two
//! words one line when their tops lay within half the FIRST word's height --
//! neither symmetric nor transitive. Rust's sort panics on such a comparator,
//! the release profile makes that an abort, and five DocLayNet validation
//! pages lost everything. The cell below holds 24 words, found by a search
//! over random cells run against master `a3d709b22`, that abort that
//! comparator; scaled so they are type.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::structured_batch::markdown_of_bytes;

/// (x, top, height) of each word in the probe's units: tops 96-100, heights
/// 0.5-1.5.
const WORDS: [(f64, f64, f64); 24] = [
    (13.436424411240122, 97.4576987891883, 1.2637746189766141),
    (25.50690257394217, 98.51369473872418, 0.9494910647887381),
    (65.15929727227629, 97.63382994659347, 0.5938595867742349),
    (2.834747652200631, 97.4927046882404, 0.9327670679050534),
    (76.2280082457942, 99.99368183994667, 0.9453871940548014),
    (72.15400323407826, 99.31371333618864, 1.4452706955539223),
    (90.14274576114836, 99.90823005089933, 0.5254458609934608),
    (54.141247279349656, 97.18255251166447, 0.8812042376882124),
    (21.659939713061338, 98.73365027325185, 0.5290407875748679),
    (22.169166627303504, 98.68633721904828, 0.9958122413818506),
    (23.308445025757262, 99.30740037537704, 0.7187810373376886),
    (45.96034657377336, 99.13065515622854, 0.5214897052659089),
    (83.75779756625728, 98.3306370320427, 1.1422943629324456),
    (18.59062658947177, 97.0223697634718, 1.3599465287952899),
    (12.088995980580641, 99.00191444391962, 1.2214844075832683),
    (71.11917696952797, 97.19067823960162, 0.9221069999614152),
    (83.0035693274327, 97.98908330075778, 0.8033685109329176),
    (58.75806061435595, 97.35256299750442, 1.3461974184283128),
    (50.528382057960044, 98.23299322605234, 0.5345258301513416),
    (24.273997354306765, 97.6077872573371, 0.9143139993007743),
    (17.300740157905093, 98.35360371583555, 1.2030407620656316),
    (67.44858305023273, 98.87589093849508, 0.9389616300445631),
    (50.84264882499818, 97.66467215499956, 1.020938417613145),
    (39.325509496422605, 98.53091943861322, 0.5295749639669071),
];

fn page() -> Vec<u8> {
    let mut content = String::new();
    // A 2 x 2 grid, ruled, 50..550 by 400..750.
    for x in [50.0, 300.0, 550.0] {
        content += &format!("{x} 400 m {x} 750 l S\n");
    }
    for y in [400.0, 575.0, 750.0] {
        content += &format!("50 {y} m 550 {y} l S\n");
    }
    // The 24 words in the top-left cell, one text object each.
    for (x, top, height) in WORDS {
        let size = height * 10.0;
        let (px, ptop) = (55.0 + x * 2.3, 745.0 - (100.0 - top) * 40.0);
        content += &format!(
            "BT /F1 {size:.3} Tf {px:.3} {:.3} Td (w) Tj ET\n",
            ptop - size
        );
    }
    // One word in each other cell, so the grid holds its words.
    for (x, y) in [(400.0, 650.0), (150.0, 480.0), (400.0, 480.0)] {
        content += &format!("BT /F1 10 Tf {x} {y} Td (cell) Tj ET\n");
    }
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Courier".to_vec()),
    }));
    let contents = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
    let pages = doc.new_object_id();
    let page = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => contents,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
    });
    doc.objects.insert(
        pages,
        Object::Dictionary(
            dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 },
        ),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
    doc.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save");
    bytes
}

#[test]
fn a_ruled_cell_of_drifting_words_does_not_abort_the_page() {
    let bytes = page();
    if let Some(path) = std::env::var_os("DRIFTING_CELL_PDF") {
        std::fs::write(path, &bytes).expect("write");
    }
    let markdown = markdown_of_bytes(bytes).expect("markdown");
    assert!(markdown.contains("cell"), "{markdown}");
}
