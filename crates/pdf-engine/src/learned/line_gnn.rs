// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The line graph network (#638): what each line of a page is, and whether it
//! belongs to the same region as the next one.
//!
//! The concept is a text-only layout network (#633): lines are nodes, each
//! line looks at its nearest neighbour above, below, left and right and at the
//! lines before and after it in reading order, and two heads read the result.
//! Trained in PyTorch (`scripts/extraction/learned/train_line_gnn.py`), run
//! here as plain matrix products over the constants `export_gnn_rust.py`
//! writes -- no runtime, so it runs wherever the engine does.
//!
//! EVERY INPUT IS ONE THE ENGINE ALREADY COMPUTES: the line's
//! `LINE_FEATURES`, its block's vector, where it stands against the page's
//! figures ([`super::captions::picture_features`]) and its box as fractions
//! of the page. The graph is built exactly as `train_line_gnn.page_graph`
//! builds it; the parity test holds the arithmetic to PyTorch's.

use super::captions::picture_features;
use super::BlockVectors;

/// One trained network: its shapes, its normalisation and its weights, all
/// constants `export_gnn_rust.py` wrote. Two ship (#574): [`V1`], the network
/// of #638, and [`V2`], retrained on the current engine's dump with what each
/// line is set in and with half the pages read as the loose pages the rig
/// sees; `Switches::line_gnn_v2` chooses.
#[doc(hidden)]
pub struct Net {
    /// The class names, in the order of the class head.
    pub classes: &'static [&'static str],
    /// The input groups past the base row, in the order the row carries them.
    pub groups: &'static [&'static str],
    pub hidden: usize,
    pub layers: usize,
    pub node_width: usize,
    pub edge_width: usize,
    node_mean: &'static [f32],
    node_spread: &'static [f32],
    edge_mean: &'static [f32],
    edge_spread: &'static [f32],
    encode: (&'static [f32], &'static [f32]),
    edges: [(&'static [f32], &'static [f32]); 6],
    rounds: [Round; 3],
    class_head: (&'static [f32], &'static [f32]),
    link_head: (&'static [f32], &'static [f32]),
}

/// A [`Net`] over one generated module.
macro_rules! net_of {
    ($w:path) => {{
        use $w as w;
        Net {
            classes: &w::CLASSES,
            groups: &w::GROUPS,
            hidden: w::HIDDEN,
            layers: w::LAYERS,
            node_width: w::NODE_WIDTH,
            edge_width: w::EDGE_WIDTH,
            node_mean: &w::NODE_MEAN,
            node_spread: &w::NODE_SPREAD,
            edge_mean: &w::EDGE_MEAN,
            edge_spread: &w::EDGE_SPREAD,
            encode: (&w::ENCODE_WEIGHT, &w::ENCODE_BIAS),
            edges: [
                (&w::EDGE_0_WEIGHT, &w::EDGE_0_BIAS),
                (&w::EDGE_1_WEIGHT, &w::EDGE_1_BIAS),
                (&w::EDGE_2_WEIGHT, &w::EDGE_2_BIAS),
                (&w::EDGE_3_WEIGHT, &w::EDGE_3_BIAS),
                (&w::EDGE_4_WEIGHT, &w::EDGE_4_BIAS),
                (&w::EDGE_5_WEIGHT, &w::EDGE_5_BIAS),
            ],
            rounds: [
                (
                    &w::OWN_0_WEIGHT,
                    &w::OWN_0_BIAS,
                    &w::MIX_0_WEIGHT,
                    &w::MIX_0_BIAS,
                ),
                (
                    &w::OWN_1_WEIGHT,
                    &w::OWN_1_BIAS,
                    &w::MIX_1_WEIGHT,
                    &w::MIX_1_BIAS,
                ),
                (
                    &w::OWN_2_WEIGHT,
                    &w::OWN_2_BIAS,
                    &w::MIX_2_WEIGHT,
                    &w::MIX_2_BIAS,
                ),
            ],
            class_head: (&w::CLASSES_WEIGHT, &w::CLASSES_BIAS),
            link_head: (&w::LINK_WEIGHT, &w::LINK_BIAS),
        }
    }};
}

/// The network of #638: the base row, five classes.
#[doc(hidden)]
pub static V1: Net = net_of!(super::models::line_gnn);

/// The network retrained on the current engine's dump (#574): the base row
/// and the line's style, seven classes, half its pages read as loose pages.
#[doc(hidden)]
pub static V2: Net = net_of!(super::models::line_gnn_v2);

impl Net {
    /// The position of the class `name`, if the network has it.
    pub fn class(&self, name: &str) -> Option<usize> {
        self.classes.iter().position(|c| *c == name)
    }
}

/// How many values a group adds to the row.
fn group_width(group: &str) -> usize {
    match group {
        "style" => 5,
        "vector" => 5,
        "judged" => 1,
        _ => 0,
    }
}

/// A missing value, as the training read it: `nan_to_num(nan=-1)`.
const MISSING: f32 = -1.0;

/// What the network says of one page's lines, in reading order.
#[derive(Debug, Clone, Default)]
#[doc(hidden)]
pub struct LineVerdicts {
    /// Class probabilities per line, in the network's [`Net::classes`] order.
    pub classes: Vec<Vec<f32>>,
    /// Per line, the probability that it belongs to the same region as the
    /// next line in reading order; the last line's is 0.
    pub link_next: Vec<f32>,
}

/// One line's place: its block, its box and its size.
struct Meta {
    block: usize,
    bbox: [f64; 4],
    size: f64,
}

/// The inputs of one page: node rows, neighbour indices per direction (`-1`
/// for none) and edge rows, before normalisation.
#[doc(hidden)]
pub struct PageGraph {
    pub nodes: Vec<Vec<f32>>,
    pub neighbours: Vec<[i64; 6]>,
    pub edges: Vec<Vec<f32>>,
}

/// The page's lines as `net`'s graph.
#[doc(hidden)]
pub fn page_graph(
    page: &[BlockVectors],
    size: (f64, f64),
    images: &[[f64; 4]],
    net: &Net,
) -> PageGraph {
    let (width, height) = (
        if size.0 > 0.0 { size.0 } else { 1.0 },
        if size.1 > 0.0 { size.1 } else { 1.0 },
    );
    let mut nodes = Vec::new();
    let mut meta = Vec::new();
    for (b, v) in page.iter().enumerate() {
        for (k, (line, &(bbox, line_size))) in v.lines.iter().zip(&v.line_boxes).enumerate() {
            let mut row: Vec<f64> = Vec::with_capacity(net.node_width);
            row.extend_from_slice(line);
            row.extend_from_slice(&v.block);
            row.extend_from_slice(&picture_features(bbox, line_size, images));
            row.extend_from_slice(&[
                bbox[0] / width,
                bbox[1] / height,
                bbox[2] / width,
                bbox[3] / height,
            ]);
            // THE GROUPS THE NETWORK WAS TRAINED ON, in its own order (#574):
            // a network trained without the style reads the row it learnt
            // from. A group the engine does not compute yet reads as missing.
            for group in net.groups {
                match *group {
                    "style" => {
                        row.extend_from_slice(&v.style.get(k).copied().unwrap_or([f64::NAN; 5]))
                    }
                    other => row.extend(std::iter::repeat_n(f64::NAN, group_width(other))),
                }
            }
            nodes.push(row.into_iter().map(as_input).collect());
            meta.push(Meta {
                block: b,
                bbox,
                size: line_size,
            });
        }
    }
    let neighbours = neighbours_of(&meta);
    let edges = neighbours
        .iter()
        .enumerate()
        .map(|(i, around)| {
            around
                .iter()
                .flat_map(|&j| edge_features(&meta, i, j))
                .map(as_input)
                .collect()
        })
        .collect();
    PageGraph {
        nodes,
        neighbours,
        edges,
    }
}

fn as_input(v: f64) -> f32 {
    if v.is_nan() {
        MISSING
    } else {
        v as f32
    }
}

/// Per line, the nearest line above, below, left and right (among lines that
/// overlap it across, or down), then the previous and next in reading order.
fn neighbours_of(meta: &[Meta]) -> Vec<[i64; 6]> {
    let n = meta.len();
    (0..n)
        .map(|i| {
            let [x0, y0, x1, y1] = meta[i].bbox;
            let mut best = [(f64::INFINITY, -1i64); 4];
            for (j, other) in meta.iter().enumerate() {
                if i == j {
                    continue;
                }
                let [a0, b0, a1, b1] = other.bbox;
                let across = x1.min(a1) - x0.max(a0) > 0.0;
                let down = y1.min(b1) - y0.max(b0) > 0.0;
                let j = j as i64;
                if across && b0 >= y1 && b0 - y1 < best[0].0 {
                    best[0] = (b0 - y1, j);
                }
                if across && y0 >= b1 && y0 - b1 < best[1].0 {
                    best[1] = (y0 - b1, j);
                }
                if down && x0 >= a1 && x0 - a1 < best[2].0 {
                    best[2] = (x0 - a1, j);
                }
                if down && a0 >= x1 && a0 - x1 < best[3].0 {
                    best[3] = (a0 - x1, j);
                }
            }
            let prev = if i > 0 { i as i64 - 1 } else { -1 };
            let next = if i + 1 < n { i as i64 + 1 } else { -1 };
            [best[0].1, best[1].1, best[2].1, best[3].1, prev, next]
        })
        .collect()
}

/// The gap in the source line's ems, the difference of the left edges in
/// ems, the size ratio, and one block or two.
fn edge_features(meta: &[Meta], i: usize, j: i64) -> [f64; 4] {
    if j < 0 {
        return [f64::NAN; 4];
    }
    let (a, b) = (&meta[i], &meta[j as usize]);
    let [x0, y0, x1, y1] = a.bbox;
    let [a0, b0, a1, b1] = b.bbox;
    let em = if a.size > 0.0 { a.size } else { 1.0 };
    let dx = (a0 - x1).max(x0 - a1).max(0.0);
    let dy = (b0 - y1).max(y0 - b1).max(0.0);
    let other = if b.size != 0.0 { b.size } else { 1.0 };
    [
        dx.hypot(dy) / em,
        (a0 - x0) / em,
        other / em,
        if a.block == b.block { 1.0 } else { 0.0 },
    ]
}

/// `out = W x + b`, `W` stored column by column (`export_gnn_rust.py`
/// transposes PyTorch's rows): `out` starts as the bias and takes one scaled
/// column per input, which vectorises where a row-by-row dot product does not.
fn linear_into(weight_t: &[f32], bias: &[f32], x: &[f32], out: &mut [f32]) {
    let outputs = bias.len();
    out.copy_from_slice(bias);
    for (column, &v) in weight_t.chunks_exact(outputs).zip(x) {
        if v == 0.0 {
            continue;
        }
        for (o, w) in out.iter_mut().zip(column) {
            *o += w * v;
        }
    }
}

fn linear(weight_t: &[f32], bias: &[f32], x: &[f32]) -> Vec<f32> {
    let mut out = vec![0.0; bias.len()];
    linear_into(weight_t, bias, x, &mut out);
    out
}

fn relu(mut v: Vec<f32>) -> Vec<f32> {
    for x in &mut v {
        *x = x.max(0.0);
    }
    v
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

fn softmax(v: &[f32]) -> Vec<f32> {
    let top = v.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let e: Vec<f32> = v.iter().map(|x| (x - top).exp()).collect();
    let sum: f32 = e.iter().sum();
    e.into_iter().map(|x| x / sum).collect()
}

/// One round of message passing: its own-state weight and bias, then its
/// mixing weight and bias.
type Round = (
    &'static [f32],
    &'static [f32],
    &'static [f32],
    &'static [f32],
);

/// The network's raw outputs for a graph: class logits per line and the
/// link logit per line (to the next). What the parity test compares.
#[doc(hidden)]
pub fn forward(graph: &PageGraph, net: &Net) -> (Vec<Vec<f32>>, Vec<f32>) {
    let n = graph.nodes.len();
    let hidden = net.hidden;
    let edge_weights = &net.edges;
    let layers = &net.rounds;
    let edge_width = net.edge_width;
    let norm_edges: Vec<Vec<f32>> = graph
        .edges
        .iter()
        .map(|e| {
            e.iter()
                .enumerate()
                .map(|(k, v)| (v - net.edge_mean[k]) / net.edge_spread[k])
                .collect()
        })
        .collect();
    let mut h: Vec<Vec<f32>> = graph
        .nodes
        .iter()
        .map(|x| {
            let x: Vec<f32> = x
                .iter()
                .enumerate()
                .map(|(k, v)| (v - net.node_mean[k]) / net.node_spread[k])
                .collect();
            relu(linear(net.encode.0, net.encode.1, &x))
        })
        .collect();
    let zero = vec![0.0f32; hidden];
    let edge_of = |edges: &[Vec<f32>], i: usize, d: usize| -> Vec<f32> {
        edges[i][d * edge_width..(d + 1) * edge_width].to_vec()
    };
    // ONE SET OF BUFFERS FOR THE WHOLE PAGE: a message, an input and the two
    // halves of a line's update are the same shape for every line.
    let mut input = vec![0.0f32; hidden + edge_width];
    let mut messages = vec![0.0f32; hidden * edge_weights.len()];
    let mut own = vec![0.0f32; hidden];
    let mut mixed = vec![0.0f32; hidden];
    let mut next = vec![vec![0.0f32; hidden]; n];
    for &(own_w, own_b, mix_w, mix_b) in layers.iter().take(net.layers) {
        for (i, out) in next.iter_mut().enumerate() {
            for (d, (ew, eb)) in edge_weights.iter().enumerate() {
                let j = graph.neighbours[i][d];
                input[..hidden].copy_from_slice(if j >= 0 { &h[j as usize] } else { &zero });
                input[hidden..]
                    .copy_from_slice(&norm_edges[i][d * edge_width..(d + 1) * edge_width]);
                let message = &mut messages[d * hidden..(d + 1) * hidden];
                linear_into(ew, eb, &input, message);
                for m in message.iter_mut() {
                    *m = m.max(0.0);
                }
            }
            linear_into(own_w, own_b, &h[i], &mut own);
            linear_into(mix_w, mix_b, &messages, &mut mixed);
            for ((o, a), b) in out.iter_mut().zip(&own).zip(&mixed) {
                *o = (a + b).max(0.0);
            }
        }
        std::mem::swap(&mut h, &mut next);
    }
    let classes = h
        .iter()
        .map(|x| linear(net.class_head.0, net.class_head.1, x))
        .collect();
    let next_dir = 5;
    let link = (0..n)
        .map(|i| {
            let j = graph.neighbours[i][next_dir];
            let mut input = h[i].clone();
            input.extend(if j >= 0 {
                h[j as usize].clone()
            } else {
                zero.clone()
            });
            input.extend(edge_of(&norm_edges, i, next_dir));
            linear(net.link_head.0, net.link_head.1, &input)[0]
        })
        .collect();
    (classes, link)
}

/// The normalised entropy of one line's class distribution: `-sum(p ln p) /
/// ln K`, 0 when one class holds everything, 1 when all K hold the same.
///
/// WHAT IT IS FOR. The per-page doubt of [`super::page_confidence`]: a line
/// whose classes are flat is one the network could not place, and a page of
/// them is a page to hand to the teacher. One class or none is 0 -- no rival,
/// no doubt -- and the clamp keeps float error inside 0..=1.
#[doc(hidden)]
pub fn normalised_entropy(probs: &[f32]) -> f64 {
    if probs.len() <= 1 {
        return 0.0;
    }
    let entropy: f64 = probs
        .iter()
        .map(|&p| {
            let p = f64::from(p);
            if p > 0.0 {
                -p * p.ln()
            } else {
                0.0
            }
        })
        .sum();
    (entropy / (probs.len() as f64).ln()).clamp(0.0, 1.0)
}

/// What `net` says of a page: class probabilities and link probabilities
/// per line, in reading order.
#[doc(hidden)]
pub fn verdicts(
    page: &[BlockVectors],
    size: (f64, f64),
    images: &[[f64; 4]],
    net: &Net,
) -> LineVerdicts {
    let graph = page_graph(page, size, images, net);
    if graph.nodes.is_empty() {
        return LineVerdicts::default();
    }
    let (classes, link) = forward(&graph, net);
    let n = link.len();
    LineVerdicts {
        classes: classes.iter().map(|c| softmax(c)).collect(),
        link_next: link
            .iter()
            .enumerate()
            .map(|(i, l)| if i + 1 < n { sigmoid(*l) } else { 0.0 })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(block: usize, bbox: [f64; 4]) -> Meta {
        Meta {
            block,
            bbox,
            size: 10.0,
        }
    }

    #[test]
    fn a_line_finds_its_neighbours_in_the_plane_and_in_reading_order() {
        // Two lines one above the other, and one to the right of the first.
        let lines = [
            meta(0, [72.0, 700.0, 200.0, 710.0]),
            meta(0, [72.0, 686.0, 200.0, 696.0]),
            meta(1, [300.0, 700.0, 400.0, 710.0]),
        ];
        let n = neighbours_of(&lines);
        assert_eq!(n[0], [-1, 1, -1, 2, -1, 1], "first line");
        assert_eq!(n[1], [0, -1, -1, -1, 0, 2], "second line");
        assert_eq!(n[2][2], 0, "the third line has the first to its left");
    }
}
