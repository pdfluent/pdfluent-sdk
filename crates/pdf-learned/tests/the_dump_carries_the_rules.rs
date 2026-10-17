//! `pdfluent.features/4`: every record carries the rules its page drew, from
//! the same pass as its blocks (#598, for #600).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::PdfDocument;
use pdf_learned::{dump_pdf, SCHEMA};
use serde_json::Value;

/// The `READABLE` set of `scripts/extraction/learned/features.py`: the
/// register of schema names the Python readers accept.
fn readable_set() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/extraction/learned/features.py");
    let text = std::fs::read_to_string(&path).expect("features.py is readable");
    text.split("READABLE =")
        .nth(1)
        .expect("features.py names READABLE")
        .split('}')
        .next()
        .expect("READABLE closes")
        .to_string()
}

/// A hairline from `left` to `right` at `y`, as a table rules a row off.
fn rule(operations: &mut Vec<Operation>, left: f64, right: f64, y: f64) {
    operations.push(Operation::new(
        "re",
        vec![left.into(), y.into(), (right - left).into(), 0.18.into()],
    ));
    operations.push(Operation::new("f", vec![]));
}

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
fn a_ruled_page_carries_its_rules_and_a_plain_page_none() {
    let mut ruled = Vec::new();
    for (i, row) in ["Region Revenue", "North 12,345", "South 23,456"]
        .iter()
        .enumerate()
    {
        let y = 700.0 - i as f64 * 20.0;
        line(&mut ruled, row, 72.0, y);
        rule(&mut ruled, 70.0, 300.0, y + 13.0);
    }
    let mut plain = Vec::new();
    line(&mut plain, "A page of prose draws no line.", 72.0, 700.0);
    let got = records(pdf_of(vec![ruled, plain]));

    assert_eq!(got.len(), 2, "one record per page");
    assert_eq!(got[0]["schema"], SCHEMA);

    // The literal, so a bump is deliberate and the Python readers
    // (`features.READABLE`) are extended in the same change. A real number:
    // this fails if the placeholder ever comes back as the schema.
    assert_eq!(SCHEMA, "pdfluent.features/18");
    assert_ne!(
        got[0]["schema"], "pdfluent.features/final-tables-next",
        "the dump writes the alias: it must write /18"
    );
    let set = readable_set();
    assert!(
        set.contains("pdfluent.features/18"),
        "/18 is not in features.READABLE: \
         extend the Python readers in the same change"
    );
    assert!(
        set.contains("pdfluent.features/17"),
        "/17 is not in features.READABLE any more: keep it readable"
    );
    // The alias every v2 training dump carries: a retrain must still read
    // those dumps, so it stays in the register beside /18.
    assert!(
        set.contains("pdfluent.features/final-tables-next"),
        "the v2 dumps' name is not in features.READABLE: a retrain reads \
         those dumps through the alias"
    );

    let rules = got[0]["rules"]
        .as_array()
        .expect("page 0 has a rules array");
    assert_eq!(rules.len(), 3, "the page drew three rules: {rules:?}");
    let mut ys: Vec<f64> = rules.iter().map(|r| r["y"].as_f64().expect("y")).collect();
    ys.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    for (y, drawn) in ys.iter().zip([673.0, 693.0, 713.0]) {
        assert!(
            (y - drawn).abs() < 1.0,
            "a rule is not where it was drawn, in the frame of bbox (y up): {ys:?}"
        );
    }
    for r in rules {
        let (left, right) = (
            r["left"].as_f64().expect("left"),
            r["right"].as_f64().expect("right"),
        );
        assert!(
            left <= 70.5 && right >= 299.5 && left < right,
            "a rule came back the wrong width: {r}"
        );
    }

    // Page 1's own rules, not page 0's: the per-page lists stay in page order.
    assert_eq!(
        got[1]["rules"],
        serde_json::json!([]),
        "a page that drew nothing has no rules"
    );
}

/// `pdfluent.features/17`: every record carries the learned pass's doubt
/// about its page, the largest of its three parts.
#[test]
fn every_record_carries_the_pass_doubt_about_its_page() {
    let mut page = Vec::new();
    line(&mut page, "A page of prose doubts little.", 72.0, 700.0);
    line(&mut page, "Its second line doubts little too.", 72.0, 688.0);
    let got = records(pdf_of(vec![page]));
    assert_eq!(got.len(), 1, "one page in, one record out");
    // /17 is /16 plus the confidence: the /16 word keys are still there.
    let words = got[0]["words"].as_array().expect("words array");
    assert!(
        !words.is_empty()
            && words
                .iter()
                .all(|w| w.get("font").is_some() && w.get("color").is_some()),
        "/17 keeps every word's /16 face and colour: {}",
        got[0]["words"]
    );
    let c = &got[0]["confidence"];
    for key in ["doubt", "heading_doubt", "link_doubt", "class_entropy"] {
        assert!(c[key].is_number(), "/17 carries {key} as a number: {c}");
    }
    let doubt = c["doubt"].as_f64().expect("doubt");
    let parts: Vec<f64> = ["heading_doubt", "link_doubt", "class_entropy"]
        .iter()
        .map(|k| c[k].as_f64().expect(k))
        .collect();
    assert!(
        (0.0..=1.0).contains(&doubt) && parts.iter().all(|p| (0.0..=1.0).contains(p)),
        "doubt and its parts are shares: {c}"
    );
    let top = parts.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    assert!(
        (doubt - top).abs() < 1e-12,
        "doubt is the largest of its parts: {c}"
    );
}

#[test]
fn the_rules_come_from_the_pass_the_blocks_come_from() {
    let mut ops = Vec::new();
    line(&mut ops, "Header", 72.0, 700.0);
    rule(&mut ops, 70.0, 300.0, 713.0);
    let pdf = pdf_of(vec![ops]);
    let got = records(pdf.clone());
    // The record's blocks are the blocks before learning, the state the models
    // are trained on (`refine: "off"`), and the rules sit beside them.
    let before = PdfDocument::open(pdf)
        .expect("open")
        .extract_blocks_before_learning()
        .expect("blocks");
    let texts: Vec<&str> = got[0]["blocks"]
        .as_array()
        .expect("blocks")
        .iter()
        .map(|b| b["text"].as_str().expect("text"))
        .collect();
    assert_eq!(
        texts,
        before.iter().map(|b| b.text.as_str()).collect::<Vec<_>>()
    );
    assert_eq!(got[0]["refine"], "off");
    assert_eq!(got[0]["rules"].as_array().map(Vec::len), Some(1));
}
