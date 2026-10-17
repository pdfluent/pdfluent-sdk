// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Collect false-table hard negatives from prepared pages.
//!
//! ```sh
//! collect_hard_negatives --input pages.jsonl --rig rig.json \
//!   --out tables.jsonl --capped hosts-capped.jsonl --summary summary.json
//! ```
//!
//! `pages.jsonl` holds one [`PageInput`] per line: engine blocks plus truth
//! regions in user space, as the driver prepares them. `rig.json` holds the
//! refused document names as a JSON list. The collector refuses rig pages and
//! counts them, labels every produced table, writes all rows to `--out`, at
//! most five hosts per document plus every positive to `--capped`, and the
//! summary beside them. It fails when a rig document gets through.
//!
//! The engine sometimes builds tables out of prose set in columns. A hand rule
//! cannot remove them without costing real tables, so the table-or-not model is
//! retrained with hard negatives: produced tables that cover annotated text.
//! This binary collects them, as labels only.
//!
//! A produced table is a run of `tableCell` blocks whose `(row, column)`
//! rises, as the markdown serializer reads it. It is a `host` when it covers
//! at least two truth text regions and no truth table region, and a `positive`
//! when it overlaps a truth table region. Every row carries the page, the
//! table's box, the label, what a host swallows, and the table's overlap with
//! its best-overlapping truth table, as IoU and as a share of the smaller
//! area. Feature values for training come from the engine's own feature dump,
//! joined on page and box -- never from a second implementation beside it.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

/// One span of a block, as `structured_markdown --json` writes it: text and
/// geometry in user space, y up, `y` the bottom of the run.
#[derive(Debug, Clone, Deserialize)]
struct SpanInput {
    /// The run's text.
    #[serde(default)]
    text: String,
    /// Left edge in user space.
    #[serde(default)]
    x: f64,
    /// Bottom edge in user space.
    #[serde(default)]
    y: f64,
    /// Width in user space.
    #[serde(default)]
    width: f64,
    /// Height in user space.
    #[serde(default)]
    height: f64,
}

/// One block of the page, as `structured_markdown --json` writes it.
///
/// The labels read the grid coordinates, the page, the box and the spans; the
/// text and the spans' reach stay parsed because the struct mirrors the
/// driver's JSON, and a format documented in one place is worth the fields.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct BlockInput {
    /// `paragraph`, `heading`, `listItem`, `tableCell`, `header`, `footer`,
    /// `caption`.
    #[serde(default)]
    kind: String,
    /// 0-based row inside the table, on a cell.
    #[serde(default)]
    row: Option<u16>,
    /// 0-based column inside the table, on a cell.
    #[serde(default)]
    column: Option<u16>,
    /// How many rows the cell covers, on a cell.
    #[serde(default)]
    row_span: Option<u16>,
    /// How many columns the cell covers, on a cell.
    #[serde(default)]
    column_span: Option<u16>,
    /// 0-based page index.
    #[serde(default)]
    page: usize,
    /// `[x0, y0, x1, y1]` in user space, y up.
    #[serde(default)]
    bbox: [f64; 4],
    /// The block's text.
    #[serde(default)]
    text: String,
    /// The spans the block is made of, in draw order.
    #[serde(default)]
    spans: Vec<SpanInput>,
}

/// One truth region, in user space, y up.
#[derive(Debug, Clone, Deserialize)]
struct RegionInput {
    /// DocLayNet category: `Text`, `Section-header`, `List-item`, `Caption`,
    /// `Footnote`, `Title`, `Table`, and the rest.
    #[serde(default)]
    category: String,
    /// `[x0, y0, x1, y1]` in user space, y up.
    #[serde(default)]
    bbox: [f64; 4],
}

/// One page to collect from: engine blocks plus truth.
///
/// The labels read the identity, the document, the category, the blocks and
/// the regions; the rest stays parsed because the struct mirrors the driver's
/// JSON, and a format documented in one place is worth the fields.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct PageInput {
    /// The dataset's page identity.
    #[serde(default)]
    page_id: String,
    /// DocLayNet's original filename.
    #[serde(default)]
    document: String,
    /// 1-based page number inside the document, where known.
    #[serde(default)]
    page_no: Option<u32>,
    /// DocLayNet document category, where known.
    #[serde(default)]
    category: Option<String>,
    /// Page width in user space.
    #[serde(default)]
    width: f64,
    /// Page height in user space.
    #[serde(default)]
    height: f64,
    /// The engine commit that produced `blocks`.
    #[serde(default)]
    commit: String,
    /// The blocks `structured_markdown --json` emitted, in reading order.
    #[serde(default)]
    blocks: Vec<BlockInput>,
    /// The truth regions, in user space.
    #[serde(default)]
    regions: Vec<RegionInput>,
}

/// One covered truth region, as carried on a host row.
#[derive(Debug, Clone, Serialize)]
struct Swallowed {
    /// The region's category.
    category: String,
    /// `[x0, y0, x1, y1]` in user space, y up.
    bbox: [f64; 4],
    /// How many of the page's words have their centre in the region.
    n_words: usize,
}

/// One produced table, labelled host or positive.
///
/// Labels only: which table on which page, what it is, what a host swallows,
/// and how it overlaps the truth. Feature values for training come from the
/// engine's own feature dump, joined on page and box.
#[derive(Debug, Clone, Serialize)]
struct TableRow {
    /// The dataset's page identity.
    page_id: String,
    /// DocLayNet's original filename.
    document: String,
    /// DocLayNet document category, where known.
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<String>,
    /// Union of the table's cells, in user space, y up.
    table_bbox: [f64; 4],
    /// `host` or `positive`.
    label: String,
    /// The covered truth regions, on a host only.
    #[serde(skip_serializing_if = "Option::is_none")]
    swallowed: Option<Vec<Swallowed>>,
    /// IoU with the best-overlapping truth Table region; 0.0 where the page
    /// names no truth table.
    truth_table_iou: f64,
    /// Overlap with that same region as a share of the smaller area; 0.0
    /// where the page names no truth table.
    truth_table_overlap: f64,
}

/// The summary beside the jsonl.
#[derive(Debug, Clone, Serialize)]
struct Summary {
    /// Pages collected.
    n_pages: usize,
    /// Produced tables labelled.
    n_tables: usize,
    /// Of them hosts.
    n_hosts: usize,
    /// Of them positives.
    n_positives: usize,
    /// Hosts per collected page.
    hosts_per_page: f64,
    /// Hosts by document.
    hosts_per_document: BTreeMap<String, usize>,
    /// Positives by document.
    positives_per_document: BTreeMap<String, usize>,
    /// Hosts by document category.
    hosts_per_category: BTreeMap<String, usize>,
    /// Positives by document category.
    positives_per_category: BTreeMap<String, usize>,
    /// The ten documents with most hosts, most first.
    top_documents: Vec<TopDocument>,
    /// Pages refused as rig documents.
    refused_rig: usize,
}

/// One row of the top-ten table.
#[derive(Debug, Clone, Serialize)]
struct TopDocument {
    /// The document.
    document: String,
    /// Its hosts.
    hosts: usize,
}

/// Whether a block is a table cell, and its coordinates where it is one.
fn cell_of(block: &BlockInput) -> Option<(u16, u16)> {
    if block.kind == "tableCell" {
        match (block.row, block.column) {
            (Some(row), Some(column)) => Some((row, column)),
            _ => None,
        }
    } else {
        None
    }
}

/// Group cell blocks into tables.
///
/// A table is a run of adjacent cells whose `(row, column)` climbs with every
/// block, ending where the blocks stop being cells, the page changes, or the
/// coordinates go backwards, which is a second table starting. The same rule
/// the markdown serializer reads, repeated here because that function is
/// private to its crate: a table the engine emits together is one table here.
fn group_tables(blocks: &[BlockInput]) -> Vec<Vec<usize>> {
    let mut tables: Vec<Vec<usize>> = Vec::new();
    let mut at = 0;
    while at < blocks.len() {
        let Some(first) = cell_of(&blocks[at]) else {
            at += 1;
            continue;
        };
        let page = blocks[at].page;
        let mut last = first;
        let mut end = at + 1;
        while end < blocks.len() && blocks[end].page == page {
            match cell_of(&blocks[end]) {
                Some(coords) if coords > last => {
                    last = coords;
                    end += 1;
                }
                _ => break,
            }
        }
        tables.push((at..end).collect());
        at = end;
    }
    tables
}

/// The union of the given boxes.
fn union_of(boxes: &[[f64; 4]]) -> [f64; 4] {
    let mut out = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for b in boxes {
        out[0] = out[0].min(b[0]);
        out[1] = out[1].min(b[1]);
        out[2] = out[2].max(b[2]);
        out[3] = out[3].max(b[3]);
    }
    if out[0].is_infinite() {
        [0.0, 0.0, 0.0, 0.0]
    } else {
        out
    }
}

/// The area of a box, zero where it is inverted.
fn area(b: &[f64; 4]) -> f64 {
    (b[2] - b[0]).max(0.0) * (b[3] - b[1]).max(0.0)
}

/// The area of the intersection of two boxes.
fn intersection_area(a: &[f64; 4], b: &[f64; 4]) -> f64 {
    let w = (a[2].min(b[2]) - a[0].max(b[0])).max(0.0);
    let h = (a[3].min(b[3]) - a[1].max(b[1])).max(0.0);
    w * h
}

/// The intersection over union of two boxes, 0.0 where either is empty.
fn iou(a: &[f64; 4], b: &[f64; 4]) -> f64 {
    let inter = intersection_area(a, b);
    let union = area(a) + area(b) - inter;
    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// Whether a point lies inside a box, edges included.
fn contains(b: &[f64; 4], x: f64, y: f64) -> bool {
    x >= b[0] && x <= b[2] && y >= b[1] && y <= b[3]
}

/// One word of the page: its box in user space.
#[derive(Debug, Clone)]
struct Word {
    bbox: [f64; 4],
}

/// The page's words, from every block's spans.
///
/// A span carries no per-glyph boxes in the JSON wire format, so its text is
/// split on whitespace and its width divided in proportion to token length.
/// Centres are what coverage reads, and a proportional split puts each centre
/// within a few points of the truth: small beside a region, and exact for the
/// single-word spans a table cell usually holds.
fn words_of_page(blocks: &[BlockInput]) -> Vec<Word> {
    let mut out = Vec::new();
    for block in blocks {
        for span in &block.spans {
            let tokens: Vec<&str> = span.text.split_whitespace().collect();
            if tokens.is_empty() {
                continue;
            }
            let total: usize = tokens.iter().map(|t| t.chars().count().max(1)).sum();
            let total = total.max(1) as f64;
            let mut x = span.x;
            for token in tokens {
                let share = token.chars().count().max(1) as f64 / total;
                let w = span.width * share;
                out.push(Word {
                    bbox: [x, span.y, x + w, span.y + span.height],
                });
                x += w;
            }
        }
    }
    out
}

/// The normalised category: lower case, `-` and ` ` read as `_`.
fn normalise_category(category: &str) -> String {
    category
        .trim()
        .to_ascii_lowercase()
        .replace(['-', ' '], "_")
}

/// Whether a truth category counts as text for coverage.
fn is_text_category(normalised: &str) -> bool {
    matches!(
        normalised,
        "text" | "section_header" | "list_item" | "caption" | "footnote" | "title"
    )
}

/// Whether a truth category is a table.
fn is_table_category(normalised: &str) -> bool {
    normalised == "table"
}

/// The overlap of two boxes as a share of the smaller one's area.
fn overlap_of_smaller(a: &[f64; 4], b: &[f64; 4]) -> f64 {
    let smaller = area(a).min(area(b));
    if smaller <= 0.0 {
        0.0
    } else {
        intersection_area(a, b) / smaller
    }
}

/// A produced table against the truth tables: the IoU with its
/// best-overlapping truth Table region, and with that same region the overlap
/// as a share of the smaller area.
///
/// Best is the highest IoU. The pair is what one number cannot say: a fragment
/// inside a big truth table has a share of 1.0 and a small IoU, which is a
/// table the engine cut too small, not a table it misplaced. (0.0, 0.0) where
/// the page names no truth table.
fn truth_table_overlap(table: &[f64; 4], regions: &[RegionInput]) -> (f64, f64) {
    let mut best = (0.0, 0.0);
    for region in regions {
        if !is_table_category(&normalise_category(&region.category)) {
            continue;
        }
        let candidate = (
            iou(table, &region.bbox),
            overlap_of_smaller(table, &region.bbox),
        );
        if candidate.0 > best.0 {
            best = candidate;
        }
    }
    best
}

/// How much of a region's words lie inside a table.
///
/// The region's words are the page's words whose centre lies in the region;
/// covered are those whose centre also lies inside the table's box. A region
/// with no words is never covered: there is nothing to judge it by.
fn coverage_of(region: &[f64; 4], table: &[f64; 4], words: &[Word]) -> (usize, usize) {
    let mut in_region = 0usize;
    let mut covered = 0usize;
    for word in words {
        let cx = (word.bbox[0] + word.bbox[2]) * 0.5;
        let cy = (word.bbox[1] + word.bbox[3]) * 0.5;
        if contains(region, cx, cy) {
            in_region += 1;
            if contains(table, cx, cy) {
                covered += 1;
            }
        }
    }
    (covered, in_region)
}

/// Label one produced table.
///
/// A host covers at least two truth text regions (each with ≥ 0.8 of its words
/// inside the table) and overlaps no truth table region by ≥ 0.5 of the
/// smaller area. A positive overlaps a truth table region by ≥ 0.5. Anything
/// else is neither and is left out.
fn label_table(
    table_bbox: &[f64; 4],
    regions: &[RegionInput],
    words: &[Word],
) -> Option<(String, Option<Vec<Swallowed>>)> {
    let mut swallowed = Vec::new();
    for region in regions {
        let normalised = normalise_category(&region.category);
        if is_text_category(&normalised) {
            let (covered, in_region) = coverage_of(&region.bbox, table_bbox, words);
            if in_region > 0 && covered as f64 / in_region as f64 >= 0.8 {
                swallowed.push(Swallowed {
                    category: region.category.clone(),
                    bbox: region.bbox,
                    n_words: in_region,
                });
            }
        }
    }
    let touches_truth_table = regions.iter().any(|region| {
        is_table_category(&normalise_category(&region.category))
            && overlap_of_smaller(table_bbox, &region.bbox) >= 0.5
    });
    if touches_truth_table {
        Some(("positive".to_string(), None))
    } else if swallowed.len() >= 2 {
        Some(("host".to_string(), Some(swallowed)))
    } else {
        None
    }
}

/// The rows of one page: every produced table that is a host or a positive.
fn process_page(page: &PageInput) -> Vec<TableRow> {
    let words = words_of_page(&page.blocks);
    let mut rows = Vec::new();
    for table in group_tables(&page.blocks) {
        let cells: Vec<&BlockInput> = table.iter().map(|&i| &page.blocks[i]).collect();
        let boxes: Vec<[f64; 4]> = cells.iter().map(|c| c.bbox).collect();
        let table_bbox = union_of(&boxes);
        let Some((label, swallowed)) = label_table(&table_bbox, &page.regions, &words) else {
            continue;
        };
        let (truth_table_iou, truth_table_overlap) =
            truth_table_overlap(&table_bbox, &page.regions);
        rows.push(TableRow {
            page_id: page.page_id.clone(),
            document: page.document.clone(),
            category: page.category.clone(),
            table_bbox,
            label,
            swallowed,
            truth_table_iou,
            truth_table_overlap,
        });
    }
    rows
}

/// Drop every page whose document the rig names, counting the refusals.
///
/// By document, not by page: one page of a rig document in training is a leak
/// whatever the other pages are.
fn refuse_rig_documents(pages: Vec<PageInput>, rig: &HashSet<String>) -> (Vec<PageInput>, usize) {
    let mut kept = Vec::with_capacity(pages.len());
    let mut refused = 0usize;
    for page in pages {
        if rig.contains(&page.document) {
            refused += 1;
        } else {
            kept.push(page);
        }
    }
    (kept, refused)
}

/// Fail when a rig document got through into the rows.
fn verify_no_rig(rows: &[TableRow], rig: &HashSet<String>) -> Result<(), String> {
    for row in rows {
        if rig.contains(&row.document) {
            return Err(format!(
                "refused document got through: {} (page {})",
                row.document, row.page_id
            ));
        }
    }
    Ok(())
}

/// Keep at most `limit` hosts per document, in row order.
///
/// Deterministic rather than sampled: the first rows stay. Positives are never
/// capped; the cap is what keeps one document from taking over the negatives.
fn cap_hosts(rows: &[TableRow], limit: usize) -> Vec<TableRow> {
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        if row.label != "host" {
            out.push(row.clone());
            continue;
        }
        let count = seen.entry(row.document.clone()).or_insert(0);
        if *count < limit {
            *count += 1;
            out.push(row.clone());
        }
    }
    out
}

/// Summarise labelled rows.
fn summarize(rows: &[TableRow], n_pages: usize, refused_rig: usize) -> Summary {
    let mut hosts_per_document: BTreeMap<String, usize> = BTreeMap::new();
    let mut positives_per_document: BTreeMap<String, usize> = BTreeMap::new();
    let mut hosts_per_category: BTreeMap<String, usize> = BTreeMap::new();
    let mut positives_per_category: BTreeMap<String, usize> = BTreeMap::new();
    let mut n_hosts = 0usize;
    let mut n_positives = 0usize;
    for row in rows {
        let category = row
            .category
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        if row.label == "host" {
            n_hosts += 1;
            *hosts_per_document.entry(row.document.clone()).or_insert(0) += 1;
            *hosts_per_category.entry(category).or_insert(0) += 1;
        } else {
            n_positives += 1;
            *positives_per_document
                .entry(row.document.clone())
                .or_insert(0) += 1;
            *positives_per_category.entry(category).or_insert(0) += 1;
        }
    }
    let mut top: Vec<(String, usize)> = hosts_per_document
        .iter()
        .map(|(d, &n)| (d.clone(), n))
        .collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    top.truncate(10);
    Summary {
        n_pages,
        n_tables: rows.len(),
        n_hosts,
        n_positives,
        hosts_per_page: if n_pages == 0 {
            0.0
        } else {
            n_hosts as f64 / n_pages as f64
        },
        hosts_per_document,
        positives_per_document,
        hosts_per_category,
        positives_per_category,
        top_documents: top
            .into_iter()
            .map(|(document, hosts)| TopDocument { document, hosts })
            .collect(),
        refused_rig,
    }
}

fn usage() -> ! {
    eprintln!(
        "usage: collect_hard_negatives --input FILE --rig FILE --out FILE --capped FILE --summary FILE [--cap N]"
    );
    std::process::exit(2);
}

fn main() {
    let mut input = None;
    let mut rig_path = None;
    let mut out_path = None;
    let mut capped_path = None;
    let mut summary_path = None;
    let mut cap = 5usize;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--input" => input = Some(args.next().unwrap_or_else(|| usage())),
            "--rig" => rig_path = Some(args.next().unwrap_or_else(|| usage())),
            "--out" => out_path = Some(args.next().unwrap_or_else(|| usage())),
            "--capped" => capped_path = Some(args.next().unwrap_or_else(|| usage())),
            "--summary" => summary_path = Some(args.next().unwrap_or_else(|| usage())),
            "--cap" => {
                cap = args
                    .next()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or_else(|| usage());
            }
            _ => usage(),
        }
    }
    let (Some(input), Some(rig_path), Some(out_path), Some(capped_path), Some(summary_path)) =
        (input, rig_path, out_path, capped_path, summary_path)
    else {
        usage();
    };
    let rig_text = match std::fs::read_to_string(&rig_path) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("read {rig_path}: {err}");
            std::process::exit(2);
        }
    };
    let rig_list: Vec<String> = match serde_json::from_str(&rig_text) {
        Ok(list) => list,
        Err(err) => {
            eprintln!("parse {rig_path}: {err}");
            std::process::exit(2);
        }
    };
    let rig: HashSet<String> = rig_list.into_iter().collect();
    let input_text = match std::fs::read_to_string(&input) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("read {input}: {err}");
            std::process::exit(2);
        }
    };
    let mut pages = Vec::new();
    for (at, line) in input_text.split('\n').enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<PageInput>(line) {
            Ok(page) => pages.push(page),
            Err(err) => {
                eprintln!("parse {input} line {}: {err}", at + 1);
                std::process::exit(2);
            }
        }
    }
    let (kept, refused) = refuse_rig_documents(pages, &rig);
    let n_pages = kept.len();
    let mut rows = Vec::new();
    for page in &kept {
        rows.extend(process_page(page));
    }
    if let Err(err) = verify_no_rig(&rows, &rig) {
        eprintln!("{err}");
        std::process::exit(1);
    }
    let capped = cap_hosts(&rows, cap);
    let summary = summarize(&rows, n_pages, refused);
    let mut out_text = String::new();
    for row in &rows {
        out_text.push_str(&serde_json::to_string(row).unwrap_or_default());
        out_text.push('\n');
    }
    let mut capped_text = String::new();
    for row in &capped {
        capped_text.push_str(&serde_json::to_string(row).unwrap_or_default());
        capped_text.push('\n');
    }
    let summary_text = serde_json::to_string_pretty(&summary).unwrap_or_default();
    for (path, text) in [
        (out_path, out_text),
        (capped_path, capped_text),
        (summary_path, summary_text + "\n"),
    ] {
        if let Err(err) = std::fs::write(&path, text) {
            eprintln!("write {path}: {err}");
            std::process::exit(2);
        }
    }
    eprintln!(
        "{} page(s), {} table(s): {} host(s), {} positive(s), {} refused as rig",
        n_pages,
        rows.len(),
        summary.n_hosts,
        summary.n_positives,
        refused
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn span(text: &str, x0: f64, y0: f64, x1: f64, y1: f64) -> SpanInput {
        SpanInput {
            text: text.to_string(),
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
        }
    }

    fn cell(row: u16, column: u16, bbox: [f64; 4], text: &str) -> BlockInput {
        let [x0, y0, x1, y1] = bbox;
        BlockInput {
            kind: "tableCell".to_string(),
            row: Some(row),
            column: Some(column),
            row_span: Some(1),
            column_span: Some(1),
            page: 0,
            bbox,
            text: text.to_string(),
            spans: vec![span(text, x0, y0, x1, y1)],
        }
    }

    fn region(category: &str, bbox: [f64; 4]) -> RegionInput {
        RegionInput {
            category: category.to_string(),
            bbox,
        }
    }

    fn page(blocks: Vec<BlockInput>, regions: Vec<RegionInput>) -> PageInput {
        page_named("p1", "doc.pdf", blocks, regions)
    }

    fn page_named(
        page_id: &str,
        document: &str,
        blocks: Vec<BlockInput>,
        regions: Vec<RegionInput>,
    ) -> PageInput {
        PageInput {
            page_id: page_id.to_string(),
            document: document.to_string(),
            page_no: Some(1),
            category: Some("financial".to_string()),
            width: 612.0,
            height: 792.0,
            commit: "abc123".to_string(),
            blocks,
            regions,
        }
    }

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("hard_negatives")
            .join(name)
    }

    /// Two text regions under one produced table, and no truth table anywhere:
    /// the table is a host. Without the two-region rule it would be nothing.
    #[test]
    fn a_table_over_two_text_regions_and_no_truth_table_is_a_host() {
        let blocks = vec![
            cell(0, 0, [100.0, 600.0, 250.0, 620.0], "January notes"),
            cell(0, 1, [260.0, 600.0, 410.0, 620.0], "February notes"),
            cell(1, 0, [100.0, 580.0, 250.0, 600.0], "March notes"),
            cell(1, 1, [260.0, 580.0, 410.0, 600.0], "April notes"),
        ];
        let regions = vec![
            region("Text", [90.0, 595.0, 420.0, 625.0]),
            region("Text", [90.0, 575.0, 420.0, 605.0]),
        ];
        let rows = process_page(&page(blocks, regions));
        assert_eq!(rows.len(), 1, "one produced table, one row");
        assert_eq!(rows[0].label, "host");
        let swallowed = rows[0]
            .swallowed
            .as_ref()
            .expect("a host names what it swallows");
        assert_eq!(swallowed.len(), 2);
        assert_eq!(rows[0].document, "doc.pdf");
        assert_eq!(
            (rows[0].truth_table_iou, rows[0].truth_table_overlap),
            (0.0, 0.0),
            "no truth table on the page, so both numbers are 0.0"
        );
    }

    /// The same grid over a truth table is a positive, never a host. Without
    /// the overlap rule it would be labelled a host.
    #[test]
    fn a_table_over_a_truth_table_is_a_positive_not_a_host() {
        let blocks = vec![
            cell(0, 0, [100.0, 600.0, 250.0, 620.0], "January notes"),
            cell(0, 1, [260.0, 600.0, 410.0, 620.0], "February notes"),
            cell(1, 0, [100.0, 580.0, 250.0, 600.0], "March notes"),
            cell(1, 1, [260.0, 580.0, 410.0, 600.0], "April notes"),
        ];
        let regions = vec![
            region("Text", [90.0, 595.0, 420.0, 625.0]),
            region("Text", [90.0, 575.0, 420.0, 605.0]),
            region("Table", [95.0, 575.0, 415.0, 625.0]),
        ];
        let rows = process_page(&page(blocks, regions));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "positive");
        assert!(rows[0].swallowed.is_none(), "a positive swallows nothing");
        // The table is [100, 580, 410, 620] (12,400); the truth
        // [95, 575, 415, 625] (16,000) holds it whole.
        assert!(
            (rows[0].truth_table_iou - 12400.0 / 16000.0).abs() < 1e-9,
            "IoU against the truth table, got {}",
            rows[0].truth_table_iou
        );
        assert!(
            (rows[0].truth_table_overlap - 1.0).abs() < 1e-9,
            "the table lies whole in the truth, so the share is 1.0, got {}",
            rows[0].truth_table_overlap
        );
    }

    /// A rig document never reaches the rows: it is refused beforehand and
    /// counted, and the verifier fails a row that carries its name. Without
    /// the refusal the rig page would be collected like any other.
    #[test]
    fn a_rig_document_is_refused_and_counted() {
        let host_blocks = vec![
            cell(0, 0, [100.0, 600.0, 250.0, 620.0], "January notes"),
            cell(0, 1, [260.0, 600.0, 410.0, 620.0], "February notes"),
            cell(1, 0, [100.0, 580.0, 250.0, 600.0], "March notes"),
            cell(1, 1, [260.0, 580.0, 410.0, 600.0], "April notes"),
        ];
        let host_regions = vec![
            region("Text", [90.0, 595.0, 420.0, 625.0]),
            region("Text", [90.0, 575.0, 420.0, 605.0]),
        ];
        let pages = vec![
            page_named("p1", "rig.pdf", host_blocks.clone(), host_regions.clone()),
            page_named("p2", "free.pdf", host_blocks, host_regions),
        ];
        let rig: HashSet<String> = HashSet::from(["rig.pdf".to_string()]);
        let (kept, refused) = refuse_rig_documents(pages, &rig);
        assert_eq!(refused, 1, "the rig page is counted");
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].document, "free.pdf");
        let rows: Vec<TableRow> = kept.iter().flat_map(process_page).collect();
        assert!(verify_no_rig(&rows, &rig).is_ok());
        let leaked = TableRow {
            page_id: "p1".to_string(),
            document: "rig.pdf".to_string(),
            category: None,
            table_bbox: [0.0, 0.0, 1.0, 1.0],
            label: "host".to_string(),
            swallowed: None,
            truth_table_iou: 0.0,
            truth_table_overlap: 0.0,
        };
        assert!(
            verify_no_rig(&[leaked], &rig).is_err(),
            "a row from a rig document fails"
        );
    }

    /// Six hosts from one document come out as five. Without the cap all six
    /// would train, and one document could take over the negatives.
    #[test]
    fn the_capped_file_holds_at_most_five_hosts_per_document() {
        let mut rows = Vec::new();
        for i in 0..6 {
            rows.push(TableRow {
                page_id: format!("p{i}"),
                document: "greedy.pdf".to_string(),
                category: None,
                table_bbox: [0.0, 0.0, 1.0, 1.0],
                label: "host".to_string(),
                swallowed: None,
                truth_table_iou: 0.0,
                truth_table_overlap: 0.0,
            });
        }
        rows.push(TableRow {
            page_id: "q".to_string(),
            document: "greedy.pdf".to_string(),
            category: None,
            table_bbox: [0.0, 0.0, 1.0, 1.0],
            label: "positive".to_string(),
            swallowed: None,
            truth_table_iou: 0.0,
            truth_table_overlap: 0.0,
        });
        let capped = cap_hosts(&rows, 5);
        let hosts = capped.iter().filter(|r| r.label == "host").count();
        let positives = capped.iter().filter(|r| r.label == "positive").count();
        assert_eq!(hosts, 5, "hosts are capped at five per document");
        assert_eq!(positives, 1, "positives are never capped");
    }

    /// The two truth numbers on hand-made boxes: a partial overlap, and a
    /// fragment inside a big truth table, whose smaller-area share is 1.0
    /// while its IoU is small. One number cannot say both: the fragment is a
    /// table the engine cut too small, not one it misplaced.
    #[test]
    fn truth_overlap_reports_iou_and_the_smaller_area_share() {
        let regions = vec![region("Table", [5.0, 5.0, 15.0, 15.0])];
        let (iou, share) = truth_table_overlap(&[0.0, 0.0, 10.0, 10.0], &regions);
        assert!(
            (iou - 25.0 / 175.0).abs() < 1e-9,
            "IoU of a quarter overlap, got {iou}"
        );
        assert!(
            (share - 0.25).abs() < 1e-9,
            "share of a quarter overlap, got {share}"
        );
        let big = vec![region("Table", [0.0, 0.0, 1000.0, 1000.0])];
        let (iou, share) = truth_table_overlap(&[100.0, 100.0, 200.0, 150.0], &big);
        assert!(
            (share - 1.0).abs() < 1e-9,
            "a fragment's smaller-area share is 1.0, got {share}"
        );
        assert!(iou < 0.01, "a fragment's IoU is small, got {iou}");
        let no_table = vec![region("Text", [0.0, 0.0, 10.0, 10.0])];
        assert_eq!(
            truth_table_overlap(&[0.0, 0.0, 10.0, 10.0], &no_table),
            (0.0, 0.0),
            "no truth table on the page, so both numbers are 0.0"
        );
    }

    /// Rows carry labels, not shape features: the exact keys of a host row and
    /// of a positive row. Feature values come from the engine's own feature
    /// dump, and a shape feature computed here would be a second implementation
    /// of the same value beside it.
    #[test]
    fn rows_carry_labels_and_no_shape_features() {
        use std::collections::BTreeSet;
        let blocks = vec![
            cell(0, 0, [100.0, 600.0, 250.0, 620.0], "January notes"),
            cell(0, 1, [260.0, 600.0, 410.0, 620.0], "February notes"),
            cell(1, 0, [100.0, 580.0, 250.0, 600.0], "March notes"),
            cell(1, 1, [260.0, 580.0, 410.0, 600.0], "April notes"),
        ];
        let host_regions = vec![
            region("Text", [90.0, 595.0, 420.0, 625.0]),
            region("Text", [90.0, 575.0, 420.0, 605.0]),
        ];
        let rows = process_page(&page(blocks.clone(), host_regions));
        assert_eq!(rows.len(), 1);
        let host = serde_json::to_value(&rows[0]).expect("a row serializes");
        let keys: BTreeSet<String> = host
            .as_object()
            .expect("a row is an object")
            .keys()
            .cloned()
            .collect();
        let host_keys: BTreeSet<String> = [
            "page_id",
            "document",
            "category",
            "table_bbox",
            "label",
            "swallowed",
            "truth_table_iou",
            "truth_table_overlap",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        assert_eq!(keys, host_keys, "a host row carries exactly its labels");
        for key in [
            "rows",
            "columns",
            "cells",
            "filled_cells",
            "row_overlap",
            "column_start_spread",
            "words_per_cell",
            "digit_share",
            "sentence_share",
            "lines_per_cell",
            "run_on_share",
        ] {
            assert!(
                !keys.contains(key),
                "a row carries no shape feature, but {key} is back"
            );
        }
        let positive_regions = vec![region("Table", [95.0, 575.0, 415.0, 625.0])];
        let rows = process_page(&page(blocks, positive_regions));
        assert_eq!(rows.len(), 1);
        let positive = serde_json::to_value(&rows[0]).expect("a row serializes");
        let keys: BTreeSet<String> = positive
            .as_object()
            .expect("a row is an object")
            .keys()
            .cloned()
            .collect();
        let mut positive_keys = host_keys;
        positive_keys.remove("swallowed");
        assert_eq!(
            keys, positive_keys,
            "a positive row carries exactly its labels, and swallows nothing"
        );
    }

    /// The collector on the committed fixture: one host, one positive, one rig
    /// refusal and one plain page, all synthetic, no PDF.
    #[test]
    fn the_fixture_collects_one_host_and_one_positive_and_refuses_the_rig_page() {
        let text = std::fs::read_to_string(fixture("pages.jsonl")).expect("fixture pages");
        let pages: Vec<PageInput> = text
            .split('\n')
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).expect("fixture page parses"))
            .collect();
        assert_eq!(pages.len(), 4);
        let rig_text = std::fs::read_to_string(fixture("rig.json")).expect("fixture rig");
        let rig_list: Vec<String> = serde_json::from_str(&rig_text).expect("fixture rig parses");
        let rig: HashSet<String> = rig_list.into_iter().collect();
        let (kept, refused) = refuse_rig_documents(pages, &rig);
        assert_eq!(refused, 1);
        assert_eq!(kept.len(), 3);
        let rows: Vec<_> = kept.iter().flat_map(process_page).collect();
        assert_eq!(rows.len(), 2, "one host and one positive, nothing else");
        assert_eq!(rows.iter().filter(|r| r.label == "host").count(), 1);
        assert_eq!(rows.iter().filter(|r| r.label == "positive").count(), 1);
    }
}
