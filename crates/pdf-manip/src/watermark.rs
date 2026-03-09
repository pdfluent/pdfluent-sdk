//! Text and image watermarking for PDF pages.
//!
//! Supports foreground/background placement, rotation, opacity,
//! tiling, and page selection.

use crate::error::{ManipError, Result};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, ObjectId, Stream};

/// Position of a watermark on the page.
#[derive(Debug, Clone, Copy)]
pub enum Position {
    /// Centered on the page.
    Center,
    /// Top-left corner with (x, y) offset from the corner.
    TopLeft(f32, f32),
    /// Top-right corner with (x, y) offset.
    TopRight(f32, f32),
    /// Bottom-left corner with (x, y) offset.
    BottomLeft(f32, f32),
    /// Bottom-right corner with (x, y) offset.
    BottomRight(f32, f32),
    /// Exact position in PDF points from bottom-left.
    Exact(f32, f32),
}

/// Z-order: whether the watermark goes behind or in front of content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// Behind existing content (background).
    Background,
    /// On top of existing content (foreground).
    Foreground,
}

/// Which pages to apply the watermark to.
#[derive(Debug, Clone)]
pub enum PageSelection {
    /// All pages.
    All,
    /// Only even pages (2, 4, 6, ...).
    Even,
    /// Only odd pages (1, 3, 5, ...).
    Odd,
    /// Specific page range (inclusive, 1-based).
    Range(u32, u32),
    /// Specific page numbers (1-based).
    Pages(Vec<u32>),
}

/// Color for text watermarks.
#[derive(Debug, Clone, Copy)]
pub enum Color {
    /// RGB color, each component 0.0–1.0.
    Rgb(f32, f32, f32),
    /// CMYK color, each component 0.0–1.0.
    Cmyk(f32, f32, f32, f32),
    /// Grayscale, 0.0 (black) to 1.0 (white).
    Gray(f32),
}

impl Default for Color {
    fn default() -> Self {
        Color::Gray(0.5)
    }
}

/// Configuration for a text watermark.
#[derive(Debug, Clone)]
pub struct TextWatermark {
    /// The text to display.
    pub text: String,
    /// Font size in points.
    pub font_size: f32,
    /// Rotation in degrees (counter-clockwise).
    pub rotation: f32,
    /// Opacity (0.0 = invisible, 1.0 = fully opaque).
    pub opacity: f32,
    /// Text color.
    pub color: Color,
    /// Position on the page.
    pub position: Position,
    /// Z-order.
    pub layer: Layer,
}

impl Default for TextWatermark {
    fn default() -> Self {
        Self {
            text: "WATERMARK".into(),
            font_size: 72.0,
            rotation: 45.0,
            opacity: 0.3,
            color: Color::Gray(0.7),
            position: Position::Center,
            layer: Layer::Background,
        }
    }
}

/// Preset stamp text templates.
#[derive(Debug, Clone, Copy)]
pub enum StampPreset {
    Draft,
    Confidential,
    Approved,
    Final,
    Copy,
    NotForDistribution,
}

impl StampPreset {
    /// Get the display text for this stamp.
    pub fn text(&self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Confidential => "CONFIDENTIAL",
            Self::Approved => "APPROVED",
            Self::Final => "FINAL",
            Self::Copy => "COPY",
            Self::NotForDistribution => "NOT FOR DISTRIBUTION",
        }
    }

    /// Create a `TextWatermark` from this preset.
    pub fn to_watermark(&self) -> TextWatermark {
        TextWatermark {
            text: self.text().into(),
            font_size: 60.0,
            rotation: 45.0,
            opacity: 0.25,
            color: Color::Rgb(1.0, 0.0, 0.0),
            position: Position::Center,
            layer: Layer::Foreground,
        }
    }
}

/// Configuration for tiled watermarks (repeating pattern).
#[derive(Debug, Clone)]
pub struct TiledConfig {
    /// Horizontal spacing between tiles in points.
    pub h_spacing: f32,
    /// Vertical spacing between tiles in points.
    pub v_spacing: f32,
}

impl Default for TiledConfig {
    fn default() -> Self {
        Self {
            h_spacing: 200.0,
            v_spacing: 200.0,
        }
    }
}

/// Apply a text watermark to selected pages.
pub fn apply_text_watermark(
    doc: &mut Document,
    watermark: &TextWatermark,
    selection: &PageSelection,
) -> Result<()> {
    apply_text_watermark_tiled(doc, watermark, selection, None)
}

/// Apply a tiled text watermark to selected pages.
///
/// If `tiled` is `Some`, the watermark repeats across the page.
pub fn apply_text_watermark_tiled(
    doc: &mut Document,
    watermark: &TextWatermark,
    selection: &PageSelection,
    tiled: Option<&TiledConfig>,
) -> Result<()> {
    let page_nums = resolve_page_selection(doc, selection)?;

    // Create an ExtGState for opacity.
    let gs_dict = dictionary! {
        "Type" => "ExtGState",
        "ca" => Object::Real(watermark.opacity),
        "CA" => Object::Real(watermark.opacity),
    };
    let gs_id = doc.add_object(Object::Dictionary(gs_dict));

    for page_num in &page_nums {
        let pages = doc.get_pages();
        let page_id = *pages
            .get(page_num)
            .ok_or(ManipError::PageOutOfRange(*page_num as usize, pages.len()))?;

        // Get page dimensions from MediaBox.
        let (width, height) = get_page_dimensions(doc, page_id)?;

        // Build the watermark content stream.
        let ops = build_text_watermark_ops(watermark, width, height, tiled);

        let content_data = Content { operations: ops }.encode().map_err(|e| {
            ManipError::Watermark(format!("failed to encode watermark content: {e}"))
        })?;

        let wm_stream = Stream::new(dictionary! {}, content_data);
        let wm_id = doc.add_object(Object::Stream(wm_stream));

        // Register the ExtGState and font in the page resources.
        ensure_page_resource(doc, page_id, "ExtGState", "GS_WM", gs_id);
        ensure_page_font(doc, page_id, "F_WM");

        // Append or prepend the watermark stream to the page content.
        add_content_to_page(doc, page_id, wm_id, watermark.layer);
    }

    Ok(())
}

/// Apply a stamp preset to selected pages.
pub fn apply_stamp(
    doc: &mut Document,
    preset: StampPreset,
    selection: &PageSelection,
) -> Result<()> {
    let wm = preset.to_watermark();
    apply_text_watermark(doc, &wm, selection)
}

/// Resolve a `PageSelection` to a list of 1-based page numbers.
pub(crate) fn resolve_page_selection(
    doc: &Document,
    selection: &PageSelection,
) -> Result<Vec<u32>> {
    let total = doc.get_pages().len() as u32;
    let pages = match selection {
        PageSelection::All => (1..=total).collect(),
        PageSelection::Even => (1..=total).filter(|p| p % 2 == 0).collect(),
        PageSelection::Odd => (1..=total).filter(|p| p % 2 != 0).collect(),
        PageSelection::Range(start, end) => {
            if *start == 0 || *end > total || *start > *end {
                return Err(ManipError::PageOutOfRange(*end as usize, total as usize));
            }
            (*start..=*end).collect()
        }
        PageSelection::Pages(ps) => {
            for &p in ps {
                if p == 0 || p > total {
                    return Err(ManipError::PageOutOfRange(p as usize, total as usize));
                }
            }
            ps.clone()
        }
    };
    Ok(pages)
}

/// Get page width and height from its MediaBox.
pub(crate) fn get_page_dimensions(doc: &Document, page_id: ObjectId) -> Result<(f32, f32)> {
    if let Some(Object::Dictionary(dict)) = doc.objects.get(&page_id) {
        if let Ok(Object::Array(media_box)) = dict.get(b"MediaBox") {
            if media_box.len() >= 4 {
                let x2 = obj_to_f32(&media_box[2]).unwrap_or(612.0);
                let y2 = obj_to_f32(&media_box[3]).unwrap_or(792.0);
                let x1 = obj_to_f32(&media_box[0]).unwrap_or(0.0);
                let y1 = obj_to_f32(&media_box[1]).unwrap_or(0.0);
                return Ok((x2 - x1, y2 - y1));
            }
        }
    }
    // Default to US Letter.
    Ok((612.0, 792.0))
}

fn obj_to_f32(obj: &Object) -> Option<f32> {
    match obj {
        Object::Integer(n) => Some(*n as f32),
        Object::Real(n) => Some(*n),
        _ => None,
    }
}

/// Build content stream operations for a text watermark.
fn build_text_watermark_ops(
    watermark: &TextWatermark,
    page_width: f32,
    page_height: f32,
    tiled: Option<&TiledConfig>,
) -> Vec<Operation> {
    let mut ops = Vec::new();

    // Save graphics state.
    ops.push(Operation::new("q", vec![]));
    // Set extended graphics state (opacity).
    ops.push(Operation::new("gs", vec![Object::Name(b"GS_WM".to_vec())]));

    // Set color.
    match watermark.color {
        Color::Rgb(r, g, b) => {
            ops.push(Operation::new(
                "rg",
                vec![Object::Real(r), Object::Real(g), Object::Real(b)],
            ));
        }
        Color::Cmyk(c, m, y, k) => {
            ops.push(Operation::new(
                "k",
                vec![
                    Object::Real(c),
                    Object::Real(m),
                    Object::Real(y),
                    Object::Real(k),
                ],
            ));
        }
        Color::Gray(g) => {
            ops.push(Operation::new("g", vec![Object::Real(g)]));
        }
    }

    let positions = if let Some(tile) = tiled {
        // Generate tiled positions.
        let mut positions = Vec::new();
        let mut y = 0.0f32;
        while y < page_height {
            let mut x = 0.0f32;
            while x < page_width {
                positions.push((x, y));
                x += tile.h_spacing;
            }
            y += tile.v_spacing;
        }
        positions
    } else {
        // Single position.
        let (x, y) = resolve_position(&watermark.position, page_width, page_height);
        vec![(x, y)]
    };

    let rad = watermark.rotation.to_radians();
    let cos_a = rad.cos();
    let sin_a = rad.sin();

    for (x, y) in positions {
        // Begin text object.
        ops.push(Operation::new("BT", vec![]));
        // Set font.
        ops.push(Operation::new(
            "Tf",
            vec![
                Object::Name(b"F_WM".to_vec()),
                Object::Real(watermark.font_size),
            ],
        ));
        // Set text matrix with rotation and position.
        ops.push(Operation::new(
            "Tm",
            vec![
                Object::Real(cos_a),
                Object::Real(sin_a),
                Object::Real(-sin_a),
                Object::Real(cos_a),
                Object::Real(x),
                Object::Real(y),
            ],
        ));
        // Show text.
        ops.push(Operation::new(
            "Tj",
            vec![Object::String(
                watermark.text.as_bytes().to_vec(),
                lopdf::StringFormat::Literal,
            )],
        ));
        // End text object.
        ops.push(Operation::new("ET", vec![]));
    }

    // Restore graphics state.
    ops.push(Operation::new("Q", vec![]));

    ops
}

/// Resolve a Position to (x, y) coordinates.
fn resolve_position(pos: &Position, page_width: f32, page_height: f32) -> (f32, f32) {
    match *pos {
        Position::Center => (page_width / 2.0, page_height / 2.0),
        Position::TopLeft(dx, dy) => (dx, page_height - dy),
        Position::TopRight(dx, dy) => (page_width - dx, page_height - dy),
        Position::BottomLeft(dx, dy) => (dx, dy),
        Position::BottomRight(dx, dy) => (page_width - dx, dy),
        Position::Exact(x, y) => (x, y),
    }
}

/// Ensure a page has a named resource entry in the given sub-dictionary.
pub(crate) fn ensure_page_resource(
    doc: &mut Document,
    page_id: ObjectId,
    category: &str,
    name: &str,
    obj_id: ObjectId,
) {
    if let Some(Object::Dictionary(ref mut page_dict)) = doc.objects.get_mut(&page_id) {
        // Check if Resources exists and has the category sub-dict.
        let has_resources = page_dict.get(b"Resources").ok().is_some();

        if !has_resources {
            let mut cat_dict = lopdf::Dictionary::new();
            cat_dict.set(name, Object::Reference(obj_id));
            let mut res_dict = lopdf::Dictionary::new();
            res_dict.set(category, Object::Dictionary(cat_dict));
            page_dict.set("Resources", Object::Dictionary(res_dict));
            return;
        }

        // Resources exists — get or create the category sub-dict.
        if let Ok(Object::Dictionary(ref mut res)) = page_dict.get_mut(b"Resources") {
            let has_cat = res.get(category.as_bytes()).ok().is_some();
            if has_cat {
                if let Ok(Object::Dictionary(ref mut cat_d)) = res.get_mut(category.as_bytes()) {
                    cat_d.set(name, Object::Reference(obj_id));
                }
            } else {
                let mut cat_dict = lopdf::Dictionary::new();
                cat_dict.set(name, Object::Reference(obj_id));
                res.set(category, Object::Dictionary(cat_dict));
            }
        }
    }
}

/// Ensure a page has a Helvetica font registered as the given name.
pub(crate) fn ensure_page_font(doc: &mut Document, page_id: ObjectId, name: &str) {
    let font_dict = dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    };
    let font_id = doc.add_object(Object::Dictionary(font_dict));
    ensure_page_resource(doc, page_id, "Font", name, font_id);
}

/// Add a content stream to a page, either prepending (background) or appending (foreground).
pub(crate) fn add_content_to_page(
    doc: &mut Document,
    page_id: ObjectId,
    stream_id: ObjectId,
    layer: Layer,
) {
    if let Some(Object::Dictionary(ref mut page_dict)) = doc.objects.get_mut(&page_id) {
        let existing = page_dict.get(b"Contents").ok().cloned();

        let new_contents = match existing {
            Some(Object::Reference(existing_id)) => match layer {
                Layer::Background => Object::Array(vec![
                    Object::Reference(stream_id),
                    Object::Reference(existing_id),
                ]),
                Layer::Foreground => Object::Array(vec![
                    Object::Reference(existing_id),
                    Object::Reference(stream_id),
                ]),
            },
            Some(Object::Array(mut arr)) => {
                match layer {
                    Layer::Background => {
                        arr.insert(0, Object::Reference(stream_id));
                    }
                    Layer::Foreground => {
                        arr.push(Object::Reference(stream_id));
                    }
                }
                Object::Array(arr)
            }
            _ => Object::Reference(stream_id),
        };

        page_dict.set("Contents", new_contents);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_doc(num_pages: usize) -> Document {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let mut kids = Vec::new();

        for i in 0..num_pages {
            let content_data = format!("BT /F1 12 Tf (Page {}) Tj ET", i + 1);
            let content_stream = Stream::new(dictionary! {}, content_data.into_bytes());
            let content_id = doc.add_object(Object::Stream(content_stream));

            let page_dict = dictionary! {
                "Type" => "Page",
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => Object::Array(vec![
                    Object::Integer(0), Object::Integer(0),
                    Object::Integer(612), Object::Integer(792),
                ]),
                "Contents" => Object::Reference(content_id),
                "Resources" => Object::Dictionary(lopdf::Dictionary::new()),
            };
            let page_id = doc.add_object(Object::Dictionary(page_dict));
            kids.push(Object::Reference(page_id));
        }

        let pages_dict = dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(num_pages as i64),
            "Kids" => Object::Array(kids),
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages_dict));

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc
    }

    #[test]
    fn test_apply_text_watermark() {
        let mut doc = make_test_doc(3);
        let wm = TextWatermark::default();
        apply_text_watermark(&mut doc, &wm, &PageSelection::All).unwrap();
        // Verify pages still exist.
        assert_eq!(doc.get_pages().len(), 3);
    }

    #[test]
    fn test_apply_stamp() {
        let mut doc = make_test_doc(2);
        apply_stamp(&mut doc, StampPreset::Draft, &PageSelection::Odd).unwrap();
        assert_eq!(doc.get_pages().len(), 2);
    }

    #[test]
    fn test_tiled_watermark() {
        let mut doc = make_test_doc(1);
        let wm = TextWatermark {
            text: "TILED".into(),
            ..TextWatermark::default()
        };
        let tile = TiledConfig {
            h_spacing: 150.0,
            v_spacing: 150.0,
        };
        apply_text_watermark_tiled(&mut doc, &wm, &PageSelection::All, Some(&tile)).unwrap();
        assert_eq!(doc.get_pages().len(), 1);
    }

    #[test]
    fn test_page_selection_even_odd() {
        let doc = make_test_doc(6);
        let even = resolve_page_selection(&doc, &PageSelection::Even).unwrap();
        assert_eq!(even, vec![2, 4, 6]);
        let odd = resolve_page_selection(&doc, &PageSelection::Odd).unwrap();
        assert_eq!(odd, vec![1, 3, 5]);
    }
}
