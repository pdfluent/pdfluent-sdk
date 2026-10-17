// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The decoder of #600, ported from the Python it was measured in.
//!
//! The cases here are the ones that caught the Python out, which is why they
//! are the ones a port has to reproduce: a spanning cell acting as a bridge, a
//! band two rows tall that no cell sits in alone, and a cell holding a
//! sentence. A port tested only on a plain grid passes while being wrong.

use pdf_engine::learned::grid::{decode, fill_empty, Cell};

/// One cell as a test writes it: the text, the rows and columns it occupies,
/// and where it sits on the page.
type Spec<'a> = (&'a str, &'a [usize], &'a [usize], [f64; 4]);

/// A table written out as cells, turned into words and true relations.
struct Table {
    boxes: Vec<[f64; 4]>,
    owner: Vec<usize>,
    rows: Vec<Vec<usize>>,
    columns: Vec<Vec<usize>>,
}

impl Table {
    fn of(spec: &[Spec<'_>]) -> Self {
        let mut boxes = Vec::new();
        let mut owner = Vec::new();
        for (index, (text, _r, _c, area)) in spec.iter().enumerate() {
            let parts: Vec<&str> = text.split_whitespace().collect();
            let width = (area[2] - area[0]) / parts.len().max(1) as f64;
            for (n, _part) in parts.iter().enumerate() {
                boxes.push([
                    area[0] + n as f64 * width,
                    area[1],
                    area[0] + (n + 1) as f64 * width,
                    area[3],
                ]);
                owner.push(index);
            }
        }
        Table {
            boxes,
            owner,
            rows: spec.iter().map(|(_t, r, _c, _b)| r.to_vec()).collect(),
            columns: spec.iter().map(|(_t, _r, c, _b)| c.to_vec()).collect(),
        }
    }

    fn decode_with(&self, span: impl Fn(usize) -> usize + Copy) -> Vec<Cell> {
        decode(
            self.boxes.len(),
            &self.boxes,
            &|i, j| self.owner[i] == self.owner[j],
            &|i, j| {
                self.rows[self.owner[i]]
                    .iter()
                    .any(|r| self.rows[self.owner[j]].contains(r))
            },
            &|i, j| {
                self.columns[self.owner[i]]
                    .iter()
                    .any(|c| self.columns[self.owner[j]].contains(c))
            },
            &span,
            &|_| 1,
        )
    }
}

#[test]
fn a_plain_grid_keeps_its_cells_and_its_order() {
    let table = Table::of(&[
        ("Region", &[0], &[0], [10.0, 90.0, 60.0, 100.0]),
        ("Revenue", &[0], &[1], [70.0, 90.0, 130.0, 100.0]),
        ("North", &[1], &[0], [10.0, 70.0, 60.0, 80.0]),
        ("12,345", &[1], &[1], [70.0, 70.0, 130.0, 80.0]),
    ]);
    let out = table.decode_with(|_| 1);
    assert_eq!(out.len(), 4, "four cells in, four out");
    let places: Vec<(Vec<usize>, Vec<usize>)> = out
        .iter()
        .map(|c| (c.rows.clone(), c.columns.clone()))
        .collect();
    assert_eq!(
        places,
        vec![
            (vec![0], vec![0]),
            (vec![0], vec![1]),
            (vec![1], vec![0]),
            (vec![1], vec![1])
        ],
        "the header is the top row because y is up here"
    );
}

#[test]
fn a_spanning_cell_does_not_bridge_the_rows_it_spans() {
    // The case components cannot do: `Total` shares a row with the cell in row
    // 0 AND with the one in row 1, which share none with each other. Joining a
    // line on ANY shared row pulls the whole table into one.
    let table = Table::of(&[
        ("Total", &[0, 1], &[0], [10.0, 70.0, 60.0, 100.0]),
        ("First", &[0], &[1], [70.0, 90.0, 130.0, 100.0]),
        ("Second", &[1], &[1], [70.0, 70.0, 130.0, 80.0]),
    ]);
    let out = table.decode_with(|_| 1);
    let rows_of = |text_words: usize| -> Vec<usize> {
        out.iter()
            .find(|c| c.words.len() == text_words)
            .map(|c| c.rows.clone())
            .unwrap_or_default()
    };
    assert_eq!(out.len(), 3, "three cells in, three out");
    let all: Vec<Vec<usize>> = out.iter().map(|c| c.rows.clone()).collect();
    assert!(
        all.contains(&vec![0]) && all.contains(&vec![1]),
        "the two single-row cells must stay in different rows, got {all:?}"
    );
    let _ = rows_of(1);
}

#[test]
fn a_band_two_rows_tall_needs_the_count_and_nothing_else_finds_it() {
    // Both header cells span rows 0 and 1; no cell occupies either alone, so
    // every pair of their words is "same row" and the division is invisible to
    // any question about a pair. This holds BOTH halves: without the count the
    // band collapses, with it the band is two rows.
    let spec: &[Spec<'_>] = &[
        ("Left", &[0, 1], &[0], [10.0, 80.0, 60.0, 100.0]),
        ("Right", &[0, 1], &[1], [70.0, 80.0, 130.0, 100.0]),
        ("Below", &[2], &[0], [10.0, 60.0, 60.0, 70.0]),
        ("Also", &[2], &[1], [70.0, 60.0, 130.0, 70.0]),
    ];
    let table = Table::of(spec);
    let without: Vec<Vec<usize>> = table
        .decode_with(|_| 1)
        .iter()
        .map(|c| c.rows.clone())
        .collect();
    assert_eq!(
        without,
        vec![vec![0], vec![0], vec![1], vec![1]],
        "without the count the band collapses -- this is the 93.9 % ceiling, \
         held here so it cannot move unnoticed"
    );
    let spans: Vec<usize> = table.owner.iter().map(|&o| spec[o].1.len()).collect();
    let with: Vec<Vec<usize>> = table
        .decode_with(|w| spans[w])
        .iter()
        .map(|c| c.rows.clone())
        .collect();
    assert_eq!(
        with,
        vec![vec![0, 1], vec![0, 1], vec![2], vec![2]],
        "the count says how far a cell reaches and the sweep says where it \
         starts; neither alone recovers this band"
    );
}

#[test]
fn a_cell_of_a_sentence_stays_one_cell() {
    let table = Table::of(&[
        ("Method", &[0], &[0], [10.0, 90.0, 60.0, 100.0]),
        (
            "Phenol and SDS in turn",
            &[0],
            &[1],
            [70.0, 90.0, 200.0, 100.0],
        ),
    ]);
    let out = table.decode_with(|_| 1);
    assert_eq!(
        out.len(),
        2,
        "the words of a prose cell are same-cell with one another, and a table \
         whose cells hold sentences is the class this ticket nearly lost"
    );
    assert!(
        out.iter().any(|c| c.words.len() == 5),
        "the sentence lost words: {:?}",
        out.iter().map(|c| c.words.len()).collect::<Vec<_>>()
    );
}

#[test]
fn an_uncovered_position_becomes_an_empty_cell() {
    let cells = vec![
        Cell {
            words: vec![0],
            bbox: [10.0, 90.0, 60.0, 100.0],
            rows: vec![0],
            columns: vec![0],
        },
        Cell {
            words: vec![1],
            bbox: [70.0, 90.0, 130.0, 100.0],
            rows: vec![0],
            columns: vec![1],
        },
        Cell {
            words: vec![2],
            bbox: [10.0, 70.0, 60.0, 80.0],
            rows: vec![1],
            columns: vec![0],
        },
    ];
    let filled = fill_empty(cells);
    assert_eq!(filled.len(), 4, "the hole at (1, 1) was not filled");
    let empty: Vec<&Cell> = filled.iter().filter(|c| c.words.is_empty()).collect();
    assert_eq!(empty.len(), 1);
    assert_eq!(
        (empty[0].rows.clone(), empty[0].columns.clone()),
        (vec![1], vec![1])
    );
}
