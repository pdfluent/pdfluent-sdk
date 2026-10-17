#![warn(missing_docs)]
//! Offline tooling for learned extraction decisions (#598).
//!
//! The extraction heuristics decide with hand-tuned thresholds today; the plan
//! is to learn those decisions from labelled pages, offline, and compile the
//! trees to plain Rust. This crate holds the first half: the feature dump the
//! models are trained on. It reads only what `pdf_engine` makes public -- the
//! blocks `extract_blocks` returns and the spans inside them -- so the engine
//! is not changed to be measured.
//!
//! The record format is [`SCHEMA`], one JSON object per page, with
//! the schema named in every record so a joined file can hold two versions
//! while the schema changes. The fields are described on #598.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

pub mod models;

use pdf_engine::learned::features::{page_features, PageWord};
use pdf_engine::learned::pairs;
use pdf_engine::text::HorizontalRule;
use pdf_engine::{Block, BlockKind, PdfDocument};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// The schema every record names: /18 is /17 plus `final_tables` -- every
/// table the pass would emit, with its final-table vector -- for the
/// final-table judge (#574, #638).
///
/// The name every v2 training dump carries,
/// `pdfluent.features/final-tables-next`, stays readable beside it -- as an
/// alias in `features.READABLE`, so a retrain still reads those dumps --
/// but nothing writes it any more.
pub const SCHEMA: &str = "pdfluent.features/18";

/// THE TREE this binary was built from (see `build.rs`): sha256 over the
/// sorted (path, blob-sha) pairs of the engine source that a dump's values
/// depend on, "<tree>-dirty" when that source had local changes, "unknown"
/// when git was missing. Stamped into every record, so the trainers can refuse
/// a dump another engine segmented (#574).
pub const ENGINE_TREE: &str = env!("PDFLUENT_ENGINE_TREE");

/// THE DECISIONS this binary was built from (see `build.rs`): the same sha256
/// layout over everything under `crates/pdf-engine/src/learned/`, models
/// included -- what the dumped MODEL outputs depend on. Stamped into every
/// record beside [`ENGINE_TREE`], so the stacked trainers can refuse a dump
/// whose verdicts another tree's decisions wrote (#574).
pub const ENGINE_DECISIONS: &str = env!("PDFLUENT_ENGINE_DECISIONS");

/// THE DECISIONS WITHOUT this binary was built from (see `build.rs`):
/// `{"name": "hash"}` over `src/learned/` minus each self-excludable model's
/// directory, so a retrain of that model reuses the dump its own retrain
/// needs. Stamped into every record as `engine.decisions_without` (#574).
pub const ENGINE_DECISIONS_WITHOUT: &str = env!("PDFLUENT_ENGINE_DECISIONS_WITHOUT");

/// `engine.decisions_without` of every record: [`ENGINE_DECISIONS_WITHOUT`]
/// parsed, or an empty map where the stamp is no JSON -- which is only an
/// "unknown"-shaped build gone wrong, and then the trainers' full check
/// applies, as it does for a dump from before the stamp.
fn decisions_without() -> Value {
    serde_json::from_str(ENGINE_DECISIONS_WITHOUT).unwrap_or_else(|_| json!({}))
}

/// The commit this binary was built from, with "-dirty" when the engine had
/// local changes. For the human reader; the trainers compare [`ENGINE_TREE`].
pub const ENGINE_COMMIT: &str = env!("PDFLUENT_ENGINE_COMMIT");

/// A word of a span: its text and the union of its glyph boxes. The engine's
/// own type, since #599 moved the feature computation into
/// `pdf_engine::learned` so that the dump and the engine read one code.
pub use pdf_engine::learned::features::SpanWord as Word;

/// The words of one span, boxed from `char_bounds` -- see
/// [`pdf_engine::learned::features::words_of`].
pub use pdf_engine::learned::features::words_of;

/// The record of one page, in schema [`SCHEMA`].
///
/// `blocks` are the page's blocks in reading order, every kind included --
/// a `paragraph` is our own verdict, not the absence of one. `page_id` is the
/// dataset's page identity when there is one (DocLayNet's `page_hash`),
/// otherwise the PDF's sha256.
///
/// `rules` are the horizontal rules the page drew (`pdfluent.features/3`),
/// from the same interpretation of the page as `blocks`: in a ruled table the
/// cell boundary IS a drawn line, and a blank cell holds no words for any
/// other feature to find it by (T4, #600).
pub fn page_record(
    blocks: &[Block],
    rules: &[HorizontalRule],
    page: usize,
    width: f64,
    height: f64,
    page_id: &str,
    pdf_sha256: &str,
) -> Value {
    page_record_with_rulings(blocks, rules, &[], page, width, height, page_id, pdf_sha256)
}

/// [`page_record`], with the lines and rectangle edges the page draws, from
/// which `table_proposals` also offers the drawn-grid and rule-framed
/// candidates of `pdf_engine::learned::vector_proposals` (#574).
#[allow(clippy::too_many_arguments)]
pub fn page_record_with_rulings(
    blocks: &[Block],
    rules: &[HorizontalRule],
    rulings: &[pdf_engine::text::Ruling],
    page: usize,
    width: f64,
    height: f64,
    page_id: &str,
    pdf_sha256: &str,
) -> Value {
    // THE ENGINE MEASURES, THIS WRITES. Every value below comes from
    // `pdf_engine::learned::features::page_features`, the function the
    // engine's learned pass calls at run time (#599): a feature computed twice
    // is a feature that can drift, and a drift here reads as a weak model.
    let refs: Vec<&Block> = blocks.iter().collect();
    let features = page_features(&refs, width, height);

    // WHICH TABLE A CELL IS IN (T2, #600): per page, 0-based, in reading
    // order. The engine emits a table's cells together, row by row, and keeps
    // no table id on a block, so a new table starts where the run of cells is
    // broken by another block, or where a cell does not come after the one
    // before it in row-major order (a second table right behind the first).
    let tables: Vec<Option<u32>> = {
        let mut count = 0u32;
        let mut previous: Option<(u16, u16)> = None;
        blocks
            .iter()
            .map(|b| match b.kind {
                BlockKind::TableCell { row, column, .. } => {
                    let continues =
                        previous.is_some_and(|(r, c)| row > r || (row == r && column > c));
                    if !continues {
                        count += 1;
                    }
                    previous = Some((row, column));
                    Some(count - 1)
                }
                _ => {
                    previous = None;
                    None
                }
            })
            .collect()
    };

    let mut out_blocks = Vec::with_capacity(blocks.len());
    for (i, (f, b)) in features.blocks.iter().zip(blocks).enumerate() {
        let mut row = json!({
            "i": i,
            "kind": f.kind,
            "level": f.level,
            // WHAT A MODEL SAID ABOUT THIS BLOCK, where one did. `--refine`
            // runs the learned pass, and a dump of what it decided without the
            // probability behind the decision cannot be used to ask whether a
            // guard on that probability would help (#602).
            "confidence": b.confidence,
            "bbox": b.bbox,
            "text": b.text,
            "n_chars": f.n_chars,
            "n_words": f.n_words,
            "n_lines": f.n_lines,
            "size": f.size,
            "size_ratio": f.size_ratio,
            "bold": f.bold,
            "italic": f.italic,
            "all_caps": f.all_caps,
            "font": f.font,
            "x_frac": f.x_frac,
            "width_frac": f.width_frac,
            "y_frac": f.y_frac,
            "gap_above_em": f.gap_above_em,
            "gap_below_em": f.gap_below_em,
            "ends_terminal": f.ends_terminal,
            "ends_colon": f.ends_colon,
            "marker": f.marker,
            "caption": f.caption,
            "section_number": f.section_number,
            "prev_kind": f.prev_kind,
            "next_kind": f.next_kind,
            "prev_ends_terminal": f.prev_ends_terminal,
            "prev_size_ratio": f.prev_size_ratio,
            "next_x_frac": f.next_x_frac,
            "next_size_ratio": f.next_size_ratio,
            // Filled by the driver, which sees every page of the document: on
            // how many pages the normalised text of this block recurs. Null
            // here because one page cannot know (T3, #599).
            "repeats_in_document": Value::Null,
            "first_on_page": f.first_on_page,
            "last_on_page": f.last_on_page,
        });
        // Only on a cell, and absent elsewhere rather than a sentinel (T2).
        if let BlockKind::TableCell {
            row: r,
            column,
            row_span,
            column_span,
            ..
        } = b.kind
        {
            row["table"] = json!(tables[i]);
            row["row"] = json!(r);
            row["column"] = json!(column);
            row["row_span"] = json!(row_span);
            row["column_span"] = json!(column_span);
            // WHAT THE BUILDER HAD TO GO ON, per table and repeated on its cells
            // (#574). The learned structure model of #600 reads this to decide
            // whether it may overrule the heuristic grid, so it travels with the
            // cell rather than in a channel of its own: a consumer holding the
            // cells has no other handle on the table they came from.
            if let Some(doubt) = b.grid {
                row["grid_thin_rows"] = json!(doubt.thin_rows);
                row["grid_fill"] = json!(doubt.fill);
                row["grid_retried_from"] = json!(doubt.retried_from);
                row["grid_full_rows"] = json!(doubt.full_rows);
                row["grid_columns"] = json!(doubt.columns);
            }
        }
        out_blocks.push(row);
    }
    let out_words: Vec<Value> = features
        .words
        .iter()
        .map(|w| {
            json!({
                "text": w.text,
                "bbox": w.bbox,
                "block": w.block,
                "span": w.span,
                "size": w.size,
                "bold": w.bold,
                "italic": w.italic,
                // /11: the face and the colour, for what a line is set in
                // against the page's body (#574).
                "font": w.font,
                "color": w.color,
            })
        })
        .collect();

    // THE PAIRS THE TABLE MODEL IS ASKED ABOUT (/4), with their features --
    // computed HERE by `pdf_engine::learned::pairs`, the same code the engine
    // runs, for the reason that module states: a feature computed twice is a
    // feature that can drift, and a drift reads as a weak model.
    //
    // `[i, j, f0 .. f24]`, where i and j index `words` and the 25 values are
    // `pairs::NAMES` in order. Only pairs inside a table region, because that
    // is where the model runs (`pairs::in_table_regions`); a page our
    // heuristics found no table on writes an empty list, which is not the same
    // as a page with no words and is why the field is always present.
    let pair_words: Vec<pairs::PairWord> = features
        .words
        .iter()
        .map(|w| {
            let (table, row, column) = match blocks.get(w.block).map(|b| b.kind) {
                Some(BlockKind::TableCell { row, column, .. }) => {
                    (tables[w.block], Some(row), Some(column))
                }
                _ => (None, None, None),
            };
            pairs::PairWord {
                text: w.text.clone(),
                bbox: w.bbox,
                table,
                row,
                column,
            }
        })
        .collect();
    let mut out_pairs: Vec<Value> = Vec::new();
    let mut seen: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
    // ONE LIST FOR THE PAGE'S TABLES, not one per table, because that is what
    // measured better: 0.5789 against 0.5372 on 150 tables with one model, one
    // set of words and one floor (#600). The reasoning that said otherwise --
    // that a word at the edge of one table spends its neighbours on the other
    // -- is real but smaller than what a per-table list costs, which is every
    // edge between a table and the line above it that belongs to it.
    // Sorted and deduplicated: two grown regions overlap, and a word listed
    // twice is two indices for one word -- which `candidates` then pairs with
    // itself.
    for region in [{
        let mut all = pairs::in_table_regions(&pair_words).concat();
        all.sort_unstable();
        all.dedup();
        all
    }] {
        let inside: Vec<pairs::PairWord> = region.iter().map(|&i| pair_words[i].clone()).collect();
        for (a, b) in pairs::candidates(&inside, pairs::NEAREST) {
            // Two grown regions can overlap, and a pair in both is one pair.
            let (i, j) = (region[a].min(region[b]), region[a].max(region[b]));
            if !seen.insert((i, j)) {
                continue;
            }
            let f = pairs::features(&inside[a], &inside[b]);
            let mut row = Vec::with_capacity(2 + f.len());
            row.push(json!(region[a]));
            row.push(json!(region[b]));
            row.extend(f.iter().map(|v| json!(v)));
            out_pairs.push(Value::Array(row));
        }
    }

    json!({
        "schema": SCHEMA,
        // WHICH ENGINE SEGMENTED THIS PAGE (#574). The blocks below are the
        // verdicts of one tree's segmentation; a model trained on them and
        // read by another tree's is a train/serving skew, so the trainers
        // refuse a record whose tree is not the serving one. `decisions` is
        // which tree's models wrote the dumped verdicts (`list_signal`,
        // `line_class_probs`, `p_heading`): a stacked model trained on them
        // and read beside another tree's is the same skew one level up, so
        // the stacked trainers refuse a record whose decisions are not the
        // serving ones. `decisions_without` is the same per self-excludable
        // model minus its own directory, so a retrain reuses the dump its
        // own retrain needs.
        "engine": {"tree": ENGINE_TREE, "decisions": ENGINE_DECISIONS,
                   "decisions_without": decisions_without(), "commit": ENGINE_COMMIT},
        "page_id": page_id,
        "pdf_sha256": pdf_sha256,
        "page": page,
        "width": width,
        "height": height,
        "body_size": features.body_size,
        "blocks": out_blocks,
        "words": out_words,
        // /3: the page's horizontal rules, in the engine's order, in the frame
        // of `bbox` (PDF user space, y up). `y` is the line's height on the
        // page; `left` < `right` its extent.
        "rules": rules
            .iter()
            .map(|r| json!({"left": r.left, "right": r.right, "y": r.y}))
            .collect::<Vec<Value>>(),
        "pair_names": pairs::NAMES,
        "pairs": out_pairs,
        "table_proposals": table_proposals_of(blocks, rules, rulings, &features.words),
    })
}

/// The page's table proposals (/6, #574): every region a detector of the
/// engine accepted, as the set of the page's LINES it holds -- lines in the
/// order `learned::lines::lines_of` gives them block by block, which is the
/// unit the line model and the table judge share (T4's interface on #574).
///
/// A line belongs to a proposal when the centre of its box lies inside the
/// proposal's hull, grown by half a line's height up and down: the hull runs
/// from the first row's baseline-plus-height to the last row's baseline, and a
/// line's box from its descent to its ascent.
fn table_proposals_of(
    blocks: &[Block],
    rules: &[HorizontalRule],
    rulings: &[pdf_engine::text::Ruling],
    words: &[PageWord],
) -> Value {
    let spans: Vec<pdf_engine::TextSpan> = blocks
        .iter()
        .flat_map(|b| b.spans.iter().cloned())
        .collect();
    let mut lines: Vec<[f64; 4]> = Vec::new();
    for block in 0..blocks.len() {
        for line in pdf_engine::learned::lines::lines_of(block, words) {
            let mut bbox = [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ];
            for w in line {
                bbox[0] = bbox[0].min(w.bbox[0]);
                bbox[1] = bbox[1].min(w.bbox[1]);
                bbox[2] = bbox[2].max(w.bbox[2]);
                bbox[3] = bbox[3].max(w.bbox[3]);
            }
            lines.push(bbox);
        }
    }
    let proposals: Vec<Value> = pdf_engine::text::table_proposals(&spans, rules)
        .into_iter()
        .map(|p| {
            let [left, bottom, right, top] = p.hull;
            let held: Vec<usize> = lines
                .iter()
                .enumerate()
                .filter(|(_, b)| {
                    let (cx, cy) = ((b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5);
                    let half = (b[3] - b[1]) * 0.5;
                    cx >= left && cx <= right && cy >= bottom - half && cy <= top + half
                })
                .map(|(i, _)| i)
                .collect();
            json!({
                "source": "region",
                "detector": p.detector,
                "accepted": p.accepted,
                "hull": p.hull,
                "lines": held,
            })
        })
        .collect();
    let mut proposals = proposals;
    // #632's alignment detector on the same lines: its tables are line sets
    // already, so the hull is the lines' own.
    for held in pdf_engine::learned::alignment::proposals(&lines) {
        let mut hull = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for &i in &held {
            hull[0] = hull[0].min(lines[i][0]);
            hull[1] = hull[1].min(lines[i][1]);
            hull[2] = hull[2].max(lines[i][2]);
            hull[3] = hull[3].max(lines[i][3]);
        }
        proposals.push(json!({
            "source": "alignment",
            "detector": "alignment_lines",
            "accepted": true,
            "hull": hull,
            "lines": held,
        }));
    }
    // Every proposal also in pieces (`learned::splits`): a proposal that holds
    // two tables is wrong about both, and the judge can only choose a piece
    // that is offered.
    let sets: Vec<Vec<usize>> = proposals
        .iter()
        .filter_map(|p| {
            p["lines"].as_array().map(|a| {
                a.iter()
                    .filter_map(|v| v.as_u64().map(|i| i as usize))
                    .collect()
            })
        })
        .collect();
    for held in pdf_engine::learned::splits::pieces(&sets, &lines) {
        let mut hull = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for &i in &held {
            hull[0] = hull[0].min(lines[i][0]);
            hull[1] = hull[1].min(lines[i][1]);
            hull[2] = hull[2].max(lines[i][2]);
            hull[3] = hull[3].max(lines[i][3]);
        }
        proposals.push(json!({
            "source": "split",
            "detector": "split",
            "accepted": true,
            "hull": hull,
            "lines": held,
        }));
    }
    // What the page draws, and the region and alignment proposals grown to its
    // rules or joined in their column (`learned::vector_proposals`). A source
    // of their own, `lattice` or `ruled`, so the judge's reader, which takes
    // `region`, `alignment` and `split`, leaves them to the verifier (T4).
    for (detector, held) in
        pdf_engine::learned::vector_proposals::vector_proposals(&lines, rules, rulings, &sets)
    {
        let mut hull = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for &i in &held {
            hull[0] = hull[0].min(lines[i][0]);
            hull[1] = hull[1].min(lines[i][1]);
            hull[2] = hull[2].max(lines[i][2]);
            hull[3] = hull[3].max(lines[i][3]);
        }
        proposals.push(json!({
            "source": if detector == "lattice" { "lattice" } else { "ruled" },
            "detector": detector,
            "accepted": true,
            "hull": hull,
            "lines": held,
        }));
    }
    Value::Array(proposals)
}

/// A page's doubt as `/17` carries it: `link_doubt` and `class_entropy` are
/// null where the network was off and measured nothing -- NaN is not JSON,
/// and a zero would say "sure", which nobody measured.
fn confidence_of(c: &pdf_engine::learned::PageConfidence) -> Value {
    let num = |v: f64| {
        if v.is_nan() {
            Value::Null
        } else {
            json!(v)
        }
    };
    json!({
        "doubt": num(c.doubt),
        "heading_doubt": num(c.heading_doubt),
        "link_doubt": num(c.link_doubt),
        "class_entropy": num(c.class_entropy),
    })
}

/// A record as ONE line for every reader. serde_json leaves U+2028, U+2029
/// and U+0085 raw inside strings, which is valid JSON -- and Python's
/// `str.splitlines()` breaks a line at each of them, so a page whose text held
/// one arrived as two lines of broken JSON (T3, DocLayNet shard 41, 23-09).
/// Written as `\u` escapes they are the same JSON and no line ending anywhere.
pub fn one_line(json: &str) -> String {
    json.replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
        .replace('\u{85}', "\\u0085")
}

/// The same record, but of the blocks a chosen set of learned switches leaves
/// behind (#599, #602).
///
/// FOR MEASURING AND NEVER FOR TRAINING. `dump_pdf` writes what the models
/// have not touched, because a record of their own output would teach a model
/// itself; this writes what they DID, which is the only way to ask what two
/// switches are worth TOGETHER. A boundary model changes the blocks a class
/// model then reads, so measuring the two apart and adding the numbers
/// measures something that never happens. Every record says which switches
/// were on, and `train_lists.py` refuses any record whose `refine` is not
/// "off".
pub fn dump_pdf_refined(
    bytes: Vec<u8>,
    page_id: Option<&str>,
    switches: pdf_engine::learned::Switches,
    says: &str,
) -> Result<String, String> {
    let sha: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let page_id = page_id.map(str::to_string).unwrap_or_else(|| sha.clone());
    let doc = PdfDocument::open(bytes).map_err(|e| format!("open: {e}"))?;
    let (blocks, rules) = doc
        .extract_blocks_and_rules_before_learning()
        .map_err(|e| format!("extract: {e}"))?;
    let sizes: Vec<(f64, f64)> = (0..doc.page_count())
        .map(|page| match doc.page_geometry(page) {
            Ok(g) => g.effective_dimensions(),
            Err(_) => (0.0, 0.0),
        })
        .collect();
    // The pictures too, so a caption decision measured through this dump sees
    // what `extract_blocks` sees (#636).
    let images = doc
        .extract_image_boxes()
        .map_err(|e| format!("images: {e}"))?;
    // /17: the pass's doubt with THESE switches, read off the blocks before
    // the pass ran -- the ones it decided on -- the way `dump_pdf` reads it
    // with the switches as shipped.
    let confidence =
        pdf_engine::learned::page_confidence_with_switches(&blocks, &sizes, &images, switches);
    // /18 from the blocks BEFORE the pass, as `dump_pdf` writes them: the
    // final tables are pre-decision (neither v1 nor v2 applied), whatever
    // `switches` refined the blocks beside them into.
    let facts = pdf_engine::learned::document_facts(&blocks);
    let mut finals_of: Vec<Value> = Vec::new();
    for page in 0..doc.page_count() {
        let (width, height) = sizes.get(page).copied().unwrap_or((0.0, 0.0));
        let on_page: Vec<_> = blocks.iter().filter(|b| b.page == page).cloned().collect();
        let drawn = rules.get(page).map(Vec::as_slice).unwrap_or(&[]);
        let (rulings, _) = doc.page_rulings_and_fills(page).unwrap_or_default();
        let pictures = images.get(page).map(Vec::as_slice).unwrap_or(&[]);
        let finals = pdf_engine::learned::final_tables_for_dump(
            on_page,
            (width, height),
            &facts,
            drawn,
            &rulings,
            pictures,
        );
        finals_of.push(
            finals
                .iter()
                .map(|t| {
                    json!({
                        "bbox": t.bbox,
                        "kind": t.kind,
                        "features": t.features.iter().map(|v| if v.is_nan() {
                            Value::Null
                        } else {
                            json!(v)
                        }).collect::<Vec<Value>>(),
                        "engine": {"tree": ENGINE_TREE, "decisions": ENGINE_DECISIONS},
                    })
                })
                .collect::<Vec<Value>>()
                .into(),
        );
    }
    let blocks =
        pdf_engine::learned::refine_with_images(blocks, &sizes, &rules, &[], &images, switches);
    // One entry per page of this document: its blocks name no page it has not
    // got, and `sizes` names them all.
    assert_eq!(confidence.len(), doc.page_count());
    let mut out = String::new();
    for (page, conf) in confidence.iter().enumerate() {
        let (width, height) = sizes.get(page).copied().unwrap_or((0.0, 0.0));
        let on_page: Vec<_> = blocks.iter().filter(|b| b.page == page).cloned().collect();
        let drawn = rules.get(page).map(Vec::as_slice).unwrap_or(&[]);
        let rulings = doc.page_rulings(page).unwrap_or_default();
        let mut record = page_record_with_rulings(
            &on_page, drawn, &rulings, page, width, height, &page_id, &sha,
        );
        record["refine"] = json!(says);
        record["confidence"] = confidence_of(conf);
        record["final_table_names"] = json!(pdf_engine::learned::tables::TABLE_IS_V2_NAMES);
        record["final_tables"] = finals_of.get(page).cloned().unwrap_or(Value::Array(vec![]));
        out.push_str(&one_line(&record.to_string()));
        out.push('\n');
    }
    Ok(out)
}

/// The records of every page of one PDF, as JSONL (one line per page, each
/// ending in a newline). `page_id` names the dataset's page when there is one;
/// otherwise the PDF's sha256 does. The same text whether it is called once
/// per process or many times in parallel -- `feature_dump`'s batch mode is
/// held to byte-identical output by `tests/batch_is_per_page.rs`.
pub fn dump_pdf(bytes: Vec<u8>, page_id: Option<&str>) -> Result<String, String> {
    let sha: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let page_id = page_id.map(str::to_string).unwrap_or_else(|| sha.clone());
    let doc = PdfDocument::open(bytes).map_err(|e| format!("open: {e}"))?;
    // BEFORE the learned pass, always: a record of blocks the models had
    // already touched would teach a model its own output (T3, #599). The
    // record says so, and train_lists.py refuses one that says "on".
    // And the rules from that same pass (/3): the lines a page drew belong
    // beside the blocks they were read with, not beside a second reading.
    let (blocks, rules) = doc
        .extract_blocks_and_rules_before_learning()
        .map_err(|e| format!("extract: {e}"))?;
    // /7: what the list models say of every line, with the facts of the
    // whole document, as the engine reads them at serve time (#574).
    let facts = pdf_engine::learned::document_facts(&blocks);
    // WHAT THE HEADING MODEL SAYS ABOUT EVERY BLOCK, asked here rather than
    // re-derived by a reader. `confidence` carries the probability behind a
    // block's KIND, so a block the model refused carries nothing and "why was
    // this heading refused" cannot be asked of a dump at all -- which is the
    // commonest question there is about this lane (#602). Computed on these
    // blocks, which are the ones the model reads: the pass has not run.
    let sizes: Vec<(f64, f64)> = (0..doc.page_count())
        .map(|page| match doc.page_geometry(page) {
            Ok(g) => g.effective_dimensions(),
            Err(_) => (0.0, 0.0),
        })
        .collect();
    let says = pdf_engine::learned::heading_probabilities(&blocks, &sizes);
    // /9: where each page draws its images, for the caption lane (#636).
    let images = doc
        .extract_image_boxes()
        .map_err(|e| format!("images: {e}"))?;
    // /10: the heading decision the pass takes on every block, as the table
    // pass reads it (`CellHint`) -- not `p_heading`, which is the line model
    // alone (T4, #574).
    let hints = pdf_engine::learned::cell_hints(&blocks, &sizes, &images);
    // /17: the learned pass's doubt about every page, from the same decisions
    // the pass acts on -- for the router that hands doubtful pages to the
    // teacher. Computed here, on these blocks, because the pass has not run
    // and these are the blocks it decides on.
    let confidence = pdf_engine::learned::page_confidence(&blocks, &sizes, &images);
    // One entry per page of this document: its blocks name no page it has not
    // got, and `sizes` names them all.
    assert_eq!(confidence.len(), doc.page_count());
    let mut out = String::new();
    for (page, conf) in confidence.iter().enumerate() {
        let (width, height) = match doc.page_geometry(page) {
            Ok(g) => g.effective_dimensions(),
            Err(_) => (0.0, 0.0),
        };
        let on_page: Vec<_> = blocks.iter().filter(|b| b.page == page).cloned().collect();
        let hints_of_page: Vec<pdf_engine::learned::tables::CellHint> = blocks
            .iter()
            .zip(&hints)
            .filter(|(b, _)| b.page == page)
            .map(|(_, h)| *h)
            .collect();
        let of_page: Vec<Option<f64>> = blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| b.page == page)
            .map(|(at, _)| says.get(at).copied().flatten())
            .collect();
        let drawn = rules.get(page).map(Vec::as_slice).unwrap_or(&[]);
        let (rulings, fills) = doc.page_rulings_and_fills(page).unwrap_or_default();
        // /12: the 72-dpi gray page the line network's image arm reads, once
        // per page. `None` where the render failed, in which case `line_image`
        // is null -- never zeros, which would read as a blank page (#574).
        let gray = pdf_engine::document::gray_page(&doc, page, 72).ok();
        let refs_for_vector: Vec<&Block> = on_page.iter().collect();
        let mut record = page_record_with_rulings(
            &on_page, drawn, &rulings, page, width, height, &page_id, &sha,
        );
        if let Some(rows) = record["blocks"].as_array_mut() {
            for (row, hint) in rows.iter_mut().zip(&hints_of_page) {
                row["heading_pass"] = json!(hint.heading);
                if let Some(p) = hint.p_heading {
                    row["p_heading_pass"] = json!(p);
                }
            }
            for (row, p) in rows.iter_mut().zip(&of_page) {
                if let Some(p) = p {
                    row["p_heading"] = json!(p);
                }
            }
        }
        record["refine"] = json!("off");
        record["confidence"] = confidence_of(conf);
        record["images"] = json!(images.get(page).cloned().unwrap_or_default());
        // /11: what the table judge says of each block, on the page as the
        // heuristics left it -- the state the heading decision reads too
        // (#574). "rejected", "table", or absent.
        let judged = pdf_engine::learned::tables::judge_regions(
            &on_page,
            (width, height),
            rules.get(page).map(Vec::as_slice).unwrap_or(&[]),
            &facts,
            pdf_engine::learned::SWITCHES,
        );
        if let Some(rows) = record["blocks"].as_array_mut() {
            for (row, judge) in rows.iter_mut().zip(&judged) {
                match judge {
                    Some(pdf_engine::learned::tables::REJECTED) => {
                        row["judged"] = json!("rejected")
                    }
                    Some(_) => row["judged"] = json!("table"),
                    None => {}
                }
            }
        }
        // /10: the page's drawings the line network reads (#574) -- every
        // ruling as `[x0, y0, x1, y1]` (a thin line or a rectangle's edge) and
        // every solid, non-white, axis-aligned fill, in the frame of `bbox`.
        record["rulings"] = rulings
            .iter()
            .map(|r| json!([r.x0, r.y0, r.x1, r.y1]))
            .collect::<Vec<Value>>()
            .into();
        record["fills"] = json!(fills);
        // /10: `line_vector`, one row per line -- blocks in the page's order,
        // each block's lines in `lines_of` order -- of
        // `learned::drawings::VECTOR_FEATURES`, for the line network (T3's
        // schema on #574).
        let words =
            pdf_engine::learned::features::page_features(&refs_for_vector, width, height).words;
        record["line_vector"] =
            pdf_engine::learned::drawings::page_vector(on_page.len(), &words, &rulings, &fills)
                .into_iter()
                .flatten()
                .map(|row| json!(row))
                .collect::<Vec<Value>>()
                .into();
        // /12: `line_image`, one row per line in exactly the same order and
        // count as `line_vector`, of `learned::raster::IMAGE_FEATURES` -- or
        // null where the page did not render, which readers must handle (#574).
        record["line_image"] = match &gray {
            Some((raster_width, raster_height, data)) => {
                let gray_page = pdf_engine::learned::raster::GrayPage {
                    width: *raster_width,
                    height: *raster_height,
                    data,
                };
                pdf_engine::learned::raster::page_image(on_page.len(), &words, &gray_page, height)
                    .into_iter()
                    .flatten()
                    .map(|row| json!(row))
                    .collect::<Vec<Value>>()
                    .into()
            }
            None => Value::Null,
        };
        let refs: Vec<&Block> = on_page.iter().collect();
        // NaN is not JSON: a line the list models did not read is `null`.
        // /8: the line model's four class probabilities for every line, in the
        // same order, for the list model and the table judge (#574).
        record["line_class_probs"] =
            pdf_engine::learned::line_class_probs(&refs, (width, height), &facts)
                .into_iter()
                .map(|p| json!(p))
                .collect::<Vec<Value>>()
                .into();
        record["list_signal"] = pdf_engine::learned::list_signal(&refs, (width, height), &facts)
            .into_iter()
            .map(|p| if p.is_nan() { Value::Null } else { json!(p) })
            .collect::<Vec<Value>>()
            .into();
        // /18: every table the pass would emit, with its final-table
        // vector (#574, #638): one entry per table with `bbox`
        // (user space, y up),
        // the proposer kind, the feature vector in `final_table_names`
        // order, and the engine/decisions stamp the record carries. NaN is
        // not JSON: a missing judge probability is `null`.
        let pictures = images.get(page).map(Vec::as_slice).unwrap_or(&[]);
        let finals = pdf_engine::learned::final_tables_for_dump(
            on_page.clone(),
            (width, height),
            &facts,
            drawn,
            &rulings,
            pictures,
        );
        record["final_table_names"] = json!(pdf_engine::learned::tables::TABLE_IS_V2_NAMES);
        record["final_tables"] = finals
            .iter()
            .map(|t| {
                json!({
                    "bbox": t.bbox,
                    "kind": t.kind,
                    "features": t.features.iter().map(|v| if v.is_nan() {
                        Value::Null
                    } else {
                        json!(v)
                    }).collect::<Vec<Value>>(),
                    "engine": {"tree": ENGINE_TREE, "decisions": ENGINE_DECISIONS},
                })
            })
            .collect::<Vec<Value>>()
            .into();
        out.push_str(&one_line(&record.to_string()));
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_engine::TextSpan;

    /// A span that actually sits somewhere, so the pair geometry is real.
    fn placed(text: &str, x0: f64, y0: f64, x1: f64, y1: f64) -> TextSpan {
        TextSpan {
            text: text.to_string(),
            char_bounds: Vec::new(),
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
            font_size: 10.0,
            ..Default::default()
        }
    }

    /// One 4x4 table. FOUR BY FOUR AND NOT TWO BY TWO: `candidates` gives each
    /// word six neighbours, and on a table of four words every word reaches
    /// every other whatever the region is -- a fixture that small cannot tell
    /// a right answer from a wrong one.
    fn table_page() -> Vec<Block> {
        let mut blocks = Vec::new();
        for row in 0..4u16 {
            for column in 0..4u16 {
                let x0 = 72.0 + f64::from(column) * 45.0;
                let y0 = 600.0 - f64::from(row) * 20.0;
                let text = format!("r{row}c{column}");
                blocks.push({
                    let mut b = Block::new(
                        BlockKind::TableCell {
                            row,
                            column,
                            row_span: 1,
                            column_span: 1,
                            header: row == 0,
                        },
                        0,
                        [x0, y0, x0 + 40.0, y0 + 10.0],
                        text.clone(),
                        blocks.len(),
                    );
                    b.spans = vec![placed(&text, x0, y0, x0 + 40.0, y0 + 10.0)];
                    b
                });
            }
        }
        blocks
    }

    fn union(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
        [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ]
    }

    fn span(text: &str, glyphs: Vec<[f64; 4]>) -> TextSpan {
        TextSpan {
            text: text.to_string(),
            char_bounds: glyphs,
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 10.0,
            font_size: 10.0,
            ..Default::default()
        }
    }

    fn cover(words: &[Word]) -> [f64; 4] {
        words
            .iter()
            .fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |a, w| {
                union(a, w.bbox)
            })
    }

    #[test]
    fn one_glyph_per_char_splits_on_spaces() {
        let g = |x: f64| [x, 0.0, x + 5.0, 10.0];
        let s = span("ab cd", vec![g(0.0), g(5.0), g(10.0), g(15.0), g(20.0)]);
        let w = words_of(&s);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].text, "ab");
        assert_eq!(w[0].bbox, [0.0, 0.0, 10.0, 10.0]);
        assert_eq!(w[1].text, "cd");
        assert_eq!(w[1].bbox, [15.0, 0.0, 25.0, 10.0]);
    }

    /// T4's trap (#600): "fine" drawn with an `fi` ligature is three glyphs and
    /// four chars. Every glyph must land in a word, so the union of the word
    /// boxes is the union of the glyph boxes, and nothing past the last glyph
    /// is invented.
    #[test]
    fn a_ligature_keeps_every_glyph_in_one_word() {
        let glyphs = vec![
            [0.0, 0.0, 9.0, 10.0],
            [9.0, 0.0, 14.0, 10.0],
            [14.0, 0.0, 19.0, 10.0],
        ];
        let s = span("fine", glyphs.clone());
        let w = words_of(&s);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].text, "fine");
        let all = glyphs
            .iter()
            .fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |a, b| {
                union(a, *b)
            });
        assert_eq!(cover(&w), all);
    }

    #[test]
    fn a_ligature_before_a_space_does_not_leak_into_the_next_word() {
        // "fi x": fi-ligature (1 glyph), space, x -> 3 glyphs, 4 chars.
        let glyphs = vec![
            [0.0, 0.0, 9.0, 10.0],
            [9.0, 0.0, 12.0, 10.0],
            [12.0, 0.0, 17.0, 10.0],
        ];
        let s = span("fi x", glyphs.clone());
        let w = words_of(&s);
        assert_eq!(
            w.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(),
            ["fi", "x"]
        );
        assert!(
            w[0].bbox[2] <= 12.0,
            "the first word ends before the x: {:?}",
            w[0].bbox
        );
        assert_eq!(w[1].bbox[2], 17.0);
    }

    #[test]
    fn a_span_without_glyph_boxes_uses_its_own_box() {
        let s = span("one two", Vec::new());
        let w = words_of(&s);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].bbox, [0.0, 0.0, 100.0, 10.0]);
    }

    #[test]
    fn a_record_is_one_line_whatever_the_text_holds() {
        let v = serde_json::json!({"text": "a\u{2028}b\u{2029}c\u{85}d\ne"});
        let line = one_line(&v.to_string());
        assert_eq!(line.lines().count(), 1);
        assert!(
            !line.contains('\u{2028}') && !line.contains('\u{2029}') && !line.contains('\u{85}')
        );
        let back: serde_json::Value = serde_json::from_str(&line).expect("still JSON");
        assert_eq!(back, v);
    }

    #[test]
    fn a_cell_names_its_table_row_and_column_and_nothing_else_does() {
        let cell = |row: u16, column: u16, i: usize| {
            let mut b = Block::new(
                BlockKind::TableCell {
                    row,
                    column,
                    row_span: 1,
                    column_span: 1,
                    header: false,
                },
                0,
                [72.0, 600.0, 100.0, 612.0],
                format!("c{i}"),
                i,
            );
            b.spans = vec![span(&format!("c{i}"), Vec::new())];
            b
        };
        let para = |i: usize| {
            let mut b = Block::new(
                BlockKind::Paragraph,
                0,
                [72.0, 500.0, 300.0, 512.0],
                format!("p{i}"),
                i,
            );
            b.spans = vec![span(&format!("p{i}"), Vec::new())];
            b
        };
        // table 0: 2x2; a paragraph; table 1: (0,0),(0,1); then table 2
        // straight behind it, starting again at (0,0).
        let blocks = vec![
            cell(0, 0, 0),
            cell(0, 1, 1),
            cell(1, 0, 2),
            cell(1, 1, 3),
            para(4),
            cell(0, 0, 5),
            cell(0, 1, 6),
            cell(0, 0, 7),
            cell(1, 0, 8),
        ];
        let r = page_record(&blocks, &[], 0, 612.0, 792.0, "id", "sha");
        let rows = r["blocks"].as_array().unwrap();
        let tables: Vec<Option<u64>> = rows
            .iter()
            .map(|b| b.get("table").and_then(|t| t.as_u64()))
            .collect();
        assert_eq!(
            tables,
            [
                Some(0),
                Some(0),
                Some(0),
                Some(0),
                None,
                Some(1),
                Some(1),
                Some(2),
                Some(2)
            ]
        );
        assert_eq!(rows[3]["row"], 1);
        assert_eq!(rows[3]["column"], 1);
        assert_eq!(rows[3]["row_span"], 1);
        assert!(
            rows[4].get("row").is_none() && rows[4].get("table").is_none(),
            "absent, not a sentinel"
        );
    }

    /// The dump's pairs ARE the engine's pairs -- the rule the whole file
    /// rests on. Recomputing them here from `pdf_engine::learned::pairs` and
    /// requiring the same bytes is what goes red if the dump ever grows its
    /// own copy of the feature computation.
    #[test]
    fn the_pairs_a_record_writes_are_the_engine_s_own() {
        let blocks = table_page();
        let r = page_record(&blocks, &[], 0, 612.0, 792.0, "id", "sha");
        let rows = r["pairs"].as_array().unwrap();
        assert!(!rows.is_empty(), "a table page writes pairs");
        assert_eq!(
            r["pair_names"].as_array().unwrap().len(),
            pdf_engine::learned::pairs::NAMES.len()
        );

        let words = r["words"].as_array().unwrap();
        let rebuilt: Vec<pdf_engine::learned::pairs::PairWord> = words
            .iter()
            .map(|w| {
                let b = w["bbox"].as_array().unwrap();
                let block = w["block"].as_u64().unwrap() as usize;
                let (table, row, column) = match blocks[block].kind {
                    BlockKind::TableCell { row, column, .. } => (Some(0), Some(row), Some(column)),
                    _ => (None, None, None),
                };
                pdf_engine::learned::pairs::PairWord {
                    text: w["text"].as_str().unwrap().to_string(),
                    bbox: [
                        b[0].as_f64().unwrap(),
                        b[1].as_f64().unwrap(),
                        b[2].as_f64().unwrap(),
                        b[3].as_f64().unwrap(),
                    ],
                    table,
                    row,
                    column,
                }
            })
            .collect();
        for row in rows {
            let row = row.as_array().unwrap();
            assert_eq!(row.len(), 2 + pdf_engine::learned::pairs::NAMES.len());
            let i = row[0].as_u64().unwrap() as usize;
            let j = row[1].as_u64().unwrap() as usize;
            let mine = pdf_engine::learned::pairs::features(&rebuilt[i], &rebuilt[j]);
            let written: Vec<f64> = row[2..].iter().map(|v| v.as_f64().unwrap()).collect();
            assert_eq!(written, mine.to_vec(), "pair {i},{j}");
        }
    }

    /// The region, not the page: a word two lines outside the table is in,
    /// one on the far side of the page is out. Without this the model would be
    /// trained on a population it is never run on.
    #[test]
    fn only_the_words_around_a_table_are_paired() {
        let mut blocks = table_page();
        // Just above the cells -- a title line, the kind the model must judge.
        blocks.push({
            let mut b = Block::new(
                BlockKind::Paragraph,
                0,
                [72.0, 624.0, 200.0, 634.0],
                "Table 1".into(),
                blocks.len(),
            );
            b.spans = vec![placed("Table 1", 72.0, 624.0, 128.0, 634.0)];
            b
        });
        // The foot of the page, far from any table.
        blocks.push({
            let mut b = Block::new(
                BlockKind::Paragraph,
                0,
                [72.0, 80.0, 400.0, 92.0],
                "elsewhere entirely".into(),
                blocks.len(),
            );
            b.spans = vec![placed("elsewhere entirely", 72.0, 80.0, 250.0, 92.0)];
            b
        });
        let r = page_record(&blocks, &[], 0, 612.0, 792.0, "id", "sha");
        let words = r["words"].as_array().unwrap();
        let mut seen: Vec<&str> = Vec::new();
        for row in r["pairs"].as_array().unwrap() {
            for n in 0..2 {
                let k = row[n].as_u64().unwrap() as usize;
                let t = words[k]["text"].as_str().unwrap();
                if !seen.contains(&t) {
                    seen.push(t);
                }
            }
        }
        assert!(seen.contains(&"Table"), "the title line is in the region");
        assert!(
            !seen.contains(&"elsewhere"),
            "the page foot is not: {seen:?}"
        );
    }

    /// EVERY WORD OF A TABLE HAS AN EDGE INSIDE THAT TABLE, which is the
    /// property the per-table region exists for and the one whose absence is
    /// invisible in a spot check. Asked over a page-wide list, a word at the
    /// edge of one table spends its six nearest neighbours on the OTHER table
    /// and comes away with nothing inside its own; the decoder closes rows and
    /// columns transitively, so such a word is a cell of its own and the grid
    /// comes apart. That read as TEDS 0.0868 against 0.7377 for the same
    /// model, the same words and the same floor (#600).
    #[test]
    fn no_word_of_a_table_is_left_without_an_edge_inside_it() {
        // Two tables side by side, close enough that the nearest neighbours of
        // the inner column of each are in the other.
        let mut blocks = table_page();
        for row in 0..4u16 {
            for column in 0..4u16 {
                let x0 = 265.0 + f64::from(column) * 45.0;
                let y0 = 600.0 - f64::from(row) * 20.0;
                let text = format!("s{row}c{column}");
                let i = blocks.len();
                blocks.push({
                    let mut b = Block::new(
                        BlockKind::TableCell {
                            row,
                            column,
                            row_span: 1,
                            column_span: 1,
                            header: row == 0,
                        },
                        0,
                        [x0, y0, x0 + 40.0, y0 + 10.0],
                        text.clone(),
                        i,
                    );
                    b.spans = vec![placed(&text, x0, y0, x0 + 40.0, y0 + 10.0)];
                    b
                });
            }
        }
        let r = page_record(&blocks, &[], 0, 612.0, 792.0, "id", "sha");
        let words = r["words"].as_array().unwrap();
        // Which table each word belongs to, by the block it came from.
        let table_of: Vec<Option<u64>> = words
            .iter()
            .map(|w| {
                let b = w["block"].as_u64().unwrap() as usize;
                r["blocks"].as_array().unwrap()[b]
                    .get("table")
                    .and_then(|t| t.as_u64())
            })
            .collect();
        assert_eq!(
            table_of.iter().filter(|t| **t == Some(0)).count(),
            table_of.iter().filter(|t| **t == Some(1)).count(),
            "two tables of the same size"
        );
        let mut has_edge = vec![false; words.len()];
        for row in r["pairs"].as_array().unwrap() {
            let i = row[0].as_u64().unwrap() as usize;
            let j = row[1].as_u64().unwrap() as usize;
            if table_of[i].is_some() && table_of[i] == table_of[j] {
                has_edge[i] = true;
                has_edge[j] = true;
            }
        }
        let orphans: Vec<&str> = (0..words.len())
            .filter(|&k| table_of[k].is_some() && !has_edge[k])
            .map(|k| words[k]["text"].as_str().unwrap())
            .collect();
        assert!(
            orphans.is_empty(),
            "every word of a table pairs with another word of that table; \
             these did not: {orphans:?}"
        );
    }

    /// A page our heuristics found no table on writes an empty list, not a
    /// missing field and not the whole page's pairs.
    #[test]
    fn a_page_without_a_table_writes_no_pairs() {
        let blocks = vec![{
            let mut b = Block::new(
                BlockKind::Paragraph,
                0,
                [72.0, 600.0, 400.0, 612.0],
                "just some prose here".into(),
                0,
            );
            b.spans = vec![placed("just some prose here", 72.0, 600.0, 300.0, 612.0)];
            b
        }];
        let r = page_record(&blocks, &[], 0, 612.0, 792.0, "id", "sha");
        assert_eq!(r["pairs"].as_array().unwrap().len(), 0);
    }

    /// Every dump record names the engine tree that segmented it (#574): the
    /// trainers refuse a record whose tree is not the serving one, so a
    /// missing stamp is a refused dump. In CI the stamp is a real tree --
    /// "unknown" is what an exported tree builds with, and CI is a checkout.
    #[test]
    fn a_dump_record_carries_the_engine_that_segmented_it() {
        let mut b = Block::new(
            BlockKind::Paragraph,
            0,
            [72.0, 600.0, 400.0, 612.0],
            "text".into(),
            0,
        );
        b.spans = vec![span("text", Vec::new())];
        let r = page_record(&[b], &[], 0, 612.0, 792.0, "id", "sha");
        assert_eq!(r["engine"]["tree"], ENGINE_TREE);
        assert_eq!(r["engine"]["decisions"], ENGINE_DECISIONS);
        assert_eq!(r["engine"]["commit"], ENGINE_COMMIT);
        assert_eq!(r["engine"]["decisions_without"], decisions_without());
        assert!(
            r["engine"]["decisions_without"].is_object(),
            "engine.decisions_without is a map of model to hash"
        );
        if std::env::var("CI").is_ok() {
            for (name, value) in [
                ("tree", ENGINE_TREE),
                ("decisions", ENGINE_DECISIONS),
                ("commit", ENGINE_COMMIT),
            ] {
                assert!(
                    !value.is_empty() && value != "unknown",
                    "engine.{name} is {value:?} in CI"
                );
            }
        }
    }

    /// The serving hash via python, or `None` where there is nothing to
    /// compare -- an exported tree ("unknown"), a dirty stamp ("-dirty" never
    /// equals a serving hash), or no python. In CI none of those hold.
    /// `serving_expr` is the `training_rules` call, e.g. `serving_tree()`.
    fn serving_via_python(stamp: &str, serving_expr: &str) -> Option<String> {
        let strict = std::env::var("CI").is_ok();
        if stamp == "unknown" || stamp.is_empty() {
            assert!(!strict, "engine stamp is {stamp:?} in CI");
            return None;
        }
        if stamp.ends_with("-dirty") {
            assert!(!strict, "engine stamp is dirty in CI: {stamp:?}");
            return None;
        }
        let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
        let root = std::path::Path::new(&manifest)
            .parent()
            .and_then(|p| p.parent())
            .unwrap_or(std::path::Path::new(&manifest));
        let learned = root.join("scripts/extraction/learned");
        let out = std::process::Command::new("python3")
            .arg("-c")
            .arg(format!(
                "import training_rules; print(training_rules.{serving_expr}, end='')"
            ))
            .env("PYTHONPATH", &learned)
            .current_dir(root)
            .output();
        let out = match out {
            Ok(out) if out.status.success() => out,
            _ => {
                assert!(
                    !strict,
                    "python3 training_rules.{serving_expr} failed in CI"
                );
                return None;
            }
        };
        let serving = String::from_utf8_lossy(&out.stdout).into_owned();
        assert!(
            !serving.is_empty() && serving != "unknown",
            "serving {serving_expr} is {serving:?}: nothing to compare against"
        );
        Some(serving)
    }

    /// build.rs and training_rules.py hash the same tree the same way (#574):
    /// the stamp above equals `serving_tree()` of this checkout. Skipped where
    /// there is nothing to compare -- an exported tree ("unknown"), a dirty
    /// engine ("-dirty" never equals a serving tree), or no python. In CI none
    /// of those hold, so CI compares.
    #[test]
    fn the_stamp_is_the_serving_tree() {
        if ENGINE_COMMIT == "unknown" || ENGINE_COMMIT.is_empty() {
            assert!(
                std::env::var("CI").is_err(),
                "engine.commit is {:?} in CI: nothing to compare against",
                ENGINE_COMMIT
            );
            return;
        }
        if let Some(serving) = serving_via_python(ENGINE_TREE, "serving_tree()") {
            assert_eq!(
                serving, ENGINE_TREE,
                "build.rs and training_rules.py hash this tree differently"
            );
        }
    }

    /// The same, for the decisions the dumped verdicts were written with: the
    /// `decisions` stamp equals `serving_decisions()` of this checkout.
    #[test]
    fn the_decisions_stamp_is_the_serving_decisions() {
        if let Some(serving) = serving_via_python(ENGINE_DECISIONS, "serving_decisions()") {
            assert_eq!(
                serving, ENGINE_DECISIONS,
                "build.rs and training_rules.py hash these decisions differently"
            );
        }
    }

    /// The same, minus the model's own directory: the `decisions_without`
    /// entry equals `serving_decisions_without()` of this checkout, for every
    /// self-excludable model the stamp names.
    #[test]
    fn the_decisions_without_stamp_is_the_serving_decisions_without() {
        let stamped: Value = serde_json::from_str(ENGINE_DECISIONS_WITHOUT)
            .expect("the without stamp is a JSON map");
        let entries = stamped.as_object().expect("the without stamp is a map");
        assert!(
            !entries.is_empty(),
            "the without stamp names no model: nothing to compare against"
        );
        for (name, hash) in entries {
            let hash = hash.as_str().expect("a without entry is a hash");
            let expr = format!("serving_decisions_without({name:?})");
            if let Some(serving) = serving_via_python(hash, &expr) {
                assert_eq!(
                    serving, hash,
                    "build.rs and training_rules.py hash these decisions \
                     without {name} differently"
                );
            }
        }
    }

    #[test]
    fn every_block_is_a_record_row_and_the_schema_is_named() {
        let mk = |kind: BlockKind, i: usize, y: f64| {
            let mut b = Block::new(kind, 0, [72.0, y, 500.0, y + 12.0], format!("block {i}"), i);
            b.spans = vec![span(&format!("block {i}"), Vec::new())];
            b
        };
        let blocks = vec![
            mk(BlockKind::Heading { level: 1 }, 0, 700.0),
            mk(BlockKind::Paragraph, 1, 650.0),
            mk(BlockKind::Footer, 2, 30.0),
        ];
        let r = page_record(&blocks, &[], 0, 612.0, 792.0, "abc", "def");
        assert_eq!(r["schema"], SCHEMA);
        assert_eq!(r["page_id"], "abc");
        let kinds: Vec<&str> = r["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["kind"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, ["heading", "paragraph", "footer"]);
        assert_eq!(r["words"].as_array().unwrap().len(), 6);
    }
}
