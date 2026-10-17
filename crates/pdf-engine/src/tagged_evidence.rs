// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Kinds and reading order from a tagged PDF's own structure tree (#636).
//!
//! A tagged PDF carries what the geometry has to guess at: the author sat over
//! the document and said "this is a heading, this is a list, this cell spans
//! two columns". The tree says it through marked content IDs -- an `H1`
//! element owns MCID 0, and the glyphs drawn inside `/H1 <</MCID 0>> BDC ...
//! EMC` are the heading. [`apply`] reads that statement and, where it checks
//! out, lets it decide the blocks' kinds and order.
//!
//! WHERE IT SITS. Last, after the learned pass and the table repairs, and only
//! when [`ExtractOptions::use_tags`] is set. Everything before it is the
//! geometric answer, and this either replaces that answer wholesale or leaves
//! it byte for byte alone: there is no middle state where half the blocks are
//! tagged and the order is geometric.
//!
//! WHAT IT REFUSES. Tags never override geometry without the validity check,
//! because a tree can be stale (edited content, unedited tags), partial (one
//! chapter tagged), or corrupt. The tree is refused -- and the geometric
//! answer returned -- when it carries no placeable content, when most of its
//! elements carry none, when two elements claim one MCID, when most of its
//! references point at nothing drawn, when most mapped blocks would be
//! headings, when it covers less than half the text, or when its order
//! disagrees with the geometry's (Kendall tau under 0.2).
//!
//! HOW BLOCKS MEET MCIDS. The text device does not record MCIDs, so this runs
//! its own pass over the pages with a device that records nothing but glyph
//! positions and the marked-content sequences open around them. A glyph votes
//! for the span its origin falls in; a block belongs to the leaves its spans
//! vote for. Points, not boxes: a width estimate is one more thing to be
//! wrong about, and an origin inside a span's box is already the answer.
//!
//! [`ExtractOptions::use_tags`]: crate::blocks::ExtractOptions::use_tags
//! [`apply`]: apply

use std::collections::{HashMap, HashSet};

use pdf_compliance::tagged::{self, StructElement, StructureTree, StructureTreeDetails};
use pdf_render::pdf_interpret::font::Glyph;
use pdf_render::pdf_interpret::PageExt;
use pdf_render::pdf_interpret::{
    interpret_page, BlendMode, ClipPath, Context, Device, GlyphDrawMode, Image,
    InterpreterSettings, Paint, PathDrawMode, SoftMask,
};
use pdf_render::pdf_syntax::Pdf;

use crate::blocks::{recompute_bbox, Block, BlockKind};
use crate::lists;
use crate::text::TextBlock;

/// How far past a span's box a glyph's origin may fall and still count as the
/// span's, in points.
///
/// Three quarters of a point: rounding through a text matrix moves an origin
/// by far less, and a neighbouring span's glyphs sit a word space or more
/// away. Past this the glyph belongs to no span of the block -- descenders
/// and rotated runs -- and the block keeps its geometry.
const GLYPH_SLOP: f64 = 0.75;

/// The share of a block's glyphs above which `/Artifact` marked content owns
/// the block.
///
/// Strictly above half: an artifact sequence and a content sequence cannot
/// both own the glyphs, and the tie -- a block straddling the boundary -- is
/// content, the conservative direction for something about to leave the flow.
const ARTIFACT_SHARE: f64 = 0.5;

/// The share of a tree's elements that may carry no content before the tree
/// is refused. Half: a tree that says nothing about most of itself is stale
/// or decorative, and tags that describe a minority do not get to reorder
/// the majority.
const MAX_CONTENTLESS_SHARE: f64 = 0.5;

/// The share of a tree's MCID references that may point at nothing drawn
/// (or at no resolvable page) before the tree is refused. Half, for the
/// reason above: references the page never drew are the signature of tags
/// left behind by an edit.
const MAX_DANGLING_SHARE: f64 = 0.5;

/// The share of mapped blocks that may be headings before the tree is
/// refused. Half: a document that is mostly headings is a tree whose author
/// -- or generator -- used `H` where it meant `P`.
const MAX_HEADING_SHARE: f64 = 0.5;

/// The least share of the document's text the tree must account for. Half:
/// below it the tags describe the document less than the geometry does, and
/// the geometric answer stands.
const MIN_COVERAGE: f64 = 0.5;

/// The least Kendall tau between the tree's order and the geometric order.
/// 0.2: reordering is the feature -- a tree that merely fixes the order must
/// pass -- and what fails here is a tree that opposes the geometry, reversed
/// or scrambled. Pairs that tie (two blocks of one element) are not pairs.
const MIN_ORDER_TAU: f64 = 0.2;

/// How many untied block pairs the order check needs before it says
/// anything. Three: below that there is no order to disagree about.
const MIN_ORDER_PAIRS: u64 = 3;

/// One drawn glyph, placed: where its origin landed and which marked-content
/// sequences were open around it.
struct PlacedGlyph {
    x: f64,
    y: f64,
    mcid: Option<i32>,
    artifact: bool,
}

/// One open marked-content sequence.
struct OpenSequence {
    mcid: Option<i32>,
    artifact: bool,
}

/// The device that maps the pages for [`apply`]: every glyph's origin with
/// its innermost MCID and whether an `/Artifact` sequence was open.
///
/// Nothing else is recorded -- no text, no widths, no boxes. The text lives
/// in the blocks already extracted; this pass only says which MCID each bit
/// of ink belongs to.
struct McidDevice {
    stack: Vec<OpenSequence>,
    glyphs: Vec<PlacedGlyph>,
}

impl McidDevice {
    fn current_mcid(&self) -> Option<i32> {
        self.stack.iter().rev().filter_map(|s| s.mcid).next()
    }

    fn in_artifact(&self) -> bool {
        self.stack.iter().any(|s| s.artifact)
    }
}

impl Device<'_> for McidDevice {
    fn set_soft_mask(&mut self, _: Option<SoftMask<'_>>) {}
    fn set_blend_mode(&mut self, _: BlendMode) {}
    fn draw_path(&mut self, _: &kurbo::BezPath, _: kurbo::Affine, _: &Paint<'_>, _: &PathDrawMode) {
    }
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'_>>, _: BlendMode) {}
    fn draw_image(&mut self, _: Image<'_, '_>, _: kurbo::Affine) {}
    fn pop_clip_path(&mut self) {}
    fn pop_transparency_group(&mut self) {}

    fn draw_glyph(
        &mut self,
        _: &Glyph<'_>,
        transform: kurbo::Affine,
        glyph_transform: kurbo::Affine,
        _: &Paint<'_>,
        _: &GlyphDrawMode,
    ) {
        let coeffs = (transform * glyph_transform).as_coeffs();
        self.glyphs.push(PlacedGlyph {
            x: coeffs[4],
            y: coeffs[5],
            mcid: self.current_mcid(),
            artifact: self.in_artifact(),
        });
    }

    fn begin_marked_content(&mut self, tag: &[u8], mcid: Option<i32>, _: Option<&str>) {
        self.stack.push(OpenSequence {
            mcid,
            artifact: tag == b"Artifact",
        });
    }

    fn end_marked_content(&mut self) {
        // A content stream may close a sequence it never opened; popping an
        // empty stack is the only safe reading of that.
        self.stack.pop();
    }

    fn draw_placed_text(&mut self, _: &str, rect: kurbo::Rect) {
        // Text with no glyph drawn for it (a form value with no appearance
        // stream): its middle stands in for an origin.
        self.glyphs.push(PlacedGlyph {
            x: (rect.x0 + rect.x1) / 2.0,
            y: (rect.y0 + rect.y1) / 2.0,
            mcid: self.current_mcid(),
            artifact: self.in_artifact(),
        });
    }
}

/// Every page's glyphs, in page order.
fn glyphs_per_page(pdf: &Pdf, settings: &InterpreterSettings) -> Vec<Vec<PlacedGlyph>> {
    let pages = pdf.pages();
    let mut out = Vec::with_capacity(pages.len());
    for page in pages.iter() {
        let mut device = McidDevice {
            stack: Vec::new(),
            glyphs: Vec::new(),
        };
        let (w, h) = page.render_dimensions();
        let mut ctx = Context::new(
            page.initial_transform(false),
            kurbo::Rect::new(0.0, 0.0, f64::from(w), f64::from(h)),
            page.xref(),
            settings.clone(),
        );
        interpret_page(page, &mut ctx, &mut device);
        out.push(device.glyphs);
    }
    out
}

/// What one structure element says about the content it owns: the tag role
/// the block it maps to takes.
#[derive(Debug, Clone, Copy, PartialEq)]
enum TagRole {
    /// `H1`-`H6`, at the tree's own level 1-6; `H` without a number is 1.
    /// Folded to the block model's 1-3 on application.
    Heading(u8),
    /// `LI` (or its `Lbl`/`LBody`), at nesting depth 0 for the outermost.
    Item(u8),
    /// `TD`/`TH`.
    Cell(CellRole),
    /// Anything mapped that is none of the above: `P`, `Span`, `Caption`,
    /// content sitting directly under a container.
    Paragraph,
}

/// A table cell's place, from the tree: which `TR` in which `Table`, which
/// cell in the row, the `/A` spans, and whether it heads.
#[derive(Debug, Clone, Copy, PartialEq)]
struct CellRole {
    row: u16,
    column: u16,
    row_span: u16,
    column_span: u16,
    header: bool,
}

/// One content-bearing element: its role, its place in the reading order,
/// and the `(page, MCID)` references it owns.
struct Leaf {
    role: TagRole,
    rank: usize,
    refs: Vec<(usize, i32)>,
}

/// The grid a `Table`'s rows are counted on while its subtree is walked.
///
/// `occupied[c]` is how many rows, counting the current one, column `c` is
/// still covered by a spanning cell placed above: a `RowSpan 2` cell placed
/// in row 2 holds its column through row 3, so the next row's first cell
/// starts one column over. A new row consumes one row of every span.
#[derive(Clone, Default)]
struct TableState {
    next_row: u16,
    row: u16,
    occupied: Vec<u16>,
}

/// What the walk carries down the tree.
struct WalkState {
    /// Next reading-order position: pre-order, so it matches
    /// [`StructureTree::reading_order`].
    rank: usize,
    /// `L` ancestors entered.
    list_depth: u8,
    /// The nearest role-bearing ancestor's role, for content sitting under
    /// an untyped carrier (`Lbl` under its `LI`, text under a `Div`).
    inherited: Option<TagRole>,
    /// The innermost open table, if any.
    table: Option<TableState>,
    /// Whether inside a `THead`: a `TD` there heads as much as a `TH`.
    thead: bool,
}

/// Everything the walk learned about the tree.
struct TreeFacts {
    leaves: Vec<Leaf>,
    /// MCID references whose page could not be resolved.
    unplaced: usize,
    /// Leaf elements (no children) with neither MCIDs nor actual text.
    contentless: usize,
    /// All elements.
    total: usize,
}

/// Walk the tree into [`TreeFacts`]: one [`Leaf`] per element that owns
/// content references, with the role its type (or its nearest typed
/// ancestor) implies.
fn collect_leaves(tree: &StructureTree, details: &StructureTreeDetails) -> TreeFacts {
    let mut facts = TreeFacts {
        leaves: Vec::new(),
        unplaced: 0,
        contentless: 0,
        total: 0,
    };
    let mut state = WalkState {
        rank: 0,
        list_depth: 0,
        inherited: None,
        table: None,
        thead: false,
    };
    let mut path = Vec::new();
    for (index, root) in tree.root_elements.iter().enumerate() {
        path.push(index);
        visit(root, &mut path, details, &mut state, &mut facts);
        path.pop();
    }
    facts
}

fn visit(
    element: &StructElement,
    path: &mut Vec<usize>,
    details: &StructureTreeDetails,
    state: &mut WalkState,
    facts: &mut TreeFacts,
) {
    facts.total += 1;
    let rank = state.rank;
    state.rank += 1;
    let has_text = element
        .actual_text
        .as_ref()
        .is_some_and(|text| !text.is_empty());
    if element.children.is_empty() && element.mcids.is_empty() && !has_text {
        facts.contentless += 1;
    }

    // Scoped state: saved here, restored after the subtree.
    let saved_depth = state.list_depth;
    let saved_inherited = state.inherited;
    let saved_table = state.table.clone();
    let saved_thead = state.thead;

    match element.standard_type.as_str() {
        "L" => state.list_depth = state.list_depth.saturating_add(1),
        "Table" => {
            state.table = Some(TableState::default());
            state.thead = false;
        }
        "TR" => {
            if let Some(table) = state.table.as_mut() {
                table.row = table.next_row;
                table.next_row = table.next_row.saturating_add(1);
                for occupied in table.occupied.iter_mut() {
                    *occupied = occupied.saturating_sub(1);
                }
            }
        }
        "THead" => state.thead = true,
        _ => {}
    }

    let own = match element.standard_type.as_str() {
        "H1" => Some(TagRole::Heading(1)),
        "H2" => Some(TagRole::Heading(2)),
        "H3" => Some(TagRole::Heading(3)),
        "H4" => Some(TagRole::Heading(4)),
        "H5" => Some(TagRole::Heading(5)),
        "H6" => Some(TagRole::Heading(6)),
        // `H` with no number: the document said "heading" with no demotion.
        "H" => Some(TagRole::Heading(1)),
        "LI" | "Lbl" | "LBody" => Some(TagRole::Item(
            state
                .list_depth
                .saturating_sub(1)
                .min(lists::MAX_LIST_LEVEL),
        )),
        "TD" | "TH" => {
            let (row_span, col_span) = details.spans(path);
            let (row, column) = match state.table.as_mut() {
                Some(table) => {
                    // The first column no span from above still holds.
                    let mut column = 0usize;
                    while table.occupied.get(column).copied().unwrap_or(0) != 0 {
                        column += 1;
                    }
                    let end = column.saturating_add(col_span.max(1) as usize);
                    if table.occupied.len() < end {
                        table.occupied.resize(end, 0);
                    }
                    for occupied in table.occupied[column..end].iter_mut() {
                        *occupied = (*occupied).max(row_span.max(1));
                    }
                    (table.row, column.min(u16::MAX as usize) as u16)
                }
                None => (0, 0),
            };
            let header = element.standard_type == "TH" || state.thead;
            Some(TagRole::Cell(CellRole {
                row,
                column,
                row_span,
                column_span: col_span,
                header,
            }))
        }
        _ => None,
    };
    if own.is_some() {
        state.inherited = own;
    }

    let mut refs = Vec::with_capacity(element.mcids.len());
    for (i, mcid) in element.mcids.iter().enumerate() {
        let page = details.mcid_page(path, i).or(element.page_index);
        match page {
            Some(page) => refs.push((page, *mcid)),
            None => facts.unplaced += 1,
        }
    }
    if !refs.is_empty() {
        facts.leaves.push(Leaf {
            role: state.inherited.unwrap_or(TagRole::Paragraph),
            rank,
            refs,
        });
    }

    for (index, child) in element.children.iter().enumerate() {
        path.push(index);
        visit(child, path, details, state, facts);
        path.pop();
    }

    state.list_depth = saved_depth;
    state.inherited = saved_inherited;
    // Only a `Table` scopes the table state: rows and columns are progress
    // shared by sibling `TR`s and `TD`s, and restoring them after each one
    // would count every row 0 and every column 0.
    if element.standard_type == "Table" {
        state.table = saved_table;
    }
    state.thead = saved_thead;
}

/// How one block met the tree: the leaves its glyphs vote for, the majority
/// leaf of each span (for splitting), and the artifact share.
struct BlockMap {
    /// `(leaf, glyphs)` pairs with at least one vote, most votes first.
    votes: Vec<(usize, usize)>,
    /// Majority leaf per span, in span order; `None` where no MCID won.
    span_leaf: Vec<Option<usize>>,
    /// Glyphs drawn inside `/Artifact` marked content.
    artifact: usize,
    /// Glyphs inside the block's box.
    total: usize,
}

impl BlockMap {
    /// The leaf the block belongs to: most votes, if any.
    fn primary(&self) -> Option<usize> {
        self.votes.first().map(|pair| pair.0)
    }

    /// Whether the tree says anything about the block at all: a voted leaf,
    /// or artifact content (which the leaves never own).
    fn mapped(&self) -> bool {
        !self.votes.is_empty() || self.is_artifact()
    }

    /// Whether `/Artifact` content owns the block: strictly more than half
    /// its glyphs. Needs at least one glyph, so an empty block is never
    /// furniture by division.
    fn is_artifact(&self) -> bool {
        self.total > 0 && (self.artifact as f64) > ARTIFACT_SHARE * (self.total as f64)
    }
}

fn inside_expanded(bbox: &[f64; 4], x: f64, y: f64) -> bool {
    x >= bbox[0] - GLYPH_SLOP
        && x <= bbox[2] + GLYPH_SLOP
        && y >= bbox[1] - GLYPH_SLOP
        && y <= bbox[3] + GLYPH_SLOP
}

/// Map every block to the tree through its glyphs' MCIDs.
///
/// Returns the per-block maps and the `(page, MCID)` pairs two leaves claim
/// -- the impossible overlaps -- as `(key, first leaf, second leaf)`.
#[allow(clippy::type_complexity)]
fn map_blocks(
    blocks: &[Block],
    leaves: &[Leaf],
    glyphs: &[Vec<PlacedGlyph>],
) -> (Vec<BlockMap>, Vec<((usize, i32), usize, usize)>) {
    let mut owner: HashMap<(usize, i32), usize> = HashMap::new();
    let mut overlaps: Vec<((usize, i32), usize, usize)> = Vec::new();
    for (index, leaf) in leaves.iter().enumerate() {
        // One element listing one MCID twice is sloppy, not impossible.
        let mut seen = HashSet::new();
        for key in &leaf.refs {
            if !seen.insert(*key) {
                continue;
            }
            if let Some(previous) = owner.insert(*key, index) {
                overlaps.push((*key, previous, index));
            }
        }
    }

    let mut maps = Vec::with_capacity(blocks.len());
    for block in blocks {
        let mut votes: HashMap<usize, usize> = HashMap::new();
        let mut span_votes: Vec<HashMap<usize, usize>> =
            (0..block.spans.len()).map(|_| HashMap::new()).collect();
        let mut artifact = 0usize;
        let mut total = 0usize;
        if let Some(page_glyphs) = glyphs.get(block.page) {
            // The block's box first: most glyphs of a page are not this
            // block's, and one box test each is the whole of the cost.
            for glyph in page_glyphs
                .iter()
                .filter(|g| inside_expanded(&block.bbox, g.x, g.y))
            {
                total += 1;
                if glyph.artifact {
                    artifact += 1;
                }
                let Some(mcid) = glyph.mcid else { continue };
                let Some(leaf) = owner.get(&(block.page, mcid)) else {
                    continue;
                };
                // The vote lands on a span, not on the block: a geometric
                // block can straddle two cells, and the split needs to know
                // where. A glyph in the box but in no span (a word gap,
                // rounding) votes for nothing.
                let at = block.spans.iter().position(|span| {
                    inside_expanded(
                        &[span.x, span.y, span.x + span.width, span.y + span.height],
                        glyph.x,
                        glyph.y,
                    )
                });
                if let Some(index) = at {
                    *span_votes[index].entry(*leaf).or_insert(0) += 1;
                    *votes.entry(*leaf).or_insert(0) += 1;
                }
            }
        }
        let mut votes: Vec<(usize, usize)> = votes.into_iter().collect();
        // Most votes first; ties by leaf order, so the answer is stable.
        votes.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let span_leaf = span_votes
            .iter()
            .map(|span| {
                span.iter()
                    .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
                    .map(|pair| *pair.0)
            })
            .collect();
        maps.push(BlockMap {
            votes,
            span_leaf,
            artifact,
            total,
        });
    }
    (maps, overlaps)
}

/// Add one at compressed rank `rank` (0-based) to a Fenwick tree over
/// `count` ranks.
fn bit_add(bit: &mut [u64], rank: usize, count: usize) {
    let mut i = rank + 1;
    while i <= count {
        bit[i] += 1;
        i += i & i.wrapping_neg();
    }
}

/// How many of the ranks added so far fall below `rank` (0-based, exclusive).
fn bit_below(bit: &[u64], rank: usize) -> u64 {
    let mut i = rank;
    let mut sum = 0;
    while i > 0 {
        sum += bit[i];
        i -= i & i.wrapping_neg();
    }
    sum
}

/// Kendall tau of structure ranks in geometric block order, in O(n log n).
///
/// Each block's rank is compared against the blocks geometrically before it:
/// lower ranks are concordant pairs, higher ones discordant, equal ones
/// (two blocks of one element) neither. `None` where fewer than
/// [`MIN_ORDER_PAIRS`] untied pairs exist: no order to judge.
fn kendall_tau(ranks: &[usize]) -> Option<f64> {
    if ranks.len() < 2 {
        return None;
    }
    let mut values = ranks.to_vec();
    values.sort_unstable();
    values.dedup();
    let mut bit = vec![0u64; values.len() + 1];
    let mut concordant = 0u64;
    let mut discordant = 0u64;
    for (seen, rank) in ranks.iter().enumerate() {
        let compressed = values.binary_search(rank).unwrap_or(0);
        concordant += bit_below(&bit, compressed);
        discordant += seen as u64 - bit_below(&bit, compressed + 1);
        bit_add(&mut bit, compressed, values.len());
    }
    let pairs = concordant + discordant;
    if pairs < MIN_ORDER_PAIRS {
        return None;
    }
    Some((concordant as f64 - discordant as f64) / pairs as f64)
}

/// The validity check: every reason the tree must not decide, or none.
///
/// Each reason is a sentence for a log, not a code: refusal is silent today
/// (the geometric answer is returned), and what names the cause names it for
/// a reader, not a match.
fn refusal_reasons(
    facts: &TreeFacts,
    maps: &[BlockMap],
    blocks: &[Block],
    leaves: &[Leaf],
    glyphs: &[Vec<PlacedGlyph>],
    overlaps: &[((usize, i32), usize, usize)],
) -> Vec<String> {
    let mut reasons = Vec::new();

    if facts.leaves.is_empty() {
        reasons.push("structure tree carries no placeable content".to_string());
        return reasons;
    }

    if facts.total > 0 && (facts.contentless as f64) > MAX_CONTENTLESS_SHARE * (facts.total as f64)
    {
        reasons.push(format!(
            "{} of {} elements carry no content",
            facts.contentless, facts.total
        ));
    }

    if !overlaps.is_empty() {
        // One MCID owned twice is impossible: the glyphs cannot belong to
        // two elements. Name the first few; the count says the rest.
        let mut keys: Vec<(usize, i32)> = overlaps.iter().map(|overlap| overlap.0).collect();
        keys.sort_unstable();
        keys.dedup();
        for key in keys.iter().take(3) {
            reasons.push(format!(
                "MCID {} on page {} is claimed by two elements",
                key.1, key.0
            ));
        }
        if keys.len() > 3 {
            reasons.push(format!("{} more MCIDs claimed twice", keys.len() - 3));
        }
    }

    // References the page never drew: the signature of tags left behind by
    // an edit. Unresolvable pages count the same: a reference that names no
    // page points at nothing by construction.
    let mut drawn: HashSet<(usize, i32)> = HashSet::new();
    for (page, glyphs) in glyphs.iter().enumerate() {
        for glyph in glyphs {
            if let Some(mcid) = glyph.mcid {
                drawn.insert((page, mcid));
            }
        }
    }
    let mut distinct: HashSet<(usize, i32)> = HashSet::new();
    for leaf in leaves {
        distinct.extend(leaf.refs.iter().copied());
    }
    let dangling = distinct.iter().filter(|key| !drawn.contains(key)).count();
    let missing = dangling + facts.unplaced;
    let total_refs = distinct.len() + facts.unplaced;
    if total_refs > 0 && (missing as f64) > MAX_DANGLING_SHARE * (total_refs as f64) {
        reasons.push(format!(
            "{missing} of {total_refs} content references point at nothing drawn"
        ));
    }

    let mapped: Vec<(&BlockMap, &Block)> = maps
        .iter()
        .zip(blocks.iter())
        .filter(|pair| pair.0.mapped())
        .collect();
    // Artifact blocks sit out: they become furniture, not headings.
    let body_mapped: Vec<(&BlockMap, &Block)> = mapped
        .iter()
        .filter(|pair| !pair.0.is_artifact())
        .map(|pair| (pair.0, pair.1))
        .collect();
    let headings = body_mapped
        .iter()
        .filter(|pair| {
            pair.0
                .primary()
                .and_then(|leaf| leaves.get(leaf))
                .is_some_and(|leaf| matches!(leaf.role, TagRole::Heading(_)))
        })
        .count();
    if !body_mapped.is_empty() && (headings as f64) > MAX_HEADING_SHARE * (body_mapped.len() as f64)
    {
        reasons.push(format!(
            "headings on {headings} of {} mapped blocks",
            body_mapped.len()
        ));
    }

    let mapped_chars: usize = mapped.iter().map(|pair| pair.1.text.chars().count()).sum();
    let total_chars: usize = blocks.iter().map(|block| block.text.chars().count()).sum();
    if total_chars == 0 {
        reasons.push("document carries no text to check the tree against".to_string());
    } else if (mapped_chars as f64) < MIN_COVERAGE * (total_chars as f64) {
        reasons.push(format!(
            "tree covers {mapped_chars} of {total_chars} characters"
        ));
    }

    // The tree's order against the geometry's, over the mapped blocks in
    // geometric order. Artifact blocks sit out: they are furniture either
    // way, and their ranks would judge an order they are not in.
    let ranks: Vec<usize> = maps
        .iter()
        .filter(|map| !map.is_artifact())
        .filter_map(|map| map.primary())
        .filter_map(|leaf| leaves.get(leaf))
        .map(|leaf| leaf.rank)
        .collect();
    if let Some(tau) = kendall_tau(&ranks) {
        if tau < MIN_ORDER_TAU {
            if tau < 0.0 {
                reasons.push(format!(
                    "structure order runs against the geometric order (tau {tau:.2})"
                ));
            } else {
                reasons.push(format!(
                    "structure order strongly disagrees with the geometry (tau {tau:.2})"
                ));
            }
        }
    }

    reasons
}

/// One block after the split, with the leaf it belongs to.
struct Piece {
    block: Block,
    leaf: Option<usize>,
    artifact: bool,
}

/// Cut the blocks whose spans vote for more than one leaf.
///
/// A geometric block can straddle a tag boundary two ways: the paragraph
/// merge joined a heading line to its body (same size, same leading), or one
/// row of a table is one line. Where the spans disagree the tags win and the
/// block is cut along the votes; consecutive spans of one leaf stay together,
/// and untagged spans join their neighbours. Artifact blocks are not cut:
/// furniture is furniture whole.
fn split_mixed(blocks: Vec<Block>, maps: &[BlockMap], cell_path_words: bool) -> Vec<Piece> {
    let mut pieces = Vec::with_capacity(blocks.len());
    for (block, map) in blocks.into_iter().zip(maps.iter()) {
        if map.is_artifact() {
            pieces.push(Piece {
                block,
                leaf: map.primary(),
                artifact: true,
            });
            continue;
        }
        // Runs of consecutive spans voting for one leaf; untagged spans
        // join the run beside them (before, else after).
        let mut runs: Vec<(Option<usize>, Vec<usize>)> = Vec::new();
        for (index, leaf) in map.span_leaf.iter().enumerate() {
            let leaf = leaf.or_else(|| {
                runs.last().and_then(|run| run.0).or_else(|| {
                    map.span_leaf[index..]
                        .iter()
                        .filter_map(|leaf| *leaf)
                        .next()
                })
            });
            match runs.last_mut() {
                Some(run) if run.0 == leaf => run.1.push(index),
                _ => runs.push((leaf, vec![index])),
            }
        }
        // One run, or nothing voted at all: the block stands.
        let voted: Vec<(Option<usize>, Vec<usize>)> =
            runs.into_iter().filter(|run| !run.1.is_empty()).collect();
        if voted.len() < 2 {
            pieces.push(Piece {
                block,
                leaf: map.primary(),
                artifact: false,
            });
            continue;
        }
        for (leaf, spans) in voted {
            let kept: Vec<crate::text::TextSpan> = spans
                .iter()
                .map(|index| block.spans[*index].clone())
                .collect();
            let text = TextBlock {
                spans: kept.clone(),
            }
            .text_with_cell_path_words(cell_path_words);
            let mut piece = Block {
                kind: block.kind,
                page: block.page,
                bbox: [0.0; 4],
                text,
                spans: kept,
                index: 0,
                continues_previous: false,
                confidence: block.confidence,
                grid: block.grid,
                table_origin: block.table_origin,
                trace_learned: block.trace_learned,
            };
            recompute_bbox(&mut piece);
            pieces.push(Piece {
                block: piece,
                leaf,
                artifact: false,
            });
        }
    }
    pieces
}

/// Join consecutive pieces of one leaf back into one block.
///
/// The mirror of the split: the geometry cut one element's content in two
/// (a column gap, a table carve-out) and the tags say it is one. Only
/// neighbours join -- two blocks of one leaf with a third block between
/// them are not one thing the page drew apart.
fn merge_same_leaf(pieces: Vec<Piece>, cell_path_words: bool) -> Vec<Piece> {
    let mut merged: Vec<Piece> = Vec::with_capacity(pieces.len());
    for piece in pieces {
        let joinable = !piece.artifact
            && piece.leaf.is_some()
            && merged
                .last()
                .is_some_and(|last: &Piece| !last.artifact && last.leaf == piece.leaf);
        if joinable {
            let last = merged.last_mut().expect("checked non-empty");
            last.block.spans.extend(piece.block.spans);
            last.block.text = TextBlock {
                spans: last.block.spans.clone(),
            }
            .text_with_cell_path_words(cell_path_words);
            recompute_bbox(&mut last.block);
        } else {
            merged.push(piece);
        }
    }
    merged
}

/// The marker a tagged list item opens with, taken off its text.
///
/// Mirrors what the geometric list pass does: the marker stays in the spans
/// (what the page drew) and leaves the text (what the page said), except an
/// outline's number, which is the clause's name as much as its marker.
fn strip_marker(text: &str) -> (String, Option<u32>) {
    let Some(marker) = lists::marker_at(text, lists::Lead::default()) else {
        return (text.to_string(), None);
    };
    let ordinal = marker.value(false).filter(|_| !marker.keeps_its_text());
    if marker.keeps_its_text() {
        return (text.to_string(), ordinal);
    }
    let prefix = text.get(..marker.len).unwrap_or("").trim();
    match text.strip_prefix(prefix) {
        Some(rest) => (rest.trim_start().to_string(), ordinal),
        None => (text.to_string(), ordinal),
    }
}

/// Relabel every piece by its leaf's role, or by its page position for
/// artifact content.
///
/// A kind the tags change loses the learned confidence and the grid doubt
/// it carries: both were measured for the geometric decision, and a number
/// about a decision nobody acted on would read as a number about this kind.
/// A kind the tags confirm keeps them.
fn relabel(pieces: &mut [Piece], leaves: &[Leaf], sizes: &[(f64, f64)]) {
    for piece in pieces.iter_mut() {
        if piece.artifact {
            let height = sizes
                .get(piece.block.page)
                .map(|size| size.1)
                .unwrap_or(0.0);
            let middle = (piece.block.bbox[1] + piece.block.bbox[3]) / 2.0;
            let kind = if height > 0.0 && middle >= height / 2.0 {
                BlockKind::Header
            } else {
                BlockKind::Footer
            };
            set_kind(&mut piece.block, kind);
            continue;
        }
        let Some(index) = piece.leaf else { continue };
        let Some(leaf) = leaves.get(index) else {
            continue;
        };
        match leaf.role {
            TagRole::Heading(level) => {
                // The block model folds below 3; H3-H6 are H3 here.
                set_kind(
                    &mut piece.block,
                    BlockKind::Heading {
                        level: level.min(3),
                    },
                );
            }
            TagRole::Item(level) => {
                let (text, ordinal) = strip_marker(&piece.block.text);
                piece.block.text = text;
                // The tags carry no ordinal of their own; where the text
                // holds no marker -- the geometric pass already took it off
                // -- its number stands, since nothing contradicts it.
                let ordinal = ordinal.or(match piece.block.kind {
                    BlockKind::ListItem { ordinal, .. } => ordinal,
                    _ => None,
                });
                set_kind(
                    &mut piece.block,
                    BlockKind::ListItem {
                        level: level.min(lists::MAX_LIST_LEVEL),
                        ordinal,
                    },
                );
            }
            TagRole::Cell(cell) => {
                set_kind(
                    &mut piece.block,
                    BlockKind::TableCell {
                        row: cell.row,
                        column: cell.column,
                        row_span: cell.row_span,
                        column_span: cell.column_span,
                        header: cell.header,
                    },
                );
            }
            TagRole::Paragraph => {
                set_kind(&mut piece.block, BlockKind::Paragraph);
            }
        }
    }
}

/// Set a block's kind, dropping the measurements of the decision it
/// replaces. Only a changed kind drops them: a confirmation keeps them.
fn set_kind(block: &mut Block, kind: BlockKind) {
    if block.kind != kind {
        block.kind = kind;
        block.confidence = None;
        block.grid = None;
    }
}

/// Put the body in the structure tree's reading order.
///
/// Mapped blocks sort by their leaf's rank; unmapped ones keep their
/// geometric place against their mapped neighbours (between, before the
/// first, after the last), or their geometric order where nothing around
/// them mapped. Furniture is not ordered here: [`arrange`] moves it after
/// the body.
///
/// [`arrange`]: crate::reading_order::arrange
fn order_by_tree(pieces: Vec<Piece>, leaves: &[Leaf]) -> Vec<Block> {
    let mut body: Vec<(f64, usize, Block)> = Vec::new();
    let mut furniture: Vec<Block> = Vec::new();
    for piece in pieces {
        if matches!(piece.block.kind, BlockKind::Header | BlockKind::Footer) {
            furniture.push(piece.block);
        } else {
            let key = piece
                .leaf
                .and_then(|index| leaves.get(index))
                .map(|leaf| leaf.rank as f64)
                .unwrap_or(f64::NAN);
            let position = body.len();
            body.push((key, position, piece.block));
        }
    }
    // Fill the unmapped keys from their mapped neighbours.
    let mut index = 0;
    while index < body.len() {
        if !body[index].0.is_nan() {
            index += 1;
            continue;
        }
        let start = index;
        while index < body.len() && body[index].0.is_nan() {
            index += 1;
        }
        let before = start.checked_sub(1).map(|i| body[i].0);
        let after = body.get(index).map(|entry| entry.0);
        let run = index - start;
        for (k, entry) in body[start..index].iter_mut().enumerate() {
            entry.0 = match (before, after) {
                (Some(first), Some(last)) => {
                    first + (last - first) * (k + 1) as f64 / (run + 1) as f64
                }
                (None, Some(last)) => last - (run - k) as f64,
                (Some(first), None) => first + (k + 1) as f64,
                (None, None) => entry.1 as f64,
            };
        }
    }
    body.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let mut blocks: Vec<Block> = body.into_iter().map(|entry| entry.2).collect();
    blocks.extend(furniture);
    for block in blocks.iter_mut() {
        // Numbering and continuations are the order's, and the order just
        // changed: both are decided again below.
        block.continues_previous = false;
    }
    crate::reading_order::arrange(blocks, 0)
}

/// Let a valid structure tree decide the blocks' kinds and order.
///
/// Parses the tree, maps the pages' glyphs to `(page, MCID)`, checks the
/// tree against the geometry, and -- only where every check passes --
/// relabels the blocks by their tag roles and reorders the body by the
/// tree. Anything else (no tree, a refused tree) returns `blocks` untouched:
/// the caller cannot tell this ran except where the tree earned it.
pub(crate) fn apply(
    pdf: &Pdf,
    settings: &InterpreterSettings,
    blocks: Vec<Block>,
    sizes: &[(f64, f64)],
    cell_path_words: bool,
) -> Vec<Block> {
    if blocks.is_empty() {
        return blocks;
    }
    let Some((tree, details)) = tagged::parse_with_details(pdf) else {
        return blocks;
    };
    let glyphs = glyphs_per_page(pdf, settings);
    let facts = collect_leaves(&tree, &details);
    let (maps, overlaps) = map_blocks(&blocks, &facts.leaves, &glyphs);
    if !refusal_reasons(&facts, &maps, &blocks, &facts.leaves, &glyphs, &overlaps).is_empty() {
        return blocks;
    }
    let pieces = split_mixed(blocks, &maps, cell_path_words);
    let mut pieces = merge_same_leaf(pieces, cell_path_words);
    relabel(&mut pieces, &facts.leaves, sizes);
    order_by_tree(pieces, &facts.leaves)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_compliance::tagged::ElementDetails;

    fn leaf(role: TagRole, rank: usize, refs: Vec<(usize, i32)>) -> Leaf {
        Leaf { role, rank, refs }
    }

    #[test]
    fn tau_sorted_is_one() {
        assert_eq!(kendall_tau(&[0, 1, 2, 3, 4]), Some(1.0));
    }

    #[test]
    fn tau_reversed_is_minus_one() {
        assert_eq!(kendall_tau(&[4, 3, 2, 1, 0]), Some(-1.0));
    }

    #[test]
    fn tau_ignores_ties() {
        // Two blocks of one element tie; the rest decide unanimously.
        assert_eq!(kendall_tau(&[0, 1, 1, 2]), Some(1.0));
    }

    #[test]
    fn tau_needs_pairs() {
        assert_eq!(kendall_tau(&[]), None);
        assert_eq!(kendall_tau(&[7]), None);
        assert_eq!(kendall_tau(&[1, 0]), None);
        // One untied pair of three blocks: still nothing to judge.
        assert_eq!(kendall_tau(&[0, 0, 1]), None);
    }

    #[test]
    fn tau_counts_partially() {
        // [1, 2, 0, 3]: 4 concordant of 6 pairs.
        let tau = kendall_tau(&[1, 2, 0, 3]).expect("enough pairs");
        assert!((tau - 1.0 / 3.0).abs() < 1e-9, "tau {tau}");
    }

    fn make_element(
        struct_type: &str,
        mcids: Vec<i32>,
        children: Vec<StructElement>,
    ) -> StructElement {
        StructElement {
            struct_type: struct_type.to_string(),
            standard_type: struct_type.to_string(),
            alt: None,
            actual_text: None,
            lang: None,
            mcids,
            page_index: Some(0),
            children,
        }
    }

    fn tree_of(roots: Vec<StructElement>) -> StructureTree {
        StructureTree {
            root_elements: roots,
            role_map: HashMap::new(),
            lang: None,
        }
    }

    #[test]
    fn item_level_follows_list_nesting() {
        // L > LI, and an LI nested through a second L: levels 0 and 1.
        let tree = tree_of(vec![make_element(
            "L",
            vec![],
            vec![
                make_element("LI", vec![0], vec![]),
                make_element(
                    "LI",
                    vec![],
                    vec![make_element(
                        "L",
                        vec![],
                        vec![make_element("LI", vec![1], vec![])],
                    )],
                ),
            ],
        )]);
        let facts = collect_leaves(&tree, &StructureTreeDetails::default());
        let roles: Vec<TagRole> = facts.leaves.iter().map(|leaf| leaf.role).collect();
        assert_eq!(roles, vec![TagRole::Item(0), TagRole::Item(1)]);
    }

    #[test]
    fn lbody_inherits_its_items_level() {
        let tree = tree_of(vec![make_element(
            "L",
            vec![],
            vec![make_element(
                "LI",
                vec![],
                vec![
                    make_element("Lbl", vec![0], vec![]),
                    make_element("LBody", vec![1], vec![]),
                ],
            )],
        )]);
        let facts = collect_leaves(&tree, &StructureTreeDetails::default());
        let roles: Vec<TagRole> = facts.leaves.iter().map(|leaf| leaf.role).collect();
        assert_eq!(roles, vec![TagRole::Item(0), TagRole::Item(0)]);
    }

    #[test]
    fn cells_count_rows_and_columns() {
        let row = |mcids: Vec<i32>| {
            make_element(
                "TR",
                vec![],
                mcids
                    .into_iter()
                    .map(|mcid| make_element("TD", vec![mcid], vec![]))
                    .collect(),
            )
        };
        let tree = tree_of(vec![make_element(
            "Table",
            vec![],
            vec![row(vec![0, 1]), row(vec![2, 3])],
        )]);
        let facts = collect_leaves(&tree, &StructureTreeDetails::default());
        let places: Vec<(u16, u16)> = facts
            .leaves
            .iter()
            .map(|leaf| match leaf.role {
                TagRole::Cell(cell) => (cell.row, cell.column),
                role => panic!("expected a cell, got {role:?}"),
            })
            .collect();
        assert_eq!(places, vec![(0, 0), (0, 1), (1, 0), (1, 1)]);
    }

    #[test]
    fn a_row_span_holds_its_column() {
        // Row 0 has two cells; the first spans two rows, so row 1's only
        // cell stands in column 1.
        let spanned = make_element("TD", vec![0], vec![]);
        let tree = tree_of(vec![make_element(
            "Table",
            vec![],
            vec![
                make_element(
                    "TR",
                    vec![],
                    vec![spanned, make_element("TD", vec![1], vec![])],
                ),
                make_element("TR", vec![], vec![make_element("TD", vec![2], vec![])]),
            ],
        )]);
        // Table [0], its first row [0, 0], the spanned cell [0, 0, 0].
        let mut details = StructureTreeDetails::default();
        details.insert(&[0, 0, 0], ElementDetails::new(vec![None], 2, 1));
        let facts = collect_leaves(&tree, &details);
        let places: Vec<(u16, u16)> = facts
            .leaves
            .iter()
            .map(|leaf| match leaf.role {
                TagRole::Cell(cell) => (cell.row, cell.column),
                role => panic!("expected a cell, got {role:?}"),
            })
            .collect();
        assert_eq!(places, vec![(0, 0), (0, 1), (1, 1)]);
    }

    #[test]
    fn headings_keep_their_level() {
        let tree = tree_of(vec![
            make_element("H1", vec![0], vec![]),
            make_element("H4", vec![1], vec![]),
        ]);
        let facts = collect_leaves(&tree, &StructureTreeDetails::default());
        let roles: Vec<TagRole> = facts.leaves.iter().map(|leaf| leaf.role).collect();
        assert_eq!(roles, vec![TagRole::Heading(1), TagRole::Heading(4)]);
        // ... and the fold to the block model's three levels happens on
        // application, not in the walk.
        assert_eq!(
            leaf(TagRole::Heading(4), 0, vec![]).role,
            TagRole::Heading(4)
        );
    }

    #[test]
    fn marker_strips_like_the_geometric_pass() {
        assert_eq!(strip_marker("3. Apples"), ("Apples".to_string(), Some(3)));
        assert_eq!(
            strip_marker("\u{2022} Oranges"),
            ("Oranges".to_string(), None)
        );
        // An outline's number is the clause's name: it stays.
        assert_eq!(
            strip_marker("II.4.1 Scope"),
            ("II.4.1 Scope".to_string(), None)
        );
    }
}
