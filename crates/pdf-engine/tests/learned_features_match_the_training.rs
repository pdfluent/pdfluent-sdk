// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The engine computes the vectors the models were trained on (#599).
//!
//! The models learned from Python's vectors (`features.py`, `list_lines.py`
//! over the dump) and run on the engine's (`pdf_engine::learned`). The fixture
//! holds what Python made of six PDFs; the engine has to make the same, value
//! for value and NaN for NaN. A NaN is a neighbour a block or line does not
//! have, and it is the first place a port goes wrong: the first and last block
//! of every page carry them, and those are where headings and list openings
//! sit. Regenerate the fixture with `scripts/extraction/learned/
//! make_feature_parity.py` when the Python side changes.

use pdf_engine::learned::document_vectors;
use pdf_engine::PdfDocument;

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn same(want: &serde_json::Value, got: f64) -> bool {
    match want.as_f64() {
        None => got.is_nan(),
        Some(w) => !got.is_nan() && (w - got).abs() <= 1e-9 * w.abs().max(1.0),
    }
}

#[test]
fn the_engine_makes_the_vectors_the_models_learned_from() {
    let path = format!("{REPO}/crates/pdf-engine/tests/fixtures/learned_feature_parity.json");
    let fixture: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("fixture")).expect("json");
    let mut compared = (0usize, 0usize, 0usize);
    for document in fixture["documents"].as_array().expect("documents") {
        let pdf = document["pdf"].as_str().expect("pdf");
        let doc =
            PdfDocument::open(std::fs::read(format!("{REPO}/{pdf}")).expect("pdf")).expect("open");
        let blocks = doc.extract_blocks_before_learning().expect("blocks");
        let sizes: Vec<(f64, f64)> = (0..doc.page_count())
            .map(|page| {
                doc.page_geometry(page)
                    .map(|g| g.effective_dimensions())
                    .unwrap_or((0.0, 0.0))
            })
            .collect();
        // The engine's own path from blocks to model input, the one `refine`
        // reads -- not a second assembly of it here.
        let vectors = document_vectors(&blocks, &sizes);
        let want_pages = document["pages"].as_array().expect("pages");
        assert_eq!(want_pages.len(), doc.page_count(), "{pdf}: page count");
        for (p, want) in want_pages.iter().enumerate() {
            let on_page: Vec<_> = vectors.iter().filter(|v| v.page == p).collect();
            let want_blocks = want["blocks"].as_array().expect("blocks");
            let want_lines = want["lines"].as_array().expect("lines");
            assert_eq!(want_blocks.len(), on_page.len(), "{pdf} p{p}: block count");
            for (i, v) in on_page.iter().enumerate() {
                let want_vector = want_blocks[i].as_array().expect("vector");
                // EVERY VALUE, AND NOT AS MANY AS THE FIXTURE HAPPENS TO HAVE.
                // `zip` stops at the shorter of the two, so a fixture written
                // before a feature was added would pass while saying nothing
                // about it -- which is exactly what happened when #599 took
                // the vector from 24 to 30.
                assert_eq!(
                    want_vector.len(),
                    v.block.len(),
                    "{pdf} p{p} block {i}: the fixture has {} values and the \
                     engine makes {}; regenerate it with \
                     scripts/extraction/learned/make_feature_parity.py",
                    want_vector.len(),
                    v.block.len()
                );
                for (k, (w, g)) in want_vector.iter().zip(v.block).enumerate() {
                    assert!(
                        same(w, g),
                        "{pdf} p{p} block {i}: {} is {g} here and {w} in training",
                        pdf_engine::learned::features::BLOCK_FEATURES[k]
                    );
                }
                compared.0 += 1;
                let want_l = want_lines[i].as_array().expect("lines of a block");
                assert_eq!(
                    want_l.len(),
                    v.lines.len(),
                    "{pdf} p{p} block {i}: line count"
                );
                for (l, (w, g)) in want_l.iter().zip(v.lines.iter()).enumerate() {
                    // The width too, as for the block: `zip` would stop at
                    // the shorter of the two (#574, parity audit).
                    assert_eq!(
                        w.as_array().expect("line").len(),
                        g.len(),
                        "{pdf} p{p} block {i} line {l}: line vector width; regenerate \
                         with scripts/extraction/learned/make_feature_parity.py"
                    );
                    for (k, (wv, gv)) in
                        w.as_array().expect("line").iter().zip(g.iter()).enumerate()
                    {
                        compared.2 += usize::from(gv.is_nan());
                        assert!(
                            same(wv, *gv),
                            "{pdf} p{p} block {i} line {l}: {} is {gv} here and {wv} in training",
                            pdf_engine::learned::lines::LINE_FEATURES[k]
                        );
                    }
                    compared.1 += 1;
                }
            }
        }
    }
    assert!(
        compared.0 > 20 && compared.1 > 40,
        "too little compared: {compared:?}"
    );
    assert!(
        compared.2 > 0,
        "no missing value was compared, so NaN routing is untested"
    );
}

/// The largest difference between the two sides, over values both have.
fn gap(want: &serde_json::Value, got: f64, worst: &mut f64) -> bool {
    if let Some(w) = want.as_f64() {
        if !got.is_nan() {
            *worst = worst.max((w - got).abs());
        }
    }
    same(want, got)
}

/// Every value of a row, and exactly as many as training had.
fn row_matches(want: &serde_json::Value, got: &[f64], what: &str, worst: &mut f64) -> usize {
    let want = want.as_array().expect("row");
    assert_eq!(
        want.len(),
        got.len(),
        "{what}: width; regenerate the fixture"
    );
    let mut nans = 0;
    for (k, (w, g)) in want.iter().zip(got).enumerate() {
        nans += usize::from(g.is_nan());
        assert!(
            gap(w, *g, worst),
            "{what}: value {k} is {g} here and {w} in training"
        );
    }
    nans
}

/// The line models read rows ASSEMBLED from the vectors the test above checks:
/// a line with its block and its neighbours for `heading_lines` and
/// `line_classes`, four picture values more for `caption_lines`, a graph of
/// the page's lines for `line_gnn`. None of that assembly was compared with the
/// Python the models were trained on (#574, parity audit): the neighbours, the
/// same-block flags, a missing neighbour's NaN, the nearest line in each
/// direction and the edge between them.
#[test]
fn the_line_models_read_the_rows_they_learned_from() {
    use pdf_engine::learned::{captions, heading_lines, line_gnn};
    let path = format!("{REPO}/crates/pdf-engine/tests/fixtures/learned_feature_parity.json");
    let fixture: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("fixture")).expect("json");
    // rows compared, NaN compared, pages with a picture, edges with a neighbour
    let mut n = [0usize; 4];
    let mut worst = [0f64; 3];
    for document in fixture["documents"].as_array().expect("documents") {
        let pdf = document["pdf"].as_str().expect("pdf");
        let doc =
            PdfDocument::open(std::fs::read(format!("{REPO}/{pdf}")).expect("pdf")).expect("open");
        let blocks = doc.extract_blocks_before_learning().expect("blocks");
        let sizes: Vec<(f64, f64)> = (0..doc.page_count())
            .map(|page| {
                doc.page_geometry(page)
                    .map(|g| g.effective_dimensions())
                    .unwrap_or((0.0, 0.0))
            })
            .collect();
        let images = doc.extract_image_boxes().expect("images");
        let vectors = document_vectors(&blocks, &sizes);
        for (p, want) in document["pages"]
            .as_array()
            .expect("pages")
            .iter()
            .enumerate()
        {
            let on_page: Vec<_> = vectors.iter().filter(|v| v.page == p).cloned().collect();
            let pictures = images.get(p).map(Vec::as_slice).unwrap_or(&[]);
            n[2] += usize::from(!pictures.is_empty());

            // The fixture keeps what the assembly adds -- the neighbours'
            // values and the same-block flags -- and the line and its block,
            // which the test above holds to training, are put in front of it.
            let rows: Vec<_> = heading_lines::page_rows(&on_page)
                .into_iter()
                .flatten()
                .collect();
            let owners: Vec<(&[f64; 23], &[f64; 30])> = on_page
                .iter()
                .flat_map(|v| v.lines.iter().map(move |line| (line, &v.block)))
                .collect();
            let want_rows = want["heading_context"].as_array().expect("heading_context");
            assert_eq!(want_rows.len(), rows.len(), "{pdf} p{p}: line count");
            assert_eq!(owners.len(), rows.len(), "{pdf} p{p}: lines of the blocks");
            for (l, (w, g)) in want_rows.iter().zip(&rows).enumerate() {
                let (line, block) = owners[l];
                assert!(
                    g[..23]
                        .iter()
                        .zip(line)
                        .chain(g[23..53].iter().zip(block))
                        .all(|(a, b)| a == b || (a.is_nan() && b.is_nan())),
                    "{pdf} p{p} heading row {l}: does not start with its line and its block"
                );
                n[1] += row_matches(
                    w,
                    &g[53..],
                    &format!("{pdf} p{p} heading row {l}"),
                    &mut worst[0],
                );
                n[0] += 1;
            }

            let caption: Vec<_> = captions::page_caption_rows(&on_page, pictures)
                .into_iter()
                .flatten()
                .collect();
            let want_pictures = want["picture"].as_array().expect("picture");
            assert_eq!(
                want_pictures.len(),
                caption.len(),
                "{pdf} p{p}: caption lines"
            );
            for (l, (w, g)) in want_pictures.iter().zip(&caption).enumerate() {
                assert!(
                    g[..heading_lines::WIDTH]
                        .iter()
                        .zip(&rows[l])
                        .all(|(a, b)| a == b || (a.is_nan() && b.is_nan())),
                    "{pdf} p{p} caption line {l}: the caption row does not start with the heading row"
                );
                row_matches(
                    w,
                    &g[heading_lines::WIDTH..],
                    &format!("{pdf} p{p} caption line {l}"),
                    &mut worst[1],
                );
            }

            // The base row: the fixture is the network of #638's, which reads no
            // group past it (`V1`).
            let graph = line_gnn::page_graph(&on_page, sizes[p], pictures, &line_gnn::V1);
            let want_gnn = &want["gnn"];
            let want_nodes = want_gnn["boxes"].as_array().expect("boxes");
            assert_eq!(
                want_nodes.len(),
                graph.nodes.len(),
                "{pdf} p{p}: graph nodes"
            );
            // The network reads f32, with a missing value as -1: training's
            // rows go through `np.nan_to_num(np.float32(..), nan=-1.0)`.
            let as_f32 = |v: &[f32]| v.iter().map(|&x| f64::from(x)).collect::<Vec<_>>();
            // A node is its line, its block, its picture values and its box;
            // the first three are held above, so the expected node is put
            // together from them and the fixture's box.
            for (i, (w, g)) in want_nodes.iter().zip(&graph.nodes).enumerate() {
                let (line, block) = owners[i];
                let mut expected: Vec<serde_json::Value> = line
                    .iter()
                    .chain(block.iter())
                    .chain(caption[i][heading_lines::WIDTH..].iter())
                    .map(|&x| {
                        if x.is_nan() {
                            serde_json::Value::Null
                        } else {
                            x.into()
                        }
                    })
                    .collect();
                expected.extend(w.as_array().expect("box").iter().cloned());
                let rounded: Vec<serde_json::Value> = expected
                    .iter()
                    .map(|x| f64::from(x.as_f64().map_or(-1.0, |x| x as f32)).into())
                    .collect();
                row_matches(
                    &serde_json::Value::Array(rounded),
                    &as_f32(g),
                    &format!("{pdf} p{p} node {i}"),
                    &mut worst[2],
                );
            }
            let want_nb = want_gnn["neighbours"].as_array().expect("neighbours");
            // Every row, not as many as the shorter side has (codex, #2063).
            assert_eq!(
                want_nb.len(),
                graph.neighbours.len(),
                "{pdf} p{p}: neighbour rows"
            );
            for (i, (w, g)) in want_nb.iter().zip(&graph.neighbours).enumerate() {
                let w: Vec<i64> = w
                    .as_array()
                    .expect("row")
                    .iter()
                    .map(|x| x.as_i64().expect("index"))
                    .collect();
                assert_eq!(w, g.to_vec(), "{pdf} p{p} line {i}: neighbours");
                n[3] += g.iter().filter(|&&j| j >= 0).count();
            }
            let want_edges = want_gnn["edges"].as_array().expect("edges");
            assert_eq!(want_edges.len(), graph.edges.len(), "{pdf} p{p}: edge rows");
            for (i, (w, g)) in want_edges.iter().zip(&graph.edges).enumerate() {
                let rounded: Vec<serde_json::Value> = w
                    .as_array()
                    .expect("edge")
                    .iter()
                    .map(|x| f64::from(x.as_f64().map_or(-1.0, |x| x as f32)).into())
                    .collect();
                row_matches(
                    &serde_json::Value::Array(rounded),
                    &as_f32(g),
                    &format!("{pdf} p{p} edges of line {i}"),
                    &mut worst[2],
                );
            }
        }
    }
    eprintln!(
        "compared: {} rows, {} NaN, {} pages with a picture, {} edges; largest difference: \
         heading rows {:e}, picture values {:e}, graph {:e}",
        n[0], n[1], n[2], n[3], worst[0], worst[1], worst[2]
    );
    assert!(n[0] > 200, "too few rows compared: {n:?}");
    assert!(n[1] > 0, "no missing neighbour was compared");
    assert!(
        n[2] > 0,
        "no page with a picture, so the caption values are untested"
    );
    assert!(n[3] > 200, "too few graph edges compared: {n:?}");
}
