//! One spine item: an XHTML chapter turned into [`Block`]s.
//!
//! The mapping is structural, never typographic: the chapter states what each
//! element is, so `h1`–`h6` become headings at levels 1–6, `p` becomes a
//! paragraph, `li` a list item at its nesting depth, and `td`/`th` table cells
//! with their `colspan`/`rowspan` as spans. `strong`/`b` and `em`/`i` become
//! inline Markdown (`**bold**`, `*italic*`); `a[href]` and `img` stay inline
//! (`[text](href)`, `![alt](src)`) inside a text-bearing element and become
//! [`Link`](crate::BlockKind::Link)/[`Image`](crate::BlockKind::Image) blocks
//! at the top level. `blockquote` bounds paragraphs without marking them:
//! a quote marker written into the text would come out backslashed from the
//! engine's renderer. `pre` becomes one paragraph holding its text verbatim
//! (whitespace and line breaks kept). Every other element is a transparent
//! container its text passes through, except `head`, `script`, `style` and
//! `svg`, whose subtrees are skipped, and `hr`, which becomes a `***`
//! paragraph.
//!
//! LENIENT WHERE A STRICT READER WOULD REFUSE. Chapters in the wild carry
//! HTML habits into XHTML: void elements without the self-closing slash,
//! uppercase tag names, `&nbsp;` and other HTML entities, a bare `&`. Tag and
//! attribute names are matched case-insensitively, voids are complete without
//! an end tag, a short table of HTML entities is resolved on top of XML's
//! five, and text that still does not unescape is kept verbatim rather than
//! failing the book. End tags are not checked against a stack: every end tag
//! closes whatever is open, so one unbalanced element cannot corrupt the rest
//! of the chapter.
//!
//! TWO FLATTENINGS, both deliberate. Block structure inside a table cell
//! (`<td><p>…`) flattens into the cell's text, because a cell is one block
//! and a paragraph inside it would need a second shape of one. And a `p`
//! directly inside an `li` or `td` keeps the item's or cell's kind rather
//! than becoming a paragraph, because `<li><p>text</p></li>` is one item that
//! happens to be wrapped, not a list followed by a paragraph.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use quick_xml::events::{BytesStart, BytesText, Event};
use quick_xml::name::LocalName;
use quick_xml::Reader;

use crate::blocks::{Block, BlockKind};
use crate::error::Result;

/// Elements whose whole subtree is skipped: metadata, code, styling and
/// vector graphics, none of which is running text.
const SKIP_ELEMENTS: [&str; 4] = ["head", "script", "style", "svg"];

/// HTML void elements: complete without an end tag even when written without
/// the self-closing slash, so a `Start` is handled as a whole `Empty`.
const VOID_ELEMENTS: [&str; 13] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

/// Largest `colspan`/`rowspan` honoured. A span past this is a corrupt or
/// hostile attribute, and clamping it bounds the occupancy grid.
const MAX_SPAN: u16 = 512;

/// Block-level elements whose text flattens into the surrounding table cell
/// rather than opening new blocks (see the module documentation).
fn is_cell_flattened(name: &str) -> bool {
    matches!(
        name,
        "h1" | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "p"
            | "li"
            | "ul"
            | "ol"
            | "table"
            | "tr"
            | "td"
            | "th"
            | "thead"
            | "tbody"
            | "tfoot"
            | "blockquote"
            | "pre"
    )
}

/// Elements whose subtree contributes nothing inside `pre`: block elements
/// and skipped subtrees carry no pre text of their own.
fn is_pre_skipped(name: &str) -> bool {
    matches!(
        name,
        "h1" | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "p"
            | "div"
            | "section"
            | "article"
            | "main"
            | "blockquote"
            | "pre"
            | "dd"
            | "dt"
            | "figcaption"
            | "address"
            | "ul"
            | "ol"
            | "li"
            | "table"
            | "tr"
            | "td"
            | "th"
            | "thead"
            | "tbody"
            | "tfoot"
            | "hr"
            | "figure"
            | "form"
            | "fieldset"
            | "details"
            | "summary"
            | "nav"
            | "header"
            | "footer"
            | "aside"
            | "script"
            | "style"
            | "noscript"
            | "head"
            | "template"
            | "dialog"
            | "svg"
    )
}

/// Named entities beyond XML's five predefined ones. EPUB chapters are XHTML
/// but written with HTML habits, and `&nbsp;` alone would otherwise fail
/// every other book.
fn resolve_entity(name: &str) -> Option<&'static str> {
    match name {
        "amp" => Some("&"),
        "lt" => Some("<"),
        "gt" => Some(">"),
        "quot" => Some("\""),
        "apos" => Some("'"),
        "nbsp" => Some("\u{a0}"),
        "copy" => Some("©"),
        "reg" => Some("®"),
        "trade" => Some("™"),
        "hellip" => Some("…"),
        "mdash" => Some("—"),
        "ndash" => Some("–"),
        "lsquo" => Some("‘"),
        "rsquo" => Some("’"),
        "ldquo" => Some("“"),
        "rdquo" => Some("”"),
        "laquo" => Some("«"),
        "raquo" => Some("»"),
        "sect" => Some("§"),
        "para" => Some("¶"),
        "middot" => Some("·"),
        "bull" => Some("•"),
        "dagger" => Some("†"),
        "Dagger" => Some("‡"),
        "times" => Some("×"),
        "divide" => Some("÷"),
        "plusmn" => Some("±"),
        _ => None,
    }
}

/// The text of a text event, entities resolved, kept verbatim when it is not
/// well-formed entity soup (a bare `&`, an unknown name) rather than failing
/// the chapter.
pub(crate) fn text_content(event: BytesText<'_>) -> String {
    match event.unescape_with(resolve_entity) {
        Ok(text) => text.into_owned(),
        Err(_) => String::from_utf8_lossy(&event.into_inner()).into_owned(),
    }
}

/// The value of the attribute `key` on `element`, matched case-insensitively
/// (chapters write `HREF` too), entities resolved with the same table and
/// fallback as [`text_content`].
pub(crate) fn attr_value(element: &BytesStart, key: &str) -> Option<String> {
    for attr in element.attributes().flatten() {
        if attr
            .key
            .local_name()
            .as_ref()
            .eq_ignore_ascii_case(key.as_bytes())
        {
            return Some(match attr.unescape_value_with(resolve_entity) {
                Ok(value) => value.into_owned(),
                Err(_) => String::from_utf8_lossy(&attr.value).into_owned(),
            });
        }
    }
    None
}

/// A `colspan`/`rowspan` attribute as a span: 1 when absent or unparsable,
/// clamped past [`MAX_SPAN`].
fn span(element: &BytesStart, key: &str) -> u16 {
    attr_value(element, key)
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(1)
        .clamp(1, MAX_SPAN)
}

/// A tag name lowercased for matching, namespace prefix dropped.
fn tag_name(name: LocalName<'_>) -> String {
    String::from_utf8_lossy(name.as_ref()).to_ascii_lowercase()
}

fn is_void(name: &str) -> bool {
    VOID_ELEMENTS.contains(&name)
}

fn is_skipped(name: &str) -> bool {
    SKIP_ELEMENTS.contains(&name)
}

/// Collapse each line's whitespace runs to one space and drop blank lines,
/// keeping the `\n` breaks `br` wrote. Whitespace-only input normalises to
/// the empty string, which is what drops empty blocks.
pub(crate) fn normalize(raw: &str) -> String {
    raw.split('\n')
        .filter_map(|line| {
            let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
            (!line.is_empty()).then_some(line)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A frame on the open-element stack. Every start tag pushes exactly one, and
/// every end tag pops one however it is named, which is what keeps one
/// unbalanced element from corrupting the chapter.
enum Open {
    /// Transparent container: `div`, `body`, stray elements, and anything
    /// flattened inside a cell.
    Flow,
    /// A text-bearing block (`p`, heading, top-level link) accumulating in
    /// [`Parser::current`].
    Block,
    /// A list item: like [`Open::Block`](Open::Block), except a `p` or
    /// heading directly inside keeps the item's kind (see the module docs).
    Item,
    /// A table cell: like [`Open::Item`](Open::Item) for cells.
    Cell,
    /// An inline marker to close in the current text.
    Inline(Inline),
    /// A `ul`/`ol` container.
    List,
    /// A `table` container.
    Table,
    /// A `tr` being read.
    Row,
    /// A `thead`: cells inside are headers whatever their tag.
    Thead,
    /// A skipped subtree (`head`, `script`, `style`, `svg`).
    Skip,
    /// A `blockquote`: bounds the paragraphs inside it without marking them.
    Quote,
    /// A `pre`: its text accumulates verbatim in [`Parser::pre`].
    Pre,
}

/// An inline marker open in the current text.
enum Inline {
    Bold,
    Italic,
    Link(String),
}

/// A block accumulating text.
struct Current {
    kind: BlockKind,
    text: String,
}

/// A list being read.
struct ListCtx {
    ordered: bool,
    /// How many items this list has produced; the next `ol` item is one more.
    next: u32,
}

/// A table being read.
struct TableCtx {
    next_row: u16,
    /// Per column, for how many rows including the current one a `rowspan`
    /// keeps it occupied; decremented when each row begins, so a positive
    /// value at placement time means "taken". Only grows with cells actually
    /// placed, never with the spans themselves, so a hostile `colspan`
    /// cannot widen it.
    occupied: Vec<u16>,
    in_row: bool,
    col: u16,
}

impl TableCtx {
    fn new() -> Self {
        Self {
            next_row: 0,
            occupied: Vec::new(),
            in_row: false,
            col: 0,
        }
    }

    fn begin_row(&mut self) {
        for slot in &mut self.occupied {
            *slot = slot.saturating_sub(1);
        }
        self.in_row = true;
        self.col = 0;
    }

    /// The grid position of the next cell, past the columns earlier rowspans
    /// still occupy, marking this cell's own span.
    fn place(&mut self, column_span: u16, row_span: u16) -> (u16, u16) {
        while (self.col as usize) < self.occupied.len() && self.occupied[self.col as usize] > 0 {
            self.col = self.col.saturating_add(1);
        }
        let column = self.col;
        let end = (column as usize).saturating_add(column_span as usize);
        if self.occupied.len() < end {
            self.occupied.resize(end, 0);
        }
        for slot in &mut self.occupied[column as usize..end] {
            *slot = (*slot).max(row_span);
        }
        self.col = self.col.saturating_add(column_span);
        (self.next_row, column)
    }
}

/// Parse one chapter into blocks, numbered from `first_index` in the book's
/// reading order.
pub(crate) fn parse_chapter(
    chapter_text: &str,
    chapter: usize,
    href: &str,
    first_index: usize,
) -> Result<Vec<Block>> {
    let mut reader = Reader::from_str(chapter_text);
    reader.config_mut().check_end_names = false;
    let mut parser = Parser::new(chapter, href, first_index);
    loop {
        match reader.read_event()? {
            Event::Start(event) => {
                let name = tag_name(event.local_name());
                if is_void(&name) {
                    parser.empty(&name, &event);
                } else {
                    parser.start(&name, &event);
                }
            }
            Event::Empty(event) => {
                let name = tag_name(event.local_name());
                parser.empty(&name, &event);
            }
            Event::End(event) => parser.end(&tag_name(event.local_name())),
            Event::Text(event) => parser.text(&text_content(event)),
            Event::CData(event) => {
                parser.text(&String::from_utf8_lossy(&event.into_inner()));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(parser.finish())
}

struct Parser {
    blocks: Vec<Block>,
    chapter: usize,
    href: String,
    next_index: usize,
    stack: Vec<Open>,
    current: Option<Current>,
    lists: Vec<ListCtx>,
    tables: Vec<TableCtx>,
    thead_depth: usize,
    cell_depth: usize,
    skip: usize,
    /// The open `pre`'s text, accumulating verbatim; `None` outside one.
    pre: Option<String>,
}

impl Parser {
    fn new(chapter: usize, href: &str, first_index: usize) -> Self {
        Self {
            blocks: Vec::new(),
            chapter,
            href: href.to_string(),
            next_index: first_index,
            stack: Vec::new(),
            current: None,
            lists: Vec::new(),
            tables: Vec::new(),
            thead_depth: 0,
            cell_depth: 0,
            skip: 0,
            pre: None,
        }
    }

    fn finish(mut self) -> Vec<Block> {
        self.finish_block();
        self.finish_pre();
        self.blocks
    }

    /// Emit the open `pre`, if there is one, as one paragraph holding its
    /// text verbatim: only the element's own edge newlines are stripped.
    fn finish_pre(&mut self) {
        let Some(raw) = self.pre.take() else {
            return;
        };
        let text = raw.trim_matches(['\n', '\r']);
        if text.trim().is_empty() {
            return;
        }
        self.blocks.push(Block::new(
            BlockKind::Paragraph,
            self.chapter,
            self.href.clone(),
            text.to_string(),
            self.next_index,
        ));
        self.next_index += 1;
    }

    /// Emit the accumulating block, dropping it when it holds no text. Cells
    /// are kept empty: a cell is a grid position whether or not it has words.
    fn finish_block(&mut self) {
        let Some(current) = self.current.take() else {
            return;
        };
        let text = normalize(&current.text);
        if text.is_empty() && !matches!(current.kind, BlockKind::TableCell { .. }) {
            return;
        }
        self.blocks.push(Block::new(
            current.kind,
            self.chapter,
            self.href.clone(),
            text,
            self.next_index,
        ));
        self.next_index += 1;
    }

    /// A word space between runs of text flattened together, unless one is
    /// already there. [`normalize`] would collapse a doubled one anyway; this
    /// keeps the buffer readable before that.
    fn separator(&mut self) {
        if let Some(current) = &mut self.current {
            if !current.text.is_empty() && !current.text.ends_with([' ', '\n']) {
                current.text.push(' ');
            }
        }
    }

    /// A paragraph to hold inline content standing on its own: formatting and
    /// stray text at the top level, which have no block to belong to.
    fn ensure_paragraph(&mut self) {
        if self.current.is_none() {
            self.current = Some(Current {
                kind: BlockKind::Paragraph,
                text: String::new(),
            });
        }
    }

    /// Open a `p` or heading, except directly inside an `li` or cell that has
    /// no text yet, where the wrapper is adopted and the item's or cell's
    /// kind kept (see the module documentation).
    fn start_text_block(&mut self, kind: BlockKind) {
        if matches!(self.stack.last(), Some(Open::Item | Open::Cell)) {
            if let Some(current) = &mut self.current {
                if current.text.trim().is_empty() {
                    current.text.clear();
                    self.stack.push(Open::Flow);
                    return;
                }
            }
        }
        self.finish_block();
        self.current = Some(Current {
            kind,
            text: String::new(),
        });
        self.stack.push(Open::Block);
    }

    fn start(&mut self, name: &str, element: &BytesStart) {
        if self.skip > 0 {
            self.stack.push(Open::Skip);
            return;
        }
        if self.cell_depth > 0 && is_cell_flattened(name) {
            self.separator();
            self.stack.push(Open::Flow);
            return;
        }
        if self.pre.is_some() {
            // Inside `pre` only inline elements are transparent; a block or
            // a skipped subtree contributes nothing of its own.
            if is_pre_skipped(name) {
                self.skip += 1;
                self.stack.push(Open::Skip);
            } else {
                self.stack.push(Open::Flow);
            }
            return;
        }
        match name {
            _ if is_skipped(name) => {
                self.skip += 1;
                self.stack.push(Open::Skip);
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let level = name.as_bytes()[1] - b'0';
                self.start_text_block(BlockKind::Heading { level });
            }
            "p" => self.start_text_block(BlockKind::Paragraph),
            "blockquote" => {
                self.finish_block();
                self.stack.push(Open::Quote);
            }
            "pre" => {
                self.finish_block();
                self.pre = Some(String::new());
                self.stack.push(Open::Pre);
            }
            "ul" => {
                self.finish_block();
                self.lists.push(ListCtx {
                    ordered: false,
                    next: 0,
                });
                self.stack.push(Open::List);
            }
            "ol" => {
                self.finish_block();
                self.lists.push(ListCtx {
                    ordered: true,
                    next: 0,
                });
                self.stack.push(Open::List);
            }
            "li" => {
                self.finish_block();
                let level = u8::try_from(self.lists.len().saturating_sub(1)).unwrap_or(u8::MAX);
                let ordinal = match self.lists.last_mut() {
                    Some(list) if list.ordered => {
                        list.next += 1;
                        Some(list.next)
                    }
                    _ => None,
                };
                self.current = Some(Current {
                    kind: BlockKind::ListItem { level, ordinal },
                    text: String::new(),
                });
                self.stack.push(Open::Item);
            }
            "table" => {
                self.finish_block();
                self.tables.push(TableCtx::new());
                self.stack.push(Open::Table);
            }
            "thead" => {
                self.thead_depth += 1;
                self.stack.push(Open::Thead);
            }
            "tr" => {
                if self.tables.is_empty() {
                    self.stack.push(Open::Flow);
                } else {
                    self.finish_block();
                    if let Some(table) = self.tables.last_mut() {
                        table.begin_row();
                    }
                    self.stack.push(Open::Row);
                }
            }
            "td" | "th" => {
                let column_span = span(element, "colspan");
                let row_span = span(element, "rowspan");
                let header = name == "th" || self.thead_depth > 0;
                let placement = self
                    .tables
                    .last_mut()
                    .filter(|table| table.in_row)
                    .map(|table| table.place(column_span, row_span));
                match placement {
                    Some((row, column)) => {
                        self.finish_block();
                        self.current = Some(Current {
                            kind: BlockKind::TableCell {
                                row,
                                column,
                                row_span,
                                column_span,
                                header,
                            },
                            text: String::new(),
                        });
                        self.cell_depth += 1;
                        self.stack.push(Open::Cell);
                    }
                    None => self.stack.push(Open::Flow),
                }
            }
            "a" => match attr_value(element, "href") {
                Some(href) => match &mut self.current {
                    Some(current) => {
                        current.text.push('[');
                        self.stack.push(Open::Inline(Inline::Link(href)));
                    }
                    None => {
                        self.current = Some(Current {
                            kind: BlockKind::Link { href },
                            text: String::new(),
                        });
                        self.stack.push(Open::Block);
                    }
                },
                None => self.stack.push(Open::Flow),
            },
            "strong" | "b" => {
                self.ensure_paragraph();
                self.push_inline("**", Inline::Bold);
            }
            "em" | "i" => {
                self.ensure_paragraph();
                self.push_inline("*", Inline::Italic);
            }
            _ => self.stack.push(Open::Flow),
        }
    }

    fn push_inline(&mut self, marker: &str, inline: Inline) {
        if let Some(current) = &mut self.current {
            current.text.push_str(marker);
        }
        self.stack.push(Open::Inline(inline));
    }

    /// A complete element: `img`, `br` and `hr` act, everything else opens
    /// and closes at once (an empty `td` still places its cell).
    fn empty(&mut self, name: &str, element: &BytesStart) {
        if self.skip > 0 {
            return;
        }
        if self.pre.is_some() {
            // Inside `pre` a line break breaks the line; an image or a rule
            // contributes nothing, and anything else opens and closes at
            // once through the same transparent-or-skipped rule as `start`.
            match name {
                "br" => {
                    if let Some(pre) = &mut self.pre {
                        pre.push('\n');
                    }
                }
                "img" | "hr" => {}
                _ => {
                    self.start(name, element);
                    self.end(name);
                }
            }
            return;
        }
        match name {
            "img" => self.image(element),
            "br" => {
                if let Some(current) = &mut self.current {
                    current.text.push('\n');
                }
            }
            "hr" => {
                if self.cell_depth > 0 {
                    self.separator();
                } else {
                    self.finish_block();
                    self.current = Some(Current {
                        kind: BlockKind::Paragraph,
                        text: "***".to_string(),
                    });
                    self.finish_block();
                }
            }
            _ => {
                self.start(name, element);
                self.end(name);
            }
        }
    }

    /// An image: inline (`![alt](src)`) inside a text-bearing element, an
    /// [`Image`](crate::BlockKind::Image) block at the top level.
    fn image(&mut self, element: &BytesStart) {
        let src = attr_value(element, "src").unwrap_or_default();
        let alt = attr_value(element, "alt").unwrap_or_default();
        match &mut self.current {
            Some(current) => {
                current.text.push_str("![");
                current.text.push_str(&alt);
                current.text.push_str("](");
                current.text.push_str(&src);
                current.text.push(')');
            }
            None => {
                if src.is_empty() && alt.is_empty() {
                    return;
                }
                self.blocks.push(Block::new(
                    BlockKind::Image {
                        src,
                        alt: alt.clone(),
                    },
                    self.chapter,
                    self.href.clone(),
                    alt,
                    self.next_index,
                ));
                self.next_index += 1;
            }
        }
    }

    fn end(&mut self, name: &str) {
        if is_void(name) {
            return;
        }
        match self.stack.pop() {
            None | Some(Open::Flow) => {}
            Some(Open::Skip) => {
                self.skip = self.skip.saturating_sub(1);
            }
            Some(Open::Quote) => self.finish_block(),
            Some(Open::Pre) => self.finish_pre(),
            Some(Open::Block | Open::Item) => self.finish_block(),
            Some(Open::Cell) => {
                self.cell_depth = self.cell_depth.saturating_sub(1);
                self.finish_block();
            }
            Some(Open::Inline(inline)) => {
                if let Some(current) = &mut self.current {
                    match inline {
                        Inline::Bold => current.text.push_str("**"),
                        Inline::Italic => current.text.push('*'),
                        Inline::Link(href) => {
                            current.text.push_str("](");
                            current.text.push_str(&href);
                            current.text.push(')');
                        }
                    }
                }
            }
            Some(Open::List) => {
                self.finish_block();
                self.lists.pop();
            }
            Some(Open::Table) => {
                self.finish_block();
                self.tables.pop();
            }
            Some(Open::Row) => {
                self.finish_block();
                if let Some(table) = self.tables.last_mut() {
                    table.in_row = false;
                    table.next_row = table.next_row.saturating_add(1);
                }
            }
            Some(Open::Thead) => {
                self.thead_depth = self.thead_depth.saturating_sub(1);
            }
        }
    }

    fn text(&mut self, text: &str) {
        if self.skip > 0 {
            return;
        }
        if let Some(pre) = &mut self.pre {
            pre.push_str(text);
            return;
        }
        match &mut self.current {
            Some(current) => current.text.push_str(text),
            None if text.trim().is_empty() => {}
            None => {
                self.current = Some(Current {
                    kind: BlockKind::Paragraph,
                    text: text.to_string(),
                });
            }
        }
    }
}
