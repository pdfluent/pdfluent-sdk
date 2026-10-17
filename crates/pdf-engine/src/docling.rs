//! The block model as a Docling `DoclingDocument` (#629).
//!
//! A pipeline built on Docling reads its documents through `docling-core`'s
//! `DoclingDocument`: it walks `body`, filters on `label`, cites `prov` and
//! asks the table for its `grid`. This writes that document from our blocks,
//! so such a pipeline can swap the converter and keep every line downstream of
//! it. `crates/pdf-python/tests/test_docling_document.py` loads what this
//! writes with docling-core's own model and checks it comes back unchanged.
//!
//! THE SCHEMA IS PINNED: `DoclingDocument` version [`SCHEMA_VERSION`], as
//! `docling-core` [`DOCLING_CORE_VERSION`] writes it (MIT, the Docling project).
//! The test above installs exactly that version, so a schema that moves is a
//! decision somebody makes here and not a surprise in a customer's pipeline.
//!
//! WHAT MAPS, block kind by block kind:
//!
//! - a paragraph is a `text` item;
//! - a heading is a `section_header` with the block's level (1 to 3);
//! - a list item is a `list_item` inside a `list` group, `enumerated` where the
//!   document numbered it and with the marker written as `N.` or `-`. A nested
//!   item opens a group under the item above it, which is where Docling puts a
//!   sub-list;
//! - a run of table cells is one `table`, its cells with their spans and
//!   `column_header` from the header row, and the `grid` Docling derives from
//!   them written out as Docling would derive it;
//! - a running head and a running footer are `page_header` and `page_footer`
//!   under `furniture`, in the `furniture` content layer, so every Docling
//!   export that leaves the furniture out leaves them out too;
//! - the half of a paragraph a page break cut off is not an item of its own: it
//!   is joined onto the paragraph it continues, which then carries one `prov`
//!   entry per page, each with the `charspan` of its own half.
//!
//! PROVENANCE: `page_no` is 1-BASED, because Docling's is -- the one place this
//! output disagrees with `Block::page` and with the page markers of
//! `crate::markdown`, which count from zero. The box is `Block::bbox` as it is:
//! the page as it is displayed, `/Rotate` applied, in points from the bottom
//! left corner of the crop box with y up (`BOTTOMLEFT`). `pages` gives each
//! page's size in that same frame, which is what makes the box relative to a
//! page and not to a sheet of paper nobody sees.
//!
//! WHAT HAS NO MAPPING, stated so nobody looks for it:
//!
//! - `Block::confidence`: `DoclingDocument` has no per-item confidence;
//! - `Block::spans`: Docling items carry no runs, fonts or sizes. The caller that
//!   needs them reads `crate::to_json`;
//! - the document's own list marker: `c)` and `iv.` come out as `3.` and `4.`,
//!   because the block keeps the number and not the glyphs;
//! - `title`, `caption`, `footnote`, `picture`, `formula`, `code`, key-value
//!   and form items: the block model has no such kind, so nothing is written
//!   under those labels, and `pictures`, `key_value_items` and `form_items`
//!   are always empty;
//! - `origin`: the serializer does not see the file, so it is `null`.
//!
//! DETERMINISTIC, like the other two serializers: keys in a fixed order, every
//! number through `crate::json::number`, two spaces per level.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::blocks::{Block, BlockKind};
use crate::json::{number, string};
#[cfg(test)]
use crate::table_trace::TableOrigin;

/// The `DoclingDocument` schema version this writes.
pub const SCHEMA_VERSION: &str = "1.10.0";

/// The `docling-core` release whose model defines [`SCHEMA_VERSION`], and the
/// one the round-trip test loads the output with.
pub const DOCLING_CORE_VERSION: &str = "2.98.1";

/// One page of the document as `DoclingDocument.pages` lists it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DoclingPage {
    /// 0-based page index, the same number as `Block::page`.
    pub page: usize,
    /// Width in points.
    pub width: f64,
    /// Height in points.
    pub height: f64,
}

/// A JSON value, written in the order it was built.
enum Value {
    Null,
    Bool(bool),
    Int(usize),
    Number(f64),
    Str(String),
    Array(Vec<Value>),
    Object(Vec<(&'static str, Value)>),
    /// An object whose keys are data (`pages` is keyed by page number).
    Map(Vec<(String, Value)>),
}

impl Value {
    fn text(text: &str) -> Value {
        Value::Str(text.to_string())
    }

    fn write(&self, out: &mut String, depth: usize) {
        match self {
            Value::Null => out.push_str("null"),
            Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Value::Int(n) => out.push_str(&n.to_string()),
            Value::Number(n) => out.push_str(&number(*n)),
            Value::Str(s) => out.push_str(&string(s)),
            Value::Array(items) if items.is_empty() => out.push_str("[]"),
            Value::Array(items) => {
                out.push_str("[\n");
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(",\n");
                    }
                    indent(out, depth + 1);
                    item.write(out, depth + 1);
                }
                out.push('\n');
                indent(out, depth);
                out.push(']');
            }
            Value::Object(fields) => {
                let fields: Vec<(&str, &Value)> = fields.iter().map(|(k, v)| (*k, v)).collect();
                write_object(out, &fields, depth);
            }
            Value::Map(fields) => {
                let fields: Vec<(&str, &Value)> =
                    fields.iter().map(|(k, v)| (k.as_str(), v)).collect();
                write_object(out, &fields, depth);
            }
        }
    }
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn write_object(out: &mut String, fields: &[(&str, &Value)], depth: usize) {
    if fields.is_empty() {
        out.push_str("{}");
        return;
    }
    out.push_str("{\n");
    for (i, (key, value)) in fields.iter().enumerate() {
        if i > 0 {
            out.push_str(",\n");
        }
        indent(out, depth + 1);
        out.push_str(&string(key));
        out.push_str(": ");
        value.write(out, depth + 1);
    }
    out.push('\n');
    indent(out, depth);
    out.push('}');
}

/// `{"cref": "#/texts/3"}`, Docling's reference to another node.
fn cref(target: &str) -> Value {
    Value::Object(vec![("cref", Value::text(target))])
}

/// A `BoundingBox` over `[x0, y0, x1, y1]`, y up.
fn bounding_box(bbox: &[f64; 4]) -> Value {
    Value::Object(vec![
        ("l", Value::Number(bbox[0])),
        ("t", Value::Number(bbox[3])),
        ("r", Value::Number(bbox[2])),
        ("b", Value::Number(bbox[1])),
        ("coord_origin", Value::text("BOTTOMLEFT")),
    ])
}

/// One `ProvenanceItem`.
fn provenance(page: usize, bbox: &[f64; 4], from: usize, to: usize) -> Value {
    Value::Object(vec![
        ("page_no", Value::Int(page + 1)),
        ("bbox", bounding_box(bbox)),
        (
            "charspan",
            Value::Array(vec![Value::Int(from), Value::Int(to)]),
        ),
    ])
}

/// Where a node hangs: under one of the two roots or under another node.
#[derive(Clone)]
struct Parent(String);

/// A text item before it is written: kept open so the half of a paragraph on
/// the next page can still be joined onto it.
struct TextNode {
    parent: Parent,
    children: Vec<String>,
    furniture: bool,
    label: &'static str,
    text: String,
    /// `(page, bbox, charspan start, charspan end)`, one per piece.
    prov: Vec<(usize, [f64; 4], usize, usize)>,
    /// The fields of the label alone, after `hyperlink`.
    extra: Vec<(&'static str, Value)>,
}

struct GroupNode {
    parent: Parent,
    children: Vec<String>,
}

struct TableNode {
    parent: Parent,
    page: usize,
    bbox: [f64; 4],
    cells: Vec<Cell>,
    rows: usize,
    columns: usize,
}

#[derive(Clone)]
struct Cell {
    bbox: [f64; 4],
    row: usize,
    column: usize,
    row_span: usize,
    column_span: usize,
    header: bool,
    text: String,
}

impl Cell {
    fn value(&self) -> Value {
        Value::Object(vec![
            ("bbox", bounding_box(&self.bbox)),
            ("row_span", Value::Int(self.row_span)),
            ("col_span", Value::Int(self.column_span)),
            ("start_row_offset_idx", Value::Int(self.row)),
            ("end_row_offset_idx", Value::Int(self.row + self.row_span)),
            ("start_col_offset_idx", Value::Int(self.column)),
            (
                "end_col_offset_idx",
                Value::Int(self.column + self.column_span),
            ),
            ("text", Value::text(&self.text)),
            ("column_header", Value::Bool(self.header)),
            ("row_header", Value::Bool(false)),
            ("row_section", Value::Bool(false)),
            ("fillable", Value::Bool(false)),
        ])
    }

    /// The empty cell Docling's `grid` puts where no cell reaches.
    fn empty_value(row: usize, column: usize) -> Value {
        Value::Object(vec![
            ("bbox", Value::Null),
            ("row_span", Value::Int(1)),
            ("col_span", Value::Int(1)),
            ("start_row_offset_idx", Value::Int(row)),
            ("end_row_offset_idx", Value::Int(row + 1)),
            ("start_col_offset_idx", Value::Int(column)),
            ("end_col_offset_idx", Value::Int(column + 1)),
            ("text", Value::text("")),
            ("column_header", Value::Bool(false)),
            ("row_header", Value::Bool(false)),
            ("row_section", Value::Bool(false)),
            ("fillable", Value::Bool(false)),
        ])
    }
}

impl TableNode {
    /// `TableData.grid` as docling-core computes it: every position an empty
    /// cell, then every cell written over the positions it covers, the later
    /// cell winning where two overlap.
    fn grid(&self) -> Value {
        let mut grid: Vec<Vec<Option<usize>>> = vec![vec![None; self.columns]; self.rows];
        for (i, cell) in self.cells.iter().enumerate() {
            for row in grid
                .iter_mut()
                .take((cell.row + cell.row_span).min(self.rows))
                .skip(cell.row.min(self.rows))
            {
                for slot in row
                    .iter_mut()
                    .take((cell.column + cell.column_span).min(self.columns))
                    .skip(cell.column.min(self.columns))
                {
                    *slot = Some(i);
                }
            }
        }
        Value::Array(
            grid.iter()
                .enumerate()
                .map(|(r, row)| {
                    Value::Array(
                        row.iter()
                            .enumerate()
                            .map(|(c, slot)| match slot {
                                Some(i) => self.cells[*i].value(),
                                None => Cell::empty_value(r, c),
                            })
                            .collect(),
                    )
                })
                .collect(),
        )
    }
}

/// The union of two boxes.
fn union(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

/// Everything the document holds, in the order Docling numbers it.
#[derive(Default)]
struct Tree {
    body: Vec<String>,
    furniture: Vec<String>,
    texts: Vec<TextNode>,
    groups: Vec<GroupNode>,
    tables: Vec<TableNode>,
}

/// One open list: its group, its level, and its last item so far.
struct OpenList {
    group: usize,
    level: u8,
    enumerated: bool,
    last_item: Option<usize>,
}

impl Tree {
    fn attach(&mut self, parent: &Parent, child: String) {
        match parent.0.as_str() {
            "#/body" => self.body.push(child),
            "#/furniture" => self.furniture.push(child),
            other => {
                if let Some(i) = other.strip_prefix("#/texts/") {
                    if let Ok(i) = i.parse::<usize>() {
                        self.texts[i].children.push(child);
                    }
                } else if let Some(i) = other.strip_prefix("#/groups/") {
                    if let Ok(i) = i.parse::<usize>() {
                        self.groups[i].children.push(child);
                    }
                }
            }
        }
    }

    fn add_text(&mut self, node: TextNode) -> usize {
        let index = self.texts.len();
        let parent = node.parent.clone();
        self.texts.push(node);
        self.attach(&parent, format!("#/texts/{index}"));
        index
    }

    fn add_group(&mut self, parent: Parent) -> usize {
        let index = self.groups.len();
        self.groups.push(GroupNode {
            parent: parent.clone(),
            children: Vec::new(),
        });
        self.attach(&parent, format!("#/groups/{index}"));
        index
    }

    fn add_table(&mut self, node: TableNode) {
        let index = self.tables.len();
        let parent = node.parent.clone();
        self.tables.push(node);
        self.attach(&parent, format!("#/tables/{index}"));
    }
}

fn body() -> Parent {
    Parent("#/body".to_string())
}

fn text_node(block: &Block, label: &'static str, parent: Parent) -> TextNode {
    TextNode {
        parent,
        children: Vec::new(),
        furniture: false,
        label,
        text: block.text.clone(),
        prov: vec![(block.page, block.bbox, 0, block.text.chars().count())],
        extra: Vec::new(),
    }
}

fn build(blocks: &[Block]) -> Tree {
    let mut tree = Tree::default();
    let mut lists: Vec<OpenList> = Vec::new();
    // The paragraph the next block may continue, if the last thing written
    // was one -- the same rule `crate::markdown` joins by.
    let mut open_paragraph: Option<usize> = None;
    let mut at = 0usize;
    while at < blocks.len() {
        if let Some(end) = crate::markdown::table_run(blocks, at) {
            let run = &blocks[at..end];
            at = end;
            lists.clear();
            open_paragraph = None;
            let cells: Vec<Cell> = run
                .iter()
                .filter_map(|block| match block.kind {
                    BlockKind::TableCell {
                        row,
                        column,
                        row_span,
                        column_span,
                        header,
                    } => Some(Cell {
                        bbox: block.bbox,
                        row: row as usize,
                        column: column as usize,
                        row_span: (row_span as usize).max(1),
                        column_span: (column_span as usize).max(1),
                        header,
                        text: block.text.trim().to_string(),
                    }),
                    _ => None,
                })
                .collect();
            let rows = cells.iter().map(|c| c.row + c.row_span).max().unwrap_or(0);
            let columns = cells
                .iter()
                .map(|c| c.column + c.column_span)
                .max()
                .unwrap_or(0);
            let bbox = run
                .iter()
                .skip(1)
                .fold(run[0].bbox, |b, x| union(b, x.bbox));
            tree.add_table(TableNode {
                parent: body(),
                page: run[0].page,
                bbox,
                cells,
                rows,
                columns,
            });
            continue;
        }
        let block = &blocks[at];
        at += 1;
        match block.kind {
            BlockKind::Paragraph => {
                lists.clear();
                if let (true, Some(open)) = (block.continues_previous, open_paragraph) {
                    let node = &mut tree.texts[open];
                    let piece = block.text.trim();
                    if crate::text::is_hyphen_wrap_candidate(&node.text, piece) {
                        node.text.pop();
                    } else {
                        node.text.push(' ');
                    }
                    let from = node.text.chars().count();
                    node.text.push_str(piece);
                    let to = node.text.chars().count();
                    node.prov.push((block.page, block.bbox, from, to));
                    continue;
                }
                open_paragraph = Some(tree.add_text(text_node(block, "text", body())));
            }
            BlockKind::Heading { level } => {
                lists.clear();
                open_paragraph = None;
                let mut node = text_node(block, "section_header", body());
                node.extra
                    .push(("level", Value::Int(level.clamp(1, 100) as usize)));
                tree.add_text(node);
            }
            BlockKind::ListItem { level, ordinal } => {
                open_paragraph = None;
                let enumerated = ordinal.is_some();
                while lists.last().is_some_and(|list| list.level > level) {
                    lists.pop();
                }
                // A numbered list straight after a bulleted one at the same
                // depth is a second list, not more of the first: the page
                // changed its marker, and Docling's own export writes a
                // group's items with one kind of marker.
                if lists
                    .last()
                    .is_some_and(|list| list.level == level && list.enumerated != enumerated)
                {
                    lists.pop();
                }
                let group = match lists.last() {
                    Some(list) if list.level == level => list.group,
                    open => {
                        // Deeper than the innermost open list: a sub-list,
                        // which Docling hangs under the item it follows.
                        let parent = match open {
                            Some(OpenList {
                                last_item: Some(item),
                                ..
                            }) => Parent(format!("#/texts/{item}")),
                            Some(list) => Parent(format!("#/groups/{}", list.group)),
                            None => body(),
                        };
                        let group = tree.add_group(parent);
                        lists.push(OpenList {
                            group,
                            level,
                            enumerated,
                            last_item: None,
                        });
                        group
                    }
                };
                let mut node = text_node(block, "list_item", Parent(format!("#/groups/{group}")));
                node.extra.push(("enumerated", Value::Bool(enumerated)));
                node.extra.push((
                    "marker",
                    Value::Str(ordinal.map_or_else(|| "-".to_string(), |n| format!("{n}."))),
                ));
                let item = tree.add_text(node);
                if let Some(list) = lists.last_mut() {
                    list.last_item = Some(item);
                }
            }
            // DOCLING'S OWN LABEL: a caption is a text of its own, in the body
            // where the page set it (#636).
            BlockKind::Caption => {
                open_paragraph = None;
                lists.clear();
                tree.add_text(text_node(block, "caption", body()));
            }
            BlockKind::Header | BlockKind::Footer => {
                open_paragraph = None;
                let label = if matches!(block.kind, BlockKind::Header) {
                    "page_header"
                } else {
                    "page_footer"
                };
                let mut node = text_node(block, label, Parent("#/furniture".to_string()));
                node.furniture = true;
                tree.add_text(node);
            }
            // Unreachable: `table_run` takes every cell above.
            BlockKind::TableCell { .. } => {}
            // No picture node: Docling's tree carries texts, tables and
            // groups, and a figure is none of those. It still breaks the
            // open runs the way any block boundary does.
            BlockKind::Figure => {
                open_paragraph = None;
                lists.clear();
            }
        }
    }
    tree
}

fn root(name: &'static str, children: &[String]) -> Value {
    Value::Object(vec![
        ("self_ref", Value::Str(format!("#/{name}"))),
        ("parent", Value::Null),
        (
            "children",
            Value::Array(children.iter().map(|c| cref(c)).collect()),
        ),
        ("content_layer", Value::text(name)),
        ("meta", Value::Null),
        ("name", Value::text("_root_")),
        ("label", Value::text("unspecified")),
    ])
}

/// The blocks as a `DoclingDocument`, pinned to [`SCHEMA_VERSION`].
///
/// Preview: output quality is still improving and the API may change in a minor release.
///
/// `name` is `DoclingDocument.name`, which Docling fills with the file's stem.
/// `pages` is every page the blocks were taken from, with its size; a block on
/// a page missing from it is still written, with nothing in `pages` to size
/// it, which Docling accepts.
///
/// ```
/// use pdf_engine::{to_docling_json, Block, BlockKind, DoclingPage};
///
/// let blocks = vec![Block::new(
///     BlockKind::Heading { level: 1 },
///     0,
///     [72.0, 700.0, 200.0, 712.0],
///     "Scope".to_string(),
///     0,
/// )];
/// let page = DoclingPage { page: 0, width: 612.0, height: 792.0 };
/// let json = to_docling_json("report", &[page], &blocks);
/// assert!(json.contains("\"label\": \"section_header\""));
/// ```
pub fn to_docling_json(name: &str, pages: &[DoclingPage], blocks: &[Block]) -> String {
    let tree = build(blocks);
    let texts: Vec<Value> = tree
        .texts
        .into_iter()
        .enumerate()
        .map(|(i, node)| {
            let mut fields = vec![
                ("self_ref", Value::Str(format!("#/texts/{i}"))),
                ("parent", cref(&node.parent.0)),
                (
                    "children",
                    Value::Array(node.children.iter().map(|c| cref(c)).collect()),
                ),
                (
                    "content_layer",
                    Value::text(if node.furniture { "furniture" } else { "body" }),
                ),
                ("meta", Value::Null),
                ("label", Value::text(node.label)),
                (
                    "prov",
                    Value::Array(
                        node.prov
                            .iter()
                            .map(|(page, bbox, from, to)| provenance(*page, bbox, *from, *to))
                            .collect(),
                    ),
                ),
                ("orig", Value::text(&node.text)),
                ("text", Value::text(&node.text)),
                ("formatting", Value::Null),
                ("hyperlink", Value::Null),
            ];
            fields.extend(node.extra);
            Value::Object(fields)
        })
        .collect();
    let groups: Vec<Value> = tree
        .groups
        .iter()
        .enumerate()
        .map(|(i, node)| {
            Value::Object(vec![
                ("self_ref", Value::Str(format!("#/groups/{i}"))),
                ("parent", cref(&node.parent.0)),
                (
                    "children",
                    Value::Array(node.children.iter().map(|c| cref(c)).collect()),
                ),
                ("content_layer", Value::text("body")),
                ("meta", Value::Null),
                ("name", Value::text("list")),
                ("label", Value::text("list")),
            ])
        })
        .collect();
    let tables: Vec<Value> = tree
        .tables
        .iter()
        .enumerate()
        .map(|(i, node)| {
            Value::Object(vec![
                ("self_ref", Value::Str(format!("#/tables/{i}"))),
                ("parent", cref(&node.parent.0)),
                ("children", Value::Array(Vec::new())),
                ("content_layer", Value::text("body")),
                ("meta", Value::Null),
                ("label", Value::text("table")),
                (
                    "prov",
                    Value::Array(vec![provenance(node.page, &node.bbox, 0, 0)]),
                ),
                ("captions", Value::Array(Vec::new())),
                ("references", Value::Array(Vec::new())),
                ("footnotes", Value::Array(Vec::new())),
                ("image", Value::Null),
                (
                    "data",
                    Value::Object(vec![
                        (
                            "table_cells",
                            Value::Array(node.cells.iter().map(Cell::value).collect()),
                        ),
                        ("num_rows", Value::Int(node.rows)),
                        ("num_cols", Value::Int(node.columns)),
                        ("orientation", Value::text("rot_0")),
                        ("grid", node.grid()),
                    ]),
                ),
                ("annotations", Value::Array(Vec::new())),
            ])
        })
        .collect();
    let mut sorted: Vec<DoclingPage> = pages.to_vec();
    sorted.sort_by_key(|p| p.page);
    sorted.dedup_by_key(|p| p.page);
    let pages = Value::Map(
        sorted
            .iter()
            .map(|p| {
                (
                    (p.page + 1).to_string(),
                    Value::Object(vec![
                        (
                            "size",
                            Value::Object(vec![
                                ("width", Value::Number(p.width)),
                                ("height", Value::Number(p.height)),
                            ]),
                        ),
                        ("image", Value::Null),
                        ("page_no", Value::Int(p.page + 1)),
                    ]),
                )
            })
            .collect(),
    );
    let document = Value::Object(vec![
        ("schema_name", Value::text("DoclingDocument")),
        ("version", Value::text(SCHEMA_VERSION)),
        ("name", Value::text(name)),
        ("origin", Value::Null),
        ("furniture", root("furniture", &tree.furniture)),
        ("body", root("body", &tree.body)),
        ("groups", Value::Array(groups)),
        ("texts", Value::Array(texts)),
        ("pictures", Value::Array(Vec::new())),
        ("tables", Value::Array(tables)),
        ("key_value_items", Value::Array(Vec::new())),
        ("form_items", Value::Array(Vec::new())),
        ("pages", pages),
    ]);
    let mut out = String::new();
    document.write(&mut out, 0);
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(kind: BlockKind, text: &str, page: usize) -> Block {
        Block {
            kind,
            page,
            bbox: [72.0, 700.0, 200.0, 712.0],
            text: text.to_string(),
            spans: Vec::new(),
            index: 0,
            continues_previous: false,
            confidence: None,
            grid: None,
            table_origin: TableOrigin::Other,
            trace_learned: None,
        }
    }

    /// A caption carries Docling's own label (#636).
    #[test]
    fn a_caption_carries_the_caption_label() {
        let json = to_docling_json("d", &[], &[block(BlockKind::Caption, "Figure 1.", 0)]);
        assert!(
            json.contains("\"label\":\"caption\"") || json.contains("\"label\": \"caption\""),
            "{json}"
        );
    }

    fn cell(row: u16, column: u16, column_span: u16, header: bool, text: &str) -> Block {
        block(
            BlockKind::TableCell {
                row,
                column,
                row_span: 1,
                column_span,
                header,
            },
            text,
            0,
        )
    }

    fn page(page: usize) -> DoclingPage {
        DoclingPage {
            page,
            width: 612.0,
            height: 792.0,
        }
    }

    #[test]
    fn nothing_in_is_an_empty_document_with_its_pages() {
        let json = to_docling_json("empty", &[page(0)], &[]);
        assert!(
            json.contains("\"schema_name\": \"DoclingDocument\""),
            "{json}"
        );
        assert!(json.contains("\"version\": \"1.10.0\""), "{json}");
        assert!(json.contains("\"texts\": []"), "{json}");
        assert!(json.contains("\"1\": {"), "{json}");
        assert!(json.contains("\"page_no\": 1"), "{json}");
    }

    #[test]
    fn a_heading_is_a_section_header_with_its_level_and_a_one_based_page() {
        let json = to_docling_json(
            "d",
            &[page(2)],
            &[block(BlockKind::Heading { level: 2 }, "Terms", 2)],
        );
        assert!(json.contains("\"label\": \"section_header\""), "{json}");
        assert!(json.contains("\"level\": 2"), "{json}");
        assert!(json.contains("\"page_no\": 3"), "{json}");
        assert!(json.contains("\"coord_origin\": \"BOTTOMLEFT\""), "{json}");
        assert!(json.contains("\"t\": 712"), "{json}");
        assert!(json.contains("\"b\": 700"), "{json}");
    }

    #[test]
    fn the_running_head_hangs_under_furniture_and_not_under_body() {
        let json = to_docling_json(
            "d",
            &[page(0)],
            &[
                block(BlockKind::Paragraph, "Body.", 0),
                block(BlockKind::Header, "Annual report", 0),
            ],
        );
        let furniture = json.find("\"furniture\": {").unwrap();
        let body = json.find("\"body\": {").unwrap();
        let head_ref = json[furniture..body].find("#/texts/1");
        assert!(head_ref.is_some(), "{json}");
        assert!(json.contains("\"label\": \"page_header\""), "{json}");
        assert!(json.contains("\"content_layer\": \"furniture\""), "{json}");
    }

    #[test]
    fn a_nested_item_opens_a_group_under_the_item_above_it() {
        let item = |level, ordinal, text| block(BlockKind::ListItem { level, ordinal }, text, 0);
        let tree = build(&[
            item(0, Some(1), "one"),
            item(1, None, "one-a"),
            item(0, Some(2), "two"),
        ]);
        assert_eq!(tree.groups.len(), 2);
        assert_eq!(tree.body, vec!["#/groups/0"]);
        assert_eq!(tree.groups[0].children, vec!["#/texts/0", "#/texts/2"]);
        assert_eq!(tree.texts[0].children, vec!["#/groups/1"]);
        assert_eq!(tree.groups[1].children, vec!["#/texts/1"]);
        let json = to_docling_json("d", &[page(0)], &[item(0, Some(3), "x")]);
        let split = build(&[item(0, None, "bullet"), item(0, Some(1), "numbered")]);
        assert_eq!(split.body, vec!["#/groups/0", "#/groups/1"]);
        assert!(json.contains("\"enumerated\": true"), "{json}");
        assert!(json.contains("\"marker\": \"3.\""), "{json}");
    }

    #[test]
    fn a_paragraph_the_break_cut_in_two_is_one_item_with_two_provenances() {
        let mut second = block(BlockKind::Paragraph, "on the next page.", 1);
        second.continues_previous = true;
        let tree = build(&[block(BlockKind::Paragraph, "A sentence carried", 0), second]);
        assert_eq!(tree.texts.len(), 1);
        let node = &tree.texts[0];
        assert_eq!(node.text, "A sentence carried on the next page.");
        assert_eq!(node.prov.len(), 2);
        assert_eq!(
            (node.prov[1].0, node.prov[1].2, node.prov[1].3),
            (1, 19, 36)
        );
    }

    #[test]
    fn a_table_carries_its_spans_and_the_grid_docling_derives() {
        let tree = build(&[
            cell(0, 0, 2, true, "Band"),
            cell(1, 0, 1, false, "A"),
            cell(1, 1, 1, false, "1"),
        ]);
        assert_eq!(tree.tables.len(), 1);
        let table = &tree.tables[0];
        assert_eq!((table.rows, table.columns), (2, 2));
        let mut grid = String::new();
        table.grid().write(&mut grid, 0);
        // The spanning header fills both positions of the first row.
        assert_eq!(grid.matches("\"text\": \"Band\"").count(), 2, "{grid}");
    }

    #[test]
    fn a_hole_in_the_grid_is_an_empty_cell_without_a_box() {
        let tree = build(&[cell(0, 0, 1, false, "a"), cell(1, 1, 1, false, "b")]);
        let mut grid = String::new();
        tree.tables[0].grid().write(&mut grid, 0);
        assert_eq!(grid.matches("\"bbox\": null").count(), 2, "{grid}");
    }

    #[test]
    fn the_same_blocks_write_the_same_bytes() {
        let blocks = vec![
            block(BlockKind::Heading { level: 1 }, "Scope", 0),
            block(BlockKind::Paragraph, "Body.", 0),
        ];
        assert_eq!(
            to_docling_json("d", &[page(0)], &blocks),
            to_docling_json("d", &[page(0)], &blocks)
        );
    }
}
