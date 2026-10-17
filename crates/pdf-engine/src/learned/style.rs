// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! What a line is set in, against what the page's body is set in (#574).
//!
//! `scripts/extraction/learned/style.py`, ported: the same five values in the
//! same order, so the heading model reads in the engine what it was trained
//! on. A heading on a financial report is often the body's size and not bold
//! by the font's own flag -- `Note 3: Employee Benefits` in
//! DIN-MediumAlternate over a DIN-Regular body -- so these read the line
//! against the page's own body rather than against an absolute bold.

use super::features::PageWord;

/// The names of [`page_style`]'s values, in order.
pub const STYLE_FEATURES: [&str; 5] = [
    "weight_vs_body",
    "colour_vs_body",
    "face_vs_body",
    "family_vs_body",
    "style_share",
];

/// First match wins, so the compound names come before the words inside them.
const WEIGHTS: [(&str, i32); 18] = [
    ("extralight", 200),
    ("ultralight", 200),
    ("semibold", 600),
    ("demibold", 600),
    ("extrabold", 800),
    ("ultrabold", 800),
    ("hairline", 100),
    ("thin", 100),
    ("light", 300),
    ("medium", 500),
    ("demi", 600),
    ("heavy", 800),
    ("black", 900),
    ("bold", 700),
    ("book", 400),
    ("regular", 400),
    ("roman", 400),
    ("normal", 400),
];

/// The face's name without a subset prefix (`ABCDEF+`).
fn face(word: &PageWord) -> &str {
    let name = word.font.as_deref().unwrap_or("");
    let bytes = name.as_bytes();
    if bytes.len() >= 7 && bytes[6] == b'+' && bytes[..6].iter().all(u8::is_ascii_uppercase) {
        &name[7..]
    } else {
        name
    }
}

/// The family: the name before its first `-` or `,`.
fn family(name: &str) -> &str {
    name.split(['-', ',']).next().unwrap_or("")
}

/// The face's weight from its name, or the bold flag where the name says
/// nothing.
fn weight(word: &PageWord) -> i32 {
    let name = face(word).to_lowercase();
    WEIGHTS
        .iter()
        .find(|(token, _)| name.contains(token))
        .map_or(if word.bold { 700 } else { 400 }, |&(_, w)| w)
}

/// A size as the Python reads it after `round(size, 1)`.
fn tenths(size: f64) -> i64 {
    (size * 10.0).round() as i64
}

/// A count per key in the order the keys first came: the Python `Counter`,
/// whose ties go to the first key inserted.
struct Tally<K: PartialEq> {
    keys: Vec<(K, usize)>,
}

impl<K: PartialEq> Tally<K> {
    fn new() -> Self {
        Self { keys: Vec::new() }
    }

    fn add(&mut self, key: K, n: usize) {
        match self.keys.iter_mut().find(|(k, _)| *k == key) {
            Some((_, count)) => *count += n,
            None => self.keys.push((key, n)),
        }
    }

    fn get(&self, key: &K) -> usize {
        self.keys
            .iter()
            .find(|(k, _)| k == key)
            .map_or(0, |(_, n)| *n)
    }

    /// The first key with the largest count.
    fn most(&self) -> Option<&K> {
        let best = self.keys.iter().map(|(_, n)| *n).max()?;
        self.keys.iter().find(|(_, n)| *n == best).map(|(k, _)| k)
    }
}

fn chars(word: &PageWord) -> usize {
    word.text.chars().count()
}

/// One row of [`STYLE_FEATURES`] per line of the page, block by block
/// (`0..n_blocks`), each block's lines in [`super::lines::lines_of`] order.
#[doc(hidden)]
pub fn page_style(n_blocks: usize, words: &[PageWord]) -> Vec<Vec<[f64; 5]>> {
    page_style_on_grouped(&super::lines::grouped_lines(words, n_blocks), words)
}

/// [`page_style`] over lines grouped once: the same rows, without
/// re-grouping the page for them.
#[doc(hidden)]
pub fn page_style_on_grouped(
    grouped: &[Vec<Vec<&PageWord>>],
    words: &[PageWord],
) -> Vec<Vec<[f64; 5]>> {
    type Style<'a> = (&'a str, Option<[u8; 3]>, i64);
    let mut body: Tally<(&str, Option<[u8; 3]>)> = Tally::new();
    let mut styles: Tally<Style> = Tally::new();
    for w in words {
        body.add((face(w), w.color), chars(w));
        styles.add((face(w), w.color, tenths(w.size)), chars(w));
    }
    let total: usize = body.keys.iter().map(|(_, n)| n).sum();
    let (body_face, body_colour) = body
        .most()
        .copied()
        .filter(|_| total > 0)
        .unwrap_or(("", None));
    let body_weight = words
        .iter()
        .find(|w| face(w) == body_face)
        .map_or(400, weight);
    let mut out = Vec::with_capacity(grouped.len());
    for lines in grouped {
        let mut rows = Vec::new();
        for line in lines {
            let n = line.iter().map(|w| chars(w)).sum::<usize>().max(1) as f64;
            let share = |pred: &dyn Fn(&PageWord) -> bool| {
                line.iter()
                    .filter(|w| pred(w))
                    .map(|w| chars(w))
                    .sum::<usize>() as f64
                    / n
            };
            let mean_weight = line
                .iter()
                .map(|w| f64::from(weight(w)) * chars(w) as f64)
                .sum::<f64>()
                / n;
            let mut own: Tally<Style> = Tally::new();
            for w in line {
                own.add((face(w), w.color, tenths(w.size)), chars(w));
            }
            let rarity = match own.most() {
                Some(mine) if total > 0 => styles.get(mine) as f64 / total as f64,
                _ => 0.0,
            };
            rows.push([
                (mean_weight - f64::from(body_weight)) / 100.0,
                share(&|w| w.color != body_colour),
                share(&|w| face(w) != body_face),
                share(&|w| family(face(w)) != family(body_face)),
                rarity,
            ]);
        }
        out.push(rows);
    }
    out
}
