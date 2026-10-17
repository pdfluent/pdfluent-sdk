#![warn(missing_docs)]
//! CommonMark to engine blocks.
//!
//! One pass with `pulldown-cmark` over the input. Headings, paragraphs, lists,
//! tables and code blocks become blocks; inline emphasis, links and images
//! become inline Markdown inside the block text. Tables have no spans in
//! CommonMark, so every cell covers exactly its own grid slot.
//!
//! # Mapping
//!
//! | Markdown | Engine output |
//! |---|---|
//! | `#`..`######` | `BlockKind::Heading { level }` (1..=6) |
//! | paragraphs | `BlockKind::Paragraph` |
//! | `-`/`*`/`1.` items | `BlockKind::ListItem { level, ordinal }` (one block per item) |
//! | pipe tables | runs of `BlockKind::TableCell` in row-major order |
//! | `[text](url)` | inline `[text](url)` in the block text |
//! | `**bold**`, `*italic*`, `~~strike~~` | the same marks in the block text |
//! | fenced and indented code | a fenced code paragraph (see the crate docs) |
//! | `` `code` `` | inline `` `code` `` in the block text |
//! | `![alt](src)` | inline `![alt](src)` reference (no rendering) |
//! | `> quote` | paragraphs quoted with `> ` |
//!
//! # Limitations (MVP)
//!
//! * HTML blocks inside Markdown are extracted as HTML and spliced in; inline
//!   HTML tags are dropped but their words are kept.
//! * A second block inside one list item (a loose list) becomes its own block:
//!   an item where it is a paragraph, its own kind otherwise.
//! * Thematic breaks (`---`) have no block shape and are skipped.
//! * Footnotes, since the parser runs without the footnote option, read as
//!   literal text.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_engine::{Block, BlockKind, TextSpan};
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::inline::{render_inlines, Inline};

/// Extract the structured blocks of a CommonMark document.
///
/// Tables, strikethrough and task lists are enabled; smart punctuation stays
/// off so the text keeps its characters. Blocks carry `page: 0` and are
/// numbered in document order: markup has no pages.
///
/// # Example
///
/// ```
/// let blocks = markup_extract::markdown::extract_blocks("# Title\n\nBody.\n");
/// assert!(matches!(
///     blocks[0].kind,
///     markup_extract::BlockKind::Heading { level: 1 }
/// ));
/// ```
pub fn extract_blocks(markdown: &str) -> Vec<Block> {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut state = State::default();
    for event in Parser::new_ext(markdown, options) {
        state.event(event);
    }
    state.finish()
}

/// Extract a CommonMark document directly as Markdown.
///
/// This is [`extract_blocks`] followed by the engine's
/// [`pdf_engine::to_markdown`], so markup and PDFs serialize through the
/// same code.
pub fn extract_markdown(markdown: &str) -> String {
    pdf_engine::to_markdown(&extract_blocks(markdown))
}

/// What a paragraph-level frame becomes.
#[derive(Debug, Clone, Copy)]
enum FlowKind {
    Paragraph,
    Heading(u8),
    Item { level: u8, ordinal: Option<u32> },
}

/// A block under construction.
#[derive(Debug)]
struct ParaFrame {
    kind: FlowKind,
    inlines: Vec<Inline>,
    /// A soft break is still owed before the next word.
    pending_space: bool,
}

/// A list under construction: ordered or not, and the next ordinal.
#[derive(Debug)]
struct ListFrame {
    ordered: bool,
    next: u32,
}

/// A table under construction.
#[derive(Debug, Default)]
struct TableFrame {
    cells: Vec<TableCell>,
    row: u16,
    col: u16,
    in_head: bool,
}

/// One finished table cell.
#[derive(Debug)]
struct TableCell {
    row: u16,
    col: u16,
    header: bool,
    text: String,
    spans: Vec<TextSpan>,
}

/// An image under construction: its alt text, collected literally.
#[derive(Debug)]
struct ImageFrame {
    target: String,
    alt: String,
}

#[derive(Debug, Default)]
struct State {
    blocks: Vec<Block>,
    paras: Vec<ParaFrame>,
    lists: Vec<ListFrame>,
    table: Option<TableFrame>,
    cell: Option<ParaFrame>,
    code: Option<CodeFrame>,
    images: Vec<ImageFrame>,
    links: Vec<String>,
    bold: u32,
    italic: u32,
    strike: u32,
    quote: u32,
}

/// A code block under construction.
#[derive(Debug)]
struct CodeFrame {
    lang: Option<String>,
    text: String,
}

impl State {
    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.text(&text),
            Event::Code(code) => self.push_inline(Inline::Code {
                text: code.into_string(),
            }),
            // A soft break is a space; a hard break is a newline.
            Event::SoftBreak => self.soft_break(),
            Event::HardBreak => self.push_inline(Inline::LineBreak),
            // An HTML block is extracted as HTML and spliced in; inline tags
            // are dropped but the words between them flow on.
            Event::Html(html) => self.splice_html(&html),
            Event::InlineHtml(_) => {}
            Event::FootnoteReference(name) => self.text(&format!("[^{name}]")),
            Event::TaskListMarker(checked) => {
                self.push_inline(Inline::Run {
                    text: if checked {
                        "[x] ".to_string()
                    } else {
                        "[ ] ".to_string()
                    },
                    bold: false,
                    italic: false,
                    strike: false,
                    link: None,
                });
            }
            // A thematic break has no block shape.
            Event::Rule => {}
            Event::InlineMath(_) | Event::DisplayMath(_) => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => self.open_para(FlowKind::Paragraph),
            Tag::Heading { level, .. } => self.open_para(FlowKind::Heading(heading_level(level))),
            Tag::BlockQuote(..) => self.quote += 1,
            Tag::CodeBlock(kind) => {
                self.flush_para();
                self.code = Some(CodeFrame {
                    lang: match kind {
                        CodeBlockKind::Fenced(info) => info
                            .split_whitespace()
                            .next()
                            .filter(|w| !w.is_empty())
                            .map(str::to_string),
                        CodeBlockKind::Indented => None,
                    },
                    text: String::new(),
                });
            }
            Tag::List(start) => {
                self.flush_para();
                match start {
                    Some(n) => self.lists.push(ListFrame {
                        ordered: true,
                        next: n.max(1) as u32,
                    }),
                    None => self.lists.push(ListFrame {
                        ordered: false,
                        next: 1,
                    }),
                }
            }
            Tag::Item => {
                self.flush_para();
                let level = self.lists.len().saturating_sub(1) as u8;
                let ordinal = match self.lists.last_mut() {
                    Some(list) if list.ordered => {
                        let ordinal = list.next;
                        list.next = ordinal.saturating_add(1);
                        Some(ordinal)
                    }
                    _ => None,
                };
                self.paras.push(ParaFrame {
                    kind: FlowKind::Item { level, ordinal },
                    inlines: Vec::new(),
                    pending_space: false,
                });
            }
            Tag::Table(_) => {
                self.flush_para();
                self.table = Some(TableFrame::default());
            }
            // The head holds its cells bare, with no row events around
            // them: it opens and closes the first grid row itself.
            Tag::TableHead => {
                if let Some(table) = self.table.as_mut() {
                    table.in_head = true;
                    table.col = 0;
                }
            }
            Tag::TableRow => {
                if let Some(table) = self.table.as_mut() {
                    table.col = 0;
                }
            }
            Tag::TableCell => {
                self.cell = Some(ParaFrame {
                    kind: FlowKind::Paragraph,
                    inlines: Vec::new(),
                    pending_space: false,
                });
            }
            Tag::Emphasis => self.italic += 1,
            Tag::Strong => self.bold += 1,
            Tag::Strikethrough => self.strike += 1,
            Tag::Link { dest_url, .. } => self.links.push(dest_url.into_string()),
            Tag::Image { dest_url, .. } => self.images.push(ImageFrame {
                target: dest_url.into_string(),
                alt: String::new(),
            }),
            // Footnote definitions and metadata blocks flow on as blocks;
            // the parser runs without their options, so these never fire.
            Tag::FootnoteDefinition(_) | Tag::MetadataBlock(_) => {}
            Tag::HtmlBlock => {}
            // Definition lists read as paragraphs; super/subscript as plain
            // text. Their options stay off, so these never fire either.
            Tag::DefinitionList => self.flush_para(),
            Tag::DefinitionListTitle | Tag::DefinitionListDefinition => {
                self.open_para(FlowKind::Paragraph);
            }
            Tag::Superscript | Tag::Subscript => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.flush_para(),
            TagEnd::Heading(_) => self.flush_para(),
            TagEnd::BlockQuote(..) => self.quote = self.quote.saturating_sub(1),
            TagEnd::CodeBlock => self.flush_code(),
            TagEnd::List(_) => {
                self.flush_para();
                self.lists.pop();
            }
            TagEnd::Item => self.flush_para(),
            TagEnd::Table => self.flush_table(),
            TagEnd::TableHead => {
                if let Some(table) = self.table.as_mut() {
                    table.in_head = false;
                    table.row = table.row.saturating_add(1);
                }
            }
            TagEnd::TableRow => {
                if let Some(table) = self.table.as_mut() {
                    table.row = table.row.saturating_add(1);
                }
            }
            TagEnd::TableCell => self.flush_cell(),
            TagEnd::Emphasis => self.italic = self.italic.saturating_sub(1),
            TagEnd::Strong => self.bold = self.bold.saturating_sub(1),
            TagEnd::Strikethrough => self.strike = self.strike.saturating_sub(1),
            TagEnd::Link => {
                self.links.pop();
            }
            TagEnd::Image => {
                if let Some(image) = self.images.pop() {
                    let inline = Inline::Image {
                        alt: image.alt,
                        target: image.target,
                        link: self.links.last().cloned(),
                    };
                    self.push_inline(inline);
                }
            }
            TagEnd::FootnoteDefinition | TagEnd::MetadataBlock(_) | TagEnd::HtmlBlock => {}
            TagEnd::DefinitionList => self.flush_para(),
            TagEnd::DefinitionListTitle | TagEnd::DefinitionListDefinition => self.flush_para(),
            TagEnd::Superscript | TagEnd::Subscript => {}
        }
    }

    /// A paragraph or heading opens: the open one is flushed first, so a
    /// loose list's paragraphs become blocks of their own. A paragraph
    /// directly in an item continues it: `- a\n\n  b` keeps both halves an
    /// item (a heading keeps its kind).
    fn open_para(&mut self, kind: FlowKind) {
        let continued = match (kind, self.paras.last()) {
            (
                FlowKind::Paragraph,
                Some(ParaFrame {
                    kind: FlowKind::Item { level, ordinal },
                    ..
                }),
            ) => Some((*level, *ordinal)),
            _ => None,
        };
        self.flush_para();
        let kind = match continued {
            Some((level, ordinal)) => FlowKind::Item { level, ordinal },
            None => kind,
        };
        self.paras.push(ParaFrame {
            kind,
            inlines: Vec::new(),
            pending_space: false,
        });
    }

    fn text(&mut self, text: &str) {
        if let Some(code) = self.code.as_mut() {
            code.text.push_str(text);
            return;
        }
        if let Some(image) = self.images.last_mut() {
            image.alt.push_str(text);
            return;
        }
        let bold = self.bold > 0;
        let italic = self.italic > 0;
        let strike = self.strike > 0;
        let link = self.links.last().cloned();
        if let Some(cell) = self.cell.as_mut() {
            push_word(
                &mut cell.inlines,
                &mut cell.pending_space,
                text,
                bold,
                italic,
                strike,
                &link,
            );
        } else if let Some(para) = self.paras.last_mut() {
            push_word(
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

    fn soft_break(&mut self) {
        if self.code.is_some() || self.images.last().is_some() {
            return;
        }
        if let Some(cell) = self.cell.as_mut() {
            if !cell.inlines.is_empty() {
                cell.pending_space = true;
            }
        } else if let Some(para) = self.paras.last_mut() {
            if !para.inlines.is_empty() {
                para.pending_space = true;
            }
        }
    }

    fn push_inline(&mut self, inline: Inline) {
        if self.images.last().is_some() {
            // Formatting inside alt text is kept as words: alt text is plain.
            match inline {
                Inline::Code { text } => {
                    if let Some(image) = self.images.last_mut() {
                        image.alt.push_str(&text);
                    }
                }
                Inline::LineBreak => {
                    if let Some(image) = self.images.last_mut() {
                        image.alt.push(' ');
                    }
                }
                _ => {}
            }
            return;
        }
        let space = || Inline::Run {
            text: " ".to_string(),
            bold: false,
            italic: false,
            strike: false,
            link: None,
        };
        if let Some(cell) = self.cell.as_mut() {
            if cell.pending_space && !matches!(inline, Inline::LineBreak) {
                cell.inlines.push(space());
                cell.pending_space = false;
            }
            cell.inlines.push(inline);
        } else if let Some(para) = self.paras.last_mut() {
            if para.pending_space && !matches!(inline, Inline::LineBreak) {
                para.inlines.push(space());
                para.pending_space = false;
            }
            para.inlines.push(inline);
        }
    }

    /// Extract an HTML block as HTML and splice its blocks in, renumbered.
    fn splice_html(&mut self, html: &str) {
        self.flush_para();
        for mut block in crate::html::extract_blocks(html) {
            block.index = self.blocks.len();
            self.blocks.push(block);
        }
    }

    fn flush_para(&mut self) {
        if let Some(para) = self.paras.pop() {
            let (text, spans) = render_inlines(para.inlines);
            let text = text.trim().to_string();
            if text.is_empty() {
                return;
            }
            let kind = match para.kind {
                FlowKind::Paragraph => BlockKind::Paragraph,
                FlowKind::Heading(level) => BlockKind::Heading { level },
                FlowKind::Item { level, ordinal } => BlockKind::ListItem { level, ordinal },
            };
            self.push(kind, text, spans);
        }
    }

    fn flush_code(&mut self) {
        if let Some(code) = self.code.take() {
            let text = code.text.trim_end_matches(['\n', '\r']);
            if text.is_empty() {
                return;
            }
            let fence = code
                .lang
                .map(|l| format!("```{l}\n"))
                .unwrap_or("```\n".to_string());
            let text = format!("{fence}{text}\n```");
            let spans = vec![TextSpan {
                text: code.text.trim_end_matches(['\n', '\r']).to_string(),
                ..Default::default()
            }];
            self.push(BlockKind::Paragraph, text, spans);
        }
    }

    fn flush_cell(&mut self) {
        let Some(cell) = self.cell.take() else {
            return;
        };
        let Some(table) = self.table.as_mut() else {
            return;
        };
        let (text, spans) = render_inlines(cell.inlines);
        table.cells.push(TableCell {
            row: table.row,
            col: table.col,
            header: table.in_head,
            text: text.trim().to_string(),
            spans,
        });
        table.col = table.col.saturating_add(1);
    }

    fn flush_table(&mut self) {
        if let Some(table) = self.table.take() {
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
                        row_span: 1,
                        column_span: 1,
                        header: cell.header,
                    },
                    cell.text,
                    spans,
                );
            }
        }
    }

    /// The blocks so far, renumbered in document order.
    fn finish(mut self) -> Vec<Block> {
        self.flush_para();
        self.flush_code();
        self.flush_table();
        self.blocks
    }

    /// Block text inside blockquotes: every line quoted, per open level.
    /// Table cells are exempt: a quote marker inside a pipe table would read
    /// as cell text.
    fn quoted(&self, text: &str) -> String {
        if self.quote == 0 {
            return text.to_string();
        }
        let prefix = "> ".repeat(self.quote.min(8) as usize);
        text.lines()
            .map(|line| format!("{prefix}{line}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn push(&mut self, kind: BlockKind, text: String, spans: Vec<TextSpan>) {
        let quoted = match kind {
            BlockKind::TableCell { .. } => text,
            _ => self.quoted(&text),
        };
        self.push_raw(kind, quoted, spans);
    }

    fn push_raw(&mut self, kind: BlockKind, text: String, spans: Vec<TextSpan>) {
        let index = self.blocks.len();
        let mut block = Block::new(kind, 0, [0.0; 4], text, index);
        block.spans = spans;
        self.blocks.push(block);
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// One word run into the open inlines, with the soft break owed before it.
fn push_word(
    inlines: &mut Vec<Inline>,
    pending_space: &mut bool,
    text: &str,
    bold: bool,
    italic: bool,
    strike: bool,
    link: &Option<String>,
) {
    let mut word = text.to_string();
    if *pending_space && !inlines.is_empty() {
        word.insert(0, ' ');
    }
    *pending_space = false;
    inlines.push(Inline::Run {
        text: word,
        bold,
        italic,
        strike,
        link: link.clone(),
    });
}
