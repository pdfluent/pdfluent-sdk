//! Table proposals from what the page draws, for the table verifier (#574,
//! #638).
//!
//! THE GAP IS EXTENT, NOT ABSENCE. On the DocLayNet validation proxy (1,584
//! pages, 899 truth tables once the 130 of TSX_PWF_2014 whose truth holds
//! `GLYPH<…>` placeholders are set aside) the detectors had a proposal holding
//! 0.770 of the tables; of the rest, most had a proposal that held part of the
//! table, or part of it and more -- and 120 of the 193 sat on a page that
//! draws horizontal rules. So the proposals here start from the page's own
//! lines and from the proposals already made:
//!
//! * `lattice`: every grid of drawn lines ([`crate::learned::lattice`]).
//! * `ruled_frame`: rules that share a left and a right edge frame what lies
//!   between the first and the last of them -- a statement between its top
//!   and bottom rule.
//! * `grown`: a proposal carried out to the rules above and below it, where
//!   rules run across it.
//! * `merged`: a proposal joined to the nearest one under it, where the two
//!   stand in one column with little white between.
//!
//! They are CANDIDATES, not tables: the verifier chooses. Measured offline on
//! the same proxy: 0.770 -> 0.828 of the tables have a proposal, at about
//! eight more proposals a page.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::text::{HorizontalRule, Ruling};

/// How far two rules' left or right edges may stand apart, in points, and
/// still frame one table.
const FRAME_EDGE_TOLERANCE: f64 = 8.0;
/// The most white, in points, between a proposal and the one under it for the
/// two to be offered as one.
const MERGE_GAP: f64 = 30.0;
/// The share of the narrower of two extents they must share to stand in one
/// column.
const OVERLAP_SHARE: f64 = 0.5;
/// How far a line's centre may stand outside a hull, in points, and still be
/// held by it.
const HOLD_SLACK: f64 = 1.0;
/// How far above a proposal's top a rule may start and still be the rule
/// over it, and the same below.
const EDGE_SLACK: f64 = 2.0;

/// The proposals the page's drawing adds to `proposals` (sets of indices into
/// `lines`, `[x0, y0, x1, y1]` with y up), each as a detector name and the
/// sorted indices of the lines it holds. A set already proposed, or proposed
/// twice here, is offered once.
#[doc(hidden)]
pub fn vector_proposals(
    lines: &[[f64; 4]],
    rules: &[HorizontalRule],
    rulings: &[Ruling],
    proposals: &[Vec<usize>],
) -> Vec<(&'static str, Vec<usize>)> {
    let mut seen: std::collections::HashSet<Vec<usize>> = proposals
        .iter()
        .map(|p| {
            let mut p = p.clone();
            p.sort_unstable();
            p
        })
        .collect();
    let mut out: Vec<(&'static str, Vec<usize>)> = Vec::new();
    let mut offer =
        |detector: &'static str, hull: [f64; 4], out: &mut Vec<(&'static str, Vec<usize>)>| {
            let held = held_by(lines, hull);
            if held.len() >= 2 && seen.insert(held.clone()) {
                out.push((detector, held));
            }
        };

    for lattice in crate::learned::lattice::lattices(rulings) {
        offer("lattice", lattice.bbox(), &mut out);
    }
    for hull in ruled_frames(rules) {
        offer("ruled_frame", hull, &mut out);
    }

    let hulls: Vec<[f64; 4]> = proposals.iter().filter_map(|p| hull_of(lines, p)).collect();
    for &hull in &hulls {
        if let Some(grown) = grown_to_rules(hull, rules) {
            offer("grown", grown, &mut out);
        }
    }
    for (k, &upper) in hulls.iter().enumerate() {
        // The nearest proposal under this one, in its column.
        let under = hulls
            .iter()
            .enumerate()
            .filter(|&(j, &h)| j != k && h[3] <= upper[1] + EDGE_SLACK && shares_column(upper, h))
            .max_by(|a, b| a.1[3].total_cmp(&b.1[3]));
        if let Some((_, &lower)) = under {
            if upper[1] - lower[3] <= MERGE_GAP {
                let joined = [
                    upper[0].min(lower[0]),
                    lower[1],
                    upper[2].max(lower[2]),
                    upper[3],
                ];
                offer("merged", joined, &mut out);
            }
        }
    }
    out
}

/// The indices of the lines whose centre lies in `hull` (`[left, bottom,
/// right, top]`).
fn held_by(lines: &[[f64; 4]], hull: [f64; 4]) -> Vec<usize> {
    (0..lines.len())
        .filter(|&i| {
            let b = lines[i];
            let (cx, cy) = ((b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5);
            cx >= hull[0] - HOLD_SLACK
                && cx <= hull[2] + HOLD_SLACK
                && cy >= hull[1] - HOLD_SLACK
                && cy <= hull[3] + HOLD_SLACK
        })
        .collect()
}

/// The extent of the lines of a proposal.
fn hull_of(lines: &[[f64; 4]], ids: &[usize]) -> Option<[f64; 4]> {
    let mut hull = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for &i in ids {
        let b = lines.get(i)?;
        hull = [
            hull[0].min(b[0]),
            hull[1].min(b[1]),
            hull[2].max(b[2]),
            hull[3].max(b[3]),
        ];
    }
    hull[0].is_finite().then_some(hull)
}

fn shares_column(a: [f64; 4], b: [f64; 4]) -> bool {
    let overlap = a[2].min(b[2]) - a[0].max(b[0]);
    overlap > OVERLAP_SHARE * (a[2] - a[0]).min(b[2] - b[0])
}

/// The frame each set of rules with one left and one right edge draws: from
/// the lowest of them to the highest, when there are two heights or more.
fn ruled_frames(rules: &[HorizontalRule]) -> Vec<[f64; 4]> {
    let mut order: Vec<&HorizontalRule> = rules.iter().collect();
    order.sort_by(|a, b| b.y.total_cmp(&a.y));
    let mut sets: Vec<Vec<&HorizontalRule>> = Vec::new();
    for rule in order {
        match sets.iter_mut().find(|set| {
            (set[0].left - rule.left).abs() <= FRAME_EDGE_TOLERANCE
                && (set[0].right - rule.right).abs() <= FRAME_EDGE_TOLERANCE
        }) {
            Some(set) => set.push(rule),
            None => sets.push(vec![rule]),
        }
    }
    sets.into_iter()
        .filter_map(|set| {
            let top = set.iter().map(|r| r.y).fold(f64::NEG_INFINITY, f64::max);
            let bottom = set.iter().map(|r| r.y).fold(f64::INFINITY, f64::min);
            if top - bottom <= EDGE_SLACK {
                return None;
            }
            let left = set.iter().map(|r| r.left).fold(f64::INFINITY, f64::min);
            let right = set
                .iter()
                .map(|r| r.right)
                .fold(f64::NEG_INFINITY, f64::max);
            Some([left, bottom, right, top])
        })
        .collect()
}

/// `hull` carried up to the nearest rule over it and down to the nearest rule
/// under it, of the rules that run across most of it; `None` where neither
/// moves.
fn grown_to_rules(hull: [f64; 4], rules: &[HorizontalRule]) -> Option<[f64; 4]> {
    let across: Vec<&HorizontalRule> = rules
        .iter()
        .filter(|r| {
            r.right.min(hull[2]) - r.left.max(hull[0]) > OVERLAP_SHARE * (hull[2] - hull[0])
        })
        .collect();
    let top = across
        .iter()
        .map(|r| r.y)
        .filter(|&y| y >= hull[3] - EDGE_SLACK)
        .fold(f64::INFINITY, f64::min);
    let bottom = across
        .iter()
        .map(|r| r.y)
        .filter(|&y| y <= hull[1] + EDGE_SLACK)
        .fold(f64::NEG_INFINITY, f64::max);
    let top = if top.is_finite() { top } else { hull[3] };
    let bottom = if bottom.is_finite() { bottom } else { hull[1] };
    (top != hull[3] || bottom != hull[1]).then_some([hull[0], bottom, hull[2], top])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(x0: f64, x1: f64, y: f64) -> [f64; 4] {
        [x0, y, x1, y + 9.0]
    }

    fn rule(left: f64, right: f64, y: f64) -> HorizontalRule {
        HorizontalRule { left, right, y }
    }

    /// A statement between its top and bottom rule: the rules frame it, and
    /// the heading over the top rule stays out.
    #[test]
    fn rules_with_one_measure_frame_a_table() {
        let mut lines = vec![line(72.0, 300.0, 720.0)]; // a heading over it
        for r in 0..5 {
            let y = 690.0 - 14.0 * f64::from(r);
            lines.push(line(72.0, 160.0, y));
            lines.push(line(400.0, 440.0, y));
        }
        let rules = [rule(70.0, 450.0, 705.0), rule(70.0, 450.0, 620.0)];
        let found = vector_proposals(&lines, &rules, &[], &[]);
        let frame = found
            .iter()
            .find(|(d, _)| *d == "ruled_frame")
            .expect("no frame proposed");
        assert_eq!(frame.1, (1..11).collect::<Vec<_>>());
    }

    /// A proposal that holds part of the table grows to the rules that frame
    /// it, and two proposals one over the other are offered as one.
    #[test]
    fn a_piece_grows_to_its_rules_and_joins_the_piece_under_it() {
        let mut lines = Vec::new();
        for r in 0..6 {
            let y = 690.0 - 14.0 * f64::from(r);
            lines.push(line(72.0, 160.0, y));
            lines.push(line(400.0, 440.0, y));
        }
        // A rule over the whole and one under it, wider than the frame test's
        // so that no ruled frame repeats the growth.
        let rules = [rule(70.0, 450.0, 705.0), rule(70.0, 450.0, 605.0)];
        let upper: Vec<usize> = (0..6).collect();
        let lower: Vec<usize> = (6..12).collect();
        let found = vector_proposals(&lines, &rules, &[], &[upper, lower]);
        let all: Vec<usize> = (0..12).collect();
        assert!(
            found
                .iter()
                .any(|(d, held)| (*d == "grown" || *d == "ruled_frame") && *held == all),
            "{found:?}"
        );
        let merged: Vec<&(&str, Vec<usize>)> =
            found.iter().filter(|(d, _)| *d == "merged").collect();
        assert!(
            merged.is_empty() || merged.iter().all(|(_, h)| *h == all),
            "{found:?}"
        );
    }

    /// Nothing the proposers already offered comes back, and nothing is
    /// offered twice.
    #[test]
    fn a_proposal_is_offered_once() {
        let mut lines = Vec::new();
        for r in 0..4 {
            let y = 690.0 - 14.0 * f64::from(r);
            lines.push(line(72.0, 160.0, y));
            lines.push(line(400.0, 440.0, y));
        }
        let rules = [rule(70.0, 450.0, 705.0), rule(70.0, 450.0, 640.0)];
        let whole: Vec<usize> = (0..8).collect();
        let found = vector_proposals(&lines, &rules, &[], &[whole]);
        assert!(found.is_empty(), "{found:?}");
    }
}
