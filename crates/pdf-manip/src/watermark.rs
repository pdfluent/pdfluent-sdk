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

/// Common preset texts used by [`StampPreset::text`] when stamping a
/// document. Each variant resolves to a fixed uppercase string written
/// to the page; use [`StampPreset::text`] to obtain the literal content.
///
/// For arbitrary text (custom legends, locale-specific phrases) build a
/// watermark directly from a string instead of using a preset.
#[derive(Debug, Clone, Copy)]
pub enum StampPreset {
    /// "DRAFT" — work-in-progress marker.
    Draft,
    /// "CONFIDENTIAL" — distribution-restricted marker.
    Confidential,
    /// "APPROVED" — review-completed marker.
    Approved,
    /// "FINAL" — completion marker indicating the document is sealed.
    Final,
    /// "COPY" — duplicate-tracking marker.
    Copy,
    /// "NOT FOR DISTRIBUTION" — explicit no-share restriction.
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
/// Resolve the dictionary that a page's `/Resources` actually lives in, so a
/// caller can mutate it.
///
/// `/Resources` reaches a page in three shapes, and all three occur in the
/// wild:
///
/// 1. a direct dictionary on the page,
/// 2. an **indirect reference** to a separate object (very common, and often
///    shared between pages),
/// 3. **inherited** from an ancestor `/Pages` node, with no `/Resources` key on
///    the page at all (ISO 32000-2 §7.7.3.4).
///
/// Returns the object id holding the resources dictionary, creating one on the
/// page when needed. For the inherited case the ancestor's dictionary is copied
/// down onto the page first: writing a fresh dictionary containing only our own
/// entry would *shadow* the inherited resources and break every other operator
/// on the page.
fn resolve_or_create_page_resources(doc: &mut Document, page_id: ObjectId) -> Option<ObjectId> {
    let existing = match doc.objects.get(&page_id) {
        Some(Object::Dictionary(page_dict)) => page_dict.get(b"Resources").ok().cloned(),
        _ => return None,
    };

    match existing {
        // Already an indirect object: mutate that object directly.
        Some(Object::Reference(res_id)) => Some(res_id),

        // Direct dictionary on the page: promote it to its own object so we
        // have a stable id to hand back.
        Some(Object::Dictionary(res)) => {
            let res_id = doc.add_object(Object::Dictionary(res));
            if let Some(Object::Dictionary(ref mut page_dict)) = doc.objects.get_mut(&page_id) {
                page_dict.set("Resources", Object::Reference(res_id));
            }
            Some(res_id)
        }

        // Absent: inherited from an ancestor /Pages node, or genuinely missing.
        _ => {
            let inherited = find_inherited_resources(doc, page_id);
            let res_id = doc.add_object(Object::Dictionary(inherited.unwrap_or_default()));
            if let Some(Object::Dictionary(ref mut page_dict)) = doc.objects.get_mut(&page_id) {
                page_dict.set("Resources", Object::Reference(res_id));
            }
            Some(res_id)
        }
    }
}

/// Walk the `/Parent` chain looking for an inherited `/Resources` dictionary.
fn find_inherited_resources(doc: &Document, page_id: ObjectId) -> Option<lopdf::Dictionary> {
    let mut current = page_id;
    // Bounded walk: malformed files can contain /Parent cycles.
    for _ in 0..32 {
        let parent = match doc.objects.get(&current) {
            Some(Object::Dictionary(d)) => match d.get(b"Parent") {
                Ok(Object::Reference(pid)) => *pid,
                _ => return None,
            },
            _ => return None,
        };
        match doc.objects.get(&parent) {
            Some(Object::Dictionary(pd)) => match pd.get(b"Resources") {
                Ok(Object::Dictionary(res)) => return Some(res.clone()),
                Ok(Object::Reference(rid)) => {
                    if let Some(Object::Dictionary(res)) = doc.objects.get(rid) {
                        return Some(res.clone());
                    }
                }
                _ => {}
            },
            _ => return None,
        }
        current = parent;
    }
    None
}

pub(crate) fn ensure_page_resource(
    doc: &mut Document,
    page_id: ObjectId,
    category: &str,
    name: &str,
    obj_id: ObjectId,
) {
    let Some(res_id) = resolve_or_create_page_resources(doc, page_id) else {
        return;
    };

    // The resources dictionary may itself hold the category as an indirect
    // reference; resolve that too before writing.
    let cat_ref = match doc.objects.get(&res_id) {
        Some(Object::Dictionary(res)) => match res.get(category.as_bytes()) {
            Ok(Object::Reference(cid)) => Some(*cid),
            _ => None,
        },
        _ => return,
    };

    if let Some(cid) = cat_ref {
        if let Some(Object::Dictionary(ref mut cat_d)) = doc.objects.get_mut(&cid) {
            cat_d.set(name, Object::Reference(obj_id));
            return;
        }
    }

    if let Some(Object::Dictionary(ref mut res)) = doc.objects.get_mut(&res_id) {
        if let Ok(Object::Dictionary(ref mut cat_d)) = res.get_mut(category.as_bytes()) {
            cat_d.set(name, Object::Reference(obj_id));
            return;
        }
        let mut cat_dict = lopdf::Dictionary::new();
        cat_dict.set(name, Object::Reference(obj_id));
        res.set(category, Object::Dictionary(cat_dict));
    }
}

/// Register the watermark font on a page, creating it once per document.
///
/// The font object is shared across every watermarked page. It used to be
/// created per page, and since the PDF/A pipeline embeds every non-embedded
/// font, a 4-page document ended up carrying four separate 423 KB copies of
/// Helvetica. Reusing one object keeps that at one copy however many pages
/// are stamped.
///
/// `/Widths` and a `/FontDescriptor` are written out rather than left to the
/// standard-14 default: PDF/A requires them on every font, and relying on a
/// later pipeline pass to fill them in means the watermark is only conformant
/// when it happens to run inside that pipeline.
pub(crate) fn ensure_page_font(doc: &mut Document, page_id: ObjectId, name: &str) {
    let font_id = find_or_create_watermark_font(doc);
    ensure_page_resource(doc, page_id, "Font", name, font_id);
}

/// The shared watermark font object, created on first use.
fn find_or_create_watermark_font(doc: &mut Document) -> ObjectId {
    // A previous page in this same run already made it. Matching on the exact
    // shape we write — not just "some Helvetica" — keeps us from adopting a
    // font the source document defined with a different encoding, which would
    // change what the watermark text draws.
    let name_is = |d: &lopdf::Dictionary, key: &[u8], want: &[u8]| {
        d.get(key).ok().and_then(|o| o.as_name().ok()) == Some(want)
    };
    let int_is = |d: &lopdf::Dictionary, key: &[u8], want: i64| {
        d.get(key).ok().and_then(|o| o.as_i64().ok()) == Some(want)
    };
    for (id, obj) in &doc.objects {
        let Object::Dictionary(d) = obj else { continue };
        if name_is(d, b"Type", b"Font")
            && name_is(d, b"BaseFont", b"Helvetica")
            && name_is(d, b"Encoding", b"WinAnsiEncoding")
            && int_is(d, b"FirstChar", 32)
            && int_is(d, b"LastChar", 126)
            && d.has(b"FontDescriptor")
        {
            return *id;
        }
    }

    let descriptor = dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => "Helvetica",
        // Non-symbolic (bit 6), the flag every Latin text font carries.
        "Flags" => 32,
        "FontBBox" => vec![(-166).into(), (-225).into(), 1000.into(), 931.into()],
        "ItalicAngle" => 0,
        "Ascent" => 718,
        "Descent" => (-207),
        "CapHeight" => 718,
        "StemV" => 88,
        "MissingWidth" => 0,
    };
    let descriptor_id = doc.add_object(Object::Dictionary(descriptor));

    let widths: Vec<Object> = HELVETICA_WIDTHS_32_126
        .iter()
        .map(|w| Object::Integer(*w as i64))
        .collect();

    let font = dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
        "FirstChar" => 32,
        "LastChar" => 126,
        "Widths" => widths,
        "FontDescriptor" => descriptor_id,
    };
    doc.add_object(Object::Dictionary(font))
}

/// Helvetica advance widths for codes 32..=126, from the Adobe AFM metrics.
///
/// Watermark text is ASCII, so the printable range is all that is needed. Any
/// character outside it falls back to the descriptor's `/MissingWidth`.
const HELVETICA_WIDTHS_32_126: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, // 32-47
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, // 48-63
    1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, // 64-79
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556, // 80-95
    333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, // 96-111
    556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584, // 112-126
];

/// Add a content stream to a page, either prepending (background) or appending (foreground).
pub(crate) fn add_content_to_page(
    doc: &mut Document,
    page_id: ObjectId,
    stream_id: ObjectId,
    layer: Layer,
) {
    let existing = match doc.objects.get(&page_id) {
        Some(Object::Dictionary(page_dict)) => page_dict.get(b"Contents").ok().cloned(),
        _ => None,
    };

    // A /Contents reference may point to an indirect *array* of streams
    // rather than a single stream (govdocs holdout 590_590336). Wrapping
    // that reference in a new array nests an array inside /Contents, which
    // is invalid — viewers ignore the nested array and the page renders
    // blank. Append to the referenced array in place instead.
    if let Some(Object::Reference(existing_id)) = existing {
        if matches!(doc.objects.get(&existing_id), Some(Object::Array(_))) {
            if let Some(Object::Array(ref mut arr)) = doc.objects.get_mut(&existing_id) {
                match layer {
                    Layer::Background => arr.insert(0, Object::Reference(stream_id)),
                    Layer::Foreground => arr.push(Object::Reference(stream_id)),
                }
            }
            return;
        }
    }

    if let Some(Object::Dictionary(ref mut page_dict)) = doc.objects.get_mut(&page_id) {
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

    /// The watermark font is created once and shared, not once per page.
    ///
    /// It used to be per page, and because the PDF/A pipeline embeds every
    /// non-embedded font, each copy pulled in its own ~423 KB Helvetica.
    #[test]
    fn watermark_font_is_shared_across_pages() {
        let mut doc = make_test_doc(6);
        apply_text_watermark(&mut doc, &TextWatermark::default(), &PageSelection::All).unwrap();

        let fonts: Vec<_> = doc
            .objects
            .iter()
            .filter(|(_, o)| {
                matches!(o, Object::Dictionary(d)
                    if d.get(b"BaseFont").ok().and_then(|o| o.as_name().ok()) == Some(b"Helvetica"))
            })
            .collect();
        assert_eq!(fonts.len(), 1, "one Helvetica per document, not per page");

        // Every page must still resolve /F_WM to that one object.
        let font_id = *fonts[0].0;
        let pages = doc.get_pages();
        assert_eq!(pages.len(), 6);
        for page_id in pages.values() {
            let res = resolve_or_create_page_resources(&mut doc, *page_id).unwrap();
            let Some(Object::Dictionary(res)) = doc.objects.get(&res) else {
                panic!("no resources")
            };
            let Ok(Object::Dictionary(font_res)) = res.get(b"Font") else {
                panic!("no /Font resources")
            };
            assert_eq!(
                font_res.get(b"F_WM").ok(),
                Some(&Object::Reference(font_id))
            );
        }
    }

    /// PDF/A requires /Widths and a /FontDescriptor on every font. Writing them
    /// here rather than leaving them to a later pipeline pass means a
    /// watermarked document is conformant regardless of what runs after.
    #[test]
    fn watermark_font_carries_pdfa_required_keys() {
        let mut doc = make_test_doc(1);
        apply_text_watermark(&mut doc, &TextWatermark::default(), &PageSelection::All).unwrap();

        let font = doc
            .objects
            .values()
            .find_map(|o| match o {
                Object::Dictionary(d)
                    if d.get(b"BaseFont").ok().and_then(|o| o.as_name().ok())
                        == Some(b"Helvetica") =>
                {
                    Some(d)
                }
                _ => None,
            })
            .expect("watermark font");

        assert_eq!(font.get(b"FirstChar").unwrap().as_i64().unwrap(), 32);
        assert_eq!(font.get(b"LastChar").unwrap().as_i64().unwrap(), 126);
        let Ok(Object::Array(widths)) = font.get(b"Widths") else {
            panic!("no /Widths")
        };
        assert_eq!(widths.len(), 95, "one width per code 32..=126");
        // Spot-check against the Adobe AFM metrics: space, 'A', 'a'.
        assert_eq!(widths[0].as_i64().unwrap(), 278);
        assert_eq!(widths[(b'A' - 32) as usize].as_i64().unwrap(), 667);
        assert_eq!(widths[(b'a' - 32) as usize].as_i64().unwrap(), 556);

        let Ok(Object::Reference(fd_id)) = font.get(b"FontDescriptor") else {
            panic!("no /FontDescriptor")
        };
        let Some(Object::Dictionary(fd)) = doc.objects.get(fd_id) else {
            panic!("descriptor missing")
        };
        for key in [
            &b"Flags"[..],
            b"FontBBox",
            b"ItalicAngle",
            b"Ascent",
            b"Descent",
            b"CapHeight",
            b"StemV",
        ] {
            assert!(
                fd.has(key),
                "descriptor missing /{}",
                String::from_utf8_lossy(key)
            );
        }
    }

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

    /// Regression: a /Contents reference may point to an indirect *array* of
    /// streams. Wrapping that reference in a new array nests an array inside
    /// /Contents — invalid, and viewers ignore the nested array so the page
    /// renders blank (govdocs holdout 590_590336).
    #[test]
    fn add_content_to_indirect_contents_array_appends_in_place() {
        let mut doc = make_test_doc(1);
        let page_id = *doc.get_pages().values().next().unwrap();

        // Replace /Contents with a reference to an indirect array of streams.
        let existing = {
            let Some(Object::Dictionary(page)) = doc.objects.get(&page_id) else {
                panic!("no page")
            };
            match page.get(b"Contents").unwrap() {
                Object::Reference(id) => *id,
                _ => panic!("expected contents reference"),
            }
        };
        let arr_id = doc.add_object(Object::Array(vec![Object::Reference(existing)]));
        if let Some(Object::Dictionary(ref mut page)) = doc.objects.get_mut(&page_id) {
            page.set("Contents", Object::Reference(arr_id));
        }

        let wm_stream = Stream::new(dictionary! {}, b"q Q".to_vec());
        let wm_id = doc.add_object(Object::Stream(wm_stream));
        add_content_to_page(&mut doc, page_id, wm_id, Layer::Foreground);

        // The page must still point at the indirect array, and that array
        // must contain both stream references — no nesting.
        let Some(Object::Dictionary(page)) = doc.objects.get(&page_id) else {
            panic!("no page")
        };
        let Ok(Object::Reference(contents_ref)) = page.get(b"Contents") else {
            panic!(
                "contents no longer a reference: {:?}",
                page.get(b"Contents")
            )
        };
        assert_eq!(*contents_ref, arr_id);
        let Some(Object::Array(arr)) = doc.objects.get(&arr_id) else {
            panic!("indirect array gone")
        };
        assert_eq!(
            arr.as_slice(),
            &[Object::Reference(existing), Object::Reference(wm_id)]
        );
    }
}
