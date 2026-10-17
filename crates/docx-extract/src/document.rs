//! WordprocessingML event parser: `word/document.xml` to engine blocks.
//!
//! One pass with quick-xml over the document body. Paragraphs become blocks,
//! tables become runs of cell blocks in row-major order, and runs become
//! inline Markdown (bold, italic, links, image references) inside the block
//! text. Element and attribute names are matched on their local part only:
//! the `w:` prefix is a convention, not a contract.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::collections::HashMap;
use std::io::BufRead;

use pdf_engine::{Block, BlockKind, TextSpan};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::numbering::Numbering;
use crate::styles::Styles;
use crate::DocxError;

/// The value of the attribute whose local name is `key`, decoded and
/// unescaped, or `None` where the element carries no such attribute.
pub(crate) fn attr_value<R: BufRead>(
    e: &BytesStart,
    reader: &Reader<R>,
    key: &str,
) -> Option<String> {
    e.attributes().filter_map(Result::ok).find_map(|attr| {
        if local_name(attr.key.0) == key.as_bytes() {
            attr.decode_and_unescape_value(reader.decoder())
                .ok()
                .map(|value| value.into_owned())
        } else {
            None
        }
    })
}

/// Everything after the last `:`, or the whole name where there is none.
fn local_name(name: &[u8]) -> &[u8] {
    match name.iter().rposition(|byte| *byte == b':') {
        Some(at) => &name[at + 1..],
        None => name,
    }
}

/// Read `word/_rels/document.xml.rels`: relationship id -> target.
///
/// Hyperlink targets are absolute URLs; image targets are part paths such as
/// `media/image1.png`. Both are kept as stored.
pub(crate) fn parse_relationships(xml: &[u8]) -> Result<HashMap<String, String>, DocxError> {
    let mut rels = HashMap::new();
    let mut reader = Reader::from_reader(xml);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                if e.local_name().as_ref() == b"Relationship" {
                    if let (Some(id), Some(target)) = (
                        attr_value(e, &reader, "Id"),
                        attr_value(e, &reader, "Target"),
                    ) {
                        rels.insert(id, target);
                    }
                }
            }
            Err(error) => return Err(DocxError::Xml(error)),
            _ => {}
        }
        buf.clear();
    }
    Ok(rels)
}

/// A finished run or image inside a paragraph, before inline Markdown is
/// rendered.
#[derive(Debug)]
enum Inline {
    /// A run of text with uniform formatting. `link` is `Some` where the run
    /// sits inside a hyperlink (or a `HYPERLINK` field).
    Run {
        text: String,
        bold: bool,
        italic: bool,
        link: Option<String>,
    },
    /// An image: alt text plus the relationship target it references.
    Image { alt: String, target: String },
}

/// A paragraph under construction: its properties plus its runs.
#[derive(Debug, Default)]
struct ParaState {
    style: Option<String>,
    outline: Option<u8>,
    ilvl: Option<u8>,
    num_id: Option<u32>,
    inlines: Vec<Inline>,
}

/// A run under construction: formatting from `w:rPr`, text from `w:t`.
#[derive(Debug, Default)]
struct RunState {
    text: String,
    bold: bool,
    italic: bool,
    link: Option<String>,
}

/// How a table cell merges vertically (`w:vMerge`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum VMerge {
    /// No vertical merge.
    #[default]
    None,
    /// `val="restart"`: the cell opens a merged region.
    Restart,
    /// Absent `val` or `val="continue"`: the cell extends the region above.
    Continue,
}

/// A table cell under construction.
#[derive(Debug, Default)]
struct CellState {
    grid_span: u16,
    vmerge: VMerge,
    /// Rendered paragraph texts, in document order.
    paras: Vec<String>,
}

/// A placed table cell, waiting for the table to close.
#[derive(Debug)]
struct Cell {
    row: u16,
    col: u16,
    row_span: u16,
    col_span: u16,
    header: bool,
    text: String,
}

/// A table under construction.
#[derive(Debug, Default)]
struct TableState {
    cells: Vec<Cell>,
    /// Per grid column, the cell whose vertical merge still covers it.
    merges: Vec<Option<usize>>,
    /// Per grid column, whether the current row continued that merge.
    continued: Vec<bool>,
    /// Grid column the next `w:tc` of this row lands on.
    cursor: usize,
    /// Current grid row.
    row: usize,
    /// Whether the current row repeats as a header (`w:tblHeader`).
    row_header: bool,
    /// The cell whose paragraphs are being collected, if any.
    cell: Option<CellState>,
}

/// A drawing under construction: alt text plus the image relationship.
#[derive(Debug, Default)]
struct DrawingState {
    alt: Option<String>,
    embed: Option<String>,
}

/// A block under construction. The reading-order index is assigned when the
/// document is finished, so nested tables (flushed after their outer table)
/// still number in the order they are emitted.
struct RawBlock {
    kind: BlockKind,
    text: String,
    spans: Vec<TextSpan>,
}

impl RawBlock {
    /// Word has no pages: every block lands on page 0 with a zero box.
    fn finish(self, index: usize) -> Block {
        let mut block = Block::new(self.kind, 0, [0.0; 4], self.text, index);
        block.spans = self.spans;
        block
    }
}

/// The document parser: one event loop over `word/document.xml`.
struct Parser<'a> {
    rels: &'a HashMap<String, String>,
    numbering: Numbering,
    styles: Styles,
    blocks: Vec<RawBlock>,
    /// Nested-table blocks, flushed after the outermost table closes so each
    /// table's cells stay one adjacent run.
    nested: Vec<RawBlock>,
    tables: Vec<TableState>,
    para: Option<ParaState>,
    run: Option<RunState>,
    /// Hyperlink targets: one entry per open `w:hyperlink`/`w:fldSimple`.
    links: Vec<Option<String>>,
    drawing: Option<DrawingState>,
    /// Local names of the open elements, innermost last.
    stack: Vec<String>,
    /// Depth inside `w:del`/`w:moveFrom`: deleted text is dropped.
    del_depth: u32,
}

/// Parse the document body into engine blocks.
pub(crate) fn parse(
    xml: &[u8],
    rels: &HashMap<String, String>,
    numbering: Numbering,
    styles: Styles,
) -> Result<Vec<Block>, DocxError> {
    let mut parser = Parser {
        rels,
        numbering,
        styles,
        blocks: Vec::new(),
        nested: Vec::new(),
        tables: Vec::new(),
        para: None,
        run: None,
        links: Vec::new(),
        drawing: None,
        stack: Vec::new(),
        del_depth: 0,
    };
    let mut reader = Reader::from_reader(xml);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(Event::Start(ref e)) => parser.start(e, &reader),
            Ok(Event::Empty(ref e)) => {
                parser.start(e, &reader);
                let name = parser.stack.pop().unwrap_or_default();
                parser.end(&name);
            }
            Ok(Event::End(ref e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                parser.stack.pop();
                parser.end(&name);
            }
            Ok(Event::Text(ref e)) => {
                let text = e.unescape()?;
                parser.text(&text);
            }
            Err(error) => return Err(DocxError::Xml(error)),
            _ => {}
        }
        buf.clear();
    }
    let mut raw = parser.blocks;
    raw.extend(parser.nested);
    Ok(raw
        .into_iter()
        .enumerate()
        .map(|(index, block)| block.finish(index))
        .collect())
}

impl Parser<'_> {
    /// Handle an opening (or self-closing) tag.
    fn start<R: BufRead>(&mut self, e: &BytesStart, reader: &Reader<R>) {
        let name = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
        match name.as_str() {
            "p" => self.para = Some(ParaState::default()),
            "pStyle" => {
                if let Some(para) = self.para.as_mut() {
                    para.style = attr_value(e, reader, "val");
                }
            }
            "outlineLvl" => {
                if let Some(para) = self.para.as_mut() {
                    para.outline = attr_value(e, reader, "val").and_then(|v| v.parse().ok());
                }
            }
            "ilvl" => {
                if let Some(para) = self.para.as_mut() {
                    para.ilvl = attr_value(e, reader, "val").and_then(|v| v.parse().ok());
                }
            }
            "numId" => {
                if let Some(para) = self.para.as_mut() {
                    para.num_id = attr_value(e, reader, "val").and_then(|v| v.parse().ok());
                }
            }
            "r" => {
                self.run = Some(RunState {
                    link: self.links.last().cloned().flatten(),
                    ..RunState::default()
                });
            }
            // Bold and italic only count inside `w:rPr`: the same local name
            // nowhere else means formatting.
            "b" => {
                if self.stack.last().is_some_and(|parent| parent == "rPr") {
                    if let Some(run) = self.run.as_mut() {
                        run.bold = is_on(attr_value(e, reader, "val").as_deref());
                    }
                }
            }
            "i" => {
                if self.stack.last().is_some_and(|parent| parent == "rPr") {
                    if let Some(run) = self.run.as_mut() {
                        run.italic = is_on(attr_value(e, reader, "val").as_deref());
                    }
                }
            }
            "t" | "delText" | "instrText" => {}
            "tab" => {
                if let Some(run) = self.run.as_mut() {
                    run.text.push('\t');
                }
            }
            "br" | "cr" => {
                if let Some(run) = self.run.as_mut() {
                    run.text.push('\n');
                }
            }
            "sym" => {
                if let Some(run) = self.run.as_mut() {
                    if let Some(symbol) = attr_value(e, reader, "char")
                        .and_then(|v| u32::from_str_radix(v.trim(), 16).ok())
                        .and_then(char::from_u32)
                    {
                        run.text.push(symbol);
                    }
                }
            }
            "noBreakHyphen" => {
                if let Some(run) = self.run.as_mut() {
                    run.text.push('\u{2011}');
                }
            }
            "hyperlink" => {
                let target = attr_value(e, reader, "id")
                    .and_then(|id| self.rels.get(&id).cloned())
                    .or_else(|| attr_value(e, reader, "anchor").map(|anchor| format!("#{anchor}")));
                self.links.push(target);
            }
            "fldSimple" => {
                let target = attr_value(e, reader, "instr")
                    .as_deref()
                    .and_then(hyperlink_field_target);
                self.links.push(target);
            }
            "del" | "moveFrom" => self.del_depth += 1,
            "tbl" => self.tables.push(TableState::default()),
            "tr" => {
                if let Some(table) = self.tables.last_mut() {
                    table.row_header = false;
                }
            }
            "tblHeader" => {
                if let Some(table) = self.tables.last_mut() {
                    table.row_header = true;
                }
            }
            "tc" => {
                if let Some(table) = self.tables.last_mut() {
                    table.cell = Some(CellState::default());
                }
            }
            "gridSpan" => {
                if let Some(cell) = self.tables.last_mut().and_then(|table| table.cell.as_mut()) {
                    cell.grid_span = attr_value(e, reader, "val")
                        .and_then(|v| v.parse().ok())
                        .map(|span: u16| span.max(1))
                        .unwrap_or(1);
                }
            }
            "vMerge" => {
                if let Some(cell) = self.tables.last_mut().and_then(|table| table.cell.as_mut()) {
                    // An absent `val` continues the merge; only `restart`
                    // opens one.
                    cell.vmerge = match attr_value(e, reader, "val").as_deref() {
                        Some("restart") => VMerge::Restart,
                        Some("continue") | None => VMerge::Continue,
                        Some(_) => VMerge::None,
                    };
                }
            }
            "drawing" => self.drawing = Some(DrawingState::default()),
            "docPr" => {
                if let Some(drawing) = self.drawing.as_mut() {
                    drawing.alt =
                        attr_value(e, reader, "descr").or_else(|| attr_value(e, reader, "name"));
                }
            }
            "blip" => {
                if let Some(drawing) = self.drawing.as_mut() {
                    drawing.embed = attr_value(e, reader, "embed");
                }
            }
            _ => {}
        }
        self.stack.push(name);
    }

    /// Handle a closing (or self-closing) tag.
    fn end(&mut self, name: &str) {
        match name {
            "r" => {
                if let (Some(run), Some(para)) = (self.run.take(), self.para.as_mut()) {
                    if !run.text.is_empty() {
                        para.inlines.push(Inline::Run {
                            text: run.text,
                            bold: run.bold,
                            italic: run.italic,
                            link: run.link,
                        });
                    }
                }
            }
            "hyperlink" | "fldSimple" => {
                self.links.pop();
            }
            "del" | "moveFrom" => self.del_depth = self.del_depth.saturating_sub(1),
            "p" => self.close_paragraph(),
            "tc" => self.close_cell(),
            "tr" => self.close_row(),
            "tbl" => self.close_table(),
            "drawing" => {
                if let (Some(drawing), Some(para)) = (self.drawing.take(), self.para.as_mut()) {
                    // An image without a resolvable target has nothing to
                    // reference, so it is dropped rather than linked nowhere.
                    if let Some(target) = drawing
                        .embed
                        .and_then(|embed| self.rels.get(&embed).cloned())
                    {
                        para.inlines.push(Inline::Image {
                            alt: drawing.alt.unwrap_or_default(),
                            target,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    /// Handle character data. Only `w:t` carries text; field codes
    /// (`w:instrText`) and deleted text are dropped.
    fn text(&mut self, content: &str) {
        if self.del_depth > 0 || self.stack.last().is_none_or(|top| top != "t") {
            return;
        }
        if let Some(run) = self.run.as_mut() {
            run.text.push_str(content);
        }
    }

    /// Close the open paragraph: a block at body level, a cell line inside a
    /// table. Empty paragraphs (Word's vertical whitespace) emit nothing.
    fn close_paragraph(&mut self) {
        let Some(para) = self.para.take() else {
            return;
        };
        let kind = self.block_kind(&para);
        let (text, spans) = render_inlines(para.inlines);
        if text.trim().is_empty() {
            return;
        }
        if let Some(cell) = self.tables.last_mut().and_then(|table| table.cell.as_mut()) {
            cell.paras.push(text);
            return;
        }
        self.blocks.push(RawBlock { kind, text, spans });
    }

    /// Decide the block kind: a heading where the outline level or the style
    /// says so, a list item where `w:numPr` says so, else a paragraph.
    ///
    /// The outline level wins over numbering, the way Word reads a paragraph
    /// that carries both. Within each, direct formatting wins over the
    /// style's `basedOn` chain, and the nearest chain setting wins.
    fn block_kind(&mut self, para: &ParaState) -> BlockKind {
        if let Some(level) = self.heading_level(para) {
            return BlockKind::Heading { level };
        }
        let num_pr = match para.num_id {
            Some(num_id) => Some((num_id, para.ilvl.unwrap_or(0))),
            None => para
                .style
                .as_deref()
                .and_then(|style| self.styles.num_pr(style)),
        };
        if let Some((num_id, ilvl)) = num_pr {
            let level = ilvl.min(8);
            if !self.numbering.is_plain(num_id, level) {
                let ordinal = self.numbering.ordinal(num_id, level);
                return BlockKind::ListItem { level, ordinal };
            }
        }
        BlockKind::Paragraph
    }

    /// The heading level: direct `w:outlineLvl`, the style chain's, the
    /// chain's `Heading1`..`9`/`Title` id or name, or the style id itself
    /// where `styles.xml` carries no such style.
    fn heading_level(&self, para: &ParaState) -> Option<u8> {
        if let Some(outline) = para.outline {
            return Some(outline.saturating_add(1).clamp(1, 9));
        }
        let style = para.style.as_deref()?;
        if let Some(outline) = self.styles.outline(style) {
            return Some(outline.saturating_add(1).clamp(1, 9));
        }
        self.styles
            .heading_name_level(style, heading_level_from_style)
            .or_else(|| heading_level_from_style(style))
    }

    /// Place the closed cell on its table's grid.
    ///
    /// A plain cell lands on the cursor, skipping columns a vertical merge
    /// from above still holds. A continuation extends the merged region it
    /// lands on instead of placing a cell. `w:gridSpan` widens both.
    fn close_cell(&mut self) {
        let cell = self.tables.last_mut().and_then(|table| table.cell.take());
        let (Some(cell), Some(table)) = (cell, self.tables.last_mut()) else {
            return;
        };
        let text = cell.paras.join("\n");
        let span = cell.grid_span.max(1) as usize;
        match cell.vmerge {
            VMerge::Continue => {
                while table.cursor < table.merges.len() && table.merges[table.cursor].is_none() {
                    table.cursor += 1;
                }
                let col = table.cursor;
                table.grow(col + span);
                let mut origin = None;
                for held in col..col + span {
                    table.continued[held] = true;
                    if origin.is_none() {
                        origin = table.merges[held];
                    }
                }
                if let Some(index) = origin {
                    if let Some(cell) = table.cells.get_mut(index) {
                        cell.row_span = cell.row_span.saturating_add(1);
                        // Continuations are empty in well-formed files; a
                        // non-empty one still has words worth keeping.
                        if !text.trim().is_empty() {
                            if !cell.text.is_empty() {
                                cell.text.push('\n');
                            }
                            cell.text.push_str(&text);
                        }
                    }
                }
                table.cursor += span;
            }
            VMerge::None | VMerge::Restart => {
                while table.cursor < table.merges.len() && table.merges[table.cursor].is_some() {
                    table.cursor += 1;
                }
                let col = table.cursor;
                table.grow(col + span);
                let index = table.cells.len();
                table.cells.push(Cell {
                    row: table.row.min(u16::MAX as usize) as u16,
                    col: col.min(u16::MAX as usize) as u16,
                    row_span: 1,
                    col_span: span.min(u16::MAX as usize) as u16,
                    header: table.row_header,
                    text,
                });
                if cell.vmerge == VMerge::Restart {
                    // The opening row continues the merge itself, or the row
                    // close below would end what this cell just opened.
                    for held in col..col + span {
                        table.merges[held] = Some(index);
                        table.continued[held] = true;
                    }
                }
                table.cursor += span;
            }
        }
    }

    /// Close the row: merges no cell continued are over, and the next row
    /// starts at column 0.
    fn close_row(&mut self) {
        if let Some(table) = self.tables.last_mut() {
            for col in 0..table.merges.len() {
                if table.merges[col].is_some() && !table.continued[col] {
                    table.merges[col] = None;
                }
                table.continued[col] = false;
            }
            table.row = table.row.saturating_add(1);
            table.cursor = 0;
        }
    }

    /// Close the table into cell blocks in row-major order. Every grid cell
    /// gets a block, empty ones included: the grid is explicit here, unlike in
    /// a PDF, and a rectangular table should serialize as a pipe table.
    ///
    /// A nested table's blocks queue behind the outermost table so each
    /// table's cells stay one adjacent run.
    fn close_table(&mut self) {
        let Some(table) = self.tables.pop() else {
            return;
        };
        let mut out = Vec::with_capacity(table.cells.len());
        for cell in table.cells {
            let spans = if cell.text.is_empty() {
                Vec::new()
            } else {
                vec![TextSpan {
                    text: cell.text.clone(),
                    ..Default::default()
                }]
            };
            out.push(RawBlock {
                kind: BlockKind::TableCell {
                    row: cell.row,
                    column: cell.col,
                    row_span: cell.row_span,
                    column_span: cell.col_span,
                    header: cell.header,
                },
                text: cell.text,
                spans,
            });
        }
        if self.tables.is_empty() {
            self.blocks.extend(out);
            self.blocks.append(&mut self.nested);
        } else {
            self.nested.extend(out);
        }
    }
}

impl TableState {
    /// Widen the merge tracking to `width` columns.
    fn grow(&mut self, width: usize) {
        if self.merges.len() < width {
            self.merges.resize(width, None);
            self.continued.resize(width, false);
        }
    }
}

/// The heading level of a paragraph style id or name: `Heading1`..`Heading9`
/// (tolerating the `Heading 1` spelling and ASCII case) and `Title` at level 1.
pub(crate) fn heading_level_from_style(style: &str) -> Option<u8> {
    let compact: String = style
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    if compact == "title" {
        return Some(1);
    }
    let digits = compact
        .strip_prefix("heading")
        .filter(|rest| !rest.is_empty())?;
    let level: u8 = digits.parse().ok()?;
    (1..=9).contains(&level).then_some(level)
}

/// An `ST_OnOff` value: absent means on, and only the explicit offs are off.
fn is_on(value: Option<&str>) -> bool {
    match value.map(str::trim) {
        None => true,
        Some(value) => !matches!(
            value.to_ascii_lowercase().as_str(),
            "false" | "0" | "off" | "none"
        ),
    }
}

/// The target of a `HYPERLINK "url"` field instruction, if it has one.
fn hyperlink_field_target(instr: &str) -> Option<String> {
    let rest = instr
        .trim_start()
        .get(.."HYPERLINK".len())
        .filter(|prefix| prefix.eq_ignore_ascii_case("hyperlink"))
        .and(instr.trim_start().get("HYPERLINK".len()..))?;
    let quoted = rest.trim_start().strip_prefix('"')?;
    quoted.split('"').next().map(str::to_string)
}

/// Render a paragraph's runs as inline Markdown, merging neighbours that share
/// their formatting. Returns the text plus one plain span per merged run: the
/// formatting already lives in the text, so styled span flags would mark every
/// run a second time in [`pdf_engine::to_markdown`].
fn render_inlines(inlines: Vec<Inline>) -> (String, Vec<TextSpan>) {
    let mut merged: Vec<Inline> = Vec::with_capacity(inlines.len());
    for inline in inlines {
        let Inline::Run {
            text,
            bold,
            italic,
            link,
        } = inline
        else {
            merged.push(inline);
            continue;
        };
        match merged.last_mut() {
            Some(Inline::Run {
                text: previous,
                bold: same_bold,
                italic: same_italic,
                link: same_link,
            }) if *same_bold == bold && *same_italic == italic && *same_link == link => {
                previous.push_str(&text);
            }
            _ => merged.push(Inline::Run {
                text,
                bold,
                italic,
                link,
            }),
        }
    }
    let mut out = String::new();
    let mut spans = Vec::new();
    for inline in merged {
        match inline {
            Inline::Run {
                text,
                bold,
                italic,
                link,
            } => {
                if text.is_empty() {
                    continue;
                }
                let mut inner = escape_run(&text, bold || italic, link.is_some());
                if bold && italic {
                    inner = format!("***{inner}***");
                } else if bold {
                    inner = format!("**{inner}**");
                } else if italic {
                    inner = format!("*{inner}*");
                }
                if let Some(url) = link {
                    inner = format!("[{inner}]({})", escape_url(&url));
                }
                spans.push(TextSpan {
                    text,
                    ..Default::default()
                });
                out.push_str(&inner);
            }
            Inline::Image { alt, target } => {
                let rendered = format!("![{}]({})", escape_link_text(&alt), escape_url(&target));
                spans.push(TextSpan {
                    text: rendered.clone(),
                    ..Default::default()
                });
                out.push_str(&rendered);
            }
        }
    }
    (out, spans)
}

/// Escape what the wrappers would otherwise read as syntax: `*` inside a
/// styled run, brackets inside a link, and backslashes everywhere.
fn escape_run(text: &str, styled: bool, linked: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '*' if styled => out.push_str("\\*"),
            '[' | ']' if linked => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// Escape link and alt text: brackets and backslashes only.
fn escape_link_text(text: &str) -> String {
    escape_run(text, false, true)
}

/// Percent-encode what would end a `(target)`: spaces and parentheses.
fn escape_url(url: &str) -> String {
    let mut out = String::with_capacity(url.len());
    for c in url.chars() {
        match c {
            ' ' => out.push_str("%20"),
            '(' => out.push_str("%28"),
            ')' => out.push_str("%29"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_styles_map_to_levels() {
        assert_eq!(heading_level_from_style("Heading1"), Some(1));
        assert_eq!(heading_level_from_style("Heading9"), Some(9));
        assert_eq!(heading_level_from_style("Heading 2"), Some(2));
        assert_eq!(heading_level_from_style("heading3"), Some(3));
        assert_eq!(heading_level_from_style("Heading10"), None);
        assert_eq!(heading_level_from_style("Heading"), None);
        assert_eq!(heading_level_from_style("Normal"), None);
        assert_eq!(heading_level_from_style("Title"), Some(1));
        assert_eq!(heading_level_from_style("Subtitle"), None);
    }

    #[test]
    fn on_off_values_toggle() {
        assert!(is_on(None));
        assert!(is_on(Some("true")));
        assert!(is_on(Some("1")));
        assert!(is_on(Some("on")));
        assert!(!is_on(Some("false")));
        assert!(!is_on(Some("0")));
        assert!(!is_on(Some("off")));
    }

    #[test]
    fn hyperlink_field_instructions_yield_their_target() {
        assert_eq!(
            hyperlink_field_target(r#"HYPERLINK "https://example.com/a" \o "tip""#),
            Some("https://example.com/a".to_string())
        );
        assert_eq!(hyperlink_field_target("PAGEREF _Toc1"), None);
        assert_eq!(hyperlink_field_target("HYPERLINK"), None);
    }
}
