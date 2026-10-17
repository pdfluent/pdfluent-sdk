// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Table confidence per page, for the quality="auto" router (#574).
//!
//! Every page reports how sure the table pass is, from the numbers the pass
//! already computes while deciding. Each test here is red without its feature:
//! take the signals out and the page reports `None` where it should report
//! doubt, or the blocks lose the `p_judge` they carry.

use pdf_engine::learned::{judge, table_signals_with, SWITCHES};
use pdf_engine::{BlockKind, ExtractOptions, PdfDocument};

fn nasdaq() -> PdfDocument {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/doclaynet/nasdaq_wwd_2002_p7.pdf"
    ))
    .expect("fixture");
    PdfDocument::open(bytes).expect("open")
}

/// `extract_blocks` is byte-identical with and without the signals requested:
/// the signals ride beside the decision, never through a global or a cache.
#[test]
fn extract_blocks_is_byte_identical_with_and_without_signals_requested() {
    let fixtures = [
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/pubtables/pmc2959354_p3_crystal_data.pdf"
        ),
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/pubtables/pmc1064095_p3_two_tables.pdf"
        ),
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/f1040.pdf"),
    ];
    for path in fixtures {
        let doc = PdfDocument::open(std::fs::read(path).expect("fixture")).expect("open");
        let before = doc
            .extract_blocks(&ExtractOptions::default())
            .expect("blocks");
        let before_json = pdf_engine::to_json(&before);
        let _ = doc.table_signals().expect("signals");
        let after = doc
            .extract_blocks(&ExtractOptions::default())
            .expect("blocks");
        assert_eq!(
            pdf_engine::to_json(&after),
            before_json,
            "{path}: asking for signals changed the blocks"
        );
    }
}

/// A ruled table reports one signal, sure; with v2's number on the shipped
/// default, with the builder's with v2 off.
///
/// A real ruled page rather than a built 3x3: a built 3x3 scores 0.03, far
/// under the floor, because the judge wants realistic features. What this
/// holds on the shipped default is one signal, `p_judge` at or over the
/// floor, and `p_is_table` SET: v2 decides, so the signal carries v2's
/// probability -- the same number the refusal used -- and names v2, whose
/// threshold the router reads it against. That v1's number still reaches the
/// signal where v1 runs is held with v2 off, below.
#[test]
fn a_ruled_table_reports_one_signal() {
    use pdf_engine::learned::tables::{
        table_is_v2_probability, IsTableModel, TABLE_IS_V2_THRESHOLD,
    };

    let doc = nasdaq();
    let signals = doc.table_signals().expect("signals");
    assert_eq!(signals.len(), 1, "one page, one record");
    let page = signals[0].as_ref().expect("the judge ran, so Some");
    assert_eq!(page.tables.len(), 1, "one table emitted: {page:?}");
    let table = &page.tables[0];
    assert!(
        table.p_judge >= judge::FLOOR,
        "p_judge {} under the floor {}",
        table.p_judge,
        judge::FLOOR
    );
    let p = table.p_is_table.unwrap_or_else(|| {
        panic!("v2 decided this table, so its probability is on the signal: {table:?}")
    });
    assert_eq!(
        table.p_is_table_model,
        Some(IsTableModel::V2),
        "the number is v2's, so the model is V2: {table:?}"
    );
    assert_eq!(
        IsTableModel::V2.threshold(),
        0.22,
        "v2's threshold moved from the shipped 0.22"
    );
    assert_eq!(
        IsTableModel::V2.threshold(),
        TABLE_IS_V2_THRESHOLD,
        "the method drifted from the threshold the refusal uses"
    );
    assert!(
        p >= TABLE_IS_V2_THRESHOLD,
        "a kept table under v2's threshold: {p} < {TABLE_IS_V2_THRESHOLD}"
    );

    // THE NUMBER THE REFUSAL SAW: the dump reads the same final table off
    // its cells and v2 scores its features again -- the same probability,
    // recomputed from the blocks rather than copied from the verdict.
    let (blocks, rules) = doc
        .extract_blocks_and_rules_before_learning()
        .expect("blocks");
    let size = doc
        .page_geometry(0)
        .map(|g| g.effective_dimensions())
        .unwrap_or((0.0, 0.0));
    let rulings = doc.page_rulings(0).expect("rulings");
    let images = doc.extract_image_boxes().expect("images");
    let facts = pdf_engine::learned::document_facts(&blocks);
    let finals = pdf_engine::learned::final_tables_for_dump_with(
        pdf_engine::learned::page_blocks(&blocks, 0),
        size,
        &facts,
        &rules[0],
        &rulings,
        &images[0],
        SWITCHES,
    );
    let again = finals
        .iter()
        .find(|t| t.bbox == table.bbox)
        .expect("the signal's table among the final tables");
    assert_eq!(
        table_is_v2_probability(&again.features),
        p,
        "the signal's number is not the probability the refusal saw"
    );

    // WITH V2 OFF, v1 runs, and a builder table stands behind this one: its
    // number reaches the signal, as before, and names v1.
    let sizes = vec![size];
    let rulings = vec![rulings];
    let mut off = SWITCHES;
    off.table_is_v2 = false;
    let signals = table_signals_with(&blocks, &sizes, &rules, &rulings, &images, off);
    assert_eq!(signals.len(), 1, "one page, one record");
    let page = signals[0].as_ref().expect("the judge ran, so Some");
    assert_eq!(page.tables.len(), 1, "one table emitted: {page:?}");
    let v1 = page.tables[0].p_is_table.unwrap_or_else(|| {
        panic!(
            "a builder table stands behind this one: {:?}",
            page.tables[0]
        )
    });
    assert_eq!(
        page.tables[0].p_is_table_model,
        Some(IsTableModel::V1),
        "the number is v1's, so the model is V1: {:?}",
        page.tables[0]
    );
    assert_eq!(
        IsTableModel::V1.threshold(),
        pdf_engine::learned::models::table_is::THRESHOLD,
        "the method drifted from v1's threshold"
    );
    assert!(
        v1 >= IsTableModel::V1.threshold(),
        "a kept table under v1's threshold: {v1}"
    );
}

/// A page of prose reports no tables and no missed table.
///
/// Built rather than shipped: four lines of running text, no gutters, no
/// rules. `p_best_rejected` writes as an absent field when there is none.
#[test]
fn a_page_of_prose_reports_no_tables() {
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Document, Object, Stream};

    let mut built = Document::with_version("1.7");
    let font = Object::Reference(built.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Courier".to_vec()),
    })));
    let mut ops = Vec::new();
    for (i, line) in [
        "The quick brown fox jumps over the lazy dog and keeps going.",
        "Pack my box with five dozen liquor jugs and a few more words.",
        "How vexingly quick daft zebras jump over the brown fox nearby.",
        "The five boxing wizards jump quickly over the lazy dogs outside.",
    ]
    .iter()
    .enumerate()
    {
        ops.push(Operation::new("BT", vec![]));
        ops.push(Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), 10.0.into()],
        ));
        ops.push(Operation::new(
            "Td",
            vec![72.0.into(), (700.0 - i as f64 * 14.0).into()],
        ));
        ops.push(Operation::new("Tj", vec![Object::string_literal(*line)]));
        ops.push(Operation::new("ET", vec![]));
    }
    let content = Content { operations: ops }.encode().expect("encode");
    let content_id = built.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
    let page_id = built.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! { "F1" => font }),
        }),
    }));
    let pages_id = built.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![Object::Reference(page_id)]),
        "Count" => Object::Integer(1),
    }));
    if let Ok(Object::Dictionary(page)) = built.get_object_mut(page_id) {
        page.set("Parent", Object::Reference(pages_id));
    }
    let catalog_id = built.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    }));
    built.trailer.set("Root", Object::Reference(catalog_id));
    let mut buf = Vec::new();
    built.save_to(&mut buf).expect("save");

    let doc = PdfDocument::open(buf).expect("open");
    let signals = doc.table_signals().expect("signals");
    assert_eq!(signals.len(), 1);
    let page = signals[0].as_ref().expect("asked, so Some");
    assert!(page.tables.is_empty(), "prose is no table: {page:?}");
    assert!(
        page.p_best_rejected.is_none_or(|p| p < judge::FLOOR),
        "nothing missed above the floor: {page:?}"
    );
    if page.p_best_rejected.is_none() {
        assert!(
            !page.to_json().contains("p_best_rejected"),
            "None writes as absent, never as 0: {}",
            page.to_json()
        );
    }
}

/// Two candidates for the same lines: the margin is the score difference the
/// verifier's `choose` saw.
#[test]
fn competing_candidates_report_the_verifier_margin() {
    use pdf_engine::learned::features::PageWord;
    use pdf_engine::learned::judge::{Line, Proposal};
    use pdf_engine::learned::verifier;

    let lines: Vec<Line> = (0..4)
        .map(|i| {
            let y = 700.0 - i as f64 * 14.0;
            Line {
                bbox: [60.0, y, 540.0, y + 10.0],
                text: format!("row {i} with several words in it"),
                words: 6,
                digits: 1,
                kind: "paragraph",
                table: None,
                grid_fill: f64::NAN,
                grid_columns: f64::NAN,
                block: i,
                list: f64::NAN,
            }
        })
        .collect();
    let words: Vec<PageWord> = (0..8)
        .map(|i| {
            let y = 700.0 - (i / 2) as f64 * 14.0;
            PageWord {
                text: format!("w{i}"),
                bbox: [
                    60.0 + (i % 2) as f64 * 200.0,
                    y,
                    140.0 + (i % 2) as f64 * 200.0,
                    y + 10.0,
                ],
                block: 0,
                span: 0,
                size: 10.0,
                bold: false,
                italic: false,
                font: None,
                color: None,
            }
        })
        .collect();
    let all = vec![
        Proposal {
            lines: vec![0, 1, 2],
            kind: "builder",
            accepted: true,
        },
        Proposal {
            lines: vec![1, 2, 3],
            kind: "aligned",
            accepted: true,
        },
    ];
    let probabilities = vec![0.9, 0.8];
    let traced = verifier::choose_traced(
        (612.0, 792.0),
        &lines,
        &words,
        &all,
        &probabilities,
        judge::FLOOR,
    );
    assert_eq!(traced.chosen.len(), 1, "one of the two wins");
    let winner = &traced.details[0];
    let margin = winner.margin.expect("a beaten candidate shares a line");
    // The loser's score, as `choose` computed it: the same features, the same
    // ranker, over the hulls `choose` read.
    let picked = verifier::candidates(&all, &probabilities);
    let hulls: Vec<[f64; 4]> = picked
        .iter()
        .map(|&i| verifier::hull_of(&lines, &all[i].lines))
        .collect();
    let mut scores: Vec<(usize, f64)> = Vec::new();
    for (k, &i) in picked.iter().enumerate() {
        if probabilities[i] < judge::FLOOR {
            continue;
        }
        let inside = verifier::inside_of(&words, &hulls[k]);
        let mine: Vec<&PageWord> = inside.iter().map(|&j| &words[j]).collect();
        let cells = verifier::grid(&mine).map(|g| g.cells).unwrap_or_default();
        let x = verifier::features(
            (612.0, 792.0),
            &lines,
            &words,
            &hulls[k],
            &hulls,
            k,
            probabilities[i],
            all[i].kind == "builder",
            &cells,
        );
        scores.push((i, verifier::score(&x)));
    }
    let loser = scores
        .iter()
        .find(|(i, _)| *i != winner.proposal)
        .expect("the beaten candidate was scored");
    assert!(
        (margin - (winner.score - loser.1)).abs() < 1e-12,
        "margin {margin} is not the score difference {} - {}",
        winner.score,
        loser.1
    );
}

/// A page the switch turns off reports `None`: not asked is not no table.
#[test]
fn a_page_the_switch_turns_off_reports_none() {
    let doc = nasdaq();
    let (blocks, rules) = doc
        .extract_blocks_and_rules_before_learning()
        .expect("blocks");
    let sizes = vec![doc
        .page_geometry(0)
        .map(|g| g.effective_dimensions())
        .unwrap_or((0.0, 0.0))];
    let rulings = vec![doc.page_rulings(0).expect("rulings")];
    let images = doc.extract_image_boxes().expect("images");
    let on = table_signals_with(&blocks, &sizes, &rules, &rulings, &images, SWITCHES);
    assert_eq!(on.len(), 1);
    assert!(on[0].is_some(), "with the switch on this page has signals");
    let mut off = SWITCHES;
    off.table_judge = false;
    let none = table_signals_with(&blocks, &sizes, &rules, &rulings, &images, off);
    assert_eq!(none.len(), 1);
    assert_eq!(none[0], None, "with the judge off the page was not asked");
}

/// A learned table carries its `p_judge`; any other table carries nothing and
/// the JSON omits it.
#[test]
fn a_learned_table_carries_its_p_judge() {
    let doc = nasdaq();
    let signals = doc.table_signals().expect("signals");
    let page = signals[0].as_ref().expect("signals");
    assert_eq!(page.tables.len(), 1);
    let p_judge = page.tables[0].p_judge;

    let blocks = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let cells: Vec<_> = blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::TableCell { .. }))
        .collect();
    assert!(!cells.is_empty(), "no table cells at all");
    for cell in &cells {
        assert_eq!(
            cell.confidence,
            Some(p_judge),
            "a learned cell without its judge's number: {cell:?}"
        );
    }
    assert!(
        pdf_engine::to_json(&blocks).contains("\"confidence\""),
        "the number never reached the JSON"
    );

    let plain = doc
        .extract_blocks(&ExtractOptions::default().without_the_learned_pass())
        .expect("blocks");
    let others: Vec<_> = plain
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::TableCell { .. }))
        .collect();
    assert!(!others.is_empty(), "no heuristic tables to compare with");
    assert!(
        others.iter().all(|b| b.confidence.is_none()),
        "a table nobody judged carries a number"
    );
    assert!(
        !pdf_engine::to_json(&plain).contains("\"confidence\""),
        "None writes as absent, never as 0"
    );
}

/// Blocks and signals from one pass: the signals match `table_signals`, and
/// the blocks match the normal extraction exactly.
#[test]
fn blocks_and_signals_come_from_one_pass() {
    let doc = nasdaq();
    let (blocks, signals) = doc.extract_blocks_with_signals().expect("combined");
    let alone = doc.table_signals().expect("signals");
    assert_eq!(signals.len(), alone.len(), "one entry per page");
    for (page, (a, b)) in signals.iter().zip(&alone).enumerate() {
        match (a, b) {
            (None, None) => {}
            (Some(a), Some(b)) => assert_eq!(
                a.to_json(),
                b.to_json(),
                "page {page}: the combined signals drifted from table_signals"
            ),
            _ => panic!("page {page}: Some against None"),
        }
    }
    let plain = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    assert_eq!(
        pdf_engine::to_json(&blocks),
        pdf_engine::to_json(&plain),
        "the combined blocks drifted from the normal extraction"
    );

    // The learned pass itself, once against twice.
    let (before, rules) = doc
        .extract_blocks_and_rules_before_learning()
        .expect("blocks");
    let sizes = [doc
        .page_geometry(0)
        .map(|g| g.effective_dimensions())
        .unwrap_or((0.0, 0.0))];
    let rulings = [doc.page_rulings(0).expect("rulings")];
    let images = doc.extract_image_boxes().expect("images");
    let (both_blocks, both_signals) = pdf_engine::learned::refine_with_images_and_signals(
        before.clone(),
        &sizes,
        &rules,
        &rulings,
        &images,
        pdf_engine::learned::SWITCHES,
    );
    let apart_blocks = pdf_engine::learned::refine_with_images(
        before.clone(),
        &sizes,
        &rules,
        &rulings,
        &images,
        pdf_engine::learned::SWITCHES,
    );
    assert_eq!(
        pdf_engine::to_json(&both_blocks),
        pdf_engine::to_json(&apart_blocks),
        "one pass drifted from the blocks pass"
    );
    let apart_signals = pdf_engine::learned::table_signals_with(
        &before,
        &sizes,
        &rules,
        &rulings,
        &images,
        pdf_engine::learned::SWITCHES,
    );
    assert_eq!(
        both_signals.len(),
        apart_signals.len(),
        "one entry per page"
    );
    for (a, b) in both_signals.iter().zip(&apart_signals) {
        match (a, b) {
            (None, None) => {}
            (Some(a), Some(b)) => {
                assert_eq!(
                    a.to_json(),
                    b.to_json(),
                    "one pass drifted from the signals pass"
                )
            }
            _ => panic!("Some against None"),
        }
    }
}

/// Each table signal points at its blocks: `block_range` holds the table's
/// cells, and their union is `bbox`.
#[test]
fn each_table_signal_points_at_its_blocks() {
    use pdf_engine::learned::{refine_page_with_signals, SWITCHES};

    let doc = nasdaq();
    let (blocks, rules) = doc
        .extract_blocks_and_rules_before_learning()
        .expect("blocks");
    let sizes = [doc
        .page_geometry(0)
        .map(|g| g.effective_dimensions())
        .unwrap_or((0.0, 0.0))];
    let rulings = [doc.page_rulings(0).expect("rulings")];
    let images = doc.extract_image_boxes().expect("images");
    let facts = pdf_engine::learned::document_facts(&blocks);
    let page = pdf_engine::learned::page_blocks(&blocks, 0);
    let (out, signals) = refine_page_with_signals(
        page,
        sizes[0],
        &facts,
        SWITCHES,
        &rules[0],
        &rulings[0],
        &images[0],
    );
    let page = signals.as_ref().expect("the judge ran, so Some");
    assert!(!page.tables.is_empty(), "no tables to point at");
    for table in &page.tables {
        let slice = out
            .get(table.block_range.clone())
            .unwrap_or_else(|| panic!("range {:?} out of {} blocks", table.block_range, out.len()));
        assert!(!slice.is_empty(), "an empty table");
        assert!(
            slice
                .iter()
                .all(|b| matches!(b.kind, BlockKind::TableCell { .. })),
            "not the table's blocks: {:?}",
            slice.iter().map(|b| &b.kind).collect::<Vec<_>>()
        );
        let mut union = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for b in slice {
            union[0] = union[0].min(b.bbox[0]);
            union[1] = union[1].min(b.bbox[1]);
            union[2] = union[2].max(b.bbox[2]);
            union[3] = union[3].max(b.bbox[3]);
        }
        assert_eq!(union, table.bbox, "the union is not the box");
    }
}

/// The candidate count beside the best rejected candidate: zero with no
/// candidates, at least one where a candidate was rejected.
#[test]
fn candidate_count_is_zero_without_candidates_and_present_with_a_reject() {
    let prose = {
        use lopdf::content::{Content, Operation};
        use lopdf::{dictionary, Document, Object, Stream};

        let mut built = Document::with_version("1.7");
        let font = Object::Reference(built.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Font".to_vec()),
            "Subtype" => Object::Name(b"Type1".to_vec()),
            "BaseFont" => Object::Name(b"Courier".to_vec()),
        })));
        let mut ops = Vec::new();
        for (i, line) in [
            "The quick brown fox jumps over the lazy dog and keeps going.",
            "Pack my box with five dozen liquor jugs and a few more words.",
        ]
        .iter()
        .enumerate()
        {
            ops.push(Operation::new("BT", vec![]));
            ops.push(Operation::new(
                "Tf",
                vec![Object::Name(b"F1".to_vec()), 10.0.into()],
            ));
            ops.push(Operation::new(
                "Td",
                vec![72.0.into(), (700.0 - i as f64 * 14.0).into()],
            ));
            ops.push(Operation::new("Tj", vec![Object::string_literal(*line)]));
            ops.push(Operation::new("ET", vec![]));
        }
        let content = Content { operations: ops }.encode().expect("encode");
        let content_id = built.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
        let page_id = built.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(dictionary! { "F1" => font }),
            }),
        }));
        let pages_id = built.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => Object::Integer(1),
        }));
        if let Ok(Object::Dictionary(page)) = built.get_object_mut(page_id) {
            page.set("Parent", Object::Reference(pages_id));
        }
        let catalog_id = built.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "Pages" => Object::Reference(pages_id),
        }));
        built.trailer.set("Root", Object::Reference(catalog_id));
        let mut buf = Vec::new();
        built.save_to(&mut buf).expect("save");
        PdfDocument::open(buf).expect("open")
    };
    let signals = prose.table_signals().expect("signals");
    let page = signals[0].as_ref().expect("asked, so Some");
    assert!(page.tables.is_empty(), "prose is no table: {page:?}");
    assert_eq!(
        page.n_candidates, 0,
        "two lines of prose drew candidates: {page:?}"
    );

    let doc = nasdaq();
    let signals = doc.table_signals().expect("signals");
    let page = signals[0].as_ref().expect("signals");
    assert!(
        page.p_best_rejected.is_some(),
        "no rejected candidate on this page: {page:?}"
    );
    assert!(
        page.n_candidates >= 1,
        "a rejected candidate with no candidates seen: {page:?}"
    );
}

/// Every trace line carries the model stamp: the judge, verifier and
/// is-table hashes the crate ships, with the final-table hash beside them.
#[test]
fn every_trace_line_carries_the_model_stamp() {
    // The lines the example writes come from this library function, so the
    // test reads them here and needs no built example binary.
    let signals = nasdaq().table_signals().expect("signals");
    let text = pdf_engine::learned::tables::table_signals_jsonl(&signals);
    // The stamp the lines carry is the crate's own function of its constants.
    let stamp = pdf_engine::learned::tables::model_stamp_json();
    for hash in [
        pdf_engine::learned::models::table_judge::MODEL_SHA256,
        pdf_engine::learned::models::table_verifier::MODEL_SHA256,
        pdf_engine::learned::models::table_is::MODEL_SHA256,
        pdf_engine::learned::models::table_is_v2::MODEL_SHA256,
    ] {
        assert!(
            stamp.contains(hash),
            "the stamp lost a hash it ships: {stamp}"
        );
    }
    let lines: Vec<&str> = text.lines().collect();
    assert!(!lines.is_empty(), "no trace lines");
    for line in lines {
        let json: serde_json::Value = serde_json::from_str(line).expect("json");
        let models = &json["models"];
        assert_eq!(
            models["table_judge"].as_str(),
            Some(pdf_engine::learned::models::table_judge::MODEL_SHA256),
            "judge hash missing or drifted: {line}"
        );
        assert_eq!(
            models["table_verifier"].as_str(),
            Some(pdf_engine::learned::models::table_verifier::MODEL_SHA256),
            "verifier hash missing or drifted: {line}"
        );
        assert_eq!(
            models["table_is"].as_str(),
            Some(pdf_engine::learned::models::table_is::MODEL_SHA256),
            "is-table hash missing or drifted: {line}"
        );
        assert_eq!(
            models["table_is_v2"].as_str(),
            Some(pdf_engine::learned::models::table_is_v2::MODEL_SHA256),
            "final-table hash missing or drifted: {line}"
        );
    }
}

/// The trace carries the is-table number and the model that decided it: the
/// router reads both from the JSONL, never from the structs.
#[test]
fn the_trace_carries_the_is_table_number_and_its_model() {
    let signals = nasdaq().table_signals().expect("signals");
    let page = signals[0].as_ref().expect("signals");
    assert_eq!(page.tables.len(), 1, "one table emitted: {page:?}");
    let p = page.tables[0]
        .p_is_table
        .expect("v2's number is on the signal");

    let text = pdf_engine::learned::tables::table_signals_jsonl(&signals);
    let line = text.lines().next().expect("one page, one line");
    let json: serde_json::Value = serde_json::from_str(line).expect("json");
    let table = &json["signals"]["tables"][0];
    assert_eq!(
        table["p_is_table"].as_f64(),
        Some(p),
        "the trace lost the number: {line}"
    );
    assert_eq!(
        table["p_is_table_model"].as_str(),
        Some("v2"),
        "the trace lost the model: {line}"
    );

    // WITH V2 OFF the same fields name v1: one trace shape, both scales.
    let doc = nasdaq();
    let (blocks, rules) = doc
        .extract_blocks_and_rules_before_learning()
        .expect("blocks");
    let sizes = vec![doc
        .page_geometry(0)
        .map(|g| g.effective_dimensions())
        .unwrap_or((0.0, 0.0))];
    let rulings = vec![doc.page_rulings(0).expect("rulings")];
    let images = doc.extract_image_boxes().expect("images");
    let mut off = SWITCHES;
    off.table_is_v2 = false;
    let signals = table_signals_with(&blocks, &sizes, &rules, &rulings, &images, off);
    let text = pdf_engine::learned::tables::table_signals_jsonl(&signals);
    let line = text.lines().next().expect("one page, one line");
    let json: serde_json::Value = serde_json::from_str(line).expect("json");
    let table = &json["signals"]["tables"][0];
    assert!(
        table["p_is_table"].as_f64().is_some(),
        "the trace lost v1's number: {line}"
    );
    assert_eq!(
        table["p_is_table_model"].as_str(),
        Some("v1"),
        "the trace lost the model: {line}"
    );
}
