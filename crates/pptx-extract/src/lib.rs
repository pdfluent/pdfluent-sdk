// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

#![warn(missing_docs)]
//! PowerPoint PPTX extraction onto the engine block model.
//!
//! [`extract_blocks`] reads a `.pptx` package and returns one
//! [`pdf_engine::Block`] stream for the whole deck, so the result serializes
//! through [`pdf_engine::to_markdown`] like any extracted PDF:
//!
//! ```no_run
//! let pptx = std::fs::read("deck.pptx").unwrap();
//! let markdown = pptx_extract::extract_markdown(&pptx).unwrap();
//! println!("{markdown}");
//! ```
//!
//! # Mapping
//!
//! Slides are visited in presentation order. Per slide the blocks are:
//!
//! * one [`pdf_engine::BlockKind::Heading`] at level 1 per non-empty
//!   paragraph of the title placeholder (`title` and `ctrTitle` alike); a
//!   slide without a title shape opens with its body, the page marker still
//!   bounding the slide;
//! * the remaining shapes in document order (the shape tree, depth-first
//!   through groups):
//!   paragraphs as [`pdf_engine::BlockKind::Paragraph`], bulleted paragraphs
//!   as [`pdf_engine::BlockKind::ListItem`] with their `lvl`, tables
//!   (`a:tbl`) as one [`pdf_engine::BlockKind::TableCell`] per cell in
//!   row-major order with `gridSpan`/`rowSpan` as spans, pictures as a
//!   paragraph holding an image reference (`![alt](target)` -- a reference,
//!   never rendered pixels).
//!
//! A paragraph's bullet resolves outward: its own `a:pPr`, the shape's
//! `a:lstStyle`, the layout placeholder's (matched by placeholder
//! index, then type), then the master's text style for the placeholder
//! type (`titleStyle`, `bodyStyle`, `otherStyle` for anything else). A
//! `buNone` anywhere stops the walk like a bullet does. Footer, date and
//! slide-number placeholders resolve through their own chain only -- the
//! shape's list style, the same-type layout placeholder's, then the
//! master's -- never through `bodyStyle`, and a paragraph's own bullet
//! does not apply: they are paragraphs unless that chain itself states a
//! bullet.
//!
//! Speaker notes are not body text and emit nothing.
//!
//! Group shapes (`p:grpSp`) recurse through the same walker. SmartArt and
//! charts contribute whatever text their XML carries (see
//! [`crate::slide`] -- they are only partly covered).
//!
//! # Slide numbers are text too
//!
//! [`pdf_engine::Block::page`] carries the 0-based slide index, which is what
//! the Markdown page comments (`<!-- page N -->`) and the JSON form read.
//! The number the slide shows is body text beside that: `sldNum`
//! placeholders emit their paragraphs, `slidenum` fields their cached run
//! text, and footer and date placeholders their own -- all as paragraphs
//! unless their own placeholder chain states a bullet.
//!
//! # What the blocks do not carry
//!
//! Spans are empty: a shape's runs have no glyph metrics worth reporting, so
//! [`pdf_engine::Block::spans`] stays vacant and the text stands on its own.
//! Bounding boxes come from `a:xfrm` in EMU, converted at 12700 EMU per point
//! with y flipped into PDF user space; shapes without a transform carry a
//! zero box.

pub mod error;
pub mod slide;

pub use error::{PptxExtractError, Result};

use pdf_engine::{Block, BlockKind};
use slide::{Bullet, Para, Shape, ShapeBody, Xfrm};
use std::collections::HashMap;
use std::io::{Cursor, Read};

/// EMU per point: 914400 EMU per inch over 72 points per inch.
const EMU_PER_POINT: f64 = 12700.0;

/// Default slide size in EMU where `p:sldSz` is missing: 10 x 7.5 inches,
/// the same default `pdf-pptx` writes.
const DEFAULT_SLIDE_SIZE_EMU: (i64, i64) = (9_144_000, 6_858_000);

/// Read a `.pptx` package into engine blocks, in slide order.
///
/// See the [crate-level documentation](crate) for the mapping.
pub fn extract_blocks(pptx: &[u8]) -> Result<Vec<Block>> {
    let mut package = Package::open(pptx)?;
    let order = package.slide_order()?;
    if order.is_empty() {
        return Err(PptxExtractError::Invalid("no slides found".into()));
    }
    let mut blocks = Vec::new();
    for (page, path) in order.iter().enumerate() {
        package.slide_blocks(path, page, &mut blocks)?;
    }
    for (index, block) in blocks.iter_mut().enumerate() {
        block.index = index;
    }
    Ok(blocks)
}

/// Read a `.pptx` package and serialize it through the engine's Markdown.
///
/// This is [`extract_blocks`] followed by [`pdf_engine::to_markdown`].
pub fn extract_markdown(pptx: &[u8]) -> Result<String> {
    Ok(pdf_engine::to_markdown(&extract_blocks(pptx)?))
}

/// One relationship target: the last segment of the `Type` plus the target.
struct Rel {
    /// Last segment of the relationship type (`slide`, `image`, ...).
    kind: String,
    /// The relationship target, as written.
    target: String,
}

/// Parse a `.rels` part into relationship-id to [`Rel`] entries.
fn parse_rels(xml: &[u8]) -> Result<HashMap<String, Rel>> {
    let root = slide::parse_xml(xml)?;
    let mut rels = HashMap::new();
    for rel in root.descendants("Relationship") {
        if let (Some(id), Some(kind), Some(target)) = (
            rel.attr("Id"),
            rel.attr("Type").and_then(|t| t.rsplit('/').next()),
            rel.attr("Target"),
        ) {
            rels.insert(
                id.to_string(),
                Rel {
                    kind: kind.to_string(),
                    target: target.to_string(),
                },
            );
        }
    }
    Ok(rels)
}

/// Resolve a relationship target against the part that holds it.
///
/// `base` is the holding part's full path (`ppt/slides/slide1.xml`); the
/// target is resolved against its directory, `..` clamped at the root.
fn resolve_target(base: &str, target: &str) -> String {
    let mut parts: Vec<&str> = base.rsplit_once('/').map_or(Vec::new(), |(dir, _)| {
        dir.split('/').filter(|s| !s.is_empty()).collect()
    });
    for segment in target.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(segment),
        }
    }
    parts.join("/")
}

/// A `.pptx` package: the ZIP archive plus the presentation-level parts.
struct Package<'a> {
    zip: zip::ZipArchive<Cursor<&'a [u8]>>,
    /// Slide size in points (y-down extent), from `p:sldSz` or the default.
    slide_size: (f64, f64),
    /// Ordered slide part paths, from `p:sldIdLst` via the presentation rels.
    order: Vec<String>,
}

impl<'a> Package<'a> {
    /// Open the package and read the presentation-level parts.
    fn open(pptx: &'a [u8]) -> Result<Self> {
        let mut zip = zip::ZipArchive::new(Cursor::new(pptx))?;
        let slide_size = presentation_size(read_part(&mut zip, "ppt/presentation.xml")?.as_deref());
        let order = presentation_order(
            read_part(&mut zip, "ppt/presentation.xml")?.as_deref(),
            read_part(&mut zip, "ppt/_rels/presentation.xml.rels")?.as_deref(),
        )?;
        let mut package = Self {
            zip,
            slide_size,
            order,
        };
        if package.order.is_empty() {
            package.order = slide_fallback_order(&mut package.zip);
        }
        Ok(package)
    }

    /// The slide part paths in presentation order.
    fn slide_order(&self) -> Result<Vec<String>> {
        Ok(self.order.clone())
    }

    /// Append one slide's blocks: the title heading, if the slide has a
    /// title shape, then the shapes in document order.
    fn slide_blocks(&mut self, path: &str, page: usize, blocks: &mut Vec<Block>) -> Result<()> {
        let xml = read_part(&mut self.zip, path)?
            .ok_or_else(|| PptxExtractError::Invalid(format!("slide part missing: {path}")))?;
        let rels_path = slide_rels_path(path);
        let rels = read_part(&mut self.zip, &rels_path)?
            .map(|bytes| parse_rels(&bytes))
            .transpose()?
            .unwrap_or_default();

        let bullets = self.bullet_context(path, &rels)?;
        let root = slide::parse_xml(&xml)?;
        let shapes = match root.descendants("spTree").into_iter().next() {
            Some(tree) => slide::extract_shapes(tree, 0, &bullets),
            None => Vec::new(),
        };

        // Every non-empty paragraph of the title placeholder becomes a
        // heading of its own. A slide without a title shape opens with its
        // body: the page marker still bounds the slide, so no synthetic
        // heading is owed.
        if let Some(title_shape) = shapes.iter().find(|shape| shape.is_title) {
            let bbox = title_shape
                .xfrm
                .map(|x| self.bbox(x))
                .unwrap_or([0.0, 0.0, 0.0, 0.0]);
            if let ShapeBody::Text(paras) = &title_shape.body {
                for para in paras {
                    if slide::is_blank(&para.text) {
                        continue;
                    }
                    blocks.push(Block::new(
                        BlockKind::Heading { level: 1 },
                        page,
                        bbox,
                        para.text.clone(),
                        0,
                    ));
                }
            }
        }

        // Document order: the shape tree is the reader's order, the way
        // assistive technology walks it, and geometric sorting only
        // reorders what the file already sequenced.
        for shape in shapes.iter().filter(|shape| !shape.is_title) {
            self.shape_blocks(shape, path, &rels, page, blocks);
        }
        Ok(())
    }

    /// What one slide's paragraphs inherit bullets from: its layout
    /// placeholders plus its master's text styles. A slide without a
    /// layout, or a layout without a master, inherits from what is there.
    fn bullet_context(
        &mut self,
        slide_path: &str,
        rels: &HashMap<String, Rel>,
    ) -> Result<slide::BulletContext> {
        let layout = self.related_part(slide_path, rels, "slideLayout")?;
        let master = match &layout {
            Some((part, _)) => {
                let layout_rels = read_part(&mut self.zip, &slide_rels_path(part))?
                    .map(|bytes| parse_rels(&bytes))
                    .transpose()?
                    .unwrap_or_default();
                self.related_part(part, &layout_rels, "slideMaster")?
                    .map(|(_, node)| node)
            }
            None => None,
        };
        Ok(slide::BulletContext::new(
            layout.as_ref().map(|(_, node)| node),
            master.as_ref(),
        ))
    }

    /// A part related to a slide or a layout, with its path: the layout of
    /// a slide, the master of a layout. Absent where the rels say nothing.
    fn related_part(
        &mut self,
        holder: &str,
        rels: &HashMap<String, Rel>,
        kind: &str,
    ) -> Result<Option<(String, slide::Node)>> {
        let Some(target) = rels
            .values()
            .find(|rel| rel.kind == kind)
            .map(|rel| resolve_target(holder, &rel.target))
        else {
            return Ok(None);
        };
        let Some(bytes) = read_part(&mut self.zip, &target)? else {
            return Ok(None);
        };
        Ok(Some((target, slide::parse_xml(&bytes)?)))
    }

    /// Append the blocks one shape holds, recursing into groups.
    fn shape_blocks(
        &self,
        shape: &Shape,
        slide_path: &str,
        rels: &HashMap<String, Rel>,
        page: usize,
        blocks: &mut Vec<Block>,
    ) {
        let bbox = shape
            .xfrm
            .map(|x| self.bbox(x))
            .unwrap_or([0.0, 0.0, 0.0, 0.0]);
        match &shape.body {
            ShapeBody::Text(paras) => {
                for para in paras {
                    blocks.push(para_block(para, page, bbox));
                }
            }
            ShapeBody::Table(table) => {
                for cell in &table.cells {
                    blocks.push(Block::new(
                        BlockKind::TableCell {
                            row: cell.row,
                            column: cell.column,
                            row_span: cell.row_span,
                            column_span: cell.column_span,
                            header: cell.header,
                        },
                        page,
                        bbox,
                        cell.text.clone(),
                        0,
                    ));
                }
            }
            ShapeBody::Picture { alt, embed } => {
                // An unresolvable embed keeps its relationship id, so the
                // reference degrades instead of vanishing.
                let target = match rels.get(embed) {
                    Some(rel) => {
                        let resolved = resolve_target(slide_path, &rel.target);
                        if resolved.is_empty() {
                            embed.clone()
                        } else {
                            resolved
                        }
                    }
                    None => embed.clone(),
                };
                let label = if alt.is_empty() { "image" } else { alt };
                blocks.push(Block::new(
                    BlockKind::Paragraph,
                    page,
                    bbox,
                    format!("![{label}]({target})"),
                    0,
                ));
            }
            ShapeBody::GraphicText { paras, values } => {
                for para in paras {
                    blocks.push(para_block(para, page, bbox));
                }
                if !values.is_empty() {
                    blocks.push(Block::new(
                        BlockKind::Paragraph,
                        page,
                        bbox,
                        values.join(" "),
                        0,
                    ));
                }
            }
            ShapeBody::Group(children) => {
                for child in children {
                    self.shape_blocks(child, slide_path, rels, page, blocks);
                }
            }
            ShapeBody::Skip => {}
        }
    }

    /// A shape's EMU box as a PDF user-space box: points, y up.
    fn bbox(&self, xfrm: Xfrm) -> [f64; 4] {
        let x0 = xfrm.x as f64 / EMU_PER_POINT;
        let y_down = xfrm.y as f64 / EMU_PER_POINT;
        let w = xfrm.width as f64 / EMU_PER_POINT;
        let h = xfrm.height as f64 / EMU_PER_POINT;
        let y0 = self.slide_size.1 - y_down - h;
        [x0, y0, x0 + w, y0 + h]
    }
}

/// Read one part from the archive, or `None` where it is absent.
fn read_part(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Option<Vec<u8>>> {
    match zip.by_name(name) {
        Ok(mut file) => {
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            Ok(Some(bytes))
        }
        Err(zip::result::ZipError::FileNotFound) => Ok(None),
        Err(err) => Err(PptxExtractError::Zip(err)),
    }
}

/// The slide size in points from `ppt/presentation.xml`, or the default.
fn presentation_size(xml: Option<&[u8]>) -> (f64, f64) {
    let bytes = match xml {
        Some(bytes) => bytes,
        None => return default_slide_size(),
    };
    let root = match slide::parse_xml(bytes) {
        Ok(root) => root,
        Err(_) => return default_slide_size(),
    };
    match root.descendants("sldSz").into_iter().next() {
        Some(sz) => {
            let cx = sz.attr("cx").and_then(|s| s.parse::<i64>().ok());
            let cy = sz.attr("cy").and_then(|s| s.parse::<i64>().ok());
            match (cx, cy) {
                (Some(cx), Some(cy)) if cx > 0 && cy > 0 => {
                    (cx as f64 / EMU_PER_POINT, cy as f64 / EMU_PER_POINT)
                }
                _ => default_slide_size(),
            }
        }
        None => default_slide_size(),
    }
}

/// The default slide size in points.
fn default_slide_size() -> (f64, f64) {
    (
        DEFAULT_SLIDE_SIZE_EMU.0 as f64 / EMU_PER_POINT,
        DEFAULT_SLIDE_SIZE_EMU.1 as f64 / EMU_PER_POINT,
    )
}

/// The slide part paths in `p:sldIdLst` order, via the presentation rels.
fn presentation_order(presentation: Option<&[u8]>, rels: Option<&[u8]>) -> Result<Vec<String>> {
    let (Some(presentation), Some(rels)) = (presentation, rels) else {
        return Ok(Vec::new());
    };
    let root = slide::parse_xml(presentation)?;
    let rels = parse_rels(rels)?;
    let mut order = Vec::new();
    for id in root.descendants("sldId") {
        if let Some(rid) = id.rel_attr("id") {
            if let Some(rel) = rels.get(rid) {
                if rel.kind == "slide" {
                    order.push(resolve_target("ppt/presentation.xml", &rel.target));
                }
            }
        }
    }
    Ok(order)
}

/// Slide parts by numeric file name, for packages without a usable
/// `p:sldIdLst`: `ppt/slides/slideN.xml` sorted by N.
fn slide_fallback_order(zip: &mut zip::ZipArchive<Cursor<&[u8]>>) -> Vec<String> {
    let mut numbered = Vec::new();
    for name in zip.file_names().map(str::to_string).collect::<Vec<_>>() {
        if let Some(n) = slide_file_number(&name) {
            numbered.push((n, name));
        }
    }
    numbered.sort();
    numbered.into_iter().map(|(_, name)| name).collect()
}

/// The N in `ppt/slides/slideN.xml`, or `None` for anything else.
fn slide_file_number(name: &str) -> Option<u32> {
    let rest = name.strip_prefix("ppt/slides/slide")?;
    let digits = rest.strip_suffix(".xml")?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // `slide9999999999.xml` overflows u32; sorting only needs an order.
    digits.parse::<u32>().ok().or(Some(u32::MAX))
}

/// The rels part holding a part's relationships:
/// `ppt/slides/slide1.xml` -> `ppt/slides/_rels/slide1.xml.rels`.
/// Layouts follow the same convention.
fn slide_rels_path(slide_path: &str) -> String {
    match slide_path.rsplit_once('/') {
        Some((dir, file)) => format!("{dir}/_rels/{file}.rels"),
        None => format!("_rels/{slide_path}.rels"),
    }
}

/// One paragraph as a block: a list item where it carries a bullet.
fn para_block(para: &Para, page: usize, bbox: [f64; 4]) -> Block {
    let kind = match para.bullet {
        Bullet::None => BlockKind::Paragraph,
        Bullet::Unordered => BlockKind::ListItem {
            level: para.level,
            ordinal: None,
        },
        Bullet::Ordered { start_at } => BlockKind::ListItem {
            level: para.level,
            ordinal: start_at,
        },
    };
    Block::new(kind, page, bbox, para.text.clone(), 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rel_target_resolution() {
        assert_eq!(
            resolve_target("ppt/slides/slide1.xml", "../media/image1.png"),
            "ppt/media/image1.png"
        );
        assert_eq!(
            resolve_target("ppt/slides/slide1.xml", "../notesSlides/notesSlide1.xml"),
            "ppt/notesSlides/notesSlide1.xml"
        );
        assert_eq!(
            resolve_target("ppt/presentation.xml", "slides/slide2.xml"),
            "ppt/slides/slide2.xml"
        );
        // `..` past the root clamps rather than escaping the package.
        assert_eq!(
            resolve_target("ppt/slides/slide1.xml", "../../../../evil.xml"),
            "evil.xml"
        );
    }

    #[test]
    fn rels_parse_by_last_type_segment() {
        let xml = "<Relationships xmlns=\"x\">\
            <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide\" Target=\"slides/slide1.xml\"/>\
            </Relationships>";
        let rels = parse_rels(xml.as_bytes()).unwrap();
        assert_eq!(rels["rId1"].kind, "slide");
        assert_eq!(rels["rId1"].target, "slides/slide1.xml");
    }

    #[test]
    fn slide_number_parses_digits_only() {
        assert_eq!(slide_file_number("ppt/slides/slide12.xml"), Some(12));
        assert_eq!(slide_file_number("ppt/slides/slide.xml"), None);
        assert_eq!(slide_file_number("ppt/slides/_rels/slide1.xml.rels"), None);
        assert_eq!(slide_file_number("ppt/notesSlides/notesSlide1.xml"), None);
    }
}
