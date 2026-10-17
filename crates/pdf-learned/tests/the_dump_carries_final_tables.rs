// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! `pdfluent.features/18`: every record carries the tables the pass would
//! emit, with their final-table vectors (#574, #638).
//!
//! Synthetic pages only: a ruled table and a page of prose. The dump's
//! `final_tables` equal the tables `extract_blocks` emits on the same page --
//! same count, same boxes -- with the proposer kind, the 29 features in
//! `final_table_names` order, and the engine/decisions stamp beside each.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::{BlockKind, ExtractOptions, PdfDocument};
use pdf_learned::{dump_pdf, SCHEMA};
use serde_json::Value;

/// A thin line: a filled rectangle 0.5 pt across, as producers draw rulings.
fn rule(operations: &mut Vec<Operation>, x0: f64, y0: f64, x1: f64, y1: f64) {
    let (w, h) = if (y1 - y0).abs() < 1e-9 {
        (x1 - x0, 0.5)
    } else {
        (0.5, y1 - y0)
    };
    operations.push(Operation::new(
        "re",
        vec![x0.into(), y0.into(), w.into(), h.into()],
    ));
    operations.push(Operation::new("f", vec![]));
}

fn line(operations: &mut Vec<Operation>, text: &str, x: f64, y: f64) {
    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F1".to_vec()), 9.into()],
    ));
    operations.push(Operation::new("Td", vec![x.into(), y.into()]));
    operations.push(Operation::new("Tj", vec![Object::string_literal(text)]));
    operations.push(Operation::new("ET", vec![]));
}

/// A table the page draws as a grid: lines between every row and column, a
/// box round the lot. Where the page draws the grid the table is the grid,
/// so this is detected whatever the heuristics make of the text alone.
fn grid_table(operations: &mut Vec<Operation>) {
    let xs = [72.0, 172.0, 372.0, 540.0];
    let ys = [700.0, 680.0, 640.0, 600.0, 560.0];
    for &y in &ys {
        rule(operations, xs[0], y, xs[3], y);
    }
    rule(operations, xs[0], ys[4], xs[0], ys[0]);
    rule(operations, xs[3], ys[4], xs[3], ys[0]);
    rule(operations, xs[1], ys[4], xs[1], ys[0]);
    rule(operations, xs[2], ys[4], xs[2], ys[0]);
    line(operations, "Region", 76.0, 686.0);
    line(operations, "Revenue", 176.0, 686.0);
    line(operations, "Share", 376.0, 686.0);
    for (k, (a, b, c)) in [("North", "12,345", "41%"), ("South", "23,456", "59%")]
        .iter()
        .enumerate()
    {
        let y = ys[k + 1] - 14.0;
        line(operations, a, 76.0, y);
        line(operations, b, 176.0, y);
        line(operations, c, 376.0, y);
    }
}

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

/// Tables as the markdown serializer reads them: runs of adjacent cells in
/// row-major order, broken by another block or a page change.
fn emitted_tables(blocks: &[pdf_engine::Block], page: usize) -> Vec<[f64; 4]> {
    let mut tables = Vec::new();
    let mut current: Vec<[f64; 4]> = Vec::new();
    let mut last: Option<(u16, u16)> = None;
    let flush = |current: &mut Vec<[f64; 4]>, tables: &mut Vec<[f64; 4]>| {
        if current.is_empty() {
            return;
        }
        let mut bbox = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for b in current.iter() {
            bbox[0] = bbox[0].min(b[0]);
            bbox[1] = bbox[1].min(b[1]);
            bbox[2] = bbox[2].max(b[2]);
            bbox[3] = bbox[3].max(b[3]);
        }
        tables.push(bbox);
        current.clear();
    };
    for b in blocks.iter().filter(|b| b.page == page) {
        match b.kind {
            BlockKind::TableCell { row, column, .. } => {
                let continues = last.is_some_and(|(r, c)| row > r || (row == r && column > c));
                if !continues {
                    flush(&mut current, &mut tables);
                }
                last = Some((row, column));
                current.push(b.bbox);
            }
            _ => {
                flush(&mut current, &mut tables);
                last = None;
            }
        }
    }
    flush(&mut current, &mut tables);
    tables
}

#[test]
fn the_dump_s_final_tables_are_the_tables_extract_blocks_emits() {
    let mut table_page = Vec::new();
    grid_table(&mut table_page);
    let mut prose_page = Vec::new();
    line(
        &mut prose_page,
        "A page of prose draws no table.",
        72.0,
        700.0,
    );
    let pdf = pdf_of(vec![table_page, prose_page]);

    let records: Vec<Value> = dump_pdf(pdf.clone(), None)
        .expect("dump")
        .lines()
        .map(|l| serde_json::from_str(l).expect("a record is JSON"))
        .collect();
    assert_eq!(records.len(), 2, "one record per page");
    assert_eq!(records[0]["schema"], SCHEMA);

    let emitted = PdfDocument::open(pdf)
        .expect("open")
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");

    for (page, record) in records.iter().enumerate() {
        let finals = record["final_tables"]
            .as_array()
            .expect("final_tables is an array");
        let names = record["final_table_names"]
            .as_array()
            .expect("final_table_names is an array");
        assert_eq!(
            names.len(),
            pdf_engine::learned::tables::TABLE_IS_V2_FEATURES,
            "page {page}: the feature count moved"
        );
        for entry in finals {
            let features = entry["features"].as_array().expect("features");
            assert_eq!(features.len(), names.len(), "page {page}: a short vector");
            assert!(
                entry["kind"].as_str().is_some_and(|k| !k.is_empty()),
                "page {page}: a table without a proposer kind"
            );
            assert_eq!(
                entry["engine"]["tree"], record["engine"]["tree"],
                "page {page}: the tree stamp is the record's"
            );
            assert_eq!(
                entry["engine"]["decisions"], record["engine"]["decisions"],
                "page {page}: the decisions stamp is the record's"
            );
        }
        let want = emitted_tables(&emitted, page);
        assert_eq!(
            finals.len(),
            want.len(),
            "page {page}: the dump names {} tables and the pass emits {}",
            finals.len(),
            want.len()
        );
        for (entry, bbox) in finals.iter().zip(&want) {
            let got = entry["bbox"].as_array().expect("bbox");
            let got = [
                got[0].as_f64().expect("x0"),
                got[1].as_f64().expect("y0"),
                got[2].as_f64().expect("x1"),
                got[3].as_f64().expect("y1"),
            ];
            for k in 0..4 {
                assert!(
                    (got[k] - bbox[k]).abs() <= 1e-6,
                    "page {page}: box {got:?} against emitted {bbox:?}"
                );
            }
        }
    }
    // And the table page actually holds a table, or this compares two empty
    // lists and proves nothing.
    assert!(
        !records[0]["final_tables"]
            .as_array()
            .expect("array")
            .is_empty(),
        "the ruled page names no table"
    );
    assert!(
        records[1]["final_tables"]
            .as_array()
            .expect("array")
            .is_empty(),
        "the prose page names a table"
    );
}
