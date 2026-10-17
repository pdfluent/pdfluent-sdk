//! Figures in the block model: listing, naming, pixels, and references.
//!
//! A figure is a placed image or a drawn vector cluster that stands beside
//! prose. The text pass records both as boxes; this module turns those boxes
//! into [`Block`](crate::blocks::Block)s late in extraction, names them, and
//! renders their pixels for embeds and written files.
//!
//! Names are derived, never stored: `figure-<page>-<ordinal>.png` with the
//! page and the figure's number on that page, both counted from one, in the
//! order the blocks list them. Every renderer -- markers, embeds, files --
//! enumerates the same way, so one figure has one name everywhere.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::blocks::{Block, BlockKind, ExtractOptions};
use crate::error::{EngineError, Result};
use crate::render::{PixelFormat, RenderOptions};

/// Resolution figure boxes are rendered at, in dots per inch.
///
/// Display points scaled by 150/72: a 20-point side becomes a 42-pixel one.
pub const FIGURE_DPI: f64 = 150.0;

/// How many pages the standalone graphics scan reads before it stops.
///
/// [`PdfDocument::extract_image_boxes`](crate::PdfDocument::extract_image_boxes)
/// re-interprets pages for their boxes alone, so past this many pages the
/// cost outweighs the listing and later pages report no figures. Extraction
/// itself is not capped: it reads every page for text anyway, and the boxes
/// come free with that pass.
pub const MAX_GRAPHICS_SCAN_PAGES: usize = 50;

/// One figure of the document, with its rendered pixels: a placed image or
/// a drawn vector cluster, unified -- a drawing beside prose is a figure
/// the way a photograph is.
#[derive(Debug, Clone, PartialEq)]
pub struct Figure {
    /// 0-based page index the figure stands on.
    pub page: usize,
    /// Bounding box in PDF user space, `[x0, y0, x1, y1]` with y up.
    pub bbox: [f64; 4],
    /// The figure's file name, [`figure_name`] of its page and number.
    pub name: String,
    /// The figure's box rendered at [`FIGURE_DPI`], encoded as PNG.
    pub png: Vec<u8>,
}

/// The file name of a figure: `figure-<page>-<ordinal>.png`.
///
/// Both counts arrive 0-based and leave 1-based and zero-padded, so the
/// first figure of the first page is `figure-0001-01.png`.
pub fn figure_name(page: usize, ordinal: usize) -> String {
    format!("figure-{:04}-{:02}.png", page + 1, ordinal + 1)
}

/// Whether a point falls strictly inside a box.
fn contains(point: (f64, f64), bbox: &[f64; 4]) -> bool {
    point.0 > bbox[0] && point.0 < bbox[2] && point.1 > bbox[1] && point.1 < bbox[3]
}

/// Drop the body blocks drawn over a placed image.
///
/// A block goes when its centre falls strictly inside an image box on its
/// own page. Running heads and feet never go: page chrome is positioned
/// independently of figures. Figures themselves carry no text and stay.
pub(crate) fn drop_text_over_images(blocks: Vec<Block>, images: &[Vec<[f64; 4]>]) -> Vec<Block> {
    blocks
        .into_iter()
        .filter(|block| {
            if block.text.trim().is_empty() {
                return true;
            }
            if matches!(block.kind, BlockKind::Header | BlockKind::Footer) {
                return true;
            }
            let centre = (
                (block.bbox[0] + block.bbox[2]) / 2.0,
                (block.bbox[1] + block.bbox[3]) / 2.0,
            );
            !images
                .get(block.page)
                .is_some_and(|boxes| boxes.iter().any(|bbox| contains(centre, bbox)))
        })
        .collect()
}

/// Merge figure blocks into finished text blocks.
///
/// One figure per placed image on every page, then one per drawn cluster on
/// every page that [`page_has_text`] marks: a page with no text block still
/// carries its images -- a scan reads as its figure markers -- but no drawn
/// cluster, which without prose beside it is page furniture. Figures stand
/// where they fall vertically -- before the first text block whose top edge
/// is below theirs -- the furniture stays trailing, and every block is
/// renumbered in place.
pub(crate) fn merge_figures(
    blocks: Vec<Block>,
    images: &[Vec<[f64; 4]>],
    graphics: &[Vec<[f64; 4]>],
    page_has_text: &[bool],
) -> Vec<Block> {
    let (mut body, furniture): (Vec<Block>, Vec<Block>) = blocks
        .into_iter()
        .partition(|block| !matches!(block.kind, BlockKind::Header | BlockKind::Footer));
    body.sort_by_key(|block| block.page);
    let pages = images.len().max(graphics.len());
    let mut merged: Vec<Block> = Vec::with_capacity(body.len() + pages);
    let mut at = 0usize;
    for page in 0..pages {
        let end = body[at..]
            .iter()
            .take_while(|block| block.page == page)
            .count()
            + at;
        let text = &body[at..end];
        at = end;
        if !page_has_text.get(page).copied().unwrap_or(false) {
            // A textless page still carries its placed images: a scan is one
            // figure marker, not an empty document. Drawn clusters stay
            // withheld -- without prose they are page furniture, not figures.
            if let Some(boxes) = images.get(page) {
                let mut figures: Vec<[f64; 4]> = boxes.clone();
                figures.sort_by(|a, b| b[3].total_cmp(&a[3]));
                for bbox in figures {
                    merged.push(Block::new(BlockKind::Figure, page, bbox, String::new(), 0));
                }
            }
            merged.extend(text.iter().cloned());
            continue;
        }
        let mut figures: Vec<([f64; 4], f64)> = Vec::new();
        if let Some(boxes) = images.get(page) {
            figures.extend(boxes.iter().map(|bbox| (*bbox, bbox[3])));
        }
        if let Some(boxes) = graphics.get(page) {
            figures.extend(boxes.iter().map(|bbox| (*bbox, bbox[3])));
        }
        figures.sort_by(|a, b| b.1.total_cmp(&a.1));
        let mut figures = figures.into_iter().peekable();
        for block in text {
            while figures.peek().is_some_and(|(_, top)| *top >= block.bbox[3]) {
                if let Some((bbox, _)) = figures.next() {
                    merged.push(Block::new(BlockKind::Figure, page, bbox, String::new(), 0));
                }
            }
            merged.push(block.clone());
        }
        for (bbox, _) in figures {
            merged.push(Block::new(BlockKind::Figure, page, bbox, String::new(), 0));
        }
    }
    merged.extend(body[at..].iter().cloned());
    merged.extend(furniture);
    for (index, block) in merged.iter_mut().enumerate() {
        block.index = index;
    }
    merged
}

/// Render one page's figure boxes to PNGs, in order, from a single render.
///
/// Boxes are in PDF points, y up; the render is [`FIGURE_DPI`] pixels per
/// point, y down. A box is floored at its low corner and ceiled at its high
/// one, so the crop always covers the figure, then clamped to the page: a
/// figure wholly off the page comes back as one white pixel rather than as
/// an error. Rotated pages map best-effort onto the same frame.
pub(crate) fn render_crops(
    doc: &crate::PdfDocument,
    page: usize,
    boxes: &[[f64; 4]],
) -> Result<Vec<Vec<u8>>> {
    if boxes.is_empty() {
        return Ok(Vec::new());
    }
    let rendered = doc.render_page(
        page,
        &RenderOptions {
            dpi: FIGURE_DPI,
            ..Default::default()
        },
    )?;
    if rendered.pixel_format != PixelFormat::Rgba8 {
        return Err(EngineError::RenderError(format!(
            "figure render of page {} came back {:?}, not RGBA",
            page, rendered.pixel_format
        )));
    }
    let scale = FIGURE_DPI / 72.0;
    let (width, height) = (rendered.width, rendered.height);
    let mut out = Vec::with_capacity(boxes.len());
    for bbox in boxes {
        let x0 = ((bbox[0] * scale).floor() as i64).clamp(0, width as i64);
        let x1 = ((bbox[2] * scale).ceil() as i64).clamp(0, width as i64);
        let y1 = (height as i64 - (bbox[1] * scale).floor() as i64).clamp(0, height as i64);
        let y0 = (height as i64 - (bbox[3] * scale).ceil() as i64).clamp(0, height as i64);
        let (x0, x1) = (x0.min(x1), x0.max(x1));
        let (y0, y1) = (y0.min(y1), y0.max(y1));
        let (crop_w, crop_h) = ((x1 - x0).max(1) as u32, (y1 - y0).max(1) as u32);
        let mut rgb = vec![255u8; crop_w as usize * crop_h as usize * 3];
        if x1 > x0 && y1 > y0 {
            for (row, y) in (y0..y1).enumerate() {
                for (col, x) in (x0..x1).enumerate() {
                    let from = (y as usize * width as usize + x as usize) * 4;
                    let Some(px) = rendered.pixels.get(from..from + 4) else {
                        continue;
                    };
                    // Premultiplied RGBA over the white page: the white shows
                    // through by exactly the missing share of alpha.
                    let missing = 255 - px[3];
                    let to = (row * crop_w as usize + col) * 3;
                    if let Some(slot) = rgb.get_mut(to..to + 3) {
                        slot[0] = px[0].saturating_add(missing);
                        slot[1] = px[1].saturating_add(missing);
                        slot[2] = px[2].saturating_add(missing);
                    }
                }
            }
        }
        out.push(png_bytes(crop_w, crop_h, &rgb));
    }
    Ok(out)
}

/// Encode RGB bytes as a PNG: 8-bit truecolour, one filter byte per row.
///
/// Stored deflate blocks -- no compressor on board, so cooperation with one
/// is out -- which every decoder reads; the bytes are deterministic for the
/// pixels. Short input reads as black past its end rather than panicking.
pub(crate) fn png_bytes(width: u32, height: u32, rgb: &[u8]) -> Vec<u8> {
    let stride = width as usize * 3;
    let mut raw = Vec::with_capacity(height as usize * (stride + 1));
    for y in 0..height as usize {
        raw.push(0);
        match rgb.get(y * stride..(y + 1) * stride) {
            Some(row) => raw.extend_from_slice(row),
            None => raw.extend(std::iter::repeat_n(0, stride)),
        }
    }
    // A zlib stream of stored blocks: header, then `BFINAL | 00` + LEN +
    // NLEN + up to 65535 bytes each, then the Adler-32 of the raw rows.
    let mut zlib = vec![0x78, 0x01];
    let mut chunks = raw.chunks(65535).peekable();
    while let Some(chunk) = chunks.next() {
        let last = chunks.peek().is_none();
        zlib.push(u8::from(last));
        let len = chunk.len() as u16;
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&(!len).to_le_bytes());
        zlib.extend_from_slice(chunk);
    }
    // An empty image still needs its one empty stored block.
    if raw.is_empty() {
        zlib.extend_from_slice(&[1, 0, 0, 255, 255]);
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut png = vec![137, 80, 78, 71, 13, 10, 26, 10];
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    push_chunk(&mut png, b"IHDR", &ihdr);
    push_chunk(&mut png, b"IDAT", &zlib);
    push_chunk(&mut png, b"IEND", &[]);
    png
}

/// Append one PNG chunk: length, type, data, and the IEEE CRC of type+data.
fn push_chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    png.extend_from_slice(&(data.len() as u32).to_be_bytes());
    png.extend_from_slice(kind);
    png.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(data);
    png.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

/// The IEEE CRC-32, bit by bit: figures are small and no table is carried.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let lsb = crc & 1;
            crc >>= 1;
            if lsb == 1 {
                crc ^= 0xedb8_8320;
            }
        }
    }
    !crc
}

/// The Adler-32 of zlib trailers, modulo the prime at every step.
fn adler32(data: &[u8]) -> u32 {
    let mut low: u32 = 1;
    let mut high: u32 = 0;
    for byte in data {
        low = (low + u32::from(*byte)) % 65521;
        high = (high + low) % 65521;
    }
    (high << 16) | low
}

/// The Markdown for blocks whose figures resolve past markers.
///
/// Embeds render each figure's box and set the block's text to the data URL;
/// a figure directory writes each render under it and sets the text to the
/// written path, creating the directory first. Markers need no document and
/// never arrive here. What each figure is called is [`figure_name`],
/// enumerated per page in block order.
pub(crate) fn resolve_figure_texts(
    doc: &crate::PdfDocument,
    blocks: &mut [Block],
    options: &ExtractOptions,
) -> Result<()> {
    use base64::Engine as _;
    use std::collections::BTreeMap;
    // One render per page that carries figures, whatever the mode. The
    // ordinal is the figure's place among its page's figures in block
    // order -- the same enumeration the marker renderer uses.
    let mut pages: BTreeMap<usize, Vec<(usize, [f64; 4])>> = BTreeMap::new();
    for (at, block) in blocks.iter().enumerate() {
        if matches!(block.kind, BlockKind::Figure) {
            pages.entry(block.page).or_default().push((at, block.bbox));
        }
    }
    if pages.is_empty() {
        return Ok(());
    }
    if let Some(dir) = options.figure_image_dir.as_ref() {
        std::fs::create_dir_all(dir).map_err(EngineError::Io)?;
    }
    for (page, refs) in &pages {
        let boxes: Vec<[f64; 4]> = refs.iter().map(|(_, bbox)| *bbox).collect();
        let renders = render_crops(doc, *page, &boxes)?;
        for (ordinal, ((at, _), png)) in refs.iter().zip(renders).enumerate() {
            let name = figure_name(*page, ordinal);
            let text = match options.figure_image_dir.as_ref() {
                Some(dir) => {
                    let path = dir.join(&name);
                    std::fs::write(&path, &png).map_err(EngineError::Io)?;
                    path.to_string_lossy().into_owned()
                }
                None => {
                    let encoded = base64::engine::general_purpose::STANDARD.encode(&png);
                    format!("data:image/png;base64,{encoded}")
                }
            };
            if let Some(block) = blocks.get_mut(*at) {
                block.text = text;
            }
        }
    }
    Ok(())
}
