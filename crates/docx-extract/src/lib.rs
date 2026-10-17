#![warn(missing_docs)]
//! DOCX to engine-block extraction (epic pdfluent-internal#636, MVP).
//!
//! Reads a `.docx` file and produces the same structured output the engine
//! produces for PDFs: [`Block`]s with [`BlockKind`]s such as
//! heading/paragraph/list item/table cell, serialized by the engine's own
//! [`pdf_engine::to_markdown`]. Downstream users get one format.
//!
//! # Mapping
//!
//! | WordprocessingML | Engine output |
//! |---|
//! | `w:p` | `BlockKind::Paragraph` |
//! | `w:outlineLvl` (direct or through the style's `basedOn` chain), `w:pStyle` `Heading1`..`Heading9`/`Title` (id or `w:name`) | `BlockKind::Heading { level }` (1..=9) |
//! | `w:numPr` (direct or through the style's `basedOn` chain; `w:ilvl`, `w:numId` + `numbering.xml`) | `BlockKind::ListItem { level, ordinal }` |
//! | `w:tbl` (`w:gridSpan`, `w:vMerge`) | runs of `BlockKind::TableCell` in row-major order |
//! | `w:hyperlink`, `HYPERLINK` field | inline `[text](url)` in the block text |
//! | bold/italic runs | inline `**bold**` / `*italic*` in the block text |
//! | `w:drawing` with `a:blip` | inline `![alt](media/...)` reference (no rendering) |
//!
//! Word has no pages, so every block carries `page: 0` and a zero bounding box.
//!
//! # Where this differs from PDF blocks, on purpose
//!
//! PDF extraction puts bold/italic into span flags and lets the Markdown
//! serializer mark them. DOCX run formatting is explicit, so this crate writes
//! the CommonMark inline spans directly into [`Block::text`] and leaves the
//! span flags plain: doing both would mark every run twice. Hyperlinks and
//! images have no engine-block shape either, so they are inline Markdown too.
//!
//! # Limitations (MVP)
//!
//! * Only the paragraph properties that decide the block kind resolve
//!   through `styles.xml` (`outlineLvl`, `numPr`, the `Heading1`..`9`/`Title`
//!   id or name). Run formatting from styles is not resolved: a paragraph
//!   that is bold through its style comes out plain.
//! * Numbering level overrides (`w:lvlOverride`) are ignored.
//! * Legacy VML images (`w:pict`/`v:imagedata`) are skipped; modern
//!   `w:drawing` images are referenced.
//! * Headers, footers, footnotes, endnotes, comments and text boxes keep no
//!   special kind (text-box paragraphs surface as body paragraphs).
//! * Nested tables are emitted as their own block runs after the outer table.
//! * Tracked deletions (`w:del`) are dropped; insertions are kept.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

mod document;
mod numbering;
mod styles;

use std::io::{Cursor, Read};

pub use pdf_engine::{Block, BlockKind};
use thiserror::Error;

/// Errors returned while reading a `.docx` file.
#[derive(Debug, Error)]
pub enum DocxError {
    /// The input is not a readable ZIP container.
    #[error("invalid DOCX container: {0}")]
    Zip(#[from] zip::result::ZipError),
    /// A part of the container could not be read.
    #[error("could not read DOCX part: {0}")]
    Io(#[from] std::io::Error),
    /// A `word/*.xml` part is not well-formed XML.
    #[error("invalid WordprocessingML: {0}")]
    Xml(#[from] quick_xml::Error),
    /// A required part (`word/document.xml`) is missing from the container.
    #[error("missing required part: {0}")]
    MissingPart(String),
}

/// Extract the structured blocks of a `.docx` file.
///
/// `docx` is the raw bytes of the file (a ZIP container). The returned blocks
/// reuse the engine's [`Block`]/[`BlockKind`] model: headings, list items and
/// table cells carry their levels and spans, and [`pdf_engine::to_markdown`]
/// serializes them the same way it serializes PDF blocks.
///
/// # Example
///
/// ```no_run
/// let docx = std::fs::read("report.docx").unwrap();
/// let blocks = docx_extract::extract_blocks(&docx).unwrap();
/// let markdown = pdf_engine::to_markdown(&blocks);
/// ```
pub fn extract_blocks(docx: &[u8]) -> Result<Vec<Block>, DocxError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(docx))?;
    let document = read_part(&mut archive, "word/document.xml")?
        .ok_or_else(|| DocxError::MissingPart("word/document.xml".to_string()))?;
    let rels = match read_part(&mut archive, "word/_rels/document.xml.rels")? {
        Some(xml) => document::parse_relationships(&xml)?,
        None => std::collections::HashMap::new(),
    };
    let numbering = match read_part(&mut archive, "word/numbering.xml")? {
        Some(xml) => numbering::Numbering::parse(&xml)?,
        None => numbering::Numbering::default(),
    };
    let styles = match read_part(&mut archive, "word/styles.xml")? {
        Some(xml) => styles::Styles::parse(&xml)?,
        None => styles::Styles::default(),
    };
    document::parse(&document, &rels, numbering, styles)
}

/// Extract a `.docx` file directly as Markdown.
///
/// This is [`extract_blocks`] followed by the engine's
/// [`pdf_engine::to_markdown`], so Word documents and PDFs serialize through
/// the same code.
pub fn extract_markdown(docx: &[u8]) -> Result<String, DocxError> {
    Ok(pdf_engine::to_markdown(&extract_blocks(docx)?))
}

/// Read one part of the container, or `Ok(None)` where the part is absent.
///
/// Only `FileNotFound` means absent; every other ZIP error is a broken file.
fn read_part(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    name: &str,
) -> Result<Option<Vec<u8>>, DocxError> {
    let mut file = match archive.by_name(name) {
        Ok(file) => file,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(error) => return Err(DocxError::Zip(error)),
    };
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    Ok(Some(buf))
}
