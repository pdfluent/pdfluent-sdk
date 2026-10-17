#![warn(missing_docs)]
//! HTML documents and fragments to engine blocks.
//!
//! One pass with the `html5ever` tokenizer over the input. Block elements
//! become blocks, tables become runs of cell blocks in row-major order, and
//! inline elements become inline Markdown inside the block text. Sloppy input
//! is closed the way a browser would: an open paragraph ends where the next
//! block starts, and stray end tags are ignored.
//!
//! # Mapping
//!
//! | HTML | Engine output |
//! |---|---|
//! | `h1`..`h6` | `BlockKind::Heading { level }` (1..=6) |
//! | `p`, bare text | `BlockKind::Paragraph` |
//! | `ul`/`ol`/`li` | `BlockKind::ListItem { level, ordinal }` (one block per item) |
//! | `table` (`colspan`, `rowspan`) | runs of `BlockKind::TableCell` in row-major order |
//! | `a[href]` | inline `[text](url)` in the block text |
//! | `strong`/`b`, `em`/`i`, `s` | inline `**bold**`, `*italic*`, `~~strike~~` |
//! | `pre` | one paragraph holding the code verbatim (whitespace and line breaks kept) |
//! | inline `code` | inline `` `code` `` in the block text |
//! | `img` | inline `![alt](src)` reference (no rendering) |
//! | `blockquote` | its paragraphs as plain paragraphs (no quote marker) |
//! | `script`, `style`, `nav`, `footer`, `del` | dropped |
//! | `hidden`, `aria-hidden="true"`, inline `display:none`/`visibility:hidden` | subtree dropped |
//!
//! # Limitations (MVP)
//!
//! * `head` never surfaces: the title and metadata are not body text.
//! * A table inside a cell is its own table, emitted after the outer one;
//!   lists inside table cells still flatten to text.
//! * A second block inside one list item (a loose list) becomes its own item
//!   block with the same level and ordinal.
//! * `hr` has no block shape and is skipped; `sub`/`sup` keep their text
//!   without marks.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::cell::RefCell;

use html5ever::tendril::StrTendril;
use html5ever::tokenizer::{
    BufferQueue, Tag, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
};
use pdf_engine::{Block, BlockKind, TextSpan};

use crate::inline::{render_inlines, Inline};

/// Extract the structured blocks of an HTML document or fragment.
///
/// The tokenizer accepts what a browser accepts; nothing here can fail, so
/// there is no `Result`. Blocks carry `page: 0` and are numbered in document
/// order: markup has no pages.
///
/// # Example
///
/// ```
/// let blocks = markup_extract::html::extract_blocks("<h1>Title</h1><p>Body.</p>");
/// assert!(matches!(
///     blocks[0].kind,
///     markup_extract::BlockKind::Heading { level: 1 }
/// ));
/// ```
pub fn extract_blocks(html: &str) -> Vec<Block> {
    let sink = BlockSink::default();
    let tokenizer = Tokenizer::new(sink, TokenizerOpts::default());
    let queue = BufferQueue::default();
    queue.push_back(StrTendril::from(html));
    let _ = tokenizer.feed(&queue);
    tokenizer.end();
    let mut state = tokenizer.sink.state.borrow_mut();
    state.unwind_to_top();
    std::mem::take(&mut state.blocks)
}

/// Extract an HTML document or fragment directly as Markdown.
///
/// This is [`extract_blocks`] followed by the engine's
/// [`pdf_engine::to_markdown`], so markup and PDFs serialize through the
/// same code.
pub fn extract_markdown(html: &str) -> String {
    pdf_engine::to_markdown(&extract_blocks(html))
}

/// What a paragraph-level frame becomes.
#[derive(Debug, Clone, Copy)]
enum FlowKind {
    Paragraph,
    Heading(u8),
    Item { level: u8, ordinal: Option<u32> },
}

/// A paragraph, heading or list item under construction.
#[derive(Debug)]
struct ParaFrame {
    kind: FlowKind,
    inlines: Vec<Inline>,
    /// A collapsed whitespace run is still owed before the next word.
    pending_space: bool,
    /// Where the text goes when the frame closes: appended to this block,
    /// for an item resumed after a nested list, instead of a block of its
    /// own.
    append_to: Option<usize>,
}

/// A list under construction: ordered or not, and the next ordinal.
#[derive(Debug)]
struct ListFrame {
    ordered: bool,
    next: u32,
}

/// A table cell under construction.
#[derive(Debug)]
struct CellFrame {
    header: bool,
    col_span: u16,
    row_span: u16,
    inlines: Vec<Inline>,
    pending_space: bool,
}

/// A placed table cell, waiting for the table to close.
#[derive(Debug)]
struct PlacedCell {
    row: u16,
    col: u16,
    row_span: u16,
    col_span: u16,
    header: bool,
    text: String,
    spans: Vec<TextSpan>,
}

/// A table under construction: its cells plus which grid slots spans cover.
#[derive(Debug, Default)]
struct TableFrame {
    cells: Vec<PlacedCell>,
    /// Per grid row, one flag per grid column.
    grid: Vec<Vec<bool>>,
    /// The grid row the next `tr` fills.
    row: u16,
    /// Document order of the `<table>` start tag: nested tables emit after
    /// their outer table, innermost document order first.
    order: u64,
}

/// A `pre` block under construction: its raw text, kept verbatim.
#[derive(Debug)]
struct PreFrame {
    text: String,
}

/// One open element that owns extraction state.
#[derive(Debug)]
enum Frame {
    Para(ParaFrame),
    List(ListFrame),
    Table(TableFrame),
    Row,
    Cell(CellFrame),
    Pre(PreFrame),
    /// Inline `code`: its text stays literal until the end tag.
    Code(String),
}

/// The item whose paragraph a nested block flushed early: `<li><p>a</p><p>b`
/// still belongs to one item, so the second paragraph opens as an item too.
/// Tagged with the list depth it was pushed at, so a list's end drops what
/// belongs to it.
#[derive(Debug)]
struct PendingItem {
    depth: usize,
    level: u8,
    ordinal: Option<u32>,
}

/// An item interrupted by a nested list: `<li>Before<ul>…</ul>After</li>`
/// is one item, so the text before the list is emitted at once (keeping the
/// outer item ahead of the nested one) and the text after it is appended to
/// the same block when the item resumes. Tagged like [`PendingItem`].
#[derive(Debug)]
struct SuspendedItem {
    depth: usize,
    kind: FlowKind,
    /// The emitted block the resumed text appends to, or `None` where the
    /// text before the list was empty and no block was emitted.
    block: Option<usize>,
}

#[derive(Debug, Default)]
struct State {
    blocks: Vec<Block>,
    stack: Vec<Frame>,
    /// Open hyperlinks, innermost last; `None` for an `a` without `href`.
    links: Vec<Option<String>>,
    bold: u32,
    italic: u32,
    strike: u32,
    /// Elements whose content is dropped (`skip_name`), possibly nested.
    skip_depth: u32,
    /// Open element names inside a hidden subtree ([`is_hidden`]), innermost
    /// last; empty outside one.
    hidden: Vec<String>,
    /// Nested tables closed while an outer table is still open, flushed in
    /// document order when the outermost table emits.
    nested_tables: Vec<TableFrame>,
    /// Document order of the next `<table>` start tag.
    next_table: u64,
    pending: Vec<PendingItem>,
    /// Items interrupted by a nested list, innermost last.
    suspended: Vec<SuspendedItem>,
}

#[derive(Debug, Default)]
struct BlockSink {
    state: RefCell<State>,
}

impl TokenSink for BlockSink {
    type Handle = ();

    fn process_token(&self, token: Token, _line: u64) -> TokenSinkResult<()> {
        self.state.borrow_mut().token(token);
        TokenSinkResult::Continue
    }
}

/// The tag's name. The tokenizer lowercases ASCII names, so this matches the
/// lowercase literals below.
fn name(tag: &Tag) -> &str {
    &tag.name
}

/// The value of the attribute called `key`, or `None` where the tag carries
/// no such attribute.
fn attr<'a>(tag: &'a Tag, key: &str) -> Option<&'a str> {
    tag.attrs
        .iter()
        .find(|a| &*a.name.local == key)
        .map(|a| &*a.value)
}

/// Elements whose whole subtree is dropped: metadata, styling, navigation,
/// page furniture, and deletions (which `docx-extract` drops too).
fn skip_name(name: &str) -> bool {
    matches!(name, "head" | "script" | "style" | "nav" | "footer" | "del")
}

/// Whether the tag hides its subtree: the `hidden` attribute (present means
/// hidden, whatever its value), `aria-hidden="true"`, or an inline style
/// burying the element. No stylesheet is read.
fn is_hidden(tag: &Tag) -> bool {
    if attr(tag, "hidden").is_some() {
        return true;
    }
    if attr(tag, "aria-hidden").is_some_and(|v| v.trim().eq_ignore_ascii_case("true")) {
        return true;
    }
    if let Some(style) = attr(tag, "style") {
        let flat: String = style.chars().filter(|c| !c.is_whitespace()).collect();
        let lower = flat.to_ascii_lowercase();
        if lower.contains("display:none") || lower.contains("visibility:hidden") {
            return true;
        }
    }
    false
}

/// Void elements: no end tag follows, so they never open a hidden level.
fn void_name(name: &str) -> bool {
    matches!(
        name,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

/// Block containers that carry no semantics of their own: they end the open
/// paragraph and hold no frame. `blockquote` is one of them: its paragraphs
/// are plain paragraphs, since a `>` marker written into the text would come
/// out backslashed from the engine's renderer.
fn transparent_block(name: &str) -> bool {
    matches!(
        name,
        "address"
            | "article"
            | "aside"
            | "blockquote"
            | "body"
            | "dialog"
            | "div"
            | "dl"
            | "dt"
            | "dd"
            | "figcaption"
            | "figure"
            | "form"
            | "header"
            | "html"
            | "main"
            | "search"
            | "section"
    )
}

/// A block-level start inside a cell cannot open a block; it breaks the line.
fn cell_break(name: &str) -> bool {
    matches!(
        name,
        "p" | "div"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "li"
            | "ul"
            | "ol"
            | "pre"
            | "table"
            | "tr"
            | "td"
            | "th"
            | "hr"
    ) || transparent_block(name)
}

/// A span attribute: `colspan`/`rowspan`, 1 where absent or unparsable,
/// clamped to the HTML maxima.
fn span_attr(tag: &Tag, key: &str, max: u16) -> u16 {
    attr(tag, key)
        .and_then(|v| v.trim().parse::<u16>().ok())
        .filter(|v| *v >= 1)
        .map(|v| v.min(max))
        .unwrap_or(1)
}

impl State {
    fn token(&mut self, token: Token) {
        match token {
            Token::TagToken(tag) => self.tag(tag),
            Token::CharacterTokens(text) => self.chars(&text),
            // Comments, the doctype and parse errors carry no blocks.
            _ => {}
        }
    }

    fn tag(&mut self, tag: Tag) {
        // While skipping, only the skip elements themselves still count:
        // script and style tokenize as plain text here (rawtext switching is
        // the tree builder's), so a `<b` inside JavaScript must not leak
        // formatting past the script. Valid markup never holds a literal
        // `</script>` inside a script, so depth counting is exact.
        if self.skip_depth > 0 {
            if skip_name(name(&tag)) {
                match tag.kind {
                    TagKind::StartTag => self.skip_depth += 1,
                    TagKind::EndTag => self.skip_depth -= 1,
                }
            }
            return;
        }
        if !self.hidden.is_empty() || (matches!(tag.kind, TagKind::StartTag) && is_hidden(&tag)) {
            self.hidden_tag(tag);
            return;
        }
        match tag.kind {
            TagKind::StartTag => self.start(tag),
            TagKind::EndTag => self.end(name(&tag)),
        }
    }

    /// One tag inside (or opening) a hidden subtree: start tags open a
    /// level, end tags close up to and including their opener, and a stray
    /// end tag matches nothing.
    fn hidden_tag(&mut self, tag: Tag) {
        let tag_name = name(&tag).to_string();
        match tag.kind {
            TagKind::StartTag => {
                if !tag.self_closing && !void_name(&tag_name) {
                    self.hidden.push(tag_name);
                }
            }
            TagKind::EndTag => {
                if let Some(at) = self.hidden.iter().rposition(|open| *open == tag_name) {
                    self.hidden.truncate(at);
                }
            }
        }
    }

    fn start(&mut self, tag: Tag) {
        let tag_name = name(&tag);
        if skip_name(tag_name) {
            self.skip_depth += 1;
            return;
        }
        // Inside `pre` the tags are transparent: only `br` breaks the line
        // and `img` leaves its alt text.
        if self.stack.iter().any(|f| matches!(f, Frame::Pre(_))) {
            match tag_name {
                "br" => self.pre_text("\n"),
                "img" => {
                    if let Some(alt) = attr(&tag, "alt") {
                        let alt = alt.to_string();
                        self.pre_text(&alt);
                    }
                }
                _ => {}
            }
            return;
        }
        // Inside a cell, blocks cannot open: they break the line instead,
        // and the cell keeps the words without their structure. A table is
        // the exception: it is its own table, parsed on top of the cell.
        if matches!(self.stack.last(), Some(Frame::Cell(_))) {
            match tag_name {
                "a" => self.links.push(attr(&tag, "href").map(str::to_string)),
                "strong" | "b" => self.bold += 1,
                "em" | "i" => self.italic += 1,
                "s" => self.strike += 1,
                "code" => self.push_code(),
                "br" => self.line_break(),
                "img" => self.image(&tag),
                "table" => {
                    if let Some(Frame::Cell(cell)) = self.stack.last_mut() {
                        if !cell.inlines.is_empty() {
                            cell.pending_space = true;
                        }
                    }
                    let table = self.new_table();
                    self.stack.push(Frame::Table(table));
                }
                _ if cell_break(tag_name) => self.line_break(),
                _ => {}
            }
            return;
        }
        match tag_name {
            "p" => {
                self.flush_para();
                let para = self.new_para(FlowKind::Paragraph);
                self.stack.push(Frame::Para(para));
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let level = tag_name.as_bytes()[1] - b'0';
                self.flush_para();
                let para = self.new_para(FlowKind::Heading(level));
                self.stack.push(Frame::Para(para));
            }
            "li" => {
                // A new item supersedes pending continuations of shallower
                // or equal depth; a deeper one belongs to an outer item.
                let depth = self.list_depth();
                self.pending.retain(|p| p.depth < depth);
                self.suspended.retain(|s| s.depth < depth);
                self.flush_para();
                // The level is 0-based where the depth counts lists: an item
                // in one list is level 0, a stray item too. Unclamped, like
                // the Markdown path: the nesting is the document's own.
                let level = depth.saturating_sub(1) as u8;
                let ordinal = self.take_ordinal(&tag);
                let para = self.new_para(FlowKind::Item { level, ordinal });
                self.stack.push(Frame::Para(para));
            }
            "ul" => {
                self.suspend_item();
                self.stack.push(Frame::List(ListFrame {
                    ordered: false,
                    next: 1,
                }));
            }
            "ol" => {
                self.suspend_item();
                let start = attr(&tag, "start")
                    .and_then(|v| v.trim().parse::<u32>().ok())
                    .filter(|v| *v >= 1)
                    .unwrap_or(1);
                self.stack.push(Frame::List(ListFrame {
                    ordered: true,
                    next: start,
                }));
            }
            "table" => {
                self.flush_para();
                let table = self.new_table();
                self.stack.push(Frame::Table(table));
            }
            "tr" => self.start_row(),
            "td" | "th" => self.start_cell(&tag, tag_name == "th"),
            "thead" | "tbody" | "tfoot" | "caption" | "colgroup" | "col" => {}
            "pre" => {
                self.flush_para();
                self.stack.push(Frame::Pre(PreFrame {
                    text: String::new(),
                }));
            }
            _ if transparent_block(tag_name) => self.flush_para(),
            "a" => {
                self.implied_para();
                self.links.push(attr(&tag, "href").map(str::to_string));
            }
            "strong" | "b" => self.bold += 1,
            "em" | "i" => self.italic += 1,
            "s" => self.strike += 1,
            "code" => {
                if !matches!(self.stack.last(), Some(Frame::Code(_))) {
                    self.implied_para();
                    self.push_code();
                }
            }
            "br" => {
                self.implied_para();
                self.line_break();
            }
            "img" => {
                self.implied_para();
                self.image(&tag);
            }
            // `hr` has no block shape; void and unknown elements are transparent.
            _ => {}
        }
    }

    fn end(&mut self, tag_name: &str) {
        if skip_name(tag_name) {
            self.skip_depth -= 1;
            return;
        }
        if self.stack.iter().any(|f| matches!(f, Frame::Pre(_))) {
            if tag_name == "pre" {
                self.pop_pre();
            }
            return;
        }
        match tag_name {
            "p" => self.close_para_kind(|k| matches!(k, FlowKind::Paragraph)),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let level = tag_name.as_bytes()[1] - b'0';
                self.close_para_kind(|k| matches!(k, FlowKind::Heading(l) if l == level));
            }
            "li" => {
                self.close_para_kind(|k| matches!(k, FlowKind::Item { .. }));
                self.pending.pop();
                // The item is over: its suspension, if it never resumed, is
                // already emitted (or was empty) and only needs dropping.
                while self
                    .suspended
                    .last()
                    .is_some_and(|s| s.depth >= self.list_depth())
                {
                    self.suspended.pop();
                }
            }
            "ul" | "ol" => {
                self.unwind_until_list();
                let depth = self.list_depth();
                self.pending.retain(|p| p.depth <= depth);
                self.suspended.retain(|s| s.depth <= depth);
            }
            "table" => self.unwind_until_table(),
            "tr" => {
                if matches!(self.stack.last(), Some(Frame::Cell(_))) {
                    self.place_cell();
                }
                if matches!(self.stack.last(), Some(Frame::Row)) {
                    self.stack.pop();
                    if let Some(Frame::Table(table)) = self.stack.last_mut() {
                        table.row = table.row.saturating_add(1);
                    }
                }
            }
            "td" | "th" => {
                if matches!(self.stack.last(), Some(Frame::Cell(_))) {
                    self.place_cell();
                }
            }
            "thead" | "tbody" | "tfoot" | "caption" | "colgroup" | "col" => {}
            "pre" => self.pop_pre(),
            _ if transparent_block(tag_name) => self.flush_para(),
            "a" => {
                self.links.pop();
            }
            "strong" | "b" => self.bold = self.bold.saturating_sub(1),
            "em" | "i" => self.italic = self.italic.saturating_sub(1),
            "s" => self.strike = self.strike.saturating_sub(1),
            "code" => self.pop_code(),
            // Stray and void end tags (`</br>`, `</img>`, unknowns) are ignored.
            _ => {}
        }
    }

    fn chars(&mut self, text: &str) {
        if self.skip_depth > 0 || !self.hidden.is_empty() {
            return;
        }
        if self.stack.iter().any(|f| matches!(f, Frame::Pre(_))) {
            self.pre_text(text);
            return;
        }
        if let Some(Frame::Code(buf)) = self.stack.last_mut() {
            buf.push_str(text);
            return;
        }
        // The formatting is read before the stack is borrowed: the run it
        // makes keeps what is open at this position.
        let bold = self.bold > 0;
        let italic = self.italic > 0;
        let strike = self.strike > 0;
        let link = self.current_link();
        match self.stack.last_mut() {
            Some(Frame::Para(para)) => {
                push_text(
                    &mut para.inlines,
                    &mut para.pending_space,
                    text,
                    bold,
                    italic,
                    strike,
                    &link,
                );
            }
            Some(Frame::Cell(cell)) => {
                push_text(
                    &mut cell.inlines,
                    &mut cell.pending_space,
                    text,
                    bold,
                    italic,
                    strike,
                    &link,
                );
            }
            // Stray text between rows is whitespace; a table holds no words
            // outside its cells.
            Some(Frame::Table(_) | Frame::Row) => {}
            _ => {
                self.implied_para();
                if let Some(Frame::Para(para)) = self.stack.last_mut() {
                    push_text(
                        &mut para.inlines,
                        &mut para.pending_space,
                        text,
                        bold,
                        italic,
                        strike,
                        &link,
                    );
                }
            }
        }
    }

    /// How many lists enclose the current position.
    fn list_depth(&self) -> usize {
        self.stack
            .iter()
            .filter(|f| matches!(f, Frame::List(_)))
            .count()
    }

    /// The ordinal of a list item: its `value`, or the enclosing ordered
    /// list's counter. `None` for unordered lists and stray items.
    fn take_ordinal(&mut self, tag: &Tag) -> Option<u32> {
        if let Some(value) = attr(tag, "value")
            .and_then(|v| v.trim().parse::<u32>().ok())
            .filter(|v| *v >= 1)
        {
            if let Some(list) = self.innermost_list() {
                list.next = value.saturating_add(1);
            }
            return Some(value);
        }
        let list = self.innermost_list()?;
        if !list.ordered {
            return None;
        }
        let ordinal = list.next;
        list.next = ordinal.saturating_add(1);
        Some(ordinal)
    }

    fn innermost_list(&mut self) -> Option<&mut ListFrame> {
        self.stack.iter_mut().rev().find_map(|f| match f {
            Frame::List(list) => Some(list),
            _ => None,
        })
    }

    /// A paragraph frame, or an item continuation where a nested block
    /// flushed an item early.
    fn new_para(&mut self, kind: FlowKind) -> ParaFrame {
        // Only plain paragraphs continue an item: a heading inside an item
        // keeps its kind.
        if matches!(kind, FlowKind::Paragraph) {
            let depth = self.list_depth();
            if let Some(item) = self.pending.last() {
                if item.depth <= depth {
                    return ParaFrame {
                        kind: FlowKind::Item {
                            level: item.level,
                            ordinal: item.ordinal,
                        },
                        inlines: Vec::new(),
                        pending_space: false,
                        append_to: None,
                    };
                }
            }
        }
        ParaFrame {
            kind,
            inlines: Vec::new(),
            pending_space: false,
            append_to: None,
        }
    }

    /// Bare text (and inline elements) outside any block open a paragraph,
    /// but never inside a table: stray text between rows is not a cell.
    /// Text after a nested list resumes the interrupted item instead.
    fn implied_para(&mut self) {
        if matches!(self.stack.last(), None | Some(Frame::List(_))) {
            if self.resume_item() {
                return;
            }
            let para = self.new_para(FlowKind::Paragraph);
            self.stack.push(Frame::Para(para));
        }
    }

    /// A nested list interrupts the open item rather than closing it: the
    /// text so far is emitted (so the outer item stays ahead of the nested
    /// one) and the item waits for its remainder. Anything else flushes.
    fn suspend_item(&mut self) {
        if !matches!(
            self.stack.last(),
            Some(Frame::Para(para)) if matches!(para.kind, FlowKind::Item { .. })
        ) {
            self.flush_para();
            return;
        }
        let Some(Frame::Para(para)) = self.stack.pop() else {
            return;
        };
        let kind = para.kind;
        let depth = self.list_depth();
        let block = self.emit_para(para);
        self.suspended.push(SuspendedItem { depth, kind, block });
    }

    /// Reopen the item a nested list interrupted, where the resuming text
    /// belongs to it: the suspension on top, at this list depth.
    fn resume_item(&mut self) -> bool {
        if self
            .suspended
            .last()
            .is_none_or(|s| s.depth != self.list_depth())
        {
            return false;
        }
        let Some(suspended) = self.suspended.pop() else {
            return false;
        };
        self.stack.push(Frame::Para(ParaFrame {
            kind: suspended.kind,
            inlines: Vec::new(),
            pending_space: false,
            append_to: suspended.block,
        }));
        true
    }

    /// Pop the open paragraph into a block, if there is one. An item flushed
    /// this way leaves its continuation behind for what follows it in the
    /// same item.
    fn flush_para(&mut self) {
        if !matches!(self.stack.last(), Some(Frame::Para(_))) {
            return;
        }
        let Some(Frame::Para(para)) = self.stack.pop() else {
            return;
        };
        if let FlowKind::Item { level, ordinal } = para.kind {
            self.pending.push(PendingItem {
                depth: self.list_depth(),
                level,
                ordinal,
            });
        }
        self.emit_para(para);
    }

    /// Close the open paragraph where it is of the expected kind; anything
    /// else (a paragraph flushed early, a stray end tag) is left alone.
    fn close_para_kind(&mut self, want: impl Fn(FlowKind) -> bool) {
        if matches!(self.stack.last(), Some(Frame::Para(para)) if want(para.kind)) {
            let Some(Frame::Para(para)) = self.stack.pop() else {
                return;
            };
            self.emit_para(para);
        }
    }

    /// Emit a paragraph frame: a block of its own, or text appended to the
    /// suspended block where the frame resumes an interrupted item. Returns
    /// the block the text went to, or `None` where it went nowhere.
    fn emit_para(&mut self, para: ParaFrame) -> Option<usize> {
        let (text, spans) = render_inlines(para.inlines);
        let text = text.trim().to_string();
        if let Some(target) = para.append_to {
            if text.is_empty() {
                return Some(target);
            }
            if let Some(block) = self.blocks.get_mut(target) {
                if !block.text.is_empty() {
                    block.text.push(' ');
                }
                block.text.push_str(&text);
                block.spans.extend(spans);
            }
            return Some(target);
        }
        if text.is_empty() {
            return None;
        }
        let kind = match para.kind {
            FlowKind::Paragraph => BlockKind::Paragraph,
            FlowKind::Heading(level) => BlockKind::Heading { level },
            FlowKind::Item { level, ordinal } => BlockKind::ListItem { level, ordinal },
        };
        let index = self.blocks.len();
        self.push_raw(kind, text, spans);
        Some(index)
    }

    /// A line break inside the open paragraph or cell.
    fn line_break(&mut self) {
        match self.stack.last_mut() {
            Some(Frame::Para(para)) => {
                para.pending_space = false;
                para.inlines.push(Inline::LineBreak);
            }
            Some(Frame::Cell(cell)) => {
                cell.pending_space = false;
                cell.inlines.push(Inline::LineBreak);
            }
            _ => {}
        }
    }

    fn image(&mut self, tag: &Tag) {
        let alt = attr(tag, "alt").unwrap_or("").to_string();
        let image = match attr(tag, "src").filter(|s| !s.is_empty()) {
            Some(src) => Inline::Image {
                alt,
                target: src.to_string(),
                link: self.links.last().cloned().flatten(),
            },
            None if !alt.is_empty() => self.styled_run(alt),
            None => return,
        };
        self.push_inline(image);
    }

    fn push_code(&mut self) {
        self.stack.push(Frame::Code(String::new()));
    }

    fn pop_code(&mut self) {
        if !matches!(self.stack.last(), Some(Frame::Code(_))) {
            return;
        }
        if let Some(Frame::Code(text)) = self.stack.pop() {
            self.push_inline(Inline::Code { text });
        }
    }

    fn push_inline(&mut self, inline: Inline) {
        // The owed space is written plain: whitespace carries no formatting,
        // and the renderer merges it with the plain word beside it.
        let space = || Inline::Run {
            text: " ".to_string(),
            bold: false,
            italic: false,
            strike: false,
            link: None,
        };
        match self.stack.last_mut() {
            Some(Frame::Para(para)) => {
                if para.pending_space && !matches!(inline, Inline::LineBreak) {
                    para.inlines.push(space());
                    para.pending_space = false;
                }
                para.inlines.push(inline);
            }
            Some(Frame::Cell(cell)) => {
                if cell.pending_space && !matches!(inline, Inline::LineBreak) {
                    cell.inlines.push(space());
                    cell.pending_space = false;
                }
                cell.inlines.push(inline);
            }
            _ => {}
        }
    }

    /// The innermost open link, if the position is inside one.
    fn current_link(&self) -> Option<String> {
        self.links.last().cloned().flatten()
    }

    fn styled_run(&self, text: String) -> Inline {
        Inline::Run {
            text,
            bold: self.bold > 0,
            italic: self.italic > 0,
            strike: self.strike > 0,
            link: self.current_link(),
        }
    }

    /// The open `pre`, if there is one.
    fn open_pre(&mut self) -> Option<&mut PreFrame> {
        self.stack.iter_mut().rev().find_map(|f| match f {
            Frame::Pre(pre) => Some(pre),
            _ => None,
        })
    }

    /// Raw text into the open `pre`.
    fn pre_text(&mut self, text: &str) {
        if let Some(pre) = self.open_pre() {
            pre.text.push_str(text);
        }
    }

    fn pop_pre(&mut self) {
        let mut pre = None;
        while let Some(frame) = self.stack.pop() {
            match frame {
                Frame::Pre(found) => {
                    pre = Some(found);
                    break;
                }
                Frame::Para(para) => {
                    self.emit_para(para);
                }
                Frame::Code(text) => self.push_inline(Inline::Code { text }),
                Frame::Cell(_) => self.place_cell(),
                Frame::Row => {
                    if let Some(Frame::Table(table)) = self.stack.last_mut() {
                        table.row = table.row.saturating_add(1);
                    }
                }
                Frame::List(_) | Frame::Table(_) => {}
            }
        }
        if let Some(pre) = pre {
            self.emit_pre(pre);
        }
    }

    fn start_row(&mut self) {
        if !self.stack.iter().any(|f| matches!(f, Frame::Table(_))) {
            return;
        }
        if matches!(self.stack.last(), Some(Frame::Cell(_))) {
            self.place_cell();
        }
        if matches!(self.stack.last(), Some(Frame::Row)) {
            self.stack.pop();
            if let Some(Frame::Table(table)) = self.stack.last_mut() {
                table.row = table.row.saturating_add(1);
            }
        }
        self.stack.push(Frame::Row);
    }

    fn start_cell(&mut self, tag: &Tag, header: bool) {
        if matches!(self.stack.last(), Some(Frame::Cell(_))) {
            self.place_cell();
        }
        if !matches!(self.stack.last(), Some(Frame::Row)) {
            return;
        }
        self.stack.push(Frame::Cell(CellFrame {
            header,
            col_span: span_attr(tag, "colspan", 1000),
            row_span: span_attr(tag, "rowspan", 1000),
            inlines: Vec::new(),
            pending_space: false,
        }));
    }

    /// Place the open cell on its table's grid: the first free column of the
    /// current row, marking what its spans cover.
    fn place_cell(&mut self) {
        if !matches!(self.stack.last(), Some(Frame::Cell(_))) {
            return;
        }
        let Some(Frame::Cell(cell)) = self.stack.pop() else {
            return;
        };
        let Some(Frame::Table(table)) = self
            .stack
            .iter_mut()
            .rev()
            .find(|f| matches!(f, Frame::Table(_)))
        else {
            return;
        };
        let (text, spans) = render_inlines(cell.inlines);
        let text = text.trim().to_string();
        let row = table.row as usize;
        while table.grid.len() <= row {
            table.grid.push(Vec::new());
        }
        let mut col = 0usize;
        while table.grid[row].get(col).copied().unwrap_or(false) {
            col += 1;
        }
        let col_span = cell.col_span as usize;
        let row_span = cell.row_span as usize;
        for r in row..row + row_span {
            while table.grid.len() <= r {
                table.grid.push(Vec::new());
            }
            while table.grid[r].len() < col + col_span {
                table.grid[r].push(false);
            }
            for held in table.grid[r].iter_mut().take(col + col_span).skip(col) {
                *held = true;
            }
        }
        table.cells.push(PlacedCell {
            row: table.row,
            col: col.min(u16::MAX as usize) as u16,
            row_span: cell.row_span,
            col_span: cell.col_span,
            header: cell.header,
            text,
            spans,
        });
    }

    /// Pop frames until the innermost list is closed, emitting paragraphs
    /// and pre blocks on the way.
    fn unwind_until_list(&mut self) {
        while let Some(frame) = self.stack.pop() {
            match frame {
                Frame::List(_) => break,
                Frame::Para(para) => {
                    self.emit_para(para);
                }
                Frame::Code(text) => self.push_inline(Inline::Code { text }),
                Frame::Cell(_) => self.place_cell(),
                Frame::Row => {
                    if let Some(Frame::Table(table)) = self.stack.last_mut() {
                        table.row = table.row.saturating_add(1);
                    }
                }
                Frame::Pre(pre) => self.emit_pre(pre),
                Frame::Table(table) => self.emit_table(table),
            }
        }
    }

    /// Pop frames until the innermost table is closed and emitted.
    fn unwind_until_table(&mut self) {
        while let Some(frame) = self.stack.pop() {
            match frame {
                Frame::Table(table) => {
                    self.emit_table(table);
                    break;
                }
                Frame::Para(para) => {
                    self.emit_para(para);
                }
                Frame::Code(text) => self.push_inline(Inline::Code { text }),
                Frame::Cell(_) => self.place_cell(),
                Frame::Row => {
                    if let Some(Frame::Table(table)) = self.stack.last_mut() {
                        table.row = table.row.saturating_add(1);
                    }
                }
                Frame::Pre(pre) => self.emit_pre(pre),
                Frame::List(_) => {}
            }
        }
    }

    /// Pop every frame: the end of input closes what the markup left open.
    fn unwind_to_top(&mut self) {
        while let Some(frame) = self.stack.pop() {
            match frame {
                Frame::Para(para) => {
                    self.emit_para(para);
                }
                Frame::Code(text) => self.push_inline(Inline::Code { text }),
                Frame::Cell(_) => self.place_cell(),
                Frame::Row => {
                    if let Some(Frame::Table(table)) = self.stack.last_mut() {
                        table.row = table.row.saturating_add(1);
                    }
                }
                Frame::Pre(pre) => self.emit_pre(pre),
                Frame::Table(table) => self.emit_table(table),
                Frame::List(_) => {}
            }
        }
    }

    /// One `pre` as a paragraph holding the code verbatim: no fences are
    /// written into the text, since the engine's renderer would escape them
    /// as literal backticks.
    fn emit_pre(&mut self, pre: PreFrame) {
        // The newline after `<pre>` is the element's, not the code's.
        let code = pre.text.strip_prefix('\n').unwrap_or(&pre.text);
        let code = code.strip_prefix('\r').unwrap_or(code);
        let code = code.trim_end_matches(['\n', '\r']);
        if code.trim().is_empty() {
            return;
        }
        let spans = vec![TextSpan {
            text: code.to_string(),
            ..Default::default()
        }];
        self.push_raw(BlockKind::Paragraph, code.to_string(), spans);
    }

    /// A table frame stamped with its document order.
    fn new_table(&mut self) -> TableFrame {
        let order = self.next_table;
        self.next_table += 1;
        TableFrame {
            order,
            ..TableFrame::default()
        }
    }

    /// Emit a table's cells in row-major order. Every placed cell gets a
    /// block, empty ones included: the grid is explicit here.
    ///
    /// A table closed while an outer table is still open waits its turn:
    /// the outer table emits first, then the nested ones in document order.
    fn emit_table(&mut self, table: TableFrame) {
        if self
            .stack
            .iter()
            .any(|frame| matches!(frame, Frame::Table(_)))
        {
            self.nested_tables.push(table);
            return;
        }
        self.emit_table_cells(table);
        if !self.nested_tables.is_empty() {
            let mut nested = std::mem::take(&mut self.nested_tables);
            nested.sort_by_key(|table| table.order);
            for table in nested {
                self.emit_table_cells(table);
            }
        }
    }

    /// One table's cells as blocks, in row-major order.
    fn emit_table_cells(&mut self, mut table: TableFrame) {
        table.cells.sort_by_key(|c| (c.row, c.col));
        for cell in table.cells {
            let spans = if cell.text.is_empty() {
                Vec::new()
            } else {
                cell.spans
            };
            self.push_raw(
                BlockKind::TableCell {
                    row: cell.row,
                    column: cell.col,
                    row_span: cell.row_span,
                    column_span: cell.col_span,
                    header: cell.header,
                },
                cell.text,
                spans,
            );
        }
    }

    fn push_raw(&mut self, kind: BlockKind, text: String, spans: Vec<TextSpan>) {
        let index = self.blocks.len();
        let mut block = Block::new(kind, 0, [0.0; 4], text, index);
        block.spans = spans;
        self.blocks.push(block);
    }
}

/// Collapsed character data into the open inlines: runs of whitespace become
/// one pending space, owed before the next word rather than written, so a
/// run at the end of a block leaves nothing behind.
fn push_text(
    inlines: &mut Vec<Inline>,
    pending_space: &mut bool,
    text: &str,
    bold: bool,
    italic: bool,
    strike: bool,
    link: &Option<String>,
) {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let leading = text.starts_with(char::is_whitespace);
    let trailing = text.ends_with(char::is_whitespace);
    if collapsed.is_empty() {
        if (leading || trailing) && !inlines.is_empty() {
            *pending_space = true;
        }
        return;
    }
    let mut word = collapsed;
    if (leading || *pending_space) && !inlines.is_empty() {
        word.insert(0, ' ');
    }
    *pending_space = trailing;
    inlines.push(Inline::Run {
        text: word,
        bold,
        italic,
        strike,
        link: link.clone(),
    });
}
