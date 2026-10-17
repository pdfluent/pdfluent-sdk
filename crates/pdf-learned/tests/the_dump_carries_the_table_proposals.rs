//! The feature dump carries the table regions the engine proposed (/6, #574):
//! every region a detector accepted, as the indices of the page's lines it
//! holds, for the table judge to choose between (T4's interface on #574).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_learned::dump_pdf;
use serde_json::Value;

/// A hairline from `left` to `right` at `y`, as a table rules a row off.
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
fn a_table_is_proposed_by_its_lines_and_prose_is_not() {
    let mut table = Vec::new();
    line(
        &mut table,
        "Revenue by region for the year, in thousands.",
        72.0,
        720.0,
    );
    for (i, (region, a, b)) in [
        ("Region", "2013", "2012"),
        ("North", "12,345", "11,002"),
        ("South", "23,456", "21,873"),
        ("East", "9,120", "8,774"),
    ]
    .iter()
    .enumerate()
    {
        let y = 690.0 - i as f64 * 14.0;
        line(&mut table, region, 72.0, y);
        line(&mut table, a, 300.0, y);
        line(&mut table, b, 400.0, y);
    }
    let mut prose = Vec::new();
    for (i, text) in [
        "A page of prose proposes no table at all, however long",
        "its lines run and however evenly they are set down the",
        "column, because no two of its lines break at one place.",
    ]
    .iter()
    .enumerate()
    {
        line(&mut prose, text, 72.0, 700.0 - i as f64 * 14.0);
    }
    // Two rows of a grid: too few for a table, a proposal the judge may rescue.
    let mut two_rows = Vec::new();
    for (i, (region, a, b)) in [("North", "12,345", "11,002"), ("South", "23,456", "21,873")]
        .iter()
        .enumerate()
    {
        let y = 690.0 - i as f64 * 14.0;
        line(&mut two_rows, region, 72.0, y);
        line(&mut two_rows, a, 300.0, y);
        line(&mut two_rows, b, 400.0, y);
    }
    let got = records(pdf_of(vec![table, prose, two_rows]));
    let proposals = got[0]["table_proposals"]
        .as_array()
        .expect("an array on page 0");
    let aligned: Vec<&Value> = proposals
        .iter()
        .filter(|p| p["detector"] == "aligned")
        .collect();
    assert_eq!(aligned.len(), 1, "one aligned region: {proposals:?}");
    // Lines are `lines_of` block by block, and a table's cells are blocks of
    // their own: four rows of three cells are twelve lines, and the caption --
    // line 0, the page's first block -- is not one of them.
    let lines: Vec<u64> = aligned[0]["lines"]
        .as_array()
        .expect("lines")
        .iter()
        .map(|l| l.as_u64().expect("an index"))
        .collect();
    assert_eq!(lines, (1..=12).collect::<Vec<u64>>(), "{proposals:?}");
    // And #632's alignment detector, on the same lines, proposes the same set.
    let by_alignment: Vec<&Value> = proposals
        .iter()
        .filter(|p| p["detector"] == "alignment_lines")
        .collect();
    assert_eq!(by_alignment.len(), 1, "{proposals:?}");
    assert_eq!(
        by_alignment[0]["lines"], aligned[0]["lines"],
        "{proposals:?}"
    );
    assert_eq!(
        got[1]["table_proposals"],
        Value::Array(Vec::new()),
        "prose proposes nothing"
    );
    assert_eq!(aligned[0]["accepted"], true, "{proposals:?}");
    let rescued: Vec<&Value> = got[2]["table_proposals"]
        .as_array()
        .expect("an array on page 2")
        .iter()
        .filter(|p| p["detector"] == "aligned")
        .collect();
    assert_eq!(rescued.len(), 1, "{rescued:?}");
    assert_eq!(rescued[0]["accepted"], false, "{rescued:?}");
}

/// A page in two columns: prose on the left, a table of keys and descriptions
/// on the right. Every band of the table also holds a line of the prose, so on
/// the whole page no row is a key and a description alone; the column read as
/// a page of its own proposes it (#574).
#[test]
fn a_table_in_one_column_of_two_is_proposed() {
    let mut page = Vec::new();
    let prose = [
        "The company reports its results by segment,",
        "and each segment is described in the table",
        "beside this paragraph, which the annual report",
        "sets in the right-hand column of the page as",
        "it does for every other such list of segments",
        "and the businesses that each of them conducts.",
        "The descriptions are those of the segments as",
        "they stood at the end of the fiscal year and",
        "they do not reflect any later reorganisation.",
    ];
    for (i, text) in prose.iter().enumerate() {
        line(&mut page, text, 72.0, 700.0 - i as f64 * 14.0);
    }
    for (i, (key, what)) in [
        ("Segment", "Business conducted"),
        ("Grocery", "Shelf-stable food products"),
        ("Refrigerated", "Meat products and foodservice"),
        ("Specialty", "Private label and portion packs"),
        ("Turkey", "Turkey products for retail"),
    ]
    .iter()
    .enumerate()
    {
        let y = 700.0 - i as f64 * 14.0;
        line(&mut page, key, 330.0, y);
        line(&mut page, what, 400.0, y);
    }
    let got = records(pdf_of(vec![page]));
    let proposals = got[0]["table_proposals"].as_array().expect("an array");
    assert!(
        proposals
            .iter()
            .any(|p| p["detector"] == "pair" && p["accepted"] == true),
        "the right column's table is proposed: {proposals:?}"
    );
}

/// A hairline rule from `left` to `right` at `y`.
fn rule(operations: &mut Vec<Operation>, left: f64, right: f64, y: f64) {
    operations.push(Operation::new("w", vec![0.5.into()]));
    operations.push(Operation::new("m", vec![left.into(), y.into()]));
    operations.push(Operation::new("l", vec![right.into(), y.into()]));
    operations.push(Operation::new("S", vec![]));
}

/// What the page draws proposes too (#574): two rules of one measure frame
/// what stands between them, and the dump offers it under a source of its own
/// -- `ruled` -- which the judge's reader leaves to the verifier.
#[test]
fn rules_that_frame_a_region_propose_it_under_their_own_source() {
    let mut operations = Vec::new();
    rule(&mut operations, 70.0, 450.0, 706.0);
    for (k, text) in [
        "Revenue from contracts with customers",
        "Interest income on financial assets",
        "Other operating income and gains",
        "Total income for the year",
    ]
    .iter()
    .enumerate()
    {
        line(&mut operations, text, 72.0, 690.0 - 14.0 * k as f64);
    }
    rule(&mut operations, 70.0, 450.0, 630.0);
    let record = &records(pdf_of(vec![operations]))[0];
    let ruled: Vec<&Value> = record["table_proposals"]
        .as_array()
        .expect("table_proposals")
        .iter()
        .filter(|p| p["source"] == "ruled")
        .collect();
    assert!(
        ruled.iter().any(|p| p["detector"] == "ruled_frame"
            && p["lines"].as_array().is_some_and(|l| l.len() == 4)),
        "{}",
        record["table_proposals"]
    );
}
