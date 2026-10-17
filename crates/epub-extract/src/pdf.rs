//! The book as a plain PDF: one page per spine chapter.
//!
//! [`to_pdf_bytes`] renders the text [`crate::extract_epub`] reads as a
//! minimal PDF, so callers that only open PDFs (the Python `Document`) read
//! EPUBs unchanged. Each spine chapter becomes one PDF page; where a chapter
//! holds more lines than fit, it continues onto further pages.
//!
//! The rendering keeps words, not looks: Helvetica at one size, headings as
//! plain lines, list items prefixed (`- `, or `1. ` where the chapter numbers
//! them), table rows joined with ` | `, standalone links and images spelled
//! out in the same inline shapes as [`crate::extract_engine_blocks`]. Text is
//! WinAnsi; characters outside Latin-1 read as `?`.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::blocks::{Block, BlockKind};
use crate::error::Result;

/// Page size in points: US Letter, the same stated size Office files use.
const PAGE_WIDTH: f64 = 612.0;
/// Page size in points: US Letter, the same stated size Office files use.
const PAGE_HEIGHT: f64 = 792.0;
/// Body size and line step in points.
const FONT_SIZE: f64 = 11.0;
/// Body size and line step in points.
const LINE_STEP: f64 = 14.0;
/// Text margins in points.
const MARGIN_X: f64 = 72.0;
/// Text margins in points.
const MARGIN_TOP: f64 = 72.0;
/// Text margins in points.
const MARGIN_BOTTOM: f64 = 72.0;

/// The book as a PDF: one page per spine chapter, at least one page.
///
/// Reads the book exactly as [`crate::extract_epub`] does, then lays its
/// text out as the [module documentation](self) says. Fails exactly when
/// reading the book fails.
pub fn to_pdf_bytes(epub: &[u8]) -> Result<Vec<u8>> {
    let blocks = crate::extract_epub(epub)?;
    let chapters = chapter_lines(&blocks);
    let chapters = if chapters.is_empty() {
        vec![Vec::new()]
    } else {
        chapters
    };
    Ok(render_pdf(&chapters))
}

/// The text lines of each chapter, in spine order.
fn chapter_lines(blocks: &[Block]) -> Vec<Vec<String>> {
    let mut chapters: Vec<Vec<String>> = Vec::new();
    let mut row: Option<(usize, u16, Vec<String>)> = None;
    let flush_row = |row: &mut Option<(usize, u16, Vec<String>)>,
                     chapters: &mut Vec<Vec<String>>| {
        if let Some((chapter, _, cells)) = row.take() {
            while chapters.len() <= chapter {
                chapters.push(Vec::new());
            }
            chapters[chapter].push(cells.join(" | "));
        }
    };
    for block in blocks {
        match &block.kind {
            BlockKind::TableCell { row: cell_row, .. } => {
                let same_row = matches!(&row, Some((chapter, first, _))
                    if *chapter == block.chapter && *first == *cell_row);
                if !same_row {
                    flush_row(&mut row, &mut chapters);
                    row = Some((block.chapter, *cell_row, Vec::new()));
                }
                if let Some((_, _, cells)) = row.as_mut() {
                    cells.push(cell_text(block));
                }
            }
            _ => {
                flush_row(&mut row, &mut chapters);
                while chapters.len() <= block.chapter {
                    chapters.push(Vec::new());
                }
                chapters[block.chapter].extend(block_lines(block));
            }
        }
    }
    flush_row(&mut row, &mut chapters);
    chapters
}

/// One block as lines: items prefixed, standalone references spelled out.
fn block_lines(block: &Block) -> Vec<String> {
    let prefix = match &block.kind {
        BlockKind::ListItem { level, ordinal } => {
            let indent = "  ".repeat(usize::from(*level));
            match ordinal {
                Some(number) => format!("{indent}{number}. "),
                None => format!("{indent}- "),
            }
        }
        BlockKind::Link { href } => return vec![format!("[{}]({href})", block.text)],
        BlockKind::Image { src, alt } => return vec![format!("![{alt}]({src})")],
        _ => String::new(),
    };
    block
        .text
        .split('\n')
        .map(|line| format!("{prefix}{line}"))
        .collect()
}

/// One table cell as the text its row joins.
fn cell_text(block: &Block) -> String {
    block.text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The chapters as a minimal PDF: catalog, pages, one Helvetica face, and a
/// content stream per page.
fn render_pdf(chapters: &[Vec<String>]) -> Vec<u8> {
    let mut pages: Vec<Vec<String>> = Vec::new();
    for chapter in chapters {
        let mut page: Vec<String> = Vec::new();
        let mut lines = chapter.iter().peekable();
        loop {
            let room = max_lines() as usize;
            for line in lines.by_ref().take(room) {
                page.push(line.clone());
            }
            if lines.peek().is_none() {
                pages.push(std::mem::take(&mut page));
                break;
            }
            pages.push(std::mem::take(&mut page));
        }
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.7\n");
    let mut offsets: Vec<usize> = Vec::new();
    // Object numbers: 1 catalog, 2 pages, 3 font, then two per page.
    let kids: Vec<String> = pages
        .iter()
        .enumerate()
        .map(|(index, _)| format!("{} 0 R", 4 + index * 2))
        .collect();
    push_object(
        &mut out,
        &mut offsets,
        1,
        b"<< /Type /Catalog /Pages 2 0 R >>",
    );
    push_object(
        &mut out,
        &mut offsets,
        2,
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.join(" "),
            pages.len()
        )
        .as_bytes(),
    );
    push_object(
        &mut out,
        &mut offsets,
        3,
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    );
    for (index, lines) in pages.iter().enumerate() {
        let page_no = 4 + index * 2;
        push_object(
            &mut out,
            &mut offsets,
            page_no,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_WIDTH} {PAGE_HEIGHT}] \
                 /Contents {} 0 R /Resources << /Font << /F1 3 0 R >> >> >>",
                page_no + 1
            )
            .as_bytes(),
        );
        let content = content_stream(lines);
        let stream = [
            format!("<< /Length {} >>\nstream\n", content.len()).as_bytes(),
            &content,
            b"endstream",
        ]
        .concat();
        push_object(&mut out, &mut offsets, page_no + 1, &stream);
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes(),
    );
    for offset in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// How many lines fit between the top and bottom margins.
fn max_lines() -> u32 {
    ((PAGE_HEIGHT - MARGIN_TOP - MARGIN_BOTTOM) / LINE_STEP) as u32
}

/// One page's content: each line set in Helvetica at the running position.
fn content_stream(lines: &[String]) -> Vec<u8> {
    let mut content = Vec::new();
    let mut y = PAGE_HEIGHT - MARGIN_TOP;
    for line in lines {
        content.extend_from_slice(
            format!("BT /F1 {FONT_SIZE} Tf {MARGIN_X:.1} {y:.1} Td (").as_bytes(),
        );
        escape_into(&mut content, line);
        content.extend_from_slice(b") Tj ET\n");
        y -= LINE_STEP;
    }
    content
}

/// A line as a PDF literal string: specials escaped, Latin-1 kept, the rest
/// as `?`.
fn escape_into(out: &mut Vec<u8>, line: &str) {
    for ch in line.chars() {
        match ch {
            '\\' => out.extend_from_slice(b"\\\\"),
            '(' => out.extend_from_slice(b"\\("),
            ')' => out.extend_from_slice(b"\\)"),
            '\n' | '\r' => out.push(b' '),
            _ if (ch as u32) < 256 => out.push(ch as u8),
            _ => out.push(b'?'),
        }
    }
}

/// Object `number` with `body`, recording its offset.
fn push_object(out: &mut Vec<u8>, offsets: &mut Vec<usize>, number: usize, body: &[u8]) {
    offsets.push(out.len());
    out.extend_from_slice(format!("{number} 0 obj\n").as_bytes());
    out.extend_from_slice(body);
    out.extend_from_slice(b"\nendobj\n");
}
