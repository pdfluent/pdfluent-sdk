//! The feature dump carries what the list models say of every line (/7, #574),
//! for the table judge, which chooses its regions before `list_item` has
//! promoted a paragraph that opens with a marker (T4's request on #574).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_learned::dump_pdf;
use serde_json::Value;

/// One line of text at `x`, `y`.
fn line(operations: &mut Vec<Operation>, text: &str, x: f64, y: f64) {
    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F1".to_vec()), 10.into()],
    ));
    operations.push(Operation::new("Td", vec![x.into(), y.into()]));
    operations.push(Operation::new("Tj", vec![Object::string_literal(text)]));
    operations.push(Operation::new("ET", vec![]));
}

/// One PDF, one page per list of operations.
fn pdf_of(pages: Vec<Vec<Operation>>) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let pages_id = doc.new_object_id();
    let mut kids: Vec<Object> = Vec::new();
    for operations in pages {
        let content = Content { operations }.encode().expect("encode content");
        let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Contents" => content_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        });
        kids.push(page_id.into());
    }
    let count = kids.len() as i64;
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => kids, "Count" => count,
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save pdf");
    out
}

fn records(pdf: Vec<u8>) -> Vec<Value> {
    dump_pdf(pdf, None)
        .expect("dump")
        .lines()
        .map(|l| serde_json::from_str(l).expect("a record is JSON"))
        .collect()
}

#[test]
fn a_bulleted_line_reads_as_a_list_and_a_line_of_prose_does_not() {
    let mut page = Vec::new();
    line(
        &mut page,
        "The contractor must deliver the following items:",
        72.0,
        720.0,
    );
    for (i, text) in [
        "- a detailed cash-flow estimate for all payments",
        "- the programme of works for the first six months",
        "- the names of the key experts and their CVs",
        "- a copy of the insurance policies in force",
    ]
    .iter()
    .enumerate()
    {
        line(&mut page, text, 72.0, 700.0 - i as f64 * 14.0);
    }
    let mut prose = Vec::new();
    for (i, text) in [
        "A page of prose is no list, however long its lines run",
        "and however evenly they are set down the column, since",
        "nothing at the head of any of them says it is an item.",
    ]
    .iter()
    .enumerate()
    {
        line(&mut prose, text, 72.0, 700.0 - i as f64 * 14.0);
    }
    let got = records(pdf_of(vec![page, prose]));
    let signal = |page: usize| -> Vec<f64> {
        got[page]["list_signal"]
            .as_array()
            .expect("list_signal is an array")
            .iter()
            .map(|v| v.as_f64().unwrap_or(f64::NAN))
            .collect()
    };
    // One value per line, the lines `lines_of` gives block by block -- the
    // same count the words' blocks and lines add up to.
    let items = signal(0);
    assert_eq!(items.len(), 5, "{items:?}");
    assert!(
        items[1..].iter().all(|p| *p >= 0.5),
        "the four items: {items:?}"
    );
    let prose = signal(1);
    assert_eq!(prose.len(), 3, "{prose:?}");
    assert!(prose.iter().all(|p| *p < 0.5), "the prose: {prose:?}");
}

/// And the line model's classes, one row of four per line in the same order
/// (/8, #574): the list model and the table judge read `line_class_probs[k]`
/// beside `list_signal[k]`, so the two must count the same lines.
#[test]
fn every_line_carries_its_four_class_probabilities() {
    let mut page = Vec::new();
    line(
        &mut page,
        "The contractor must deliver the following items:",
        72.0,
        720.0,
    );
    for (i, text) in [
        "- a detailed cash-flow estimate for all payments",
        "- the programme of works for the first six months",
        "- the names of the key experts and their CVs",
    ]
    .iter()
    .enumerate()
    {
        line(&mut page, text, 72.0, 700.0 - i as f64 * 14.0);
    }
    let got = records(pdf_of(vec![page]));
    let classes = got[0]["line_class_probs"]
        .as_array()
        .expect("line_class_probs is an array");
    let signal = got[0]["list_signal"].as_array().expect("list_signal");
    assert_eq!(
        classes.len(),
        signal.len(),
        "one row per line, as list_signal"
    );
    assert!(!classes.is_empty());
    for (k, row) in classes.iter().enumerate() {
        let p: Vec<f64> = row
            .as_array()
            .expect("a row is an array")
            .iter()
            .map(|v| v.as_f64().expect("a probability"))
            .collect();
        assert_eq!(p.len(), 4, "line {k}: other, heading, list, table");
        let sum: f64 = p.iter().sum();
        assert!((sum - 1.0).abs() < 1e-3, "line {k} sums to {sum}: {p:?}");
    }
    // The items are a list's lines, in the model's own reading.
    let list = |k: usize| classes[k][2].as_f64().unwrap_or(0.0);
    assert!(
        (1..classes.len()).all(|k| list(k) >= 0.5),
        "the bulleted lines: {classes:?}"
    );
}

/// And the heading decision the pass takes on every block, as the table pass
/// reads it (/10, T4 on #574): `p_heading_pass` is the mean over the block's
/// lines of the ensemble that decides -- the graph network and `heading_lines`
/// averaged -- and NOT `p_heading`, the line model alone. A table model
/// trained on the one and served the other reads a shifted input.
#[test]
fn the_dump_carries_the_heading_probability_the_pass_decides_on() {
    use pdf_engine::learned::{document_vectors, heading_lines, line_gnn, models};
    let mut page = Vec::new();
    line(&mut page, "Operating leases", 72.0, 720.0);
    for (i, text) in [
        "The minimum lease payments of operating leases, where the lessor",
        "effectively retains substantially all the risks and benefits of",
        "ownership of the leased item, are recognised as an expense.",
    ]
    .iter()
    .enumerate()
    {
        line(&mut page, text, 72.0, 700.0 - i as f64 * 12.0);
    }
    let pdf = pdf_of(vec![page]);
    let got = records(pdf.clone());
    let doc = pdf_engine::PdfDocument::open(pdf).expect("open");
    let blocks = doc
        .extract_blocks(&pdf_engine::ExtractOptions::default().without_the_learned_pass())
        .expect("blocks");
    let size = (612.0, 792.0);
    let vectors = document_vectors(&blocks, &[size]);
    let rows = heading_lines::page_rows(&vectors);
    // THE NETWORK THE PASS RUNS, as the switches ship it.
    let net = if pdf_engine::learned::SWITCHES.line_gnn_v2 {
        &line_gnn::V2
    } else {
        &line_gnn::V1
    };
    let gnn = line_gnn::verdicts(&vectors, size, &[], net);
    let heading = net.class("heading").expect("a heading class");
    let mut at = 0;
    let rows_of_dump = got[0]["blocks"].as_array().expect("blocks");
    assert_eq!(rows_of_dump.len(), blocks.len());
    let mut compared = 0;
    for ((block, rows), dumped) in blocks.iter().zip(&rows).zip(rows_of_dump) {
        let n = rows.len();
        let want: f64 = rows
            .iter()
            .enumerate()
            .map(|(k, row)| {
                let p = 1.0 / (1.0 + (-models::heading_lines::predict_margin(row)).exp());
                0.5 * (f64::from(gnn.classes[at + k][heading]) + p)
            })
            .sum::<f64>()
            / n.max(1) as f64;
        at += n;
        if !block.text.trim().is_empty() && n > 0 {
            let have = dumped["p_heading_pass"]
                .as_f64()
                .expect("a paragraph or heading carries p_heading_pass");
            assert!(
                (have - want).abs() < 1e-9,
                "{:?}: dump {have} vs the pass's {want}",
                block.text
            );
            compared += 1;
        }
    }
    assert!(compared >= 2, "the heading and its paragraph were compared");
}
