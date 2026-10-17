//! Learned decisions over the finished blocks (#598, #599).
//!
//! WHERE AND WHY HERE. At the very end of [`crate::PdfDocument::extract_blocks`],
//! after furniture, headings and reading order. The models were trained on the
//! records `feature_dump` writes, and those are made from exactly these blocks:
//! a model run anywhere earlier would read input it never saw in training. It
//! is also the one place that sees the whole document, which a heading model
//! needs (`repeats_in_document`, #583) -- so every lane's model is asked from
//! here, and there is one call site to read (T3, T5).
//!
//! WHAT A MODEL MAY DO. Two actions, and nothing else:
//!
//! * propose a BOUNDARY: "a new block opens at line N of this block". The block
//!   is split there. Generic on purpose: a list item swallowed by the line
//!   above and a heading swallowed by the paragraph under it are one defect
//!   (#602), and the lane that has a model for it switches it on.
//! * propose a CLASS for a block. The class it may set or take away is the
//!   model's own and no other.
//!
//! Each is switched per class in [`SWITCHES`], and each switch has an operator
//! test that is red with it off.
//!
//! ONE PAGE AT A TIME. A page is decided from its own blocks and the
//! document's facts ([`DocumentFacts`]: on how many pages a text occurs, the
//! one feature that is about the whole document), gathered once before any
//! page and handed over behind a shared reference. [`refine_page`] reads
//! nothing else and keeps nothing between calls, so pages can run in parallel
//! and come out as they do one by one -- which a test holds it to.
//!
//! ONE SNAPSHOT. Every decision is computed from the blocks as they came in,
//! and only then applied. Applied block by block, block i+1 would read block
//! i's new class and the outcome would depend on the order of the loop -- and
//! the parity with the training records, which were made without any of this,
//! would be gone (T3). A test runs the decisions in reverse and requires the
//! same result.
//!
//! THE ORDER AFTER A SPLIT. The parts of a block take consecutive places where
//! the block stood, and every later block moves down; [`Block::index`] is then
//! counted again. Reading order was computed before this pass and is the one
//! axis we stand above the reference on, so it is pinned by a test.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

pub mod alignment;
pub mod captions;
pub mod cell_merge;
pub mod drawings;
pub mod features;
pub mod grid;
pub mod heading_lines;
pub mod judge;
pub mod lattice;
pub mod line_gnn;
pub mod lines;
#[doc(hidden)]
pub mod models;
pub mod pairs;
pub mod raster;
pub mod region;
pub mod separations;
pub mod splits;
pub mod stitch;
pub mod style;
/// Split one emitted table back into the tables glued into it (#574).
pub mod table_split;
/// The learned table pass. Public because its hybrid gate (`tables::Weakness`,
/// `tables::may_overrule`) is what decides when the model may overrule the
/// heuristic builder, and that decision is tested from outside (#600).
pub mod tables;
/// TATR table structure behind `ort-tables`: the ONNX table model over the
/// shared runtime layer, returning `rebuild_region`'s own cell type.
#[cfg(feature = "ort-tables")]
pub mod tables_tatr;
pub mod vector_proposals;
/// Which of the judge's candidates is the table (#638).
pub mod verifier;

use crate::blocks::{Block, BlockKind};
#[cfg(test)]
use crate::table_trace::TableOrigin;
pub use features::DocumentFacts;
use features::{block_vector, page_features};
use lines::{lines_of, LineContext};

/// Which learned decision is switched on. One row per class a model decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Switches {
    /// The list-item block model (`list_item`) may make a paragraph an item
    /// and take a whole confirmed list back out.
    pub list_item: bool,
    /// The region model (`table_region`) decides which blocks are inside a
    /// table, instead of the heuristics' own `table_cell` verdicts. Separate
    /// from `table_structure` because the two are worth different amounts and
    /// fail differently: the region is 0.6093 to 0.6407 of TEDS on its own
    /// (#600), and a round that disagrees with one should be able to turn
    /// that one off.
    pub table_region: bool,
    /// The pair model (`table_structure`) rebuilds a table's grid from its
    /// words. With `table_region` off it works on the regions the heuristics
    /// found, which is how it is measured on its own.
    pub table_structure: bool,
    /// The table judge (`table_judge`) chooses which of the proposers' regions
    /// are the page's tables: the builder's, T2's four detectors, #632's
    /// alignment proposer and their unions (#574). With it on, its regions
    /// replace the region model's and the builder's runs in the table pass.
    pub table_judge: bool,
    /// Tables the page draws as a grid of lines become tables with the cells
    /// the lines make (`lattice`, #574). No model: where the page draws the
    /// grid, the cells are where its lines cross.
    pub table_lattice: bool,
    /// The line model (`list_opens`) may open a list item inside a block: the
    /// boundary action, for the list class. Its operator page is a real one
    /// (`tests/fixtures/doclaynet/`), because no constructed page makes it
    /// fire -- the layouts it repairs are the ones the heuristics' own tests
    /// were never built from.
    pub list_opens: bool,
    /// The heading LINE model (`heading_lines`) decides, line by line with
    /// the lines around it on the page as context, which lines are a heading;
    /// a block whose lines disagree is cut where they change (#574, #632).
    /// It answers the class and the boundary from one reading, and replaced
    /// both models that asked them apart: `heading`, the block model, and
    /// `heading_ends`, the boundary model -- both taken out of the tree when
    /// they were no longer consulted, to make room in the model budget.
    pub heading_lines: bool,
    /// The caption line model (`caption_lines`) decides which lines are a
    /// caption, from the heading row and where the line stands against the
    /// figures the page places or draws; a paragraph whose lines disagree is
    /// cut where they change (#636).
    pub captions: bool,
    /// The line graph network (`line_gnn`) decides, from every line and its
    /// neighbours in the plane, which lines are a heading and where a region
    /// ends inside a block; with it on, its heading probability is averaged
    /// with `heading_lines`' and decides in its place (#638).
    pub line_gnn: bool,
    /// The line network's image arm (`line_image`) reads the raster's six
    /// per-line statistics beside the vector arm (#574). It gates nothing
    /// yet, because no shipped model reads the column: it is the switch T3's
    /// model will consult, so the arm can be weighed against the cost of
    /// rasterising every page before it ships.
    pub line_image: bool,
    /// With `line_gnn` on, which network: [`line_gnn::V2`], retrained on the
    /// current engine's dump with what each line is set in and with half the
    /// pages read as loose pages, and decided at [`LINE_GNN_V2_HEADING`]; off,
    /// [`line_gnn::V1`] at `LINE_GNN_HEADING` (#574).
    pub line_gnn_v2: bool,
    /// A table the judge rejects goes back to prose column by column, each
    /// heading cell a heading, rather than row by row (#574).
    pub demote_per_cell: bool,
    /// A chosen table takes in the header rows above its first figure row:
    /// year rows, per-column labels, and centred labels spanning two or more
    /// figure columns, with a stub cell on the same line joining with them --
    /// and a lone stub-only line is held, joining only where a header or
    /// figure row joins above it, the section label under the column heads; a
    /// second consecutive stub-only line caps the table, as does a held line
    /// nothing commits, so a statement's title stays out (`stitch`, #574).
    /// Separate from `table_stitch_rows` because the two are measured apart.
    pub table_stitch_header: bool,
    /// A chosen table takes in the label-only lines below its rows -- wrapped
    /// row labels and section label rows, held until a figure row commits
    /// them -- and the figure rows left out between (`stitch`, #574).
    /// Separate from `table_stitch_header` because the two are measured
    /// apart.
    pub table_stitch_rows: bool,
    /// Two chosen tables with matching figure columns and nothing between
    /// them become one (`stitch`, #574). With both absorb switches off the
    /// walk vets without claiming, so only a merge takes the lines between.
    pub table_stitch_merge: bool,
    /// The final-table judge (`table_is_v2`) reads every table the pass would
    /// emit -- each chosen region after its cells are decoded, and each
    /// builder table it keeps -- and sends a refused one back to prose,
    /// whole (#574, #638). With it on it replaces v1 (`table_is`, which
    /// reads builder tables only); with it off v1 stays.
    pub table_is_v2: bool,
    /// A table column with no text in any row is not emitted (`tables`,
    /// #574). The lattice numbers its columns from the page's rulings and
    /// skips the stripes no word falls into, so a statement comes out with
    /// columns nothing stands in -- the `$` gutter beside its figures -- and
    /// the header block, decoded with fewer columns, stays a table apart.
    /// With this on, such a column is removed from the table's final grid:
    /// a cell over it loses one span, a cell only in it is gone, and the
    /// rest keeps its geometry. A table left with fewer than two columns
    /// keeps its grid.
    pub table_drop_empty_columns: bool,
    /// A rebuilt cell's words join top line first, left to right within a
    /// line, cut by the same rule `tables::in_reading_order` uses
    /// (`tables::cell_block`, #574). The decoder's own within-cell order
    /// merges stacked lines whose extents touch and sorts by x, so a header
    /// cell holding "March 31," over "2025" reads "March 2025 31,".
    pub table_cell_text_by_line: bool,
    /// A refused table goes back to prose as paragraphs, not one block per
    /// cell: within each column, a cell joins the block above it where
    /// m157's narrow-column pair conditions hold, and a block built from
    /// two cells or more is a paragraph (#574).
    pub table_v2_release_rejoin: bool,
    /// With the release rejoin on, a refused table of two columns or more
    /// with a joined block in some column is emitted column by column --
    /// the left column top to bottom, then the next -- rather than in the
    /// row-major order its cells stood in (#574). Without the rejoin there
    /// is no joined block, so on its own this changes nothing.
    pub table_v2_release_column_order: bool,
    /// A table whose final grid is two statements side by side comes out as
    /// two: split vertically where a stub column with a figure column right
    /// of it repeats the left part's column kinds and the gap before it
    /// stands out against its neighbours (`table_split`, #574).
    pub table_split_side_by_side: bool,
    /// A table whose final grid is two statements stacked comes out as two:
    /// split horizontally before a year row with figure rows above and below
    /// it, and the stitch merge refuses into a set starting with its own
    /// year head (`table_split`, #574).
    pub table_split_stacked: bool,
    /// A line that is a complete accounting figure on its own never runs on
    /// from a previous line that is also one, so `split_stacked_rows` splits
    /// stacked figure rows; prose run-ons keep today's rule (`cell_merge`,
    /// #574).
    pub table_runs_on_figures: bool,
    /// A cell whose whitespace-separated tokens are all figures or nil dashes,
    /// with or without a currency symbol, counts as a figure for the markdown
    /// header block, so fused figure rows read as body (`markdown`, #574).
    pub markdown_header_block_figures: bool,
    /// A stripe the bridge model left joined is split where a white corridor
    /// runs down through its lines, and the words crossing the corridor span
    /// the columns (`separations::place_with_corridors`, #574). With it off
    /// the decoder is exactly the one the boundary models were measured with.
    pub table_corridor_split: bool,
    /// The header block's rowspan veto fires only where a rowspan crosses the
    /// head boundary: a span inside the header (a group stub two rows high)
    /// or inside the body (a row group) no longer cancels header marking,
    /// only a span straddling the boundary does (`markdown`, #636).
    pub markdown_header_block_rowspan_crossing: bool,
    /// A table with no figures row still heads its first row: the first row
    /// of a word grid is the header by position (`markdown`, #636).
    pub markdown_header_block_word_table: bool,
    /// A row mixing label and figure cells is body: the header ends where
    /// rows turn label to value, even below the majority line (`markdown`,
    /// #636).
    pub markdown_header_block_mixed_body: bool,
    /// A rectangular table with no header row is HTML, not a pipe table: a
    /// pipe table forces its first row into a header the table does not have
    /// (`markdown`, #636).
    pub markdown_headerless_html: bool,
    /// A row whose value cells are all years is a year row even with a stub
    /// label in the first column: `($ in millions) | 2010 | 2009` heads its
    /// columns (`markdown`, #636).
    pub markdown_header_block_stub_years: bool,
}

/// The switches as shipped.
pub const SWITCHES: Switches = Switches {
    list_item: true,
    list_opens: true,
    // ON, AND IT REPLACED THE BLOCK MODEL AND THE BOUNDARY MODEL (#574, #632),
    // both gone from the tree once nothing consulted them: the boundary model
    // `heading_ends` first, 2.56 MB the budget needed, and
    // a question this model answers per line.
    //
    // Measured through this pass on the 1 200 validation pages of 98 documents
    // the pair was measured on, the same build, lists and paragraphs moved by
    // nothing else:
    //
    //     heading axis, per document (88)   pair 0.7592   lines 0.8126
    //     heading axis, per page            pair 0.7568   lines 0.7505
    //     financial reports, per page       pair 0.6636   lines 0.6693
    //
    // 34 documents better, 19 worse, 35 the same. The per-page figure is
    // nearly flat because ONE document -- perl-all-en-5.8.5.pdf, the only
    // manual in the sample -- is 278 of its pages, 23 per cent, and it loses
    // 0.10; per document that is one vote of 88. The rig's DocLayNet sample
    // is 81 per cent financial reports, where this wins 0.006.
    heading_lines: true,
    captions: CAPTIONS_SHIPPED,
    line_gnn: LINE_GNN_SHIPPED,
    line_image: LINE_IMAGE_SHIPPED,
    line_gnn_v2: LINE_GNN_V2_SHIPPED,
    demote_per_cell: DEMOTE_PER_CELL_SHIPPED,
    // ON, BEHIND A GATE, AND THE GATE IS THE WHOLE REASON. Ungated, both of
    // these turned 15 tests red, and the reason was never a bad average: they
    // win on TEDS (0.6093 to 0.6407 over PubTables) while each still loses
    // something the engine gets right today.
    //
    // `table_region` calls things tables that are not. Of the 3,683 blocks it
    // wrongly calls table on DocLayNet, 28.9 per cent are a PICTURE (a chart's
    // data labels), 10.1 per cent a LIST ITEM and 6.8 per cent a FORMULA --
    // the three shapes T2 named from the text axis, and together 45.8 per
    // cent. On `nasdaq_wwd_2002_p7` it turns the bulleted customer columns
    // into table cells, undoing what #599's line model does to that page.
    //
    // `table_structure` loses COLUMN SPANS. The decoder is asked for spans of
    // one (`grid_of`), so a header over two columns comes back as a cell plus
    // an empty one -- `golden/merged.md` holds that table with `colspan="2"`
    // today and would lose it. A span classifier measured WORSE on TEDS, which
    // is why it is not there; TEDS over PubTables is not sensitive to the
    // thing the golden file is.
    //
    // So the models do not replace the heuristic builder; they only overrule
    // it where the builder's own record (`Block::grid`, #624) says it had
    // little to go on -- `tables::may_overrule`, fitted at fill < 0.8 and at
    // most two full rows. That opens on 7.7 per cent of PubTables tables, and
    // on those the engine's own grid reads TEDS 0.4312 where the model reads
    // 0.5350, while all 23 tests in `tables_from_columns_and_spans.rs` -- the
    // cases the builder was written for -- stay green. Over every table,
    // 0.5996 to 0.6068.
    //
    // How WIDE the gate should be is still a question for the rig, whose
    // tables are the doubtful ones; PubTables' are clean (median fill 0.94)
    // and the width it prefers is one the 23 tests would refuse (#600).
    table_region: true,
    table_structure: true,
    // ON, MEASURED END TO END (#574). DocLayNet's tuning half through
    // `structured_markdown` and the rig's own `score_page`, 1,155 pages with a
    // table: tables F1 0.347 -> 0.534, 707 exact table counts against 540.
    // ParseBench's 30 table pages: 0.152 -> 0.250. What it costs on the same
    // pages: text 0.653 -> 0.644, order 0.775 -> 0.766, headings 0.594 ->
    // 0.591; lists unchanged at 0.492, because a list item stays out of a
    // judged region (`tables::judged_runs`). On the 276 table-less pages, 64
    // tables drawn against 67, and lists unchanged.
    table_judge: true,
    // ON, MEASURED END TO END (#574). ParseBench's 30 table pages: 0.250 ->
    // 0.438, every page we drew no table on now drawn, one page 0.05 worse.
    // DocLayNet's tuning half, 1,155 pages with a table: tables 0.534 ->
    // 0.549, lists 0.492 -> 0.500, text 0.644 -> 0.641; on the 276 table-less
    // pages it draws 75 tables against 64 -- drawn grids DocLayNet does not
    // call tables, which the page-level gate is for.
    table_lattice: true,
    // MERGE ON, ROWS ABSORB ON, HEADER ABSORB ON (v3), EMPTY-COLUMN DROP
    // ON, each measured apart against this tree with no set dropping
    // (#574). Merge: proxy tables on the tuning half 0.671 -> 0.706,
    // held-out equal, coverage equal, PubTables-300 TEDS 0.7517 -> 0.7536
    // and detection F1 0.8356 -> 0.8379, ParseBench equal.
    //
    // Rows absorb alone (m158, arm d vs a, master dc650fb81): ParseBench
    // tuning tables record match 0.4271 -> 0.4271, GriTS 0.6091 -> 0.6098
    // (2 pages up, 0 down); FinTabNet 614 TEDS 0.5678 -> 0.5704 (14 up, 5
    // down), struct 0.6109 -> 0.6121; PubTables-300 TEDS 0.7625 -> 0.7637,
    // PT-strat-300 0.7712 -> 0.7736 (3 up, 0 down), detection F1 strat
    // 0.8761 -> 0.8797, first-1000 equal; proxy tables tuning half 0.6849
    // -> 0.6946, held-out equal (0.6422), headings equal on both halves,
    // coverage unmatched 1,893 -> 1,892; independent 1,198 tables 0.5376
    // -> 0.5413 (3 up, 0 down).
    //
    // Header absorb v3 (m171, off -> on over rows on at 9d6f0a2c9):
    // ParseBench tuning tables record match 0.4271 -> 0.4396, GriTS 0.6098
    // -> 0.6177 (8 pages up, 0 down; per-document bootstrap +0.0252,
    // [+0.0065, +0.0488], 7 documents up, 0 down); FinTabNet 614 TEDS
    // 0.5704 -> 0.5762 (68 up, 32 down), struct 0.6121 -> 0.6162;
    // PubTables-300 TEDS 0.7637 -> 0.7635, PT-strat-300 0.7736 -> 0.7734,
    // detection F1 equal (strat) / -0.0001 (first 1000); proxy held-out
    // tables equal, held-out headings 0.7980 -> 0.8006, tuning tables
    // -0.0052 (one page, a heading taken in, as the old rule did);
    // independent 1,198 tables 0.5413 -> 0.5443. The old rule (m158) cost
    // proxy tuning tables -0.0146 and PT detection F1 strat -0.0023; v3
    // repairs the proxy page that cost.
    //
    // Empty-column drop alone (m168, off -> on over rows-on 9d6f0a2c9):
    // FinTabNet 614 TEDS 0.5704 -> 0.5739 (6 up, 0 down), struct 0.6121
    // -> 0.6155, found equal; PubTables-300 TEDS 0.7637 -> 0.7658,
    // PT-strat-300 0.7736 -> 0.7797 (9 up, 0 down), detection F1 equal on
    // both; independent 1,198 tables equal (0 movers); proxy tuning half
    // and coverage equal; ParseBench record match equal, GriTS 0.6098 ->
    // 0.6158. Emitted tables with an all-empty column: FinTabNet 38 -> 0,
    // independent 43 -> 1, ParseBench 15 -> 2.
    table_stitch_header: true,
    table_stitch_rows: true,
    table_stitch_merge: true,
    table_is_v2: TABLE_IS_V2_SHIPPED,
    table_drop_empty_columns: true,
    // ON, MEASURED OVER HEADER ABSORB V3 (`v3` -> `v3+lines`, m179, #574).
    // corp-q1-2025 p13 record match 0.1299 -> 0.5455 ("March 31, 2025" now
    // one key); ParseBench tuning record match 0.4396 -> 0.4446, bootstrap
    // +0.0078 [+0.0000, +0.0229], 2 documents up, 0 down. FinTabNet 614
    // TEDS 0.5762 -> 0.5814 (the switch alone 0.5704 -> 0.5751, 46 up, 0
    // down). Independent tables 0.5443 -> 0.5453; proxy 0 movers.
    // A known small cost on PubTables, within +-0.002: PubTables-300 TEDS
    // 0.7635 -> 0.7625, PT-strat-300 0.7734 -> 0.7729, detection F1 -0.0005
    // / -0.0016 -- many pages a few hundredths down, where corrected text
    // lets the row tidy keep rows apart.
    table_cell_text_by_line: true,
    // ON, MEASURED OFF AGAINST ON (m185, confirmed m201, #574). Proxy tuning
    // false released headings 29 -> 5 (released blocks 174 -> 101),
    // coverage +127 blocks matched, headings/text/order not down;
    // ParseBench tuning identical, independent within noise. FinTabNet 614
    // TEDS 0.5704, struct 0.6121, found 450/614 identical (0 movers);
    // PubTables eval-300 TEDS 0.7626, strat-300 0.7735, detection F1 0.8450
    // / 0.8810 identical (0/300 pages differ). The all-heading-hinted
    // refinement stays out: 2 correct against 8 false headings.
    table_v2_release_rejoin: true,
    // OFF: the column-by-column order is measured apart (m201: identical to
    // the rejoin on FinTabNet and PubTables) and ships only with the order
    // axis and coverage not down against it.
    table_v2_release_column_order: TABLE_V2_RELEASE_COLUMN_ORDER_SHIPPED,
    // STACKED SPLIT ON, SIDE BY SIDE OFF (#574). Stacked measured on this
    // tree with rows absorb on, off against stacked (m169b): FinTabNet 614
    // TEDS 0.5704 -> 0.5848 (19 up, 4 down), 19 pages emitting one more
    // table; ParseBench tuning record match 0.4271 -> 0.4357;
    // PubTables-300 and PT-strat-300 TEDS and detection F1 identical (no
    // page splits); independent 1,198 tables 0.5413 -> 0.5408 (3 up, 3
    // down -- false splits on 3c97042f, effc00f6, 0402c35d, a known loss);
    // proxy tuning tables 0.6946 -> 0.6930 (one page). Side by side stays
    // off: it costs PubTables detection F1 -0.024 on eval-300 and -0.019
    // on strat-300.
    table_split_side_by_side: false,
    table_split_stacked: true,
    // ON, MEASURED OFF AGAINST ON (m248, #574). B with the stub rule: a
    // fused row whose stub holds stacked labels splits on the figure rule
    // regardless of parentheses, otherwise the bracket guard decides.
    // FinTabNet 614 TEDS 0.5989 -> 0.6006 (5 up, 0 down); PubTables-300
    // TEDS 0.7669 -> 0.7678 (1 up, 0 down), PT-strat-300 equal, detection
    // identical, the two statistics-stack pages at base; ParseBench record
    // match +0.0009 (DLR p61 0.07 -> 0.10), bootstrap not negative; false
    // tables identical; independent tables -0.0004 (two ASX_ORI_2012 pages:
    // one real, unannotated table; one error-form shift). Blocked fused
    // rows vs the bracket guard alone: FinTabNet 7 -> 2, PB 1 -> 1.
    table_runs_on_figures: true,
    // ON, MEASURED OFF AGAINST ON (m211, #574). Reading fused figure cells
    // as figures for the header block: FinTabNet 614 TEDS 0.5971 -> 0.5989
    // (23 up, 1 down); PubTables-300 TEDS 0.7633 -> 0.7669, PT-strat-300
    // 0.7789 -> 0.7828, detection identical; ParseBench record match 0.4497
    // -> 0.4532, bootstrap +0.0067 [+0.0002, +0.0162], 4 documents up, 0
    // down; independent, proxy and false tables equal; FinTabNet tables
    // with header_block >= 2: 8 -> 3.
    markdown_header_block_figures: true,
    // ON, with the G1 occupancy guard at 0.7 (CORRIDOR_GUARD_OCC): the
    // TUNING grid over G1-G4 (23 configs on m396's 1,500 PubTables TRAIN +
    // 800 FinTabNet TRAIN-side pages, split by document with even sha as
    // TUNING) ships G1 alone -- the best PT+FTN mean whose lower CI bound
    // vs the shipped split stays at or above zero on both sets. TUNING
    // page-mean dTEDS vs shipped, cluster bootstrap per document: PubTables
    // +0.00167 [+0.00028, +0.00342] (8 of 10 worsenings fixed, 3 of 4
    // improvements kept), FinTabNet +0.00113 [+0.00002, +0.00266] (3 of 3
    // fixed, 7 of 7 kept). HELD-OUT, descriptive: PubTables +0.00031
    // [-0.00074, +0.00137], FinTabNet +0.00292 [-0.00091, +0.00674].
    // Proxy and independent TUNING halves: false tables and table counts
    // equal to shipped, no page differs. G2/G3/G4 stay in code, off: G2
    // vetoes true gutters (3 of 4 TUNING PT improvements lost alone), G3
    // moves 3 TUNING pages with no mean effect (decode header rows only
    // rise under splits), G4 vetoes true narrow FinTabNet tables (FTN mean
    // -0.00137 alone). With it off the output is byte-identical: the
    // decoder never sees the split.
    table_corridor_split: TABLE_CORRIDOR_SPLIT_SHIPPED,
    // ON, MEASURED AS A COMBINATION ON TUNING (#636). The five header
    // switches ship as one: word-table, stub-years and mixed-body decide the
    // head, headerless-html renders head-0 rectangles as HTML, and
    // rowspan-crossing narrows the veto. Paired page-mean dTEDS vs master,
    // cluster bootstrap over documents (2,000 reps), 793 PT + 395 FTN pages:
    // PT +0.0105 [+0.0073,+0.0138], FTN +0.0113 [+0.0085,+0.0143], dStruct
    // +0.0086/+0.0086, 249 pages up, 47 down. Fixed on right-shape grids: 34
    // too-few and 15 too-many header rows; 18 right-before tables newly
    // mismatch (13 headed word grids, 4 swept body-label rows, 1 other).
    // rowspan-crossing (+0.0018/+0.0011, both lower bounds above 0, nothing
    // broken) and stub-years (FTN +0.0047, PT no movers) also pass alone;
    // word-table, mixed-body and headerless-html fail alone and ship only in
    // the combination, where mixed-body repairs word-table's TOC, stub-years
    // keeps the stubbed-year headers, and word-table + stub-years head the
    // pipe-luck tables headerless-html would unmark. HELD-OUT, descriptive
    // and never selected on: PT +0.0095, FTN +0.0146. DocLayNet proxy TUNING
    // (197 pages) and independent TUNING (616) halves: table count and false
    // tables equal between the arms.
    markdown_header_block_rowspan_crossing: true,
    markdown_header_block_word_table: true,
    markdown_header_block_mixed_body: true,
    markdown_headerless_html: true,
    markdown_header_block_stub_years: true,
};

/// The probability at or above which `list_item` calls a block a list item.
/// Chosen on the validation third of the training documents, never on the
/// rig; the model's manifest records the same number.
const LIST_ITEM_THRESHOLD: f64 = models::list_item::THRESHOLD;

/// The probability at or above which `list_opens` opens an item at a line.
const LIST_OPENS_THRESHOLD: f64 = models::list_opens::THRESHOLD;
/// The probability at or above which `heading_lines` calls a line a heading.
const HEADING_LINES_THRESHOLD: f64 = models::heading_lines::THRESHOLD;
/// The probability at or above which `caption_lines` calls a line a caption.
const CAPTION_LINES_THRESHOLD: f64 = models::caption_lines::THRESHOLD;
/// The probability of `heading` at or above which the graph network calls a
/// line a heading.
const LINE_GNN_HEADING: f64 = 0.6;
/// Whether the line graph network ships on (#638). ON, measured on the
/// rig-like proxy (`scripts/extraction/rig_like_proxy.py`: 389 DocLayNet
/// validation pages in the rig's genre shares, the rig's own scorer), the same
/// build with the switch off and on:
///
/// ```text
/// headings, per document (36)   0.7678 -> 0.7910   per page 0.6204 -> 0.6629
/// lists                          0.5866 -> 0.5864
/// tables                         0.4601 -> 0.4601
/// text (1-NED similarity)        0.6074 -> 0.6074
/// order (annotated), per doc     0.7503 -> 0.7535   per page 0.7449 -> 0.7434
/// ```
///
/// The network alone was +0.013 per document -- financial reports gained and
/// patents and single-page articles lost -- and averaged with `heading_lines`
/// +0.023 (with cuts kept to lines one above the other): the two fail on
/// different genres. About 1.3 ms a page for the network, single-threaded.
const LINE_GNN_SHIPPED: bool = true;
/// Whether the line network reads the raster's per-line statistics (#574).
/// OFF: the switch gates nothing yet, because no shipped model reads the
/// column. It ships off so T3's image-arm model can be weighed against the
/// cost of rasterising every page -- about 8 ms a page at the median, 34 ms
/// at p95, against the network's own 1.3 ms -- before it ships.
const LINE_IMAGE_SHIPPED: bool = false;
/// Whether the retrained network decides (#574). ON, for one rig round with
/// T4's table-or-not retrained on its verdicts -- the two land together,
/// because that model reads this one's `p_heading_pass`.
///
/// Measured through this pass on the landed master's tree, against `V1`, the
/// heading axis weighted to the rig's genre mix:
///
/// ```text
///                         per page   per document
/// tuning set (1 111 pp)    +0.028      +0.026      financial +0.032/doc
/// T6's proxy (291 pp)      +0.023      +0.004      financial -0.008/doc (8 docs)
/// ```
///
/// Lists +0.005 and +0.001; tables unmoved but for one patent page whose
/// table T4's model, trained on `V1`'s verdicts, no longer keeps -- the
/// reason the two land together. Trained the old way, on whole documents
/// only, the same network lost per page on both sets: the rig's pages are
/// one-page PDFs, where a running head repeats nowhere.
const LINE_GNN_V2_SHIPPED: bool = true;
/// The heading bar for `V2`, chosen on the tuning set (0.5 against 0.6).
const LINE_GNN_V2_HEADING: f64 = 0.5;
/// Whether a rejected table goes back to prose column by column (#574).
const DEMOTE_PER_CELL_SHIPPED: bool = true;
/// Whether the final-table judge ships on (#574, #638). ON, measured off
/// against on at head 181e25741 (m134): PubTables-300 TEDS 0.7612 -> 0.7601
/// (within +-0.002) and detection F1 0.8382 -> 0.8415 (13 pages move, all
/// up); PT-strat-300 TEDS 0.7708 -> 0.7708 (0 pages move), F1 0.8761 ->
/// 0.8776; false tables over prose 84 -> 52, the blocks they swallow 684 ->
/// 336; unmatched annotated blocks 1,895 -> 1,726; proxy headings held-out
/// 0.8006 -> 0.8060, T5's headings back 6/69 + 34/168; ParseBench 0.5069 ->
/// 0.5069 (0 examples move).
const TABLE_IS_V2_SHIPPED: bool = true;
/// Whether a rejoined refusal is emitted column by column (#574). OFF: the
/// order arm is measured apart from the rejoin, and ships only with the
/// order axis and coverage not down against it.
const TABLE_V2_RELEASE_COLUMN_ORDER_SHIPPED: bool = false;
/// Whether a joined stripe splits at a white corridor (#574). ON
/// provisionally with the opening constants; the grid in #574 picks them on
/// the train-side sets and the go/no-go there decides whether this stays on.
const TABLE_CORRIDOR_SPLIT_SHIPPED: bool = true;
/// The mean at which a block the heuristics already call a heading keeps
/// that kind (#574). Measured on T6's rig-like proxy, the heading axis is the
/// same to four decimals at 0.5 as without it (0.6690), and three of the four
/// ParseBench pages round 4 lost a title on get it back.
const LINE_GNN_KEEP_HEADING: f64 = 0.5;

/// The heading probability at or above which the pass calls a line of `kind`
/// a heading under `switches`.
///
/// ONE BAR FOR BOTH READERS: `decide_page` decides at it and
/// `confidence_of_page` measures the doubt around it, so the doubt is about
/// the decision the pass took -- the retrained network's 0.5 with
/// `line_gnn_v2`, not the old network's 0.6. Without the graph network the
/// line model's bar decides; a heading the heuristics already made keeps its
/// kind on less, because the pair has to agree to take a title away, not
/// only to make one (the mean fell to 0.51-0.58 on ParseBench titles where
/// `heading_lines` read 0.66-0.94: `AUSTRALIA`, `Explanation :`, a council's
/// names; round 4).
fn heading_bar(kind: &str, switches: Switches) -> f64 {
    if !switches.line_gnn {
        HEADING_LINES_THRESHOLD
    } else if kind == "heading" {
        LINE_GNN_KEEP_HEADING
    } else if switches.line_gnn_v2 {
        LINE_GNN_V2_HEADING
    } else {
        LINE_GNN_HEADING
    }
}
/// Whether the caption decision ships on (#636). ON, measured through this
/// pass on the 1 200 validation pages of the #632 proxy, the same build, with
/// and without: captions per page 0.000 -> 0.537 (per document 0.481) on the
/// 222 pages that have one; headings and lists unmoved to four decimals;
/// paragraphs +0.004, because a caption cut off a paragraph is the paragraph
/// DocLayNet draws. Of the 391 captions made, 26 are on pages with no
/// annotated caption. The rig reads a caption as a paragraph, so there it
/// moves only the paragraphs it is cut from.
const CAPTIONS_SHIPPED: bool = true;

/// Below this MEAN probability `list_item` takes a confirmed run of list items
/// back out of the list, whole.
///
/// ASYMMETRIC ON PURPOSE. An item the heuristics confirmed stands on evidence
/// the model does not read -- a run of agreeing markers down the whole column
/// (`lists::is_a_run`) -- and on clean lists the model is merely unsure of
/// such an item (0.29 to 0.67 on the operator pages of
/// `lists_from_markers_and_indentation.rs`, the first item of a list lowest).
/// Applied to the mean of a run, see `decide_page`.
const LIST_ITEM_DEMOTE_BELOW: f64 = 0.15;

fn probability(margin: f64) -> f64 {
    1.0 / (1.0 + (-margin).exp())
}

/// What the heading model says about each block, whether the pass acts on it or
/// not -- for a dump, so that a diagnosis reads the ENGINE's number (#602).
///
/// WHY THIS EXISTS. `Block::confidence` carries the probability of the decision
/// that produced a block's kind, so a block the model REFUSED carries nothing,
/// and "why was this heading refused" cannot be asked of a dump at all. The
/// answer was being re-derived in Python from the dumped features, which is a
/// second implementation of the vector and got the snapshot wrong twice: the
/// model reads the blocks BEFORE the pass runs, and a dump of the blocks after
/// it has different neighbours.
///
/// `None` where the model is not asked, which is a different fact from a low
/// probability and reads as one if it is flattened to zero.
#[doc(hidden)]
pub fn heading_probabilities(blocks: &[Block], pages: &[(f64, f64)]) -> Vec<Option<f64>> {
    // THE LINE MODEL'S, since it is the one that decides: the mean over the
    // block's lines, the number a block it promotes carries as `confidence`.
    let facts = document_facts(blocks);
    let mut out = vec![None; blocks.len()];
    for (page, members) in pages_of(blocks).iter().enumerate() {
        let refs: Vec<&Block> = members.iter().map(|&i| &blocks[i]).collect();
        let size = pages.get(page).copied().unwrap_or((0.0, 0.0));
        let vectors = page_vectors(&refs, size, &facts);
        let rows = heading_lines::page_rows(&vectors);
        for ((&i, v), rows) in members.iter().zip(&vectors).zip(&rows) {
            if matches!(v.kind, "paragraph" | "heading" | "table_cell") && !rows.is_empty() {
                let sum: f64 = rows
                    .iter()
                    .map(|row| probability(models::heading_lines::predict_margin(row)))
                    .sum();
                out[i] = Some(sum / rows.len() as f64);
            }
        }
    }
    out
}

/// What the models decided about one block, before anything is applied.
#[derive(Debug, Clone, Default, PartialEq)]
struct Decision {
    /// `Some(true)`: make it a list item; `Some(false)`: it is not one;
    /// `None`: leave the heuristics' verdict.
    item: Option<bool>,
    /// Lines (index into [`lines_of`]) at which a new list item opens.
    opens_at: Vec<usize>,
    /// Whether the block's FIRST line opens a list item too: when the block is
    /// split, its first part is then an item and not the paragraph it was.
    first_opens: bool,
    /// One verdict per line from `heading_lines`, in [`lines_of`] order;
    /// empty where the model was not asked.
    heading_lines: Vec<bool>,
    /// The probabilities behind them.
    p_heading_lines: Vec<f64>,
    /// Every line's class probabilities from the line model, in
    /// [`heading_lines::LINE_CLASSES`] order and [`lines_of`] order; empty
    /// when the model was not asked (#574).
    line_classes: Vec<[f64; 4]>,
    /// One verdict per line from `caption_lines`, and the probabilities
    /// behind them; empty where the model was not asked (#636).
    caption_lines: Vec<bool>,
    p_caption_lines: Vec<f64>,
    /// Lines (index into [`lines_of`]) before which a region ends inside the
    /// block, from `line_gnn`'s link head (#638).
    line_breaks: Vec<usize>,
    /// `line_gnn`'s link probability between the block's LAST line and the
    /// next line in reading order: whether the block goes on into the next
    /// one (#638, T5). `None` where the network was not asked.
    link_after: Option<f64>,
    /// The probabilities behind them, for [`explain`].
    p_item: Option<f64>,
    p_opens: Vec<f64>,
    /// The opening probability of the block's FIRST line, which drives
    /// `first_opens` and which nothing else keeps (#624, codex on #1977). Without
    /// it a piece that became an item because of that decision has no number of
    /// its own to report.
    p_first_opens: Option<f64>,
}

/// Every block's probabilities, for diagnosis: `(index, p_item, p_opens per
/// line from the second)`. Nothing is applied.
#[doc(hidden)]
pub fn explain(blocks: &[Block], pages: &[(f64, f64)]) -> Vec<(usize, Option<f64>, Vec<f64>)> {
    decide(blocks, pages, SWITCHES)
        .into_iter()
        .enumerate()
        .map(|(i, d)| (i, d.p_item, d.p_opens))
        .collect()
}

/// What one block measures, as the models read it: the page it is on, its
/// kind as the dump names it, its block vector and one vector per line.
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct BlockVectors {
    pub page: usize,
    pub kind: &'static str,
    pub block: [f64; 30],
    pub lines: Vec<[f64; 23]>,
    /// Each line's box and size, in the same order as `lines`: where the line
    /// stands, which the caption row measures against the page's pictures.
    pub line_boxes: Vec<([f64; 4], f64)>,
    /// Each line's [`style::STYLE_FEATURES`], in the same order: what it is
    /// set in against the page's body (#574).
    pub style: Vec<[f64; 5]>,
}

/// The document's pages as the positions of their blocks in `blocks`, in page
/// order, each page's blocks in the order the document gives them -- the
/// order the dump measured them in.
fn pages_of(blocks: &[Block]) -> Vec<Vec<usize>> {
    let page_count = blocks.iter().map(|b| b.page + 1).max().unwrap_or(0);
    let mut by_page: Vec<Vec<usize>> = vec![Vec::new(); page_count];
    for (i, b) in blocks.iter().enumerate() {
        by_page[b.page].push(i);
    }
    by_page
}

/// The facts of the document `blocks` make up, for [`refine_page`].
pub fn document_facts(blocks: &[Block]) -> DocumentFacts {
    // THE BODY FACE IS TAKEN FROM THE BLOCKS THEMSELVES, not from a page's
    // features: a face is on the spans, and asking for the page's features
    // here would mean measuring every page twice for one document-wide fact
    // (#599).
    let faces = blocks.iter().map(|b| {
        (
            b.spans.iter().find_map(|s| s.font_name.clone()),
            b.text.chars().count(),
        )
    });
    DocumentFacts::of_texts_and_faces(
        pages_of(blocks)
            .into_iter()
            .map(|members| members.into_iter().map(|i| blocks[i].text.as_str())),
        faces,
    )
}

/// One page's vectors, in the order of `page`: the ONE path from blocks to
/// model input. Every other function here that reads the models reads it, and
/// so does the parity test that holds it to the training's vectors -- so a
/// mistake in how the engine wires a feature (a neighbour taken from the wrong
/// block, a page measured in the wrong order) fails that test, and not only a
/// mistake inside a feature.
fn page_vectors(page: &[&Block], size: (f64, f64), facts: &DocumentFacts) -> Vec<BlockVectors> {
    let number = page.first().map_or(0, |b| b.page);
    let texts: Vec<String> = page.iter().map(|b| b.text.clone()).collect();
    let mut features = page_features(page, size.0, size.1);
    facts.fill(&mut features, &texts);
    let grouped = lines::grouped_lines(&features.words, page.len());
    vectors_from_parts(page, number, &features, &grouped)
}

/// [`page_vectors`] over one measurement and one grouping: the same vectors,
/// without re-grouping the page for its heads, its style, its lines and its
/// boxes one by one.
fn vectors_from_parts(
    page: &[&Block],
    number: usize,
    features: &features::PageFeatures,
    grouped: &[Vec<Vec<&features::PageWord>>],
) -> Vec<BlockVectors> {
    let heads = lines::heads_on_grouped(grouped);
    let mut styles = style::page_style_on_grouped(grouped, &features.words).into_iter();
    page.iter()
        .enumerate()
        .map(|(at, own)| {
            let block = &features.blocks[at];
            let context = LineContext {
                bbox: own.bbox,
                kind: block.kind,
                size: block.size,
                body_size: features.body_size,
                prev_ends_colon: (at > 0).then(|| page[at - 1].text.trim().ends_with(':')),
            };
            let piece: &[Vec<&features::PageWord>] =
                grouped.get(at).map(Vec::as_slice).unwrap_or(&[]);
            BlockVectors {
                page: number,
                kind: block.kind,
                block: block_vector(block),
                lines: lines::line_vectors_on_lines(piece, &context, &heads),
                line_boxes: lines::line_boxes_on_lines(piece),
                style: styles.next().unwrap_or_default(),
            }
        })
        .collect()
}

/// Every block's vectors, in the order of `blocks`: [`page_vectors`] page by
/// page, under the document's facts.
#[doc(hidden)]
pub fn document_vectors(blocks: &[Block], pages: &[(f64, f64)]) -> Vec<BlockVectors> {
    let facts = document_facts(blocks);
    let mut out: Vec<Option<BlockVectors>> = vec![None; blocks.len()];
    for (page, members) in pages_of(blocks).iter().enumerate() {
        let refs: Vec<&Block> = members.iter().map(|&i| &blocks[i]).collect();
        let size = pages.get(page).copied().unwrap_or((0.0, 0.0));
        for (&i, v) in members.iter().zip(page_vectors(&refs, size, &facts)) {
            out[i] = Some(v);
        }
    }
    out.into_iter()
        .map(|v| v.expect("every block is on a page"))
        .collect()
}

/// Every decision for one page's blocks, from one snapshot of them, in their
/// order.
fn decide_page(
    page: &[&Block],
    size: (f64, f64),
    facts: &DocumentFacts,
    switches: Switches,
    images: &[[f64; 4]],
) -> Vec<Decision> {
    decide_page_traced(page, size, facts, switches, images).0
}

/// `decide_page`, with the page's vectors and the graph network's verdicts
/// beside the decisions: what `page_confidence` reads its doubt from, so the
/// number is about the very values the pass decided on and the network runs
/// once for both.
fn decide_page_traced(
    page: &[&Block],
    size: (f64, f64),
    facts: &DocumentFacts,
    switches: Switches,
    images: &[[f64; 4]],
) -> (
    Vec<Decision>,
    Vec<BlockVectors>,
    Option<line_gnn::LineVerdicts>,
) {
    let vectors = page_vectors(page, size, facts);
    if !switches.list_item
        && !switches.list_opens
        && !switches.heading_lines
        && !switches.captions
        && !switches.line_gnn
    {
        return (vec![Decision::default(); page.len()], vectors, None);
    }
    // THE LINE MODEL READS THE PAGE, not the block: a line's context is the
    // lines above and below it in reading order, across blocks (#632).
    let line_rows = switches
        .heading_lines
        .then(|| heading_lines::page_rows(&vectors));
    let classes = switches
        .list_item
        .then(|| heading_lines::page_class_probabilities(&vectors));
    // THE GRAPH NETWORK READS THE WHOLE PAGE AT ONCE; its verdicts are per
    // line in reading order, so each block takes its own slice of them.
    let gnn = switches
        .line_gnn
        .then(|| line_gnn::verdicts(&vectors, size, images, gnn_of(switches)));
    let heading_rows = if switches.line_gnn {
        heading_lines::page_rows(&vectors)
    } else {
        Vec::new()
    };
    let caption_rows = switches
        .captions
        .then(|| captions::page_caption_rows(&vectors, images));
    let decisions = decide_from_parts(
        page,
        &vectors,
        switches,
        line_rows.as_ref(),
        classes.as_ref(),
        gnn.as_ref(),
        &heading_rows,
        caption_rows.as_ref(),
    );
    (decisions, vectors, gnn)
}

/// The decisions over one page's vectors and model readings: the tail
/// [`decide_page_traced`] and the shared pass both decide through, so the
/// two cannot disagree on what a reading means.
#[allow(clippy::too_many_arguments)]
fn decide_from_parts(
    page: &[&Block],
    vectors: &[BlockVectors],
    switches: Switches,
    line_rows: Option<&Vec<Vec<[f64; heading_lines::WIDTH]>>>,
    classes: Option<&Vec<Vec<[f64; 4]>>>,
    gnn: Option<&line_gnn::LineVerdicts>,
    heading_rows: &[Vec<[f64; heading_lines::WIDTH]>],
    caption_rows: Option<&Vec<Vec<[f64; captions::CAPTION_WIDTH]>>>,
) -> Vec<Decision> {
    let mut decisions = vec![Decision::default(); page.len()];
    let mut first_line = Vec::with_capacity(vectors.len());
    let mut running = 0usize;
    for v in vectors {
        first_line.push(running);
        running += v.lines.len();
    }
    for (at, (decision, v)) in decisions.iter_mut().zip(vectors).enumerate() {
        if let Some(rows) = caption_rows {
            if v.kind == "paragraph" {
                let p: Vec<f64> = rows[at]
                    .iter()
                    .map(|row| probability(models::caption_lines::predict_margin(row)))
                    .collect();
                decision.caption_lines = p.iter().map(|p| *p >= CAPTION_LINES_THRESHOLD).collect();
                decision.p_caption_lines = p;
            }
        }
        if let Some(gnn) = gnn {
            if !v.lines.is_empty() {
                let last = first_line[at] + v.lines.len() - 1;
                decision.link_after = gnn.link_next.get(last).map(|p| f64::from(*p));
            }
            if matches!(v.kind, "paragraph" | "heading" | "table_cell") && !v.lines.is_empty() {
                let from = first_line[at];
                let to = from + v.lines.len();
                let heading = gnn_of(switches).class("heading").unwrap_or(1);
                // THE TWO LINE MODELS TOGETHER: the graph network's heading
                // probability averaged with `heading_lines`'. They fail on
                // different genres (#638), and the mean is steadier than either.
                let p: Vec<f64> = gnn.classes[from..to]
                    .iter()
                    .zip(&heading_rows[at])
                    .map(|(c, row)| {
                        0.5 * (f64::from(c[heading])
                            + probability(models::heading_lines::predict_margin(row)))
                    })
                    .collect();
                let bar = heading_bar(v.kind, switches);
                decision.heading_lines = p.iter().map(|p| *p >= bar).collect();
                decision.p_heading_lines = p;
                if v.kind != "table_cell" {
                    // ONLY BETWEEN LINES ONE ABOVE THE OTHER. Two lines side by
                    // side -- the cells of one row, two headings over their own
                    // columns -- are one row of the block, and a cut between
                    // them is a cut through the row, which the heuristics'
                    // own tests hold against (`the_gutter_of_a_page_set_in_columns`).
                    decision.line_breaks = (from + 1..to)
                        .filter(|&i| gnn.link_next[i - 1] < 0.5)
                        .map(|i| i - from)
                        .filter(|&k| {
                            let (above, _) = v.line_boxes[k - 1];
                            let (below, _) = v.line_boxes[k];
                            below[3] <= above[1]
                        })
                        .collect();
                }
            }
        }
        if let Some(rows) = line_rows.filter(|_| !switches.line_gnn) {
            // EVERY LINE'S CLASSES, kept whatever the block's kind, because the
            // list and table decisions read them too (#574).
            decision.line_classes = rows[at]
                .iter()
                .map(heading_lines::class_probabilities)
                .collect();
            // THE HEADING DECISION STAYS THE BINARY MODEL'S. The class model
            // decided it worse through the pass -- heading axis per document
            // 0.8070 to 0.7673 on the 1 200 validation pages, lists -0.012 --
            // so it carries the classes for the other lanes and decides nothing
            // here (#574).
            if matches!(v.kind, "paragraph" | "heading" | "table_cell") {
                let p: Vec<f64> = rows[at]
                    .iter()
                    .map(|row| probability(models::heading_lines::predict_margin(row)))
                    .collect();
                decision.heading_lines = p.iter().map(|p| *p >= HEADING_LINES_THRESHOLD).collect();
                decision.p_heading_lines = p;
            }
        }
        if switches.list_opens && matches!(v.kind, "paragraph" | "list_item") {
            let p: Vec<f64> = v
                .lines
                .iter()
                .map(|line| probability(models::list_opens::predict_margin(line)))
                .collect();
            decision.first_opens = p.first().is_some_and(|p| *p >= LIST_OPENS_THRESHOLD);
            decision.p_first_opens = p.first().copied();
            decision.opens_at = p
                .iter()
                .enumerate()
                .skip(1)
                .filter(|(_, p)| **p >= LIST_OPENS_THRESHOLD)
                .map(|(k, _)| k)
                .collect();
            decision.p_opens = p.into_iter().skip(1).collect();
        }
        if switches.list_item && matches!(v.kind, "paragraph" | "list_item") {
            let p = list_item_probability(v, classes.map(|c| c[at].as_slice()));
            // Promotion is per block; demotion is per run, below.
            decision.item = (p >= LIST_ITEM_THRESHOLD).then_some(true);
            decision.p_item = Some(p);
        }
    }
    let members: Vec<usize> = (0..page.len()).collect();
    demote_confirmed_runs(&members, page, vectors, &mut decisions);
    decisions
}

/// [`decide_page`] over one page's vectors, heading rows and line classes:
/// the same decisions, without re-measuring the page for them. The rows and
/// classes are the caller's, read once for the judge and the decisions
/// together; each lane still gates on its own switch.
#[allow(clippy::too_many_arguments)]
fn decide_page_on_shared(
    page: &[&Block],
    vectors: &[BlockVectors],
    size: (f64, f64),
    switches: Switches,
    images: &[[f64; 4]],
    rows: Option<&Vec<Vec<[f64; heading_lines::WIDTH]>>>,
    classes: Option<&Vec<Vec<[f64; 4]>>>,
) -> Vec<Decision> {
    if !switches.list_item
        && !switches.list_opens
        && !switches.heading_lines
        && !switches.captions
        && !switches.line_gnn
    {
        return vec![Decision::default(); page.len()];
    }
    // THE GRAPH NETWORK READS THE WHOLE PAGE AT ONCE; its verdicts are per
    // line in reading order, so each block takes its own slice of them.
    let gnn = switches
        .line_gnn
        .then(|| line_gnn::verdicts(vectors, size, images, gnn_of(switches)));
    let caption_rows = rows
        .filter(|_| switches.captions)
        .map(|rows| captions::page_caption_rows_on_rows(vectors, images, rows));
    decide_from_parts(
        page,
        vectors,
        switches,
        rows.filter(|_| switches.heading_lines),
        classes.filter(|_| switches.list_item),
        gnn.as_ref(),
        rows.filter(|_| switches.line_gnn)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
        caption_rows.as_ref(),
    )
}

/// The judge's regions and the decisions over one measurement of the page
/// (T1).
///
/// The features, vectors, heading rows and line classes computed once and
/// passed along, instead of once for the judge, once for the list signal and
/// once for the decisions -- and the heading rows once instead of once per
/// lane. What the judge and each lane read is what they read apart before;
/// only the repetition is gone, so the regions and decisions come out as
/// they did.
///
/// PURE PER PAGE. What it reads is its arguments -- the page's own blocks,
/// size, rules and images, the document's facts and the switches -- and what
/// it changes is what it returns; there is no cache and no state between
/// calls.
fn judge_and_decide_shared(
    page: &[Block],
    size: (f64, f64),
    facts: &DocumentFacts,
    switches: Switches,
    rules: &[crate::text::HorizontalRule],
    images: &[[f64; 4]],
) -> (Option<tables::JudgedRegions>, Vec<Decision>) {
    let refs: Vec<&Block> = page.iter().collect();
    let texts: Vec<String> = page.iter().map(|b| b.text.clone()).collect();
    let mut features = page_features(&refs, size.0, size.1);
    facts.fill(&mut features, &texts);
    let grouped = lines::grouped_lines(&features.words, page.len());
    let number = page.first().map_or(0, |b| b.page);
    let vectors = vectors_from_parts(&refs, number, &features, &grouped);
    // The heading rows once for every reader: the heading lanes, the list
    // classes, the graph network's heading mean, the captions, and -- through
    // the classes -- the list signal the judge reads.
    let rows = (switches.heading_lines
        || switches.list_item
        || switches.line_gnn
        || switches.captions
        || switches.table_judge)
        .then(|| heading_lines::page_rows(&vectors));
    // Rows are Some here: both conditions below are in the rows' condition.
    let classes = (switches.list_item || switches.table_judge).then(|| {
        heading_lines::class_probabilities_of_rows(
            rows.as_ref().expect("rows are read wherever classes are"),
        )
    });
    let judged = switches.table_judge.then(|| {
        let signal = list_signal_from_parts(
            &vectors,
            classes
                .as_ref()
                .expect("classes are read wherever the signal is"),
        );
        let lines = judge::lines_of_grouped(&refs, &grouped, &signal);
        tables::judge_regions_with_lines(page, size, rules, switches, &lines, &features.words)
    });
    let decisions = decide_page_on_shared(
        &refs,
        &vectors,
        size,
        switches,
        images,
        rows.as_ref(),
        classes.as_ref(),
    );
    (judged, decisions)
}

/// The judge and the decisions as they read the page apart before the shared
/// pass: the frozen pre-change composition the byte-identity test holds the
/// shared pass to. Test-only; serving reads [`judge_and_decide_shared`].
#[cfg(test)]
fn judge_and_decide_legacy(
    page: &[Block],
    size: (f64, f64),
    facts: &DocumentFacts,
    switches: Switches,
    rules: &[crate::text::HorizontalRule],
    images: &[[f64; 4]],
) -> (Option<tables::JudgedRegions>, Vec<Decision>) {
    let judged = switches
        .table_judge
        .then(|| tables::judge_regions_traced(page, size, rules, facts, switches));
    let decisions = {
        let refs: Vec<&Block> = page.iter().collect();
        decide_page(&refs, size, facts, switches, images)
    };
    (judged, decisions)
}

/// What `list_item` says of one block: the probability that it is a list item.
fn list_item_probability(v: &BlockVectors, classes: Option<&[[f64; 4]]>) -> f64 {
    let mut x = [f64::NAN; 50];
    // THE FIRST TWENTY-FOUR, and not the whole vector: the six the heading
    // lane appended (#599) came after this model was trained, and a model
    // reads its features by index. Appending is what keeps an older model's
    // indices meaning what they meant.
    x[..24].copy_from_slice(&v.block[..24]);
    if let Some(first) = v.lines.first() {
        x[24..47].copy_from_slice(first);
    }
    // WHAT THE LINE CLASS MODEL SAYS OF THE BLOCK'S LINES (#574, arm d): the
    // mean, the largest and the first line's `list` probability. Trained on
    // probabilities out of fold, so the model learnt how far to trust them on
    // a page the class model has not seen.
    if let Some(p) = classes.filter(|c| !c.is_empty()) {
        let list: Vec<f64> = p.iter().map(|c| c[2]).collect();
        x[47] = list.iter().sum::<f64>() / list.len() as f64;
        x[48] = list.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        x[49] = list[0];
    }
    probability(models::list_item::predict_margin(&x))
}

/// Every line's class probabilities on one page, in [`lines_of`] order block
/// after block, each in [`heading_lines::LINE_CLASSES`] order -- what the
/// dump carries as `line_class_probs` for the list model and the table judge
/// to read (#574). Computed on the blocks BEFORE the pass, the ones the line
/// model reads.
#[doc(hidden)]
pub fn line_class_probs(page: &[&Block], size: (f64, f64), facts: &DocumentFacts) -> Vec<[f32; 4]> {
    heading_lines::page_class_probabilities(&page_vectors(page, size, facts))
        .into_iter()
        .flatten()
        .map(|p| p.map(|v| v as f32))
        .collect()
}

/// How much the list models say each line of `page` belongs to a list item,
/// read before the learned pass has changed a block (#574).
///
/// FOR THE TABLE JUDGE, which chooses its regions on the page as the
/// heuristics left it -- before `list_item` has promoted a paragraph that
/// opens with a marker. Two raw marker features did not keep a bulleted list
/// set beside a table out of the region the judge drew over both; the list
/// models' own verdict is the signal the judge was missing (T4, #574).
///
/// One value per line, block by block, the lines in `lines::lines_of` order
/// -- the unit the table proposals and the line model share. A line of a
/// paragraph or a list item reads the larger of its block's `list_item`
/// probability and the largest `list_opens` probability at or above it in
/// the block: an item that opens on the third line of a paragraph makes the
/// third line and every line under it a list's. A line of any other kind is
/// NaN, because neither model has read it -- a table cell's zero would say
/// "not a list", which nobody measured.
pub fn list_signal(page: &[&Block], size: (f64, f64), facts: &DocumentFacts) -> Vec<f32> {
    let vectors = page_vectors(page, size, facts);
    let classes = heading_lines::page_class_probabilities(&vectors);
    list_signal_from_parts(&vectors, &classes)
}

/// [`list_signal`] over one page's vectors and line classes: the same signal,
/// without re-measuring the page for it.
fn list_signal_from_parts(vectors: &[BlockVectors], classes: &[Vec<[f64; 4]>]) -> Vec<f32> {
    let mut out = Vec::new();
    for (v, classes) in vectors.iter().zip(classes) {
        if !matches!(v.kind, "paragraph" | "list_item") {
            out.extend(std::iter::repeat_n(f32::NAN, v.lines.len()));
            continue;
        }
        let item = list_item_probability(v, Some(classes));
        let mut opened = 0.0f64;
        for line in &v.lines {
            opened = opened.max(probability(models::list_opens::predict_margin(line)));
            out.push(item.max(opened) as f32);
        }
    }
    out
}

/// The heading decision the pass took on one block, as the table pass reads
/// it.
fn hint_of(d: &Decision) -> tables::CellHint {
    tables::CellHint {
        heading: !d.heading_lines.is_empty() && d.heading_lines.iter().all(|h| *h),
        p_heading: (!d.p_heading_lines.is_empty())
            .then(|| d.p_heading_lines.iter().sum::<f64>() / d.p_heading_lines.len() as f64),
    }
}

/// The [`tables::CellHint`] every block of `blocks` carries into the table
/// pass, as shipped: the same decisions from the same blocks, for a dump that
/// trains on what the pass reads (T4, #574). `heading_probabilities` is the
/// line model alone; this is what decides -- with `line_gnn` on, the mean of
/// the network and `heading_lines` -- and the two are not the same number.
#[doc(hidden)]
pub fn cell_hints(
    blocks: &[Block],
    pages: &[(f64, f64)],
    images: &[Vec<[f64; 4]>],
) -> Vec<tables::CellHint> {
    let facts = document_facts(blocks);
    let mut out = vec![tables::CellHint::default(); blocks.len()];
    for (page, members) in pages_of(blocks).iter().enumerate() {
        let refs: Vec<&Block> = members.iter().map(|&i| &blocks[i]).collect();
        let size = pages.get(page).copied().unwrap_or((0.0, 0.0));
        let pictures = images.get(page).map(Vec::as_slice).unwrap_or(&[]);
        for (&i, d) in members
            .iter()
            .zip(decide_page(&refs, size, &facts, SWITCHES, pictures))
        {
            out[i] = hint_of(&d);
        }
    }
    out
}

/// What the learned pass doubted on one line: the probabilities behind the
/// heading decision it took, for the router that hands doubtful pages to the
/// teacher.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct LineConfidence {
    /// The heading probability the pass decided on -- with the graph network
    /// on, its mean with `heading_lines`' -- or `None` where no heading model
    /// read the line.
    pub p_heading: Option<f64>,
    /// The graph network's class probabilities, in
    /// `models::line_gnn::CLASSES` order; empty with the network off.
    pub class_probs: Vec<f32>,
    /// The network's link probability to the next line in reading order;
    /// NaN with the network off.
    pub p_link_next: f32,
}

/// What the learned pass doubted on one page: one doubt score, and the
/// heading probability per line to localise it.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct PageConfidence {
    /// Every line of the page, block by block in `lines::lines_of` order.
    pub lines: Vec<LineConfidence>,
    /// The page's doubt: the largest of the three parts below, in 0..=1 --
    /// with the graph network off, `heading_doubt`, the only part measured.
    pub doubt: f64,
    /// The share of decided lines whose `p_heading` falls within 0.15 of the
    /// threshold the pass decided them at.
    pub heading_doubt: f64,
    /// The share of stacked line pairs whose link probability falls in
    /// [0.35, 0.65]; NaN with the network off.
    pub link_doubt: f64,
    /// The mean normalised entropy of the network's class distributions;
    /// NaN with the network off.
    pub class_entropy: f64,
}

/// Every page's doubt, one `PageConfidence` per page, from the same decisions
/// `refine` acts on, with the switches as shipped.
///
/// PURELY PER PAGE: each entry reads its own page's blocks and the document's
/// facts, the way `refine_page` does, so the number belongs to what the pass
/// outputs. A page without lines reads doubt 0 and no lines. No new model:
/// every part is read off probabilities the pass already computes.
#[doc(hidden)]
pub fn page_confidence(
    blocks: &[Block],
    pages: &[(f64, f64)],
    images: &[Vec<[f64; 4]>],
) -> Vec<PageConfidence> {
    page_confidence_with_switches(blocks, pages, images, SWITCHES)
}

/// `page_confidence` with the switches given, so the doubt with the graph
/// network off can be asked for the way `refine_with` asks for the pass.
#[doc(hidden)]
pub fn page_confidence_with_switches(
    blocks: &[Block],
    pages: &[(f64, f64)],
    images: &[Vec<[f64; 4]>],
    switches: Switches,
) -> Vec<PageConfidence> {
    let facts = document_facts(blocks);
    let members = pages_of(blocks);
    let count = members.len().max(pages.len());
    let mut out = Vec::with_capacity(count);
    for number in 0..count {
        let on_page: Vec<&Block> = members
            .get(number)
            .map(|m| m.iter().map(|&i| &blocks[i]).collect())
            .unwrap_or_default();
        let size = pages.get(number).copied().unwrap_or((0.0, 0.0));
        let pictures = images.get(number).map(Vec::as_slice).unwrap_or(&[]);
        out.push(confidence_of_page(
            &on_page, size, &facts, switches, pictures,
        ));
    }
    out
}

/// One page's doubt from one snapshot of its blocks.
fn confidence_of_page(
    page: &[&Block],
    size: (f64, f64),
    facts: &DocumentFacts,
    switches: Switches,
    images: &[[f64; 4]],
) -> PageConfidence {
    let (decisions, vectors, gnn) = decide_page_traced(page, size, facts, switches, images);
    let mut lines = Vec::new();
    let mut near = 0usize;
    let mut decided = 0usize;
    let mut entropy_sum = 0.0;
    // Every line's box, to find the stacked pairs the link doubt counts.
    let mut boxes: Vec<[f64; 4]> = Vec::new();
    for (decision, v) in decisions.iter().zip(&vectors) {
        // THE BAR THE PASS DECIDED AT, from the same place it decided from.
        let bar = heading_bar(v.kind, switches);
        for k in 0..v.lines.len() {
            let p = decision.p_heading_lines.get(k).copied();
            if let Some(p) = p {
                decided += 1;
                if (p - bar).abs() <= 0.15 {
                    near += 1;
                }
            }
            let at = lines.len();
            if let Some(g) = &gnn {
                entropy_sum += line_gnn::normalised_entropy(&g.classes[at]);
            }
            lines.push(LineConfidence {
                p_heading: p,
                class_probs: gnn
                    .as_ref()
                    .map(|g| g.classes[at].clone())
                    .unwrap_or_default(),
                p_link_next: gnn.as_ref().map(|g| g.link_next[at]).unwrap_or(f32::NAN),
            });
            boxes.push(v.line_boxes[k].0);
        }
    }
    let heading_doubt = if decided == 0 {
        0.0
    } else {
        near as f64 / decided as f64
    };
    // THE STACKED PAIRS: two lines one above the other, the seams the pass
    // may cut (line_breaks) or join across (merge_items) on. Side by side is
    // one row, not a seam.
    let mut linked = 0usize;
    let mut stacked = 0usize;
    if let Some(g) = &gnn {
        for i in 0..lines.len().saturating_sub(1) {
            if boxes[i + 1][3] <= boxes[i][1] {
                stacked += 1;
                if (0.35..=0.65).contains(&f64::from(g.link_next[i])) {
                    linked += 1;
                }
            }
        }
    }
    // `gnn` is `Some` exactly when the network switch is on, so the data
    // carries the branch: NaN where nothing measured, 0 where nothing wavered.
    let (link_doubt, class_entropy, doubt) = match &gnn {
        None => (f64::NAN, f64::NAN, heading_doubt),
        Some(_) => {
            let link = if stacked == 0 {
                0.0
            } else {
                linked as f64 / stacked as f64
            };
            let entropy = if lines.is_empty() {
                0.0
            } else {
                entropy_sum / lines.len() as f64
            };
            (link, entropy, heading_doubt.max(link).max(entropy))
        }
    };
    PageConfidence {
        lines,
        doubt,
        heading_doubt,
        link_doubt,
        class_entropy,
    }
}

/// Every table the pass would emit on one page, with its final-table vector,
/// for the dump.
///
/// `page` holds the page's blocks as the heuristics left them -- the state
/// the judge was trained on -- `size` its dimensions, `facts` the
/// document's, `rules` and `rulings` what it drew, and `images` where it
/// drew its pictures. The tables are decoded with NEITHER v1 NOR v2
/// applied: v2 replaces v1, so training must see every table v2 will see,
/// including the ones v1 would have refused.
///
/// FROM THE SAME TRACED PASS THE TABLE SIGNALS USE: the judge runs once,
/// with its trace (stitch included), and the tables decode once from it --
/// the final tables are read off that one computation's outputs, never off
/// a second verdict beside it. Serving refuses from the same
/// [`tables::final_tables_for_is_table_v2`] computation, so there is no
/// train/serve skew in what is refused and what is trained on.
#[doc(hidden)]
pub fn final_tables_for_dump(
    page: Vec<Block>,
    size: (f64, f64),
    facts: &DocumentFacts,
    rules: &[crate::text::HorizontalRule],
    rulings: &[crate::text::Ruling],
    images: &[[f64; 4]],
) -> Vec<tables::FinalTableV2> {
    final_tables_for_dump_with(page, size, facts, rules, rulings, images, SWITCHES)
}

/// [`final_tables_for_dump`], with the switches given, so the stitch wiring
/// can be tested on and off: training reads the shipped switches, and the
/// final-table switch is forced on inside either way (v1 off, v2 not yet).
#[doc(hidden)]
pub fn final_tables_for_dump_with(
    page: Vec<Block>,
    size: (f64, f64),
    facts: &DocumentFacts,
    rules: &[crate::text::HorizontalRule],
    rulings: &[crate::text::Ruling],
    images: &[[f64; 4]],
    switches: Switches,
) -> Vec<tables::FinalTableV2> {
    // THE JUDGE WITH ITS TRACE, on the page as the heuristics left it.
    let (judged_before, decisions) =
        judge_and_decide_shared(&page, size, facts, switches, rules, images);
    let marked: Vec<bool> = page
        .iter()
        .map(|b| matches!(b.kind, BlockKind::ListItem { .. }) || leading_marker(&b.text).is_some())
        .collect();
    let (page, origin) = apply_traced(page, &decisions);
    let (page, origin) = if switches.line_gnn && switches.list_item {
        merge_items(
            page,
            origin,
            &decisions,
            &marked,
            judged_before.as_ref().map(|j| j.regions.as_slice()),
        )
    } else {
        (page, origin)
    };
    let judged: Option<Vec<Option<usize>>> = judged_before
        .as_ref()
        .map(|j| origin.iter().map(|&o| j.regions[o]).collect());
    let trace = judged_before.as_ref().and_then(|j| j.trace.clone());
    let meta = trace
        .as_ref()
        .map(tables::JudgeRegionMeta::of_trace)
        .unwrap_or_default();
    let builder_p: Vec<Option<f64>> = trace.as_ref().map_or_else(
        || vec![None; origin.len()],
        |t| {
            origin
                .iter()
                .map(|&o| t.builder_p.get(o).copied().flatten())
                .collect()
        },
    );
    let lattices = if switches.table_lattice {
        lattice::lattices(rulings)
    } else {
        Vec::new()
    };
    let hints: Vec<tables::CellHint> = origin.iter().map(|&o| hint_of(&decisions[o])).collect();
    // V1 OFF, V2 NOT YET: `tables::refine_traced` with the final-table switch
    // on skips v1, and the v2 refusal itself lives in `refine_page_traced`'s
    // post-pass, which this does not run -- so what comes back is every
    // decoded table, refused by nothing. The signals come back from the same
    // call and are dropped here: the dump trains on the tables, not the doubt.
    let switches = Switches {
        table_is_v2: true,
        ..switches
    };
    let drawn = !rules.is_empty() || !rulings.is_empty();
    let (out, mine, _) = tables::refine_traced(
        page,
        size,
        facts,
        switches,
        judged.as_deref(),
        trace.as_ref(),
        &lattices,
        &hints,
        drawn,
        // The dump trains on the grid's tables, not on TATR's: the opt-in
        // never reaches this path.
        None,
        // Frozen entry: the dump keeps the shipped cell words.
        true,
    );
    // THE RECORDS DESCRIBE THE TABLES AS EMITTED: the stacked split runs
    // after the judge on the serving path, so it runs here too -- one
    // record per part, the whole's features on each
    // (`tables::split_final_tables_stacked`).
    let finals = tables::final_tables_for_is_table_v2(
        &out,
        &mine,
        judged.as_deref(),
        &meta,
        &builder_p,
        &hints,
        rules,
        rulings,
        size.1,
    );
    if switches.table_split_stacked {
        tables::split_final_tables_stacked(out, finals).1
    } else {
        finals
    }
}

/// Every decision for every block, in the order of `blocks`: [`decide_page`]
/// page by page, under the document's facts.
fn decide(blocks: &[Block], pages: &[(f64, f64)], switches: Switches) -> Vec<Decision> {
    let facts = document_facts(blocks);
    let mut out = vec![Decision::default(); blocks.len()];
    for (page, members) in pages_of(blocks).iter().enumerate() {
        let refs: Vec<&Block> = members.iter().map(|&i| &blocks[i]).collect();
        let size = pages.get(page).copied().unwrap_or((0.0, 0.0));
        for (&i, d) in members
            .iter()
            .zip(decide_page(&refs, size, &facts, switches, &[]))
        {
            out[i] = d;
        }
    }
    out
}

/// A confirmed run of list items on one page, taken out whole or kept whole.
fn demote_confirmed_runs(
    members: &[usize],
    page: &[&Block],
    vectors: &[BlockVectors],
    decisions: &mut [Decision],
) {
    // A CONFIRMED LIST IS DEMOTED WHOLE OR NOT AT ALL. The heuristics confirm
    // an item only as part of a run of agreeing markers, and the run is the
    // statement: the model, reading one block at a time, is least sure of a
    // run's FIRST item, because what stands above it is a finished sentence
    // where DocLayNet's lists mostly follow a colon (0.054 against 0.459 for
    // the second item on the operator page). Taking the first item out and
    // leaving the rest would cut a list the page drew as one. So a run of
    // consecutive items on the page is taken out when the MEAN of its items is
    // below the bound, and kept whole otherwise. Measured on real runs per page
    // (validation documents, F1 of the item class): demoting item by item
    // 0.9063, a run whose mean is below 0.20 0.9018 and below 0.15 0.9002, a
    // run whose every item is low 0.9003, never demoting 0.8867; the heuristics
    // alone 0.8364. The bound is 0.15 and not 0.20 because numbered paragraphs
    // a blank line apart -- the shape #589 fixed, and one DocLayNet itself
    // calls a list -- score a mean of 0.198: a bound that close would flip
    // with the next retrain.
    let mut at = 0;
    while at < members.len() {
        if vectors[members[at]].kind != "list_item" {
            at += 1;
            continue;
        }
        let mut end = at;
        while end < members.len() && vectors[members[end]].kind == "list_item" {
            end += 1;
        }
        let run = &members[at..end];
        let scored: Vec<f64> = run.iter().filter_map(|&i| decisions[i].p_item).collect();
        let low = !scored.is_empty()
            && scored.iter().sum::<f64>() / (scored.len() as f64) < LIST_ITEM_DEMOTE_BELOW;
        // A RUN THAT COUNTS IS NOT DEMOTED. `1.` `2.` `3.` `4.` down one
        // column, each the next number, is the page saying "list" in the one
        // way that is not a matter of typography, and it is the shape the
        // model is worst at: numbered PARAGRAPHS, whose lines the line class
        // model reads as body text. With the line model's `list` probability
        // among its inputs (#574, arm d) such a run scored a mean of 0.046
        // on the operator page, where the shipped model read 0.198.
        let counts = run.len() >= 2
            && run
                .windows(2)
                .all(|w| match (&page[w[0]].kind, &page[w[1]].kind) {
                    (
                        BlockKind::ListItem {
                            ordinal: Some(a), ..
                        },
                        BlockKind::ListItem {
                            ordinal: Some(b), ..
                        },
                    ) => *b == a + 1,
                    _ => false,
                });
        // And of WORDS: `1. 4,512` `2. 5,118` counts too, and is a column of
        // figures the model is right to take out
        // (`a_numbered_column_of_figures_is_taken_out_of_the_list_whole`).
        let worded = run.iter().all(|&i| {
            page[i]
                .text
                .split_whitespace()
                .any(|w| w.chars().filter(|c| c.is_alphabetic()).count() >= 3)
        });
        let low = low && !(counts && worded);
        if low {
            for &i in run {
                if decisions[i].p_item.is_some() {
                    decisions[i].item = Some(false);
                }
            }
        }
        at = end;
    }
}

/// One page through the learned pass: `page` holds that page's blocks and
/// nothing else, in reading order, `size` is its width and height in points.
///
/// PURE. What it reads is its arguments -- the page's own blocks and the
/// document's [`DocumentFacts`], behind a shared reference -- and what it
/// changes is what it returns; there is no cache and no state between calls.
/// Pages can therefore be run in any order and at the same time, and
/// `learned_pages_run_in_parallel.rs` holds them to the serial result.
/// [`Block::index`] of what it returns counts within the page; the document's
/// numbering is [`refine_with`]'s.
pub fn refine_page(
    page: Vec<Block>,
    size: (f64, f64),
    facts: &DocumentFacts,
    switches: Switches,
) -> Vec<Block> {
    // Frozen entry: direct learned callers keep the shipped cell words.
    refine_page_traced(page, size, facts, switches, &[], &[], &[], None, true).0
}

/// [`refine_page`], and for every block it returns the position in `page` of
/// the block it came from.
/// The link probability at or above which a list item takes the block under
/// it in (#638). Measured on #632's proxy with the offline link model: 0.8
/// gained +0.012 per page, 0.7 +0.015, 0.6 +0.019, 0.5 the same as 0.6.
const LIST_MERGE_AT: f64 = 0.6;

/// A LIST ITEM THE SEGMENTATION CUT IN TWO, put back (#638, T5).
///
/// A wrapped item arrives as two blocks where the white between its lines
/// read as a paragraph break -- a reference set double-spaced, an item whose
/// second line the merge took for the next paragraph -- and the list model,
/// which decides block by block, cannot join them. The graph network's link
/// head reads the seam: where it says the item's last line and the next
/// block's first line are one region, the next block joins the item. Only a
/// block that opened with no marker of its own -- a marker is the page
/// saying "next item" -- and only when it is the first part of the input
/// block that followed. On #632's proxy the pieces were 220 against
/// pymupdf4llm's 101.
/// Whether `below` stands where an item's next line would: under it, a step
/// of no more than [`MERGE_MAX_GAP_EM`] from baseline to baseline, not past
/// its right, and no wider than the item ran -- and either hung under its
/// text, set in from the item's left edge by at least [`MERGE_MIN_HANG_EM`],
/// or flush with its marker, where a bibliography with no hang indent sets
/// the entry's second line.
/// The link head alone joined the paragraph after a list to its last item
/// (`a_short_item_is_not_wrapped_into_the_paragraph_after_it`), a line three
/// ems down (`white_wider_than_double_spacing_still_ends_an_item`) and the
/// top of the next column (`a_bibliography_keeps_its_columns`); an item's own
/// continuation hangs under its text, which none of those do. A flush line is
/// also where the paragraph after a list stands, so the flush case is asked
/// for the full measure where the hanging case is asked for room: the item's
/// own first line ran to the column's edge, and the line under it is no wider
/// than the item ran.
fn under_the_item(item: &Block, below: &Block) -> bool {
    let em = crate::blocks::dominant_font_size(item.spans.iter()).max(1.0);
    let [left, _, right, _] = item.bbox;
    let [b_left, _, b_right, _] = below.bbox;
    // Baselines, as `blocks` measures the step: the item's last line and the
    // next block's first.
    let last = item.spans.iter().map(|s| s.y).fold(f64::INFINITY, f64::min);
    let first = below
        .spans
        .iter()
        .map(|s| s.y)
        .fold(f64::NEG_INFINITY, f64::max);
    let step = last - first;
    // UNDER THE TEXT, NOT A FIXED STEP FROM THE MARKER. A `>` or a thin
    // bullet sets the text 0.6 em in, and a fixed 0.8 em refused the
    // continuations of NYSE_MS's items, which the link head read at 0.99.
    // The edge the item's words start at is measured from its glyphs.
    let hanging = b_left >= left + MERGE_MIN_HANG_EM * em
        || (b_left > left + 0.3 * em && (b_left - text_left(item, em)).abs() <= 0.35 * em);
    // FLUSH WITH THE MARKER, WHERE A BIBLIOGRAPHY WITHOUT A HANG SETS AN
    // ENTRY'S SECOND LINE -- and where the paragraph after a list stands too.
    // Only when the item's own first line ran the full measure: a short item
    // is finished, and the flush line under it is the list's next paragraph
    // (`a_short_item_does_not_take_a_flush_paragraph`).
    // Equal widths alone do not say that: `2. Pears` over `and quinces.` are
    // as wide as each other (`a_short_item_does_not_take_a_short_flush_line`).
    let flush = (b_left - left).abs() <= FLUSH_EM * em
        && right - left >= FLUSH_MIN_MEASURE_EM * em
        && b_right <= right + 0.5 * em;
    step > 0.0
        && step <= MERGE_MAX_GAP_EM * em
        && (hanging || flush)
        && b_left < right
        // THE ITEM RAN OUT OF ROOM. A line goes on into the next only when
        // it could not hold the next word: `2. Pears` over `Notwithstanding.`
        // is a short item and a paragraph after it, however well the second
        // sits under the first's text. Three ems of slack, not one: one em
        // gave back half of the gain on the rig-like proxy (lists +0.011
        // per page against +0.022), and the operator page holds at three.
        && right >= b_right - 3.0 * em
}

/// Where an item's words start on its first line: past its marker, from the
/// glyph boxes where the marker shares a span with the text (`•Nordstrom`),
/// from the next span where it is a span of its own.
fn text_left(item: &Block, em: f64) -> f64 {
    let top = item
        .spans
        .iter()
        .map(|s| s.y)
        .fold(f64::NEG_INFINITY, f64::max);
    let mut line: Vec<&crate::text::TextSpan> = item
        .spans
        .iter()
        .filter(|s| (s.y - top).abs() <= 0.5 * em && !s.text.trim().is_empty())
        .collect();
    line.sort_by(|a, b| a.x.total_cmp(&b.x));
    let Some(first) = line.first() else {
        return item.bbox[0];
    };
    let trimmed = first.text.trim_start();
    // A MARKER SET AS A SPAN OF ITS OWN -- `>` then the words -- has no text
    // after it for `leading_marker` to read; one or two glyphs with no letter
    // or digit in them are the marker, and the words start at the next span.
    let glyph = trimmed.trim_end();
    if !glyph.is_empty() && glyph.chars().count() <= 2 && !glyph.chars().any(char::is_alphanumeric)
    {
        return line.get(1).map_or(first.x, |s| s.x);
    }
    let lead = first.text.chars().count() - trimmed.chars().count();
    let Some((len, _)) = leading_marker(trimmed) else {
        return first.x;
    };
    let marker_chars = lead + trimmed[..len.min(trimmed.len())].chars().count();
    if marker_chars < first.text.chars().count() {
        first
            .char_bounds
            .get(marker_chars)
            .map_or(first.x, |b| b[0])
    } else {
        line.get(1).map_or(first.x, |s| s.x)
    }
}

/// The longest step, in ems, from an item's last baseline to the line that
/// goes on with it: `blocks::HANG_MAX_LEADING_EM`, the leading a hung
/// continuation is allowed.
const MERGE_MAX_GAP_EM: f64 = 2.2;
/// The least an item's continuation is set in from its marker, in ems:
/// `blocks::INDENT_MIN_EM`.
const MERGE_MIN_HANG_EM: f64 = 0.8;
/// How far, in ems, a continuation may stand from the item's own left edge
/// and still count as flush with its marker: the marker's edge and the
/// text's are the same column, and a third of an em is the jitter of the
/// extraction, not an indent.
const FLUSH_EM: f64 = 0.35;
/// The least width, in ems, of an item whose flush next line may join it: a
/// flush line is also where the paragraph after a list stands, and only an
/// item that ran a line's measure says the line under it is the entry going
/// on. A reference in a two-column paper runs about 22 ems, one across the
/// page 40 and more; a short item like `2. Pears from the orchard` runs 11.
const FLUSH_MIN_MEASURE_EM: f64 = 18.0;

fn merge_items(
    out: Vec<Block>,
    origin: Vec<usize>,
    decisions: &[Decision],
    marked: &[bool],
    judged: Option<&[Option<usize>]>,
) -> (Vec<Block>, Vec<usize>) {
    let mut blocks: Vec<Block> = Vec::with_capacity(out.len());
    let mut from: Vec<usize> = Vec::with_capacity(out.len());
    // The input block the last kept block's text reaches down to.
    let mut reach: Vec<usize> = Vec::with_capacity(out.len());
    for (block, o) in out.into_iter().zip(origin) {
        let joins = match (blocks.last(), reach.last()) {
            (Some(item), Some(&upto)) => {
                matches!(item.kind, BlockKind::ListItem { .. })
                    && matches!(
                        block.kind,
                        BlockKind::ListItem { .. } | BlockKind::Paragraph
                    )
                    && o == upto + 1
                    && !marked.get(o).copied().unwrap_or(true)
                    // A CHOSEN PARAGRAPH ONLY. The table pass keeps list items
                    // out of a judged region (`tables::judged_runs`), so holding
                    // an item back makes no table, only a loose item (#636).
                    && !(matches!(block.kind, BlockKind::Paragraph)
                        && judged.is_some_and(|j| {
                            j.get(o)
                                .copied()
                                .flatten()
                                .is_some_and(|k| k != tables::REJECTED)
                        }))
                    && item.page == block.page
                    && under_the_item(item, &block)
                    && decisions
                        .get(upto)
                        .and_then(|d| d.link_after)
                        .is_some_and(|p| p >= LIST_MERGE_AT)
            }
            _ => false,
        };
        if joins {
            let item = blocks.last_mut().expect("a block to join");
            item.text = crate::blocks::join_lines(&[item.text.clone(), block.text.clone()]);
            item.spans.extend(block.spans);
            crate::blocks::recompute_bbox(item);
            *reach.last_mut().expect("a reach") = o;
        } else {
            blocks.push(block);
            from.push(o);
            reach.push(o);
        }
    }
    (blocks, from)
}

#[allow(clippy::too_many_arguments)]
fn refine_page_traced(
    page: Vec<Block>,
    size: (f64, f64),
    facts: &DocumentFacts,
    switches: Switches,
    rules: &[crate::text::HorizontalRule],
    rulings: &[crate::text::Ruling],
    images: &[[f64; 4]],
    tatr: Option<&tables::TatrContext>,
    cell_path_words: bool,
) -> (Vec<Block>, Vec<usize>, Option<tables::PageTableSignals>) {
    // THE JUDGE READS THE PAGE AS THE HEURISTICS LEFT IT, because that is what
    // it was trained on (`feature_dump`, `refine: "off"`). Its regions are
    // carried through `apply_traced`'s origin map to the table pass, which
    // runs last. The judge and the decisions read one measurement of it (T1).
    let (judged, decisions) = judge_and_decide_shared(&page, size, facts, switches, rules, images);
    let marked: Vec<bool> = page
        .iter()
        .map(|b| matches!(b.kind, BlockKind::ListItem { .. }) || leading_marker(&b.text).is_some())
        .collect();
    let (page, origin) = apply_traced(page, &decisions);
    let (page, origin) = if switches.line_gnn && switches.list_item {
        merge_items(
            page,
            origin,
            &decisions,
            &marked,
            judged.as_ref().map(|j| j.regions.as_slice()),
        )
    } else {
        (page, origin)
    };

    // THE TABLE PASS RUNS LAST AND DOES NOT TRACE. Its action is a grid, not
    // a boundary or a class (see [`tables`]), so the blocks it returns are not
    // the blocks it was given and no block of its output came from one block
    // of its input -- a cell is made of words from several. `refine_with`
    // needs every returned block to name a slot it came from, so a re-gridded
    // region reports the position of its FIRST block: the grid stands where
    // the table started, which is where reading order wants it.
    if !switches.table_region
        && !switches.table_structure
        && judged.is_none()
        && !switches.table_lattice
    {
        return (page, origin, None);
    }
    let trace = judged.as_ref().and_then(|j| j.trace.clone());
    let judged: Option<Vec<Option<usize>>> =
        judged.map(|j| origin.iter().map(|&o| j.regions[o]).collect());
    let lattices = if switches.table_lattice {
        lattice::lattices(rulings)
    } else {
        Vec::new()
    };
    // THE HEADING DECISION ON EVERY CELL, carried to the table pass: a cell
    // the judge sends back to prose keeps what the line models said of it.
    let hints: Vec<tables::CellHint> = origin.iter().map(|&o| hint_of(&decisions[o])).collect();
    let drawn = !rules.is_empty() || !rulings.is_empty();
    let (out, mine, mut signals) = tables::refine_traced(
        page,
        size,
        facts,
        switches,
        judged.as_deref(),
        trace.as_ref(),
        &lattices,
        &hints,
        drawn,
        tatr,
        cell_path_words,
    );
    // THE FINAL-TABLE JUDGE READS WHAT WOULD BE EMITTED, after the cells are
    // decoded: a refused table goes back to prose, whole. Its provenance is
    // the same trace the signals use -- no second verdict -- and with the
    // switch off this runs nothing, so the off path is byte for byte what it
    // was. The verdicts stay beside the refusal for the split below.
    let (out, mine, verdicts) = if switches.table_is_v2 {
        let meta = trace
            .as_ref()
            .map(tables::JudgeRegionMeta::of_trace)
            .unwrap_or_default();
        let builder_p: Vec<Option<f64>> = trace.as_ref().map_or_else(
            || vec![None; hints.len()],
            |t| {
                origin
                    .iter()
                    .map(|&o| t.builder_p.get(o).copied().flatten())
                    .collect()
            },
        );
        let (out, mine, verdicts, map) = tables::apply_table_is_v2_traced(
            out,
            mine,
            judged.as_deref(),
            &meta,
            &builder_p,
            &hints,
            rules,
            rulings,
            size.1,
            switches,
        );
        if let Some(signals) = signals.as_mut() {
            // Through the rejoin first: joining shrinks the page, so the
            // signals' ranges remap to the survivors before the split below
            // divides them and the verdicts fill them.
            tables::remap_table_signal_ranges(signals, &map, &out);
        }
        (out, mine, Some(verdicts))
    } else {
        (out, mine, None)
    };
    // THE STACKED SPLIT RUNS AFTER THE JUDGE, on the tables v2 kept: a
    // refused table is prose by now and never splits, and a kept table that
    // splits keeps its verdict for both parts -- one verdict and one signal
    // per part, the same probability, the ranges remapped
    // (`tables::split_stacked_after_v2`). With the switch off the page is
    // what the judge left.
    //
    // THE SIGNALS CARRY ITS NUMBER: the verdicts beside the refusal fill each
    // signal's `p_is_table` -- the same number the refusal used, never
    // recomputed beside it -- so the router reads v2's scale with v2 on.
    // The fill runs after the split, so a part reads its own verdict.
    let (out, mine, verdicts, mut signals) = if switches.table_split_stacked {
        tables::split_stacked_after_v2(out, mine, verdicts, signals)
    } else {
        (out, mine, verdicts, signals)
    };
    if let (Some(signals), Some(verdicts)) = (signals.as_mut(), verdicts.as_ref()) {
        tables::fill_is_table_v2_signals(signals, verdicts);
    }
    // Composed, not replaced: `mine` points into the page the table pass was
    // given, and `origin` says where each of THOSE came from in the page this
    // function was given.
    let origin = mine.into_iter().map(|i| origin[i]).collect();
    (out, origin, signals)
}

/// [`refine_with_images`] with the switches given, so each can be tested off: the
/// document's facts once, then [`refine_page`] page by page, and every block
/// back where its page's pass left it IN THE DOCUMENT'S ORDER, which is not
/// page order -- the running heads of every page follow the flow of all of
/// them. A split block's parts take its place; the blocks are then numbered
/// again.
#[doc(hidden)]
pub fn refine_with(blocks: Vec<Block>, pages: &[(f64, f64)], switches: Switches) -> Vec<Block> {
    refine_with_rules(blocks, pages, &[], switches)
}

/// [`refine_with`], with the horizontal rules each page drew, in page order:
/// what the table judge's region detectors read (#574). A page past the end
/// of `rules` is read as drawing none.
#[doc(hidden)]
pub fn refine_with_rules(
    blocks: Vec<Block>,
    pages: &[(f64, f64)],
    rules: &[Vec<crate::text::HorizontalRule>],
    switches: Switches,
) -> Vec<Block> {
    refine_with_rulings(blocks, pages, rules, &[], switches)
}

/// [`refine_with_rules`], with each page's lattice rulings too: the thin
/// lines and rectangle edges the table lattice reads (#574).
#[doc(hidden)]
pub fn refine_with_rulings(
    blocks: Vec<Block>,
    pages: &[(f64, f64)],
    rules: &[Vec<crate::text::HorizontalRule>],
    rulings: &[Vec<crate::text::Ruling>],
    switches: Switches,
) -> Vec<Block> {
    refine_with_images(blocks, pages, rules, rulings, &[], switches)
}

/// [`refine_with_rulings`], with each page's image boxes too: where the page
/// draws its pictures, which the caption decision reads (#636).
#[doc(hidden)]
pub fn refine_with_images(
    blocks: Vec<Block>,
    pages: &[(f64, f64)],
    rules: &[Vec<crate::text::HorizontalRule>],
    rulings: &[Vec<crate::text::Ruling>],
    images: &[Vec<[f64; 4]>],
    switches: Switches,
) -> Vec<Block> {
    // Frozen entry: direct learned callers keep the shipped cell words.
    refine_with_tatr(blocks, pages, rules, rulings, images, switches, None, true)
}

/// [`refine_with_images`], with TATR table structure for the pass's regions
/// (track B, P3). `tatr` is the extraction's context; `None` rebuilds every
/// region from the grid, exactly as without the hook. `cell_path_words` is
/// [`ExtractOptions::cell_path_words`](crate::ExtractOptions::cell_path_words):
/// rebuilt cells read with the cell-path word rules where true.
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn refine_with_tatr(
    blocks: Vec<Block>,
    pages: &[(f64, f64)],
    rules: &[Vec<crate::text::HorizontalRule>],
    rulings: &[Vec<crate::text::Ruling>],
    images: &[Vec<[f64; 4]>],
    switches: Switches,
    tatr: Option<&tables::TatrContext>,
    cell_path_words: bool,
) -> Vec<Block> {
    let facts = document_facts(&blocks);
    let members = pages_of(&blocks);
    let mut slots: Vec<Option<Block>> = blocks.into_iter().map(Some).collect();
    let mut placed: Vec<Vec<Block>> = (0..slots.len()).map(|_| Vec::new()).collect();
    for (number, members) in members.iter().enumerate() {
        let page: Vec<Block> = members
            .iter()
            .map(|&i| slots[i].take().expect("a block is on one page"))
            .collect();
        let size = pages.get(number).copied().unwrap_or((0.0, 0.0));
        let drawn = rules.get(number).map(Vec::as_slice).unwrap_or(&[]);
        let lines = rulings.get(number).map(Vec::as_slice).unwrap_or(&[]);
        let pictures = images.get(number).map(Vec::as_slice).unwrap_or(&[]);
        let (out, origin, _) = refine_page_traced(
            page,
            size,
            &facts,
            switches,
            drawn,
            lines,
            pictures,
            tatr,
            cell_path_words,
        );
        for (block, from) in out.into_iter().zip(origin) {
            placed[members[from]].push(block);
        }
    }
    let mut out: Vec<Block> = placed.into_iter().flatten().collect();
    for (i, block) in out.iter_mut().enumerate() {
        block.index = i;
        // A BLOCK THAT IS NO LONGER A CELL CARRIES NO EVIDENCE ABOUT A TABLE
        // (codex on #1989). `by_heading_lines` can promote a cell to a heading and
        // the split paths can turn one into a list item, and either would have
        // kept the `GridDoubt` of the table the cell came from -- breaking the
        // field's documented contract and handing routing code a table's doubt as
        // if it were about a heading.
        //
        // Swept here rather than patched at those two sites on purpose: this
        // function is the one door every refined block leaves by, so a third path
        // added later cannot forget.
        if !matches!(block.kind, BlockKind::TableCell { .. }) {
            block.grid = None;
        }
    }
    out
}

/// One page's table signals, in page order: the same pass [`refine_with_images`]
/// runs, with the numbers beside the decision rather than the blocks.
#[doc(hidden)]
pub fn table_signals_with(
    blocks: &[Block],
    pages: &[(f64, f64)],
    rules: &[Vec<crate::text::HorizontalRule>],
    rulings: &[Vec<crate::text::Ruling>],
    images: &[Vec<[f64; 4]>],
    switches: Switches,
) -> Vec<Option<tables::PageTableSignals>> {
    let facts = document_facts(blocks);
    let members = pages_of(blocks);
    (0..pages.len())
        .map(|number| {
            let page: Vec<Block> = members
                .get(number)
                .map(|members| members.iter().map(|&i| blocks[i].clone()).collect())
                .unwrap_or_default();
            let size = pages.get(number).copied().unwrap_or((0.0, 0.0));
            let drawn = rules.get(number).map(Vec::as_slice).unwrap_or(&[]);
            let lines = rulings.get(number).map(Vec::as_slice).unwrap_or(&[]);
            let pictures = images.get(number).map(Vec::as_slice).unwrap_or(&[]);
            refine_page_traced(
                page, size, &facts, switches, drawn, lines, pictures, None, true,
            )
            .2
        })
        .collect()
}

/// One page's blocks and its table signals from one pass: the same
/// computation [`refine_page`] and [`table_signals_with`] run apart, so a
/// caller that wants both never runs the table pass twice.
#[doc(hidden)]
pub fn refine_page_with_signals(
    page: Vec<Block>,
    size: (f64, f64),
    facts: &DocumentFacts,
    switches: Switches,
    rules: &[crate::text::HorizontalRule],
    rulings: &[crate::text::Ruling],
    images: &[[f64; 4]],
) -> (Vec<Block>, Option<tables::PageTableSignals>) {
    let (blocks, _, signals) = refine_page_traced(
        page, size, facts, switches, rules, rulings, images, None, true,
    );
    (blocks, signals)
}

/// The blocks and the table signals from one pass: [`refine_with_images`]
/// and [`table_signals_with`] together, the table pass once per page rather
/// than once for each. The blocks are those [`refine_with_images`] returns;
/// the signals those [`table_signals_with`] returns.
#[doc(hidden)]
pub fn refine_with_images_and_signals(
    blocks: Vec<Block>,
    pages: &[(f64, f64)],
    rules: &[Vec<crate::text::HorizontalRule>],
    rulings: &[Vec<crate::text::Ruling>],
    images: &[Vec<[f64; 4]>],
    switches: Switches,
) -> (Vec<Block>, Vec<Option<tables::PageTableSignals>>) {
    refine_with_images_and_signals_with_cell_path_words(
        blocks, pages, rules, rulings, images, switches, true,
    )
}

/// [`refine_with_images_and_signals`] with the cell-path word rules where
/// `cell_path_words` is true. [`PdfDocument::extract_blocks_with_signals`]
/// reads this one with false, which is what that entry promises: the blocks
/// [`PdfDocument::extract_blocks`] returns with [`ExtractOptions::default`].
#[doc(hidden)]
pub fn refine_with_images_and_signals_with_cell_path_words(
    blocks: Vec<Block>,
    pages: &[(f64, f64)],
    rules: &[Vec<crate::text::HorizontalRule>],
    rulings: &[Vec<crate::text::Ruling>],
    images: &[Vec<[f64; 4]>],
    switches: Switches,
    cell_path_words: bool,
) -> (Vec<Block>, Vec<Option<tables::PageTableSignals>>) {
    let facts = document_facts(&blocks);
    let members = pages_of(&blocks);
    let mut slots: Vec<Option<Block>> = blocks.into_iter().map(Some).collect();
    let mut placed: Vec<Vec<Block>> = (0..slots.len()).map(|_| Vec::new()).collect();
    let mut signals: Vec<Option<tables::PageTableSignals>> = Vec::with_capacity(pages.len());
    for number in 0..members.len().max(pages.len()) {
        let page: Vec<Block> = members
            .get(number)
            .map(|members| {
                members
                    .iter()
                    .map(|&i| slots[i].take().expect("a block is on one page"))
                    .collect()
            })
            .unwrap_or_default();
        let size = pages.get(number).copied().unwrap_or((0.0, 0.0));
        let drawn = rules.get(number).map(Vec::as_slice).unwrap_or(&[]);
        let lines = rulings.get(number).map(Vec::as_slice).unwrap_or(&[]);
        let pictures = images.get(number).map(Vec::as_slice).unwrap_or(&[]);
        let (out, origin, page_signals) = refine_page_traced(
            page,
            size,
            &facts,
            switches,
            drawn,
            lines,
            pictures,
            None,
            cell_path_words,
        );
        if number < pages.len() {
            signals.push(page_signals);
        }
        if let Some(members) = members.get(number) {
            for (block, from) in out.into_iter().zip(origin) {
                placed[members[from]].push(block);
            }
        }
    }
    let mut out: Vec<Block> = placed.into_iter().flatten().collect();
    for (i, block) in out.iter_mut().enumerate() {
        block.index = i;
        if !matches!(block.kind, BlockKind::TableCell { .. }) {
            block.grid = None;
        }
    }
    (out, signals)
}

/// The blocks of the page `number`, in the order they came: what
/// [`refine_page`] is given.
#[doc(hidden)]
pub fn page_blocks(blocks: &[Block], number: usize) -> Vec<Block> {
    blocks
        .iter()
        .filter(|b| b.page == number)
        .cloned()
        .collect()
}

/// A list marker at the head of `text`: the marker's length in bytes (with the
/// white space after it) and the number it carries, if any.
fn leading_marker(text: &str) -> Option<(usize, Option<u32>)> {
    let trimmed = text.trim_start();
    let lead = text.len() - trimmed.len();
    let token = trimmed.split_whitespace().next()?;
    let bullet = token.chars().count() <= 2
        && token.chars().next().is_some_and(|c| {
            "\u{2022}\u{2023}\u{2043}\u{2219}\u{b7}\u{25aa}\u{25ab}\u{25a0}\u{25a1}\u{25cf}\u{25cb}\u{25e6}\u{25c6}\u{25c7}\u{2756}\u{2713}\u{2714}".contains(c)
        });
    // The dashes the line model reads as bullets (`lines::marker_kind`), and
    // the asterisk the block features do (`features::marker`), come off too: a
    // promoted `- item` kept its dash and came out as `- - item` (codex, #1964).
    let bullet = bullet || matches!(token, "-" | "\u{2013}" | "\u{2014}" | "*");
    // And the pointers and private-use pictures `crate::lists` reads as
    // bullets, glued to the word or set apart (#1973).
    let glued = token
        .chars()
        .next()
        .filter(|&c| crate::lists::is_a_pointer_or_a_picture(c));
    if let Some(c) = glued {
        let rest = &trimmed[c.len_utf8()..];
        if crate::lists::opens_like_an_item(c, rest) {
            let after = rest.len() - rest.trim_start().len();
            return Some((lead + c.len_utf8() + after, None));
        }
    }
    let digits: String = token
        .trim_start_matches(['(', '['])
        .trim_end_matches(['.', ')', ']'])
        .to_string();
    let ordered = digits != token && !digits.is_empty() && digits.chars().count() <= 3;
    if !bullet && !ordered {
        return None;
    }
    let rest = &trimmed[token.len()..];
    // A capital initial before another initial is the first author of a
    // reference, not a lettered item, as `crate::lists` reads it: taken off,
    // `S. G. Ovchinnikov` came out as item 19, `G. Ovchinnikov` (#599).
    if ordered
        && !token.starts_with(['(', '['])
        && digits.len() == 1
        && digits.chars().all(|c| c.is_ascii_uppercase())
        && crate::lists::opens_with_an_initial(rest)
    {
        return None;
    }
    let after = rest.len() - rest.trim_start().len();
    // A number, a single letter (its place in the alphabet, as the heuristics
    // read a bare letter), or a Roman numeral of two letters or more -- `iv.`
    // is 4 (codex, #1964). A single `i` stays a letter: alone, nothing says it
    // is Roman.
    let ordinal = if ordered {
        digits.parse::<u32>().ok().or_else(|| {
            let mut cs = digits.chars();
            match (cs.next(), cs.next()) {
                (Some(c), None) if c.is_ascii_alphabetic() => {
                    Some(u32::from(c.to_ascii_lowercase() as u8 - b'a' + 1))
                }
                _ => crate::lists::roman_value(&digits.to_ascii_lowercase()),
            }
        })
    } else {
        None
    };
    Some((lead + token.len() + after, ordinal))
}

/// A block's text rebuilt from its lines, as [`crate::blocks`] joins them.
fn text_of(lines: &[Vec<&features::PageWord>]) -> String {
    let texts: Vec<String> = lines
        .iter()
        .map(|l| {
            l.iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    crate::blocks::join_lines(&texts)
}

/// A part's text, assembled the way the block's own was: each line by
/// [`crate::text::TextBlock::text`], which settles the spacing between spans,
/// and the lines by [`crate::blocks::join_lines`]. Joining the part's words
/// with a space instead re-tokenised it -- a word drawn in two spans, half
/// bold, came back as `appl es` (codex, #1966).
fn text_of_spans(spans: &[crate::text::TextSpan]) -> String {
    let mut rows: Vec<Vec<crate::text::TextSpan>> = Vec::new();
    for span in spans {
        match rows.last_mut() {
            Some(row)
                if (row[0].y - span.y).abs() <= row[0].font_size.max(span.font_size) * 0.5 =>
            {
                row.push(span.clone())
            }
            _ => rows.push(vec![span.clone()]),
        }
    }
    let texts: Vec<String> = rows
        .into_iter()
        .map(|spans| crate::text::TextBlock { spans }.text())
        .collect();
    crate::blocks::join_lines(&texts)
}

fn bbox_of(spans: &[crate::text::TextSpan]) -> Option<[f64; 4]> {
    let mut out: Option<[f64; 4]> = None;
    for s in spans {
        let boxes: Vec<[f64; 4]> = if s.char_bounds.is_empty() {
            vec![[s.x, s.y, s.x + s.width, s.y + s.height]]
        } else {
            s.char_bounds.clone()
        };
        for b in boxes {
            out = Some(match out {
                None => b,
                Some(o) => [
                    o[0].min(b[0]),
                    o[1].min(b[1]),
                    o[2].max(b[2]),
                    o[3].max(b[3]),
                ],
            });
        }
    }
    out
}

/// A block as `heading_lines` read it: made a heading where every line is
/// one, made body text where none is, and cut where its lines change between
/// the two -- each heading part a heading, each other part what the block was,
/// or a paragraph where the block was a heading (#574, #632).
///
/// A TABLE CELL IS NOT CUT: a cell is a place in a grid, and half of one is
/// not. It is promoted whole or left as it is.
fn by_heading_lines(block: Block, decision: &Decision) -> Vec<Block> {
    let flags = &decision.heading_lines;
    let p = &decision.p_heading_lines;
    let mean = |from: usize, to: usize| -> Option<f64> {
        let part = p.get(from..to)?;
        (!part.is_empty()).then(|| part.iter().sum::<f64>() / part.len() as f64)
    };
    let body = |kind: BlockKind| match kind {
        BlockKind::Heading { .. } => BlockKind::Paragraph,
        other => other,
    };
    // A TABLE'S HEADER CELL IS THE TABLE'S. "Characteristic" over a column of
    // labels reads like a heading line by line, and promoting it cut the table
    // it heads (`a_column_of_labels_beside_a_bank_of_figures_is_one_table`).
    //
    // AND A CELL BELOW A TABLE'S FIRST ROW IS NOT A HEADING, the rule the
    // block model kept (aface5d19): a row label read as a section
    // title splits one table into two with a heading between.
    if matches!(
        block.kind,
        BlockKind::TableCell { header: true, .. } | BlockKind::TableCell { row: 1.., .. }
    ) {
        return vec![block];
    }
    let breaks = &decision.line_breaks;
    if flags.iter().all(|f| *f) && breaks.is_empty() {
        let mut block = block;
        if !matches!(block.kind, BlockKind::Heading { .. }) {
            block.kind = BlockKind::Heading { level: 1 };
            block.confidence = mean(0, p.len());
        }
        return vec![block];
    }
    if (!flags.iter().any(|f| *f) && breaks.is_empty())
        || matches!(block.kind, BlockKind::TableCell { .. })
    {
        let mut block = block;
        if matches!(block.kind, BlockKind::Heading { .. }) {
            block.kind = BlockKind::Paragraph;
            block.confidence = mean(0, p.len());
        }
        return vec![block];
    }
    // WHERE THE CLASS CHANGES, AND WHERE A REGION ENDS without a change of
    // class: two paragraphs the heuristics set as one are two (#638).
    let cuts: Vec<usize> = (1..flags.len())
        .filter(|&k| flags[k] != flags[k - 1] || breaks.contains(&k))
        .collect();
    let mut bounds = vec![0];
    bounds.extend(&cuts);
    bounds.push(flags.len());
    let parts = if every_part_has_text(&block, &cuts) {
        parts_of(&block, &cuts)
    } else {
        Vec::new()
    };
    // EVERY RUN ITS OWN PART, or the block stays whole: a span that crosses a
    // line boundary goes to its first line's part, which can leave a run with
    // no span, and then the parts no longer line up with the runs.
    if parts.len() != cuts.len() + 1 {
        return vec![block];
    }
    let kind = block.kind;
    parts
        .into_iter()
        .enumerate()
        .map(|(r, mut piece)| {
            piece.kind = if flags[bounds[r]] {
                BlockKind::Heading { level: 1 }
            } else {
                body(kind)
            };
            piece.confidence = mean(bounds[r], bounds[r + 1]);
            piece
        })
        .collect()
}

/// A paragraph as `caption_lines` read it: a caption where every line is one,
/// and cut where its lines change -- each caption part a caption, each other
/// part the paragraph it was (#636). A part that would hold no text keeps the
/// block whole, as a heading cut does.
fn by_caption_lines(block: Block, decision: &Decision) -> Vec<Block> {
    let flags = &decision.caption_lines;
    let p = &decision.p_caption_lines;
    let mean = |from: usize, to: usize| -> Option<f64> {
        let part = p.get(from..to)?;
        (!part.is_empty()).then(|| part.iter().sum::<f64>() / part.len() as f64)
    };
    if flags.iter().all(|f| *f) {
        let mut block = block;
        block.kind = BlockKind::Caption;
        block.confidence = mean(0, p.len());
        return vec![block];
    }
    let cuts: Vec<usize> = (1..flags.len())
        .filter(|&k| flags[k] != flags[k - 1])
        .collect();
    let mut bounds = vec![0];
    bounds.extend(&cuts);
    bounds.push(flags.len());
    let parts = if every_part_has_text(&block, &cuts) {
        parts_of(&block, &cuts)
    } else {
        Vec::new()
    };
    if parts.len() != cuts.len() + 1 {
        return vec![block];
    }
    parts
        .into_iter()
        .enumerate()
        .map(|(r, mut piece)| {
            if flags[bounds[r]] {
                piece.kind = BlockKind::Caption;
                piece.confidence = mean(bounds[r], bounds[r + 1]);
            }
            piece
        })
        .collect()
}

/// The graph network the switches choose.
fn gnn_of(switches: Switches) -> &'static line_gnn::Net {
    if switches.line_gnn_v2 {
        &line_gnn::V2
    } else {
        &line_gnn::V1
    }
}

/// The block made a list item: its marker off the text, its number kept.
fn as_item(mut block: Block, text: String) -> Block {
    let (strip, ordinal) = leading_marker(&text).unwrap_or((0, None));
    block.text = text[strip..].trim().to_string();
    block.kind = BlockKind::ListItem { level: 0, ordinal };
    block
}

/// How far apart two left edges may be and still be one indent step, in ems:
/// the tolerance `crate::blocks` groups list items by (`INDENT_MATCH_EM`).
const INDENT_MATCH_EM: f64 = 0.35;

fn size_of(block: &Block) -> f64 {
    let mut sizes: Vec<f64> = block.spans.iter().map(|s| s.font_size).collect();
    sizes.sort_by(|a, b| a.total_cmp(b));
    sizes
        .get(sizes.len() / 2)
        .copied()
        .filter(|s| *s > 0.0)
        .unwrap_or(10.0)
}

/// The level of an item the model promoted: the rank of its left edge among
/// the indent steps of the list items it stands among -- the items on its page
/// whose column it shares. The heuristics' own rule (`mark_column_items`):
/// the nearest step, never deeper than four. A promoted item used to be level
/// 0 wherever it stood, so a nested one was flattened into the outer list
/// (codex, #1964).
fn promoted_level(item: &Block, others: &[&Block]) -> u8 {
    let tolerance = INDENT_MATCH_EM * size_of(item);
    let same_column = |b: &Block| {
        let overlap = b.bbox[2].min(item.bbox[2]) - b.bbox[0].max(item.bbox[0]);
        b.page == item.page && overlap > 0.0
    };
    let mut edges: Vec<f64> = others
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::ListItem { .. }) && same_column(b))
        .map(|b| b.bbox[0])
        .chain(std::iter::once(item.bbox[0]))
        .collect();
    edges.sort_by(|a, b| a.total_cmp(b));
    let mut steps: Vec<f64> = Vec::new();
    for edge in edges {
        if steps.last().is_none_or(|last| edge - last > tolerance) {
            steps.push(edge);
        }
    }
    let edge = item.bbox[0];
    steps
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (edge - **a).abs().total_cmp(&(edge - **b).abs()))
        .map_or(0, |(rank, _)| rank)
        .min(crate::lists::MAX_LIST_LEVEL as usize) as u8
}

#[cfg(test)]
fn apply(blocks: Vec<Block>, decisions: &[Decision]) -> Vec<Block> {
    apply_traced(blocks, decisions).0
}

/// [`apply`], and for every block it returns the position in `blocks` of the
/// block it came from: a split block's parts all name the one block.
fn apply_traced(blocks: Vec<Block>, decisions: &[Decision]) -> (Vec<Block>, Vec<usize>) {
    let mut out: Vec<Block> = Vec::with_capacity(blocks.len());
    let mut origin: Vec<usize> = Vec::with_capacity(blocks.len());
    let mut promoted: Vec<usize> = Vec::new();
    for (k, (block, decision)) in blocks.into_iter().zip(decisions).enumerate() {
        // Whatever the arms below push, it came from block `k`.
        let before = out.len();
        let traced = |out: &Vec<Block>, origin: &mut Vec<usize>| origin.resize(out.len(), k);
        debug_assert_eq!(origin.len(), before);
        // THE HEADING LINES, when that model was asked, and again only where
        // the list boundary has nothing to say about the block.
        // A BLOCK THE MODEL LEAVES ALONE falls through to the arms below, so
        // the list model still decides it: a paragraph with no heading line
        // in it is not this model's to keep.
        let heading_lines_act = decision.heading_lines.iter().any(|h| *h)
            || !decision.line_breaks.is_empty()
            || (!decision.heading_lines.is_empty()
                && matches!(block.kind, BlockKind::Heading { .. }));
        if heading_lines_act && decision.opens_at.is_empty() {
            out.extend(by_heading_lines(block, decision));
            traced(&out, &mut origin);
            continue;
        }
        // A CAPTION, when that model found one in a paragraph, and again only
        // where no list boundary cuts the block.
        if decision.caption_lines.iter().any(|c| *c)
            && decision.opens_at.is_empty()
            && matches!(block.kind, BlockKind::Paragraph)
        {
            out.extend(by_caption_lines(block, decision));
            traced(&out, &mut origin);
            continue;
        }
        if decision.opens_at.is_empty() || !every_part_has_text(&block, &decision.opens_at) {
            // THE NUMBER BELONGS TO THE DECISION THAT PRODUCED THE KIND (#624,
            // codex on #1977). `p_item` is the item model's probability, so it is
            // reported on the two arms where that model changed the kind and
            // nowhere else. A block whose kind the heuristics decided carries
            // `None` even when the model happened to run on it, because a number
            // about a decision nobody acted on would read as a number about this
            // block's kind.
            match (decision.item, &block.kind) {
                (Some(true), BlockKind::Paragraph) => {
                    let text = block.text.clone();
                    let mut item = as_item(block, text);
                    item.confidence = decision.p_item;
                    promoted.push(out.len());
                    out.push(item);
                }
                (Some(false), BlockKind::ListItem { .. }) => {
                    let mut block = block;
                    block.text = with_marker_back(&block);
                    block.kind = BlockKind::Paragraph;
                    block.confidence = decision.p_item;
                    out.push(block);
                }
                _ => out.push(block),
            }
            traced(&out, &mut origin);
            continue;
        }
        // The items a split makes: a nested item's parts stay at its level,
        // and a paragraph's take the indent step they stand on, as a promoted
        // block does -- `as_item` alone would put every one at the margin
        // (codex, #1966).
        let nested = match block.kind {
            BlockKind::ListItem { level, .. } => Some(level),
            _ => None,
        };
        for mut piece in split(block, decision) {
            if let BlockKind::ListItem { ordinal, .. } = piece.kind {
                match nested {
                    Some(level) => piece.kind = BlockKind::ListItem { level, ordinal },
                    None => promoted.push(out.len()),
                }
            }
            out.push(piece);
        }
        traced(&out, &mut origin);
    }
    let levels: Vec<(usize, u8)> = promoted
        .iter()
        .map(|&i| {
            let others: Vec<&Block> = out
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, b)| b)
                .collect();
            (i, promoted_level(&out[i], &others))
        })
        .collect();
    for (i, level) in levels {
        if let BlockKind::ListItem { ordinal, .. } = out[i].kind {
            out[i].kind = BlockKind::ListItem { level, ordinal };
        }
    }
    for (i, block) in out.iter_mut().enumerate() {
        block.index = i;
    }
    (out, origin)
}

/// A demoted item's text: its own body text, with the marker the heuristics
/// took off put back in front.
///
/// The body text is kept as the heuristics joined it. Rebuilding it from the
/// words of each span re-tokenised it -- a word split across two spans, half
/// bold, came back as `appl es` -- and lost the spacing the block assembly had
/// already settled (codex, #1964). Only the marker is read off the words: the
/// first token of the block's first line, where it was drawn.
fn with_marker_back(block: &Block) -> String {
    let words = words_of_block(block);
    let lines = lines_of(0, &words);
    let Some(marker) = lines
        .first()
        .and_then(|l| l.first())
        .map(|w| w.text.as_str())
    else {
        return block.text.clone();
    };
    if block.text.starts_with(marker) {
        return block.text.clone();
    }
    format!("{marker} {}", block.text)
}

fn words_of_block(block: &Block) -> Vec<features::PageWord> {
    let mut words = Vec::new();
    for (s, span) in block.spans.iter().enumerate() {
        for w in features::words_of(span) {
            words.push(features::PageWord {
                text: w.text,
                bbox: w.bbox,
                block: 0,
                span: s,
                size: span.font_size,
                bold: span.is_bold,
                italic: span.is_italic,
                font: span.font_name.clone(),
                color: span.color.map(|[r, g, b, _]| [r, g, b]),
            });
        }
    }
    words
}

/// The block cut where a new item opens. The first part keeps the block's own
/// class unless the block model said otherwise; every later part opens an item.
/// Whether cutting `block` before the lines `opens_at` leaves text in every
/// part once its marker is gone. A column of bare bullets whose words sit in
/// another block -- `• • • •` -- opens at every line, and splitting it makes
/// items with no text at all: worse than the paragraph it was, so such a
/// block is left whole.
fn every_part_has_text(block: &Block, opens_at: &[usize]) -> bool {
    let words = words_of_block(block);
    let lines = lines_of(0, &words);
    let mut parts: Vec<Vec<Vec<&features::PageWord>>> = Vec::new();
    for (k, line) in lines.iter().enumerate() {
        if parts.is_empty() || opens_at.contains(&k) {
            parts.push(Vec::new());
        }
        if let Some(part) = parts.last_mut() {
            part.push(line.clone());
        }
    }
    parts.iter().all(|part| {
        let text = text_of(part);
        let strip = leading_marker(&text).map_or(0, |(strip, _)| strip);
        !text[strip..].trim().is_empty()
    })
}

/// A block cut at `cuts`, every part keeping the block's own kind.
///
/// `split` is the LIST boundary's: its parts after the first are list items by
/// construction, because that is what `list_opens` means. A heading boundary
/// means something else, so it divides the spans the same way and decides the
/// kinds itself.
fn parts_of(block: &Block, cuts: &[usize]) -> Vec<Block> {
    let words = words_of_block(block);
    let lines = lines_of(0, &words);
    let mut line_of_span: Vec<Option<usize>> = vec![None; block.spans.len()];
    for (k, line) in lines.iter().enumerate() {
        for w in line {
            if line_of_span[w.span].is_none_or(|l| k < l) {
                line_of_span[w.span] = Some(k);
            }
        }
    }
    let mut cuts: Vec<usize> = cuts.to_vec();
    cuts.sort_unstable();
    cuts.dedup();
    let part_of_line = |k: usize| cuts.iter().filter(|&&c| c <= k).count();
    let mut spans: Vec<Vec<crate::text::TextSpan>> = vec![Vec::new(); cuts.len() + 1];
    let mut last = 0usize;
    for (s, span) in block.spans.iter().enumerate() {
        let part = line_of_span[s].map_or(last, part_of_line);
        last = part;
        spans[part].push(span.clone());
    }
    let mut out = Vec::new();
    for (p, part_spans) in spans.into_iter().enumerate() {
        if part_spans.is_empty() {
            continue;
        }
        let mut piece = block.clone();
        piece.bbox = bbox_of(&part_spans).unwrap_or(block.bbox);
        piece.text = text_of_spans(&part_spans);
        piece.spans = part_spans;
        if p > 0 {
            piece.continues_previous = false;
        }
        out.push(piece);
    }
    out
}

fn split(block: Block, decision: &Decision) -> Vec<Block> {
    let words = words_of_block(&block);
    let lines = lines_of(0, &words);
    // Each span goes to the line its first word is on; a span without words
    // stays with the part before it.
    let mut line_of_span: Vec<Option<usize>> = vec![None; block.spans.len()];
    for (k, line) in lines.iter().enumerate() {
        for w in line {
            if line_of_span[w.span].is_none_or(|l| k < l) {
                line_of_span[w.span] = Some(k);
            }
        }
    }
    let mut cuts = decision.opens_at.clone();
    cuts.sort_unstable();
    cuts.dedup();
    let part_of_line = |k: usize| cuts.iter().filter(|&&c| c <= k).count();
    let parts = cuts.len() + 1;
    let mut spans: Vec<Vec<crate::text::TextSpan>> = vec![Vec::new(); parts];
    let mut last = 0usize;
    for (s, span) in block.spans.iter().enumerate() {
        let part = line_of_span[s].map_or(last, part_of_line);
        last = part;
        spans[part].push(span.clone());
    }
    let mut out = Vec::with_capacity(parts);
    for (p, part_spans) in spans.into_iter().enumerate() {
        if part_spans.is_empty() {
            continue;
        }
        let mut piece = block.clone();
        piece.bbox = bbox_of(&part_spans).unwrap_or(block.bbox);
        let text = text_of_spans(&part_spans);
        piece.spans = part_spans;
        // EVERY PIECE REPORTS THE PROBABILITY THAT MADE IT AN ITEM, and the pieces
        // of one split do not all come from the same decision (#624, codex on
        // #1977). The first part is an item because of `first_opens` or because of
        // the item model; every later part is an item because the OPENS model
        // fired on the line it starts at, which is a different model with a
        // different probability. Inheriting the block's `p_item` would have
        // published a number about a decision that did not produce that piece's
        // kind -- which is precisely the reading the field is documented against.
        piece.confidence = None;
        if p == 0 {
            piece.text = text;
            piece = match (decision.item, &block.kind) {
                _ if decision.first_opens => {
                    let t = piece.text.clone();
                    let mut item = as_item(piece, t);
                    item.confidence = decision.p_first_opens;
                    item
                }
                (Some(true), _) | (None, BlockKind::ListItem { .. }) => {
                    let t = piece.text.clone();
                    let mut item = as_item(piece, t);
                    item.confidence = decision.p_item;
                    item
                }
                _ => {
                    piece.kind = BlockKind::Paragraph;
                    piece
                }
            };
        } else {
            piece.continues_previous = false;
            piece = as_item(piece, text);
            // `cuts[p - 1]` is the line this part opens at, and `p_opens` is the
            // per-line probability from the SECOND line on -- so the line's own
            // number is at `cut - 1`. Absent rather than guessed if the vectors
            // ever disagree about length.
            piece.confidence = cuts
                .get(p - 1)
                .and_then(|cut| cut.checked_sub(1))
                .and_then(|at| decision.p_opens.get(at).copied());
        }
        out.push(piece);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::TextSpan;

    /// One line of text at height `y`, glyph boxes included, so `words_of`
    /// and `lines_of` see it the way they see a real page.
    fn line(text: &str, x: f64, y: f64) -> TextSpan {
        let glyphs = text
            .chars()
            .enumerate()
            .map(|(i, _)| {
                let x0 = x + i as f64 * 5.0;
                [x0, y, x0 + 5.0, y + 10.0]
            })
            .collect();
        TextSpan {
            text: text.to_string(),
            x,
            y,
            width: text.chars().count() as f64 * 5.0,
            height: 10.0,
            font_size: 10.0,
            char_bounds: glyphs,
            ..Default::default()
        }
    }

    /// A reference whose second line arrived as a block of its own is one item
    /// again where the link head reads the seam as inside it; the next
    /// reference, which opens with its marker, stays its own (#638, T5).
    #[test]
    fn a_list_item_cut_by_the_segmentation_is_put_back() {
        let block = |kind: BlockKind, text: &str, x: f64, y: f64| {
            let mut b = Block::new(kind, 0, [0.0; 4], text.to_string(), 0);
            b.spans = vec![line(text, x, y)];
            crate::blocks::recompute_bbox(&mut b);
            b
        };
        let item = BlockKind::ListItem {
            level: 0,
            ordinal: Some(5),
        };
        let out = vec![
            block(
                item,
                "S.-H. Park, O. Simeone, and S. Shamai, Robust compression for cloud radio access",
                72.0,
                700.0,
            ),
            block(
                BlockKind::Paragraph,
                "networks, IEEE Trans. Veh. Techn., vol. 62, pp. 692-703, 2013.",
                84.0,
                688.0,
            ),
            block(
                item,
                "J. Hoydis, M. Kobayashi, and M. Debbah, Optimal channel training",
                72.0,
                676.0,
            ),
        ];
        let decisions = |p: f64| {
            vec![
                Decision {
                    link_after: Some(p),
                    ..Decision::default()
                },
                Decision {
                    link_after: Some(0.9),
                    ..Decision::default()
                },
                Decision::default(),
            ]
        };
        // Input block 1 opened with no marker; 0 and 2 with one.
        let marked = [true, false, true];
        let (joined, origin) =
            merge_items(out.clone(), vec![0, 1, 2], &decisions(0.9), &marked, None);
        assert_eq!(
            joined.len(),
            2,
            "{:?}",
            joined.iter().map(|b| &b.text).collect::<Vec<_>>()
        );
        assert!(
            joined[0].text.ends_with("pp. 692-703, 2013."),
            "{}",
            joined[0].text
        );
        assert!(joined[1].text.starts_with("J. Hoydis"));
        assert_eq!(origin, vec![0, 2]);
        // Where the link head says the item ends there, the two stay apart.
        let (apart, _) = merge_items(out, vec![0, 1, 2], &decisions(0.3), &marked, None);
        assert_eq!(apart.len(), 3);
    }

    /// A thin bullet set as a span of its own puts the item's text 0.6 em in;
    /// the continuation under that text joins, measured from the glyphs and
    /// not from a fixed step off the marker (NYSE_MS on the rig-like proxy).
    #[test]
    fn a_continuation_under_a_thin_bullets_text_joins_its_item() {
        let mut item = Block::new(
            BlockKind::ListItem {
                level: 0,
                ordinal: None,
            },
            0,
            [0.0; 4],
            "We were pleased that our market share gains continued".to_string(),
            0,
        );
        item.spans = vec![
            line(">", 72.0, 700.0),
            line(
                "We were pleased that our market share gains continued",
                78.0,
                700.0,
            ),
        ];
        crate::blocks::recompute_bbox(&mut item);
        let mut next = Block::new(
            BlockKind::Paragraph,
            0,
            [0.0; 4],
            "in the equity and fixed income markets.".to_string(),
            0,
        );
        next.spans = vec![line("in the equity and fixed income markets.", 78.0, 688.0)];
        crate::blocks::recompute_bbox(&mut next);
        let decisions = vec![
            Decision {
                link_after: Some(0.99),
                ..Decision::default()
            },
            Decision::default(),
        ];
        let (joined, _) = merge_items(
            vec![item, next],
            vec![0, 1],
            &decisions,
            &[true, false],
            None,
        );
        assert_eq!(
            joined.len(),
            1,
            "{:?}",
            joined.iter().map(|b| &b.text).collect::<Vec<_>>()
        );
        assert!(joined[0].text.ends_with("fixed income markets."));
    }

    /// A bibliography entry with no hang indent: the first line ran the full
    /// measure and the second starts back at the marker's edge, so where the
    /// link head reads the seam as inside the entry the two are one item
    /// (#574, T5).
    #[test]
    fn a_flush_continuation_joins_its_item_where_the_link_head_says_so() {
        let block = |kind: BlockKind, text: &str, x: f64, y: f64| {
            let mut b = Block::new(kind, 0, [0.0; 4], text.to_string(), 0);
            b.spans = vec![line(text, x, y)];
            crate::blocks::recompute_bbox(&mut b);
            b
        };
        let item = block(
            BlockKind::ListItem {
                level: 0,
                ordinal: Some(9),
            },
            "Summermeyer, D et al: Chimeric antigen receptor-modified T cells derived from defined CD44",
            72.0,
            700.0,
        );
        let next = block(
            BlockKind::Paragraph,
            "and CD8+ subsets confer superior antitumor reactivity in vivo.",
            72.0,
            688.0,
        );
        let decisions = vec![
            Decision {
                link_after: Some(0.99),
                ..Decision::default()
            },
            Decision::default(),
        ];
        let (joined, _) = merge_items(
            vec![item, next],
            vec![0, 1],
            &decisions,
            &[true, false],
            None,
        );
        assert_eq!(
            joined.len(),
            1,
            "{:?}",
            joined.iter().map(|b| &b.text).collect::<Vec<_>>()
        );
        assert!(joined[0].text.ends_with("reactivity in vivo."));
    }

    /// A flush line under an item that stopped short of it is the paragraph
    /// after the list, however the link head reads the seam: the item ran out
    /// of no room, so the line under it is not the entry going on (#574, T5).
    /// Red with the flush width guards out.
    #[test]
    fn a_flush_paragraph_after_a_short_item_stays_its_own() {
        let block = |kind: BlockKind, text: &str, x: f64, y: f64| {
            let mut b = Block::new(kind, 0, [0.0; 4], text.to_string(), 0);
            b.spans = vec![line(text, x, y)];
            crate::blocks::recompute_bbox(&mut b);
            b
        };
        // One and a half ems wider than the item: inside the three the
        // hanging case allows, past the one the flush case does.
        let item = block(
            BlockKind::ListItem {
                level: 0,
                ordinal: Some(2),
            },
            "2. Pears from the orchard and the garden",
            72.0,
            700.0,
        );
        let next = block(
            BlockKind::Paragraph,
            "and quinces from the walled garden were in.",
            72.0,
            688.0,
        );
        let decisions = vec![
            Decision {
                link_after: Some(0.99),
                ..Decision::default()
            },
            Decision::default(),
        ];
        let (apart, _) = merge_items(
            vec![item, next],
            vec![0, 1],
            &decisions,
            &[true, false],
            None,
        );
        assert_eq!(
            apart.len(),
            2,
            "{:?}",
            apart.iter().map(|b| &b.text).collect::<Vec<_>>()
        );
    }

    /// A short item over a flush line no wider than itself: equal widths say
    /// nothing about room, so only the item's own measure keeps them apart
    /// (#574, T5). Red without `FLUSH_MIN_MEASURE_EM`.
    #[test]
    fn a_short_flush_line_under_a_short_item_stays_its_own() {
        let block = |kind: BlockKind, text: &str, x: f64, y: f64| {
            let mut b = Block::new(kind, 0, [0.0; 4], text.to_string(), 0);
            b.spans = vec![line(text, x, y)];
            crate::blocks::recompute_bbox(&mut b);
            b
        };
        let item = block(
            BlockKind::ListItem {
                level: 0,
                ordinal: Some(2),
            },
            "2. Pears from the orchard",
            72.0,
            700.0,
        );
        let next = block(BlockKind::Paragraph, "and quinces, in.", 72.0, 688.0);
        let decisions = vec![
            Decision {
                link_after: Some(0.99),
                ..Decision::default()
            },
            Decision::default(),
        ];
        let (kept, _) = merge_items(
            vec![item, next],
            vec![0, 1],
            &decisions,
            &[true, false],
            None,
        );
        assert_eq!(
            kept.len(),
            2,
            "{:?}",
            kept.iter().map(|b| &b.text).collect::<Vec<_>>()
        );
    }

    /// A block the table judge chose is not swallowed as a list item's
    /// continuation: the table takes priority over the list merge (#636).
    ///
    /// A list item followed by two paragraphs joins into one where no judge
    /// ran, as above; where the judge chose both paragraphs for a table, all
    /// three stay their own. Red without the judge in `merge_items`.
    #[test]
    fn a_block_the_judge_chose_is_not_swallowed_as_a_list_continuation() {
        let block = |kind: BlockKind, text: &str, x: f64, y: f64| {
            let mut b = Block::new(kind, 0, [0.0; 4], text.to_string(), 0);
            b.spans = vec![line(text, x, y)];
            crate::blocks::recompute_bbox(&mut b);
            b
        };
        let out = || {
            vec![
                block(
                    BlockKind::ListItem {
                        level: 0,
                        ordinal: Some(5),
                    },
                    "S.-H. Park, O. Simeone, and S. Shamai, Robust compression for cloud radio access",
                    72.0,
                    700.0,
                ),
                block(
                    BlockKind::Paragraph,
                    "networks, IEEE Trans. Veh. Techn., vol. 62, pp. 692-703, 2013.",
                    84.0,
                    688.0,
                ),
                block(
                    BlockKind::Paragraph,
                    "and further notes on the same entry in the bibliography.",
                    84.0,
                    676.0,
                ),
            ]
        };
        let decisions = vec![
            Decision {
                link_after: Some(0.9),
                ..Decision::default()
            },
            Decision {
                link_after: Some(0.9),
                ..Decision::default()
            },
            Decision::default(),
        ];
        let marked = [true, false, false];
        let (joined, _) = merge_items(out(), vec![0, 1, 2], &decisions, &marked, None);
        assert_eq!(
            joined.len(),
            1,
            "{:?}",
            joined.iter().map(|b| &b.text).collect::<Vec<_>>()
        );
        let judged = [None, Some(0), Some(0)];
        let (kept, origin) = merge_items(out(), vec![0, 1, 2], &decisions, &marked, Some(&judged));
        assert_eq!(
            kept.len(),
            3,
            "{:?}",
            kept.iter().map(|b| &b.text).collect::<Vec<_>>()
        );
        assert_eq!(origin, vec![0, 1, 2]);
    }

    /// A block the judge rejected still joins its item: only a chosen table
    /// blocks the list merge (#636). Red when `REJECTED` counts as chosen.
    #[test]
    fn a_block_the_judge_rejected_still_joins_its_item() {
        let block = |kind: BlockKind, text: &str, x: f64, y: f64| {
            let mut b = Block::new(kind, 0, [0.0; 4], text.to_string(), 0);
            b.spans = vec![line(text, x, y)];
            crate::blocks::recompute_bbox(&mut b);
            b
        };
        let out = vec![
            block(
                BlockKind::ListItem {
                    level: 0,
                    ordinal: Some(5),
                },
                "S.-H. Park, O. Simeone, and S. Shamai, Robust compression for cloud radio access",
                72.0,
                700.0,
            ),
            block(
                BlockKind::Paragraph,
                "networks, IEEE Trans. Veh. Techn., vol. 62, pp. 692-703, 2013.",
                84.0,
                688.0,
            ),
            block(
                BlockKind::Paragraph,
                "and further notes on the same entry in the bibliography.",
                84.0,
                676.0,
            ),
        ];
        let decisions = vec![
            Decision {
                link_after: Some(0.9),
                ..Decision::default()
            },
            Decision {
                link_after: Some(0.9),
                ..Decision::default()
            },
            Decision::default(),
        ];
        let judged = [
            None,
            Some(super::tables::REJECTED),
            Some(super::tables::REJECTED),
        ];
        let (joined, _) = merge_items(
            out,
            vec![0, 1, 2],
            &decisions,
            &[true, false, false],
            Some(&judged),
        );
        assert_eq!(
            joined.len(),
            1,
            "{:?}",
            joined.iter().map(|b| &b.text).collect::<Vec<_>>()
        );
    }

    /// A list item the judge chose still joins its item: the table pass keeps
    /// list items out of a chosen region (`tables::judged_runs`), so blocking
    /// the merge creates no table, only a loose item (#636). Red without the
    /// Paragraph guard on the judge in `merge_items`.
    #[test]
    fn a_list_item_the_judge_chose_still_joins_its_item() {
        let block = |kind: BlockKind, text: &str, x: f64, y: f64| {
            let mut b = Block::new(kind, 0, [0.0; 4], text.to_string(), 0);
            b.spans = vec![line(text, x, y)];
            crate::blocks::recompute_bbox(&mut b);
            b
        };
        // The next block is an item the list model made from a paragraph: it
        // opened with no marker of its own, so `marked` still reads false.
        let out = vec![
            block(
                BlockKind::ListItem {
                    level: 0,
                    ordinal: Some(5),
                },
                "S.-H. Park, O. Simeone, and S. Shamai, Robust compression for cloud radio access",
                72.0,
                700.0,
            ),
            block(
                BlockKind::ListItem {
                    level: 0,
                    ordinal: None,
                },
                "networks, IEEE Trans. Veh. Techn., vol. 62, pp. 692-703, 2013.",
                84.0,
                688.0,
            ),
        ];
        let decisions = vec![
            Decision {
                link_after: Some(0.9),
                ..Decision::default()
            },
            Decision::default(),
        ];
        let judged = [None, Some(0)];
        let (joined, _) = merge_items(out, vec![0, 1], &decisions, &[true, false], Some(&judged));
        assert_eq!(
            joined.len(),
            1,
            "{:?}",
            joined.iter().map(|b| &b.text).collect::<Vec<_>>()
        );
    }

    /// Every piece of a split reports the probability that made IT an item, not
    /// the block's (#624, codex on #1977).
    ///
    /// A block the opens model cuts in three becomes three items, and the reason
    /// each one is an item is a different line's probability. Inheriting the
    /// block's `p_item` published a number about the item model's decision on a
    /// piece whose kind the OPENS model produced -- a number that reads as being
    /// about this block's kind and is about something else. That is the one
    /// failure `confidence` cannot have, because the field's whole value is that
    /// a number means a model measured THIS.
    ///
    /// Red before the fix: every piece carried one and the same value.
    #[test]
    fn each_piece_of_a_split_reports_its_own_probability() {
        let decision = Decision {
            item: None,
            opens_at: vec![1, 2],
            first_opens: true,
            p_item: Some(0.11),
            p_opens: vec![0.71, 0.72],
            p_first_opens: Some(0.91),
            heading_lines: Vec::new(),
            p_heading_lines: Vec::new(),
            line_classes: Vec::new(),
            caption_lines: Vec::new(),
            p_caption_lines: Vec::new(),
            line_breaks: Vec::new(),
            link_after: None,
        };
        let spans = vec![
            line("first line", 72.0, 700.0),
            line("second line", 72.0, 686.0),
            line("third line", 72.0, 672.0),
        ];
        let pieces = split(
            block("first line second line third line", spans, 0),
            &decision,
        );
        assert_eq!(pieces.len(), 3, "three lines, three parts: {pieces:?}");
        let got: Vec<Option<f64>> = pieces.iter().map(|p| p.confidence).collect();
        assert_eq!(
            got,
            vec![Some(0.91), Some(0.71), Some(0.72)],
            "a piece reported a probability that belongs to another decision"
        );
        assert!(
            got.iter().all(|c| *c != Some(0.11)),
            "a piece inherited the block's p_item, which decided nothing about it"
        );
    }

    /// The line model cuts a block where its lines change between heading and
    /// body, and each part takes the kind its lines were given (#574).
    ///
    /// A heading glued to the paragraph under it is two runs of lines; the
    /// heading run becomes a heading and the rest stays what the block was.
    /// A block the model reads as all body, but which the heuristics called a
    /// heading, loses the heading; one read as all heading gains it.
    #[test]
    fn the_line_model_cuts_where_heading_lines_end_and_labels_each_part() {
        let spans = || {
            vec![
                line("Operating review", 72.0, 700.0),
                line("Sales rose in every region.", 72.0, 686.0),
                line("Margins held.", 72.0, 672.0),
            ]
        };
        let decision = |flags: Vec<bool>, p: Vec<f64>| Decision {
            heading_lines: flags,
            p_heading_lines: p,
            ..Decision::default()
        };
        let text = "Operating review Sales rose in every region. Margins held.";

        let parts = by_heading_lines(
            block(text, spans(), 0),
            &decision(vec![true, false, false], vec![0.9, 0.1, 0.2]),
        );
        assert_eq!(parts.len(), 2, "one cut, two parts: {parts:?}");
        assert!(matches!(parts[0].kind, BlockKind::Heading { .. }));
        assert_eq!(parts[0].text.trim(), "Operating review");
        assert!(matches!(parts[1].kind, BlockKind::Paragraph));
        assert_eq!(parts[0].confidence, Some(0.9));
        assert!(parts[1].confidence.is_some_and(|c| (c - 0.15).abs() < 1e-9));

        let mut heading = block(text, spans(), 0);
        heading.kind = BlockKind::Heading { level: 2 };
        let demoted = by_heading_lines(heading, &decision(vec![false; 3], vec![0.1; 3]));
        assert_eq!(demoted.len(), 1);
        assert!(matches!(demoted[0].kind, BlockKind::Paragraph));

        let promoted = by_heading_lines(
            block(text, spans(), 0),
            &decision(vec![true; 3], vec![0.8; 3]),
        );
        assert_eq!(promoted.len(), 1);
        assert!(matches!(promoted[0].kind, BlockKind::Heading { .. }));
    }

    fn block(text: &str, spans: Vec<TextSpan>, index: usize) -> Block {
        Block {
            kind: BlockKind::Paragraph,
            page: 0,
            bbox: bbox_of(&spans).unwrap_or([0.0; 4]),
            text: text.to_string(),
            spans,
            index,
            continues_previous: false,
            confidence: None,
            grid: None,
            table_origin: TableOrigin::Other,
            trace_learned: None,
        }
    }

    #[test]
    fn every_decision_is_read_from_one_snapshot() {
        // T3's first condition: the decisions come from the blocks as they
        // came in, whatever order the loop meets them in. Two pages, handed
        // over once in order and once with the pages the other way round:
        // every block has to get the same decision both times.
        let page = |p: usize, base: usize| {
            vec![
                Block {
                    page: p,
                    ..block(
                        "the following matters are to be reported on:",
                        vec![line(
                            "the following matters are to be reported on:",
                            72.0,
                            700.0,
                        )],
                        base,
                    )
                },
                Block {
                    page: p,
                    ..block(
                        "\u{2022} that the accounts be laid before the members.",
                        vec![line(
                            "\u{2022} that the accounts be laid before the members.",
                            72.0,
                            680.0,
                        )],
                        base + 1,
                    )
                },
                Block {
                    page: p,
                    ..block("1. 4,512", vec![line("1. 4,512", 480.0, 650.0)], base + 2)
                },
            ]
        };
        let pages = [(612.0, 792.0), (612.0, 792.0)];
        let forward: Vec<Block> = page(0, 0).into_iter().chain(page(1, 3)).collect();
        let backward: Vec<Block> = page(1, 3).into_iter().chain(page(0, 0)).collect();
        let a = decide(&forward, &pages, SWITCHES);
        let b = decide(&backward, &pages, SWITCHES);
        let by_index = |blocks: &[Block], d: Vec<Decision>| {
            let mut v: Vec<(usize, Decision)> = blocks.iter().map(|b| b.index).zip(d).collect();
            v.sort_by_key(|(i, _)| *i);
            v
        };
        assert_eq!(by_index(&forward, a), by_index(&backward, b));
    }

    #[test]
    fn a_split_block_takes_consecutive_places_and_everything_after_moves_down() {
        // T3's second condition: reading order was computed before this pass,
        // so the parts of a block stand where the block stood, in order, and
        // every later block's index moves down by the parts added.
        let blocks = vec![
            block("Before.", vec![line("Before.", 72.0, 700.0)], 0),
            block(
                "Apples are ripe. 2. Pears are not yet ripe.",
                vec![
                    line("Apples are ripe.", 72.0, 680.0),
                    line("2. Pears are not", 72.0, 668.0),
                    line("yet ripe.", 72.0, 656.0),
                ],
                1,
            ),
            block("After.", vec![line("After.", 72.0, 630.0)], 2),
        ];
        let decisions = vec![
            Decision::default(),
            Decision {
                opens_at: vec![1],
                ..Decision::default()
            },
            Decision::default(),
        ];
        let out = apply(blocks, &decisions);
        let shape: Vec<(usize, &str)> = out.iter().map(|b| (b.index, b.text.as_str())).collect();
        assert_eq!(
            shape,
            vec![
                (0, "Before."),
                (1, "Apples are ripe."),
                (2, "Pears are not yet ripe."),
                (3, "After."),
            ]
        );
        assert!(matches!(out[1].kind, BlockKind::Paragraph));
        assert!(matches!(
            out[2].kind,
            BlockKind::ListItem {
                ordinal: Some(2),
                ..
            }
        ));
        assert_eq!(out[1].spans.len(), 1);
        assert_eq!(out[2].spans.len(), 2);
        assert!(
            out[2].bbox[3] <= out[1].bbox[1],
            "the second part is below the first"
        );
    }

    #[test]
    fn a_demoted_item_keeps_its_body_text_and_gets_its_marker_back() {
        // Codex on #1964: a demoted item's text was rebuilt from the words of
        // each span, so a word split across two spans came back split. The
        // body text stays as it was; only the marker is put back.
        let spans = vec![
            line("1.", 72.0, 700.0),
            line("appl", 90.0, 700.0),
            line("es are ripe", 110.0, 700.0),
        ];
        let item = Block {
            kind: BlockKind::ListItem {
                level: 0,
                ordinal: Some(1),
            },
            ..block("apples are ripe", spans, 0)
        };
        let out = apply(
            vec![item],
            &[Decision {
                item: Some(false),
                ..Decision::default()
            }],
        );
        assert!(matches!(out[0].kind, BlockKind::Paragraph));
        assert_eq!(out[0].text, "1. apples are ripe");
    }

    #[test]
    fn a_promoted_item_takes_the_indent_step_it_stands_on() {
        // Codex on #1964: a promoted item was level 0 wherever it stood. Beside
        // a confirmed item at the margin, one set a full step in is level 1;
        // one at the margin is level 0.
        let at = |x: f64, kind: BlockKind, index: usize| Block {
            kind,
            ..block(
                "apples",
                vec![line("apples", x, 700.0 - index as f64 * 12.0)],
                index,
            )
        };
        let outer = at(
            72.0,
            BlockKind::ListItem {
                level: 0,
                ordinal: None,
            },
            0,
        );
        let paragraph = BlockKind::Paragraph;
        let blocks = vec![outer, at(100.0, paragraph, 1), at(72.0, paragraph, 2)];
        let decisions = vec![
            Decision::default(),
            Decision {
                item: Some(true),
                ..Decision::default()
            },
            Decision {
                item: Some(true),
                ..Decision::default()
            },
        ];
        let levels: Vec<u8> = apply(blocks, &decisions)
            .iter()
            .map(|b| match b.kind {
                BlockKind::ListItem { level, .. } => level,
                _ => u8::MAX,
            })
            .collect();
        assert_eq!(levels, vec![0, 1, 0]);
    }

    #[test]
    fn the_parts_of_a_split_item_keep_its_indent_step() {
        // Codex on #1966: every part of a split came out level 0. A nested
        // item's parts stay at its level; a paragraph's parts, set a full step
        // in beside an item at the margin, are level 1.
        let item = |x: f64, level: u8, index: usize, lines: &[&str]| Block {
            kind: BlockKind::ListItem {
                level,
                ordinal: None,
            },
            ..block(
                &lines.join(" "),
                lines
                    .iter()
                    .enumerate()
                    .map(|(k, t)| line(t, x, 700.0 - (index * 3 + k) as f64 * 12.0))
                    .collect(),
                index,
            )
        };
        let outer = item(72.0, 0, 0, &["apples"]);
        let nested = item(100.0, 2, 1, &["pears", "\u{2022} plums"]);
        let paragraph = Block {
            kind: BlockKind::Paragraph,
            ..item(100.0, 0, 2, &["\u{2022} figs", "\u{2022} dates"])
        };
        let split_here = Decision {
            opens_at: vec![1],
            first_opens: true,
            ..Decision::default()
        };
        let decisions = vec![Decision::default(), split_here.clone(), split_here];
        let levels: Vec<(String, u8)> = apply(vec![outer, nested, paragraph], &decisions)
            .iter()
            .map(|b| match b.kind {
                BlockKind::ListItem { level, .. } => (b.text.clone(), level),
                _ => (b.text.clone(), u8::MAX),
            })
            .collect();
        let want: Vec<(String, u8)> = [
            ("apples", 0),
            ("pears", 2),
            ("plums", 2),
            ("figs", 1),
            ("dates", 1),
        ]
        .iter()
        .map(|(t, l)| (t.to_string(), *l))
        .collect();
        assert_eq!(levels, want);
    }

    #[test]
    fn a_split_keeps_a_word_drawn_in_two_spans_whole() {
        // Codex on #1966: the parts of a split were rebuilt from their words,
        // so `apples` drawn as `appl` + `es` (half bold) became `appl es`.
        let mut tail = line("es are ripe.", 92.0, 680.0);
        tail.x = 92.0;
        let blocks = vec![block(
            "apples are ripe. 2. Pears are not",
            vec![
                line("appl", 72.0, 680.0),
                tail,
                line("2. Pears are not", 72.0, 668.0),
            ],
            0,
        )];
        let decisions = vec![Decision {
            opens_at: vec![1],
            ..Decision::default()
        }];
        let texts: Vec<String> = apply(blocks, &decisions)
            .into_iter()
            .map(|b| b.text)
            .collect();
        assert_eq!(texts, vec!["apples are ripe.", "Pears are not"]);
    }

    #[test]
    fn a_first_authors_initial_is_not_a_marker_to_take_off() {
        assert_eq!(leading_marker("S. G. Ovchinnikov and V. N. Zabluda"), None);
        assert_eq!(leading_marker("IV. Definitions"), Some((4, Some(4))));
        assert_eq!(leading_marker("B. Scope of the works"), Some((3, Some(2))));
        // A Roman numeral of two letters is no initial (codex, #1970).
        assert_eq!(leading_marker("II. A. Smith"), Some((4, Some(2))));
    }

    #[test]
    fn a_marker_comes_off_the_front_and_its_number_stays() {
        assert_eq!(leading_marker("3. Pears"), Some((3, Some(3))));
        assert_eq!(leading_marker("(c) Plums"), Some((4, Some(3))));
        assert_eq!(
            leading_marker("\u{2022} Apples"),
            Some(("\u{2022} ".len(), None))
        );
        assert_eq!(leading_marker("Apples and pears"), None);
        // The dashes the line model reads as bullets (codex, #1964).
        for dash in ["-", "\u{2013}", "\u{2014}"] {
            let text = format!("{dash} Pears");
            assert_eq!(
                leading_marker(&text),
                Some((dash.len() + 1, None)),
                "{text:?}"
            );
        }
        assert_eq!(
            leading_marker("-5.2 per cent"),
            None,
            "a negative figure is no dash"
        );
        // The asterisk the block features read as a bullet, and a Roman
        // numeral's value (codex, #1964).
        assert_eq!(leading_marker("* Pears"), Some((2, None)));
        // A pointer glued to its word comes off as a bullet does (#1973).
        assert_eq!(leading_marker(">Our market share"), Some((1, None)));
        assert_eq!(leading_marker("\u{f0b7} Pears"), Some((4, None)));
        assert_eq!(leading_marker("iv. Pears"), Some((4, Some(4))));
        assert_eq!(leading_marker("(xii) Pears"), Some((6, Some(12))));
        assert_eq!(
            leading_marker("i. Pears"),
            Some((3, Some(9))),
            "a lone `i` is a letter"
        );
    }

    #[test]
    fn the_image_arm_ships_off() {
        // The raster is weighed against its cost before it ships (#574). A
        // const assertion, as `line_gnn`'s shipped state is pinned: clippy
        // refuses a runtime assert on a constant.
        const { assert!(!SWITCHES.line_image) };
    }

    /// The stacked-figures default: C on, B on (#574).
    ///
    /// Each is measured apart before any of them ships; a flip without the
    /// measurement turns this red.
    #[test]
    fn the_stacked_figures_default_has_c_on_and_b_on() {
        // Const assertions, as `line_image`'s shipped state is pinned: clippy
        // refuses a runtime assert on a constant.
        const { assert!(SWITCHES.table_runs_on_figures) };
        const { assert!(SWITCHES.markdown_header_block_figures) };
    }

    /// The shipped stitch default: rows absorb on, header absorb on (#574).
    ///
    /// The numbers behind both are in the comment above the stitch switches
    /// in [`SWITCHES`]; a flip of either without updating them there turns
    /// this red.
    #[test]
    fn the_shipped_default_has_rows_on_and_header_on() {
        // Const assertions, as `line_image`'s shipped state is pinned: clippy
        // refuses a runtime assert on a constant.
        const { assert!(SWITCHES.table_stitch_rows) };
        const { assert!(SWITCHES.table_stitch_header) };
    }

    /// The shipped empty-column default: dropped (#574).
    ///
    /// The numbers behind it are in the comment above the stitch switches
    /// in [`SWITCHES`]; a flip without updating them there turns this red.
    #[test]
    fn the_shipped_default_drops_empty_columns() {
        // A const assertion, as the stitch default is pinned: clippy
        // refuses a runtime assert on a constant.
        const { assert!(SWITCHES.table_drop_empty_columns) };
    }

    /// The shipped default reads a cell line by line (#574).
    ///
    /// The numbers behind it are in the comment above the switch in
    /// [`SWITCHES`]; a flip without updating them there turns this red.
    #[test]
    fn the_shipped_default_reads_a_cell_line_by_line() {
        // A const assertion, as the stitch defaults are pinned: clippy
        // refuses a runtime assert on a constant.
        const { assert!(SWITCHES.table_cell_text_by_line) };
    }

    /// The shipped release default: rejoin on, column order off (#574).
    ///
    /// The numbers behind both are in the comment above the switches in
    /// [`SWITCHES`]; a flip without updating them there turns this red.
    #[test]
    fn the_shipped_default_rejoins_a_refusal_in_row_order() {
        // Const assertions, as the release defaults are pinned: clippy
        // refuses a runtime assert on a constant.
        const { assert!(SWITCHES.table_v2_release_rejoin) };
        const { assert!(!SWITCHES.table_v2_release_column_order) };
    }

    /// The shipped separation default: stacked split on, side by side off
    /// (#574).
    ///
    /// The numbers behind both are in the comment above the split switches
    /// in [`SWITCHES`]; a flip of either without updating them there turns
    /// this red.
    #[test]
    fn the_shipped_default_has_stacked_on_and_side_by_side_off() {
        // Const assertions, as `line_image`'s shipped state is pinned: clippy
        // refuses a runtime assert on a constant.
        const { assert!(SWITCHES.table_split_stacked) };
        const { assert!(!SWITCHES.table_split_side_by_side) };
    }

    /// The shipped corridor default: a joined stripe splits at white (#574).
    ///
    /// The numbers behind it are in the comment above the switch in
    /// [`SWITCHES`]; a flip without updating them there turns this red.
    #[test]
    fn the_shipped_default_splits_a_joined_stripe_at_white() {
        // A const assertion, as the corridor default is pinned: clippy
        // refuses a runtime assert on a constant.
        const { assert!(SWITCHES.table_corridor_split) };
    }

    /// With the retrained network on, the heading bar is 0.5: a line at 0.55
    /// is a heading there, where the old network's 0.6 calls it prose -- and
    /// the doubt around the retrained bar counts a line at 0.40 that the old
    /// window does not reach.
    #[test]
    fn the_retrained_bar_counts_a_line_at_0_55() {
        let v2 = Switches {
            line_gnn: true,
            line_gnn_v2: true,
            ..SWITCHES
        };
        let v1 = Switches {
            line_gnn: true,
            line_gnn_v2: false,
            ..SWITCHES
        };
        assert_eq!(heading_bar("paragraph", v2), 0.5);
        assert_eq!(heading_bar("paragraph", v1), 0.6);
        let p = 0.55;
        assert!(
            p >= heading_bar("paragraph", v2),
            "0.55 is a heading at the retrained bar"
        );
        assert!(
            p < heading_bar("paragraph", v1),
            "0.55 is prose at the old bar"
        );
        let q = 0.40;
        assert!(
            (q - heading_bar("paragraph", v2)).abs() <= 0.15,
            "0.40 counts in the doubt around 0.5"
        );
        assert!(
            (q - heading_bar("paragraph", v1)).abs() > 0.15,
            "0.40 is outside the doubt around 0.6"
        );
        // The keep bar and the line model without the network are untouched.
        assert_eq!(heading_bar("heading", v2), LINE_GNN_KEEP_HEADING);
        assert_eq!(heading_bar("heading", v1), LINE_GNN_KEEP_HEADING);
        let off = Switches {
            line_gnn: false,
            ..SWITCHES
        };
        assert_eq!(heading_bar("paragraph", off), HEADING_LINES_THRESHOLD);
    }

    /// `decide_page` and `confidence_of_page` read the same bar: every
    /// decided line's verdict from the pass equals `p >= heading_bar`, and
    /// the reported heading doubt is the share within 0.15 of it -- on one
    /// real page, with the switches as shipped.
    #[test]
    fn the_pass_and_its_doubt_agree_on_every_line_of_a_real_page() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/doclaynet/nyse_ssl_2003_p59.pdf"
        );
        let doc = crate::PdfDocument::open(std::fs::read(path).expect("fixture")).expect("open");
        let sizes: Vec<(f64, f64)> = (0..doc.page_count())
            .map(|p| {
                doc.page_geometry(p)
                    .map(|g| g.effective_dimensions())
                    .unwrap_or((0.0, 0.0))
            })
            .collect();
        let blocks = doc.extract_blocks_before_learning().expect("blocks");
        let facts = document_facts(&blocks);
        let mut lines_checked = 0usize;
        let mut verdict_split = 0usize;
        let mut doubt_split = 0usize;
        for (page_no, members) in pages_of(&blocks).iter().enumerate() {
            let page: Vec<&Block> = members.iter().map(|&i| &blocks[i]).collect();
            let size = sizes.get(page_no).copied().unwrap_or((0.0, 0.0));
            let (decisions, vectors, _) = decide_page_traced(&page, size, &facts, SWITCHES, &[]);
            let conf = confidence_of_page(&page, size, &facts, SWITCHES, &[]);
            let mut at = 0usize;
            let mut near = 0usize;
            let mut decided = 0usize;
            for (decision, v) in decisions.iter().zip(&vectors) {
                assert_eq!(
                    decision.heading_lines.len(),
                    decision.p_heading_lines.len(),
                    "a decided line carries both a verdict and its probability"
                );
                let bar = heading_bar(v.kind, SWITCHES);
                for k in 0..v.lines.len() {
                    let p = decision.p_heading_lines.get(k).copied();
                    assert_eq!(
                        p, conf.lines[at].p_heading,
                        "line {at}: the doubt reads another probability than the pass decided on"
                    );
                    if let Some(p) = p {
                        decided += 1;
                        assert_eq!(
                            decision.heading_lines[k],
                            p >= bar,
                            "line {at} at {p}: the pass and its doubt disagree"
                        );
                        if (p - bar).abs() <= 0.15 {
                            near += 1;
                        }
                        if v.kind != "heading" {
                            if (0.5..0.6).contains(&p) {
                                verdict_split += 1;
                            }
                            if ((p - 0.5).abs() <= 0.15) != ((p - 0.6).abs() <= 0.15) {
                                doubt_split += 1;
                            }
                        }
                    }
                    at += 1;
                }
            }
            assert_eq!(at, conf.lines.len(), "every line belongs to a block");
            let expected = if decided == 0 {
                0.0
            } else {
                near as f64 / decided as f64
            };
            assert!(
                (conf.heading_doubt - expected).abs() < 1e-12,
                "heading doubt {} is not the share around the pass's bar ({expected})",
                conf.heading_doubt
            );
            lines_checked += at;
        }
        assert!(lines_checked > 0, "the page has no lines to agree on");
        assert!(
            verdict_split > 0,
            "no line splits the old and the retrained verdict"
        );
        assert!(
            doubt_split > 0,
            "no line splits the doubt around 0.5 from the doubt around 0.6"
        );
    }

    /// The first page of a real document, with the page's size and the
    /// document's facts: what the speed round's counter tests measure.
    fn first_page_of(path: &str) -> (Vec<Block>, (f64, f64), DocumentFacts) {
        let doc = crate::PdfDocument::open(std::fs::read(path).expect("fixture")).expect("open");
        let size = doc
            .page_geometry(0)
            .map(|g| g.effective_dimensions())
            .unwrap_or((0.0, 0.0));
        let blocks = doc.extract_blocks_before_learning().expect("blocks");
        let facts = document_facts(&blocks);
        let page = page_blocks(&blocks, 0);
        assert!(!page.is_empty(), "the fixture's first page has no blocks");
        (page, size, facts)
    }

    /// The judge and the decisions apart measure the page three times: the
    /// judge's own features, the list signal's vectors and the decisions'
    /// vectors (T1). The legacy composition the shared pass replaces.
    #[test]
    fn the_judge_and_the_decisions_measure_a_page_three_times() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/doclaynet/nyse_ssl_2003_p59.pdf"
        );
        let (page, size, facts) = first_page_of(path);
        features::reset_page_features_calls();
        let _ = judge_and_decide_legacy(&page, size, &facts, SWITCHES, &[], &[]);
        assert_eq!(
            features::page_features_calls(),
            3,
            "the legacy composition no longer measures the page three times"
        );
    }

    /// The shared pass measures the page once: one features, read by the
    /// judge and the decisions together (T1). Undo the sharing and this
    /// reads three again.
    #[test]
    fn the_shared_pass_measures_a_page_once() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/doclaynet/nyse_ssl_2003_p59.pdf"
        );
        let (page, size, facts) = first_page_of(path);
        features::reset_page_features_calls();
        let _ = judge_and_decide_shared(&page, size, &facts, SWITCHES, &[], &[]);
        assert_eq!(
            features::page_features_calls(),
            1,
            "the shared pass measures the page more than once"
        );
    }

    /// The whole page pass measures twice: the shared measurement and the
    /// table tail's, on blocks the pass has changed (T1).
    #[test]
    fn refine_page_measures_twice() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/doclaynet/nyse_ssl_2003_p59.pdf"
        );
        let (page, size, facts) = first_page_of(path);
        features::reset_page_features_calls();
        let _ = refine_page(page, size, &facts, SWITCHES);
        assert_eq!(
            features::page_features_calls(),
            2,
            "the page pass no longer measures twice"
        );
    }

    /// The shared pass decides what the separate pass decided: the judge's
    /// regions and every decision, value for value, on real pages the models
    /// change (T1). The legacy composition is the frozen pre-change path;
    /// serving reads the shared one.
    #[test]
    fn the_shared_pass_decides_what_the_separate_pass_decided() {
        let pages = [
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/doclaynet/nasdaq_wwd_2002_p7.pdf"
            ),
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/doclaynet/nyse_ssl_2003_p59.pdf"
            ),
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/learned/fw7.pdf"
            ),
        ];
        let mut changed = 0usize;
        for path in pages {
            let (page, size, facts) = first_page_of(path);
            let shared = judge_and_decide_shared(&page, size, &facts, SWITCHES, &[], &[]);
            let legacy = judge_and_decide_legacy(&page, size, &facts, SWITCHES, &[], &[]);
            assert_eq!(shared.0, legacy.0, "{path}: the judge differs");
            assert_eq!(shared.1, legacy.1, "{path}: the decisions differ");
            let decided = legacy.1.iter().any(|d| d != &Decision::default())
                || legacy
                    .0
                    .as_ref()
                    .is_some_and(|j| j.regions.iter().any(Option::is_some));
            changed += usize::from(decided);
        }
        assert!(
            changed > 0,
            "the models change nothing on any page, so the identity tests nothing"
        );
    }
}
