//! Annotation builder for creating PDF annotations via lopdf.
//!
//! Provides `AnnotationBuilder` for constructing annotation dictionaries
//! with appearance streams and adding them to a PDF document.

#[cfg(feature = "write")]
use lopdf::{dictionary, Document, Object, ObjectId, Stream};

#[cfg(feature = "write")]
use crate::appearance_writer::{AppearanceColor, AppearanceStreamBuilder};
#[cfg(feature = "write")]
use crate::error::AnnotBuildError;

/// Type alias for a custom appearance builder closure.
#[cfg(feature = "write")]
type AppearanceFn = Box<dyn FnOnce(&mut AppearanceStreamBuilder)>;

/// PDF annotation rectangle in user-space coordinates.
#[cfg(feature = "write")]
#[derive(Debug, Clone, Copy)]
pub struct AnnotRect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

#[cfg(feature = "write")]
impl AnnotRect {
    pub fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self { x0, y0, x1, y1 }
    }

    pub fn width(&self) -> f64 {
        (self.x1 - self.x0).abs()
    }

    pub fn height(&self) -> f64 {
        (self.y1 - self.y0).abs()
    }

    fn as_array(&self) -> Object {
        Object::Array(vec![
            Object::Real(self.x0 as f32),
            Object::Real(self.y0 as f32),
            Object::Real(self.x1 as f32),
            Object::Real(self.y1 as f32),
        ])
    }
}

/// The annotation subtype to create.
#[cfg(feature = "write")]
#[derive(Debug, Clone, Copy)]
pub enum AnnotSubtype {
    Square,
    Circle,
    Line,
    Highlight,
    Underline,
    StrikeOut,
    Squiggly,
    FreeText,
    Text,
    Stamp,
    Ink,
    Polygon,
    PolyLine,
    Link,
}

#[cfg(feature = "write")]
impl AnnotSubtype {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Square => "Square",
            Self::Circle => "Circle",
            Self::Line => "Line",
            Self::Highlight => "Highlight",
            Self::Underline => "Underline",
            Self::StrikeOut => "StrikeOut",
            Self::Squiggly => "Squiggly",
            Self::FreeText => "FreeText",
            Self::Text => "Text",
            Self::Stamp => "Stamp",
            Self::Ink => "Ink",
            Self::Polygon => "Polygon",
            Self::PolyLine => "PolyLine",
            Self::Link => "Link",
        }
    }
}

/// Builder for creating a PDF annotation and adding it to a document.
///
/// # Example
/// ```no_run
/// use pdf_annot::builder::{AnnotationBuilder, AnnotSubtype, AnnotRect};
///
/// let mut doc = lopdf::Document::with_version("1.7");
/// // ... add pages ...
/// let annot_id = AnnotationBuilder::new(AnnotSubtype::Square, AnnotRect::new(100.0, 200.0, 300.0, 400.0))
///     .color(1.0, 0.0, 0.0)
///     .border_width(2.0)
///     .contents("A red square")
///     .build(&mut doc)
///     .unwrap();
/// ```
#[cfg(feature = "write")]
pub struct AnnotationBuilder {
    subtype: AnnotSubtype,
    rect: AnnotRect,
    color: Option<AppearanceColor>,
    interior_color: Option<AppearanceColor>,
    opacity: Option<f64>,
    border_width: f64,
    contents: Option<String>,
    flags: u32,
    /// QuadPoints for markup annotations (Highlight, Underline, StrikeOut, Squiggly).
    quad_points: Option<Vec<f64>>,
    /// Line endpoints /L [x1 y1 x2 y2] for Line annotations.
    line_endpoints: Option<[f64; 4]>,
    /// Line endings /LE for Line annotations.
    line_endings: Option<[LineEnding; 2]>,
    /// InkList for Ink annotations: list of stroke paths.
    ink_list: Option<Vec<Vec<f64>>>,
    /// Vertices for Polygon/PolyLine annotations.
    vertices: Option<Vec<f64>>,
    /// Dash pattern for stroked annotations.
    dash_pattern: Option<Vec<f64>>,
    /// Default appearance string (/DA) for FreeText annotations.
    default_appearance_str: Option<String>,
    /// Text alignment (/Q) for FreeText: 0=left, 1=center, 2=right.
    text_alignment: Option<i64>,
    /// Icon name (/Name) for Text (sticky note) and Stamp annotations.
    icon_name: Option<String>,
    /// URI action (/A) for Link annotations.
    uri_action: Option<String>,
    /// Named destination (/Dest) for Link annotations.
    destination: Option<String>,
    /// Custom appearance builder function. If None, a default appearance is generated.
    custom_appearance: Option<AppearanceFn>,
}

/// Standard stamp names per ISO 32000-2 §12.5.6.12.
#[cfg(feature = "write")]
#[derive(Debug, Clone, Copy)]
pub enum StampName {
    Approved,
    Experimental,
    NotApproved,
    AsIs,
    Expired,
    NotForPublicRelease,
    Confidential,
    Final,
    Sold,
    Departmental,
    ForComment,
    TopSecret,
    Draft,
    ForPublicRelease,
}

#[cfg(feature = "write")]
impl StampName {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Approved => "Approved",
            Self::Experimental => "Experimental",
            Self::NotApproved => "NotApproved",
            Self::AsIs => "AsIs",
            Self::Expired => "Expired",
            Self::NotForPublicRelease => "NotForPublicRelease",
            Self::Confidential => "Confidential",
            Self::Final => "Final",
            Self::Sold => "Sold",
            Self::Departmental => "Departmental",
            Self::ForComment => "ForComment",
            Self::TopSecret => "TopSecret",
            Self::Draft => "Draft",
            Self::ForPublicRelease => "ForPublicRelease",
        }
    }
}

/// Standard icon names for Text (sticky note) annotations.
#[cfg(feature = "write")]
#[derive(Debug, Clone, Copy)]
pub enum TextIcon {
    Comment,
    Key,
    Note,
    Help,
    NewParagraph,
    Paragraph,
    Insert,
}

#[cfg(feature = "write")]
impl TextIcon {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Comment => "Comment",
            Self::Key => "Key",
            Self::Note => "Note",
            Self::Help => "Help",
            Self::NewParagraph => "NewParagraph",
            Self::Paragraph => "Paragraph",
            Self::Insert => "Insert",
        }
    }
}

/// Line ending style for Line annotations (ISO 32000-2 Table 179).
#[cfg(feature = "write")]
#[derive(Debug, Clone, Copy)]
pub enum LineEnding {
    None,
    Square,
    Circle,
    Diamond,
    OpenArrow,
    ClosedArrow,
    Butt,
    ROpenArrow,
    RClosedArrow,
    Slash,
}

#[cfg(feature = "write")]
impl LineEnding {
    fn as_str(&self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Square => "Square",
            Self::Circle => "Circle",
            Self::Diamond => "Diamond",
            Self::OpenArrow => "OpenArrow",
            Self::ClosedArrow => "ClosedArrow",
            Self::Butt => "Butt",
            Self::ROpenArrow => "ROpenArrow",
            Self::RClosedArrow => "RClosedArrow",
            Self::Slash => "Slash",
        }
    }
}

#[cfg(feature = "write")]
impl AnnotationBuilder {
    /// Create a new annotation builder for the given subtype and rectangle.
    pub fn new(subtype: AnnotSubtype, rect: AnnotRect) -> Self {
        Self {
            subtype,
            rect,
            color: None,
            interior_color: None,
            opacity: None,
            border_width: 1.0,
            contents: None,
            flags: 4, // Print flag set by default
            quad_points: None,
            line_endpoints: None,
            line_endings: None,
            ink_list: None,
            vertices: None,
            dash_pattern: None,
            default_appearance_str: None,
            text_alignment: None,
            icon_name: None,
            uri_action: None,
            destination: None,
            custom_appearance: None,
        }
    }

    /// Create a FreeText annotation with the given text and font size.
    pub fn free_text(rect: AnnotRect, text: &str, font_size: f64) -> Self {
        let da = format!("/Helv {font_size} Tf 0 g");
        let mut b = Self::new(AnnotSubtype::FreeText, rect).contents(text);
        b.default_appearance_str = Some(da);
        b
    }

    /// Create a Text (sticky note) annotation.
    pub fn sticky_note(rect: AnnotRect, icon: TextIcon) -> Self {
        let mut b = Self::new(AnnotSubtype::Text, rect);
        b.icon_name = Some(icon.as_str().to_string());
        b
    }

    /// Create a Stamp annotation with a standard stamp name.
    pub fn stamp(rect: AnnotRect, name: StampName) -> Self {
        let mut b = Self::new(AnnotSubtype::Stamp, rect);
        b.icon_name = Some(name.as_str().to_string());
        b
    }

    /// Create a Stamp annotation with a custom name.
    pub fn stamp_custom(rect: AnnotRect, name: &str) -> Self {
        let mut b = Self::new(AnnotSubtype::Stamp, rect);
        b.icon_name = Some(name.to_string());
        b
    }

    /// Create a Link annotation with a URI action.
    pub fn link_uri(rect: AnnotRect, uri: &str) -> Self {
        let mut b = Self::new(AnnotSubtype::Link, rect);
        b.uri_action = Some(uri.to_string());
        b.border_width = 0.0; // Links typically have no border.
        b
    }

    /// Create a Link annotation with a named destination.
    pub fn link_dest(rect: AnnotRect, dest: &str) -> Self {
        let mut b = Self::new(AnnotSubtype::Link, rect);
        b.destination = Some(dest.to_string());
        b.border_width = 0.0;
        b
    }

    /// Create a Square annotation.
    pub fn square(rect: AnnotRect) -> Self {
        Self::new(AnnotSubtype::Square, rect)
    }

    /// Create a Circle annotation.
    pub fn circle(rect: AnnotRect) -> Self {
        Self::new(AnnotSubtype::Circle, rect)
    }

    /// Create a Line annotation between two points.
    ///
    /// Automatically pads the bounding rect so it is never zero-area.
    pub fn line(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        let pad = 1.0; // Minimum 1pt padding to prevent zero-area rect.
        let mut min_x = x1.min(x2);
        let mut min_y = y1.min(y2);
        let mut max_x = x1.max(x2);
        let mut max_y = y1.max(y2);
        if (max_x - min_x).abs() < f64::EPSILON {
            min_x -= pad;
            max_x += pad;
        }
        if (max_y - min_y).abs() < f64::EPSILON {
            min_y -= pad;
            max_y += pad;
        }
        let rect = AnnotRect::new(min_x, min_y, max_x, max_y);
        let mut b = Self::new(AnnotSubtype::Line, rect);
        b.line_endpoints = Some([x1, y1, x2, y2]);
        b
    }

    /// Create an Ink annotation from stroke paths.
    pub fn ink(rect: AnnotRect, strokes: Vec<Vec<f64>>) -> Self {
        let mut b = Self::new(AnnotSubtype::Ink, rect);
        b.ink_list = Some(strokes);
        b
    }

    /// Create a Polygon annotation from vertices.
    pub fn polygon(rect: AnnotRect, vertices: Vec<f64>) -> Self {
        let mut b = Self::new(AnnotSubtype::Polygon, rect);
        b.vertices = Some(vertices);
        b
    }

    /// Create a PolyLine annotation from vertices.
    pub fn polyline(rect: AnnotRect, vertices: Vec<f64>) -> Self {
        let mut b = Self::new(AnnotSubtype::PolyLine, rect);
        b.vertices = Some(vertices);
        b
    }

    /// Create a Highlight markup annotation.
    pub fn highlight(rect: AnnotRect) -> Self {
        Self::new(AnnotSubtype::Highlight, rect)
            .color(1.0, 1.0, 0.0) // Yellow
            .opacity(0.4)
    }

    /// Create an Underline markup annotation.
    pub fn underline(rect: AnnotRect) -> Self {
        Self::new(AnnotSubtype::Underline, rect).color(0.0, 0.0, 1.0)
    }

    /// Create a StrikeOut markup annotation.
    pub fn strikeout(rect: AnnotRect) -> Self {
        Self::new(AnnotSubtype::StrikeOut, rect).color(1.0, 0.0, 0.0)
    }

    /// Create a Squiggly markup annotation.
    pub fn squiggly(rect: AnnotRect) -> Self {
        Self::new(AnnotSubtype::Squiggly, rect).color(0.0, 0.8, 0.0)
    }

    /// Set the annotation color (RGB, 0.0–1.0).
    pub fn color(mut self, r: f64, g: f64, b: f64) -> Self {
        self.color = Some(AppearanceColor::new(r, g, b));
        self
    }

    /// Set the interior (fill) color for annotations that support it.
    pub fn interior_color(mut self, r: f64, g: f64, b: f64) -> Self {
        self.interior_color = Some(AppearanceColor::new(r, g, b));
        self
    }

    /// Set the opacity (CA/ca, 0.0–1.0).
    pub fn opacity(mut self, alpha: f64) -> Self {
        self.opacity = Some(alpha.clamp(0.0, 1.0));
        self
    }

    /// Set the border/stroke width.
    pub fn border_width(mut self, width: f64) -> Self {
        self.border_width = width;
        self
    }

    /// Set the /Contents text.
    pub fn contents(mut self, text: impl Into<String>) -> Self {
        self.contents = Some(text.into());
        self
    }

    /// Set the annotation flags (raw u32).
    pub fn flags(mut self, flags: u32) -> Self {
        self.flags = flags;
        self
    }

    /// Set text alignment for FreeText annotations (0=left, 1=center, 2=right).
    pub fn alignment(mut self, q: i64) -> Self {
        self.text_alignment = Some(q);
        self
    }

    /// Set line endings for Line annotations.
    pub fn line_endings(mut self, start: LineEnding, end: LineEnding) -> Self {
        self.line_endings = Some([start, end]);
        self
    }

    /// Set a dash pattern for stroked annotations.
    pub fn dash(mut self, pattern: Vec<f64>) -> Self {
        self.dash_pattern = Some(pattern);
        self
    }

    /// Set QuadPoints for text markup annotations.
    ///
    /// Each quadrilateral is defined by 8 values in page coordinates:
    /// `[x1,y1, x2,y2, x3,y3, x4,y4]` where the points define the
    /// corners of the marked text region. Multiple quads can be
    /// concatenated for multi-line selections.
    pub fn quad_points(mut self, points: Vec<f64>) -> Self {
        self.quad_points = Some(points);
        self
    }

    /// Set QuadPoints from a simple rectangle (single quad).
    pub fn quad_points_from_rect(self, rect: &AnnotRect) -> Self {
        // PDF QuadPoints order: top-left, top-right, bottom-left, bottom-right
        self.quad_points(vec![
            rect.x0, rect.y1, // top-left
            rect.x1, rect.y1, // top-right
            rect.x0, rect.y0, // bottom-left
            rect.x1, rect.y0, // bottom-right
        ])
    }

    /// Provide a custom appearance builder closure.
    pub fn appearance(mut self, f: impl FnOnce(&mut AppearanceStreamBuilder) + 'static) -> Self {
        self.custom_appearance = Some(Box::new(f));
        self
    }

    /// Build the annotation, add it to the document, and return the annotation object ID.
    ///
    /// This creates the annotation dictionary, generates the appearance stream,
    /// and adds both as objects to the document. The annotation is NOT automatically
    /// added to any page's /Annots array — use `add_to_page` for that.
    pub fn build(mut self, doc: &mut Document) -> Result<ObjectId, AnnotBuildError> {
        let w = self.rect.width();
        let h = self.rect.height();
        if w < f64::EPSILON || h < f64::EPSILON {
            return Err(AnnotBuildError::InvalidRect);
        }

        // Extract custom appearance before building (avoids borrow issues).
        let custom_appearance = self.custom_appearance.take();

        // Build appearance stream.
        let ap_stream_id = self.build_appearance(doc, w, h, custom_appearance)?;

        // Build the annotation dictionary.
        let mut annot_dict = dictionary! {
            "Type" => "Annot",
            "Subtype" => Object::Name(self.subtype.as_str().as_bytes().to_vec()),
            "Rect" => self.rect.as_array(),
            "F" => Object::Integer(self.flags as i64),
        };

        // Color (/C).
        if let Some(ref c) = self.color {
            annot_dict.set(
                "C",
                Object::Array(vec![
                    Object::Real(c.r as f32),
                    Object::Real(c.g as f32),
                    Object::Real(c.b as f32),
                ]),
            );
        }

        // Interior color (/IC) — for Square, Circle.
        if let Some(ref ic) = self.interior_color {
            annot_dict.set(
                "IC",
                Object::Array(vec![
                    Object::Real(ic.r as f32),
                    Object::Real(ic.g as f32),
                    Object::Real(ic.b as f32),
                ]),
            );
        }

        // Opacity.
        if let Some(alpha) = self.opacity {
            annot_dict.set("CA", Object::Real(alpha as f32));
        }

        // Contents.
        if let Some(ref text) = self.contents {
            annot_dict.set(
                "Contents",
                Object::String(text.as_bytes().to_vec(), lopdf::StringFormat::Literal),
            );
        }

        // QuadPoints for markup annotations.
        if let Some(ref qp) = self.quad_points {
            let arr: Vec<Object> = qp.iter().map(|&v| Object::Real(v as f32)).collect();
            annot_dict.set("QuadPoints", Object::Array(arr));
        }

        // Line endpoints (/L) for Line annotations.
        if let Some(ref l) = self.line_endpoints {
            annot_dict.set(
                "L",
                Object::Array(vec![
                    Object::Real(l[0] as f32),
                    Object::Real(l[1] as f32),
                    Object::Real(l[2] as f32),
                    Object::Real(l[3] as f32),
                ]),
            );
        }

        // Line endings (/LE).
        if let Some(ref le) = self.line_endings {
            annot_dict.set(
                "LE",
                Object::Array(vec![
                    Object::Name(le[0].as_str().as_bytes().to_vec()),
                    Object::Name(le[1].as_str().as_bytes().to_vec()),
                ]),
            );
        }

        // InkList for Ink annotations.
        if let Some(ref ink) = self.ink_list {
            let ink_arr: Vec<Object> = ink
                .iter()
                .map(|stroke| {
                    Object::Array(stroke.iter().map(|&v| Object::Real(v as f32)).collect())
                })
                .collect();
            annot_dict.set("InkList", Object::Array(ink_arr));
        }

        // Vertices for Polygon/PolyLine annotations.
        if let Some(ref verts) = self.vertices {
            let arr: Vec<Object> = verts.iter().map(|&v| Object::Real(v as f32)).collect();
            annot_dict.set("Vertices", Object::Array(arr));
        }

        // Border style.
        let has_dash = self.dash_pattern.is_some();
        if (self.border_width - 1.0).abs() > f64::EPSILON || has_dash {
            let mut bs = dictionary! {
                "W" => Object::Real(self.border_width as f32),
            };
            if has_dash {
                bs.set("S", Object::Name(b"D".to_vec()));
                let d_arr: Vec<Object> = self
                    .dash_pattern
                    .as_ref()
                    .unwrap()
                    .iter()
                    .map(|&v| Object::Real(v as f32))
                    .collect();
                bs.set("D", Object::Array(d_arr));
            } else {
                bs.set("S", Object::Name(b"S".to_vec()));
            }
            annot_dict.set("BS", Object::Dictionary(bs));
        }

        // Default appearance string (/DA) for FreeText.
        if let Some(ref da) = self.default_appearance_str {
            annot_dict.set(
                "DA",
                Object::String(da.as_bytes().to_vec(), lopdf::StringFormat::Literal),
            );
        }

        // Text alignment (/Q) for FreeText.
        if let Some(q) = self.text_alignment {
            annot_dict.set("Q", Object::Integer(q));
        }

        // Icon name (/Name) for Text, Stamp.
        if let Some(ref name) = self.icon_name {
            annot_dict.set("Name", Object::Name(name.as_bytes().to_vec()));
        }

        // URI action (/A) for Link.
        if let Some(ref uri) = self.uri_action {
            let action = dictionary! {
                "S" => "URI",
                "URI" => Object::String(uri.as_bytes().to_vec(), lopdf::StringFormat::Literal),
            };
            annot_dict.set("A", Object::Dictionary(action));
        }

        // Named destination (/Dest) for Link.
        if let Some(ref dest) = self.destination {
            annot_dict.set(
                "Dest",
                Object::String(dest.as_bytes().to_vec(), lopdf::StringFormat::Literal),
            );
        }

        // Normal appearance.
        let ap = dictionary! {
            "N" => Object::Reference(ap_stream_id),
        };
        annot_dict.set("AP", Object::Dictionary(ap));

        Ok(doc.add_object(Object::Dictionary(annot_dict)))
    }

    /// Build appearance stream and add it as a Form XObject to the document.
    fn build_appearance(
        &self,
        doc: &mut Document,
        w: f64,
        h: f64,
        custom_appearance: Option<AppearanceFn>,
    ) -> Result<ObjectId, AnnotBuildError> {
        let mut builder = AppearanceStreamBuilder::new(w, h);

        if let Some(custom) = custom_appearance {
            custom(&mut builder);
        } else {
            self.default_appearance(&mut builder, w, h);
        }

        let content_bytes = builder
            .encode()
            .map_err(AnnotBuildError::AppearanceEncode)?;

        let mut stream_dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => Object::Array(vec![
                Object::Real(0.0),
                Object::Real(0.0),
                Object::Real(w as f32),
                Object::Real(h as f32),
            ]),
        };

        // Build resources for the form XObject.
        let needs_multiply = matches!(self.subtype, AnnotSubtype::Highlight);
        let needs_gs = self.opacity.is_some() || needs_multiply;
        let needs_font = matches!(self.subtype, AnnotSubtype::FreeText | AnnotSubtype::Stamp);

        if needs_gs || needs_font {
            let mut resources = lopdf::Dictionary::new();

            if needs_gs {
                let mut gs_dict = dictionary! {
                    "Type" => "ExtGState",
                };
                if let Some(alpha) = self.opacity {
                    gs_dict.set("ca", Object::Real(alpha as f32));
                    gs_dict.set("CA", Object::Real(alpha as f32));
                }
                if needs_multiply {
                    gs_dict.set("BM", Object::Name(b"Multiply".to_vec()));
                }
                let gs_id = doc.add_object(Object::Dictionary(gs_dict));
                let mut gs_res = lopdf::Dictionary::new();
                gs_res.set("GS0", Object::Reference(gs_id));
                resources.set("ExtGState", Object::Dictionary(gs_res));
            }

            if needs_font {
                let font_dict = dictionary! {
                    "Type" => "Font",
                    "Subtype" => "Type1",
                    "BaseFont" => "Helvetica",
                };
                let font_id = doc.add_object(Object::Dictionary(font_dict));
                let mut font_res = lopdf::Dictionary::new();
                font_res.set("Helv", Object::Reference(font_id));
                resources.set("Font", Object::Dictionary(font_res));
            }

            stream_dict.set("Resources", Object::Dictionary(resources));
        }

        let stream = Stream::new(stream_dict, content_bytes);
        Ok(doc.add_object(Object::Stream(stream)))
    }

    /// Generate a default appearance based on the annotation subtype.
    fn default_appearance(&self, builder: &mut AppearanceStreamBuilder, w: f64, h: f64) {
        let stroke = self.color.unwrap_or(AppearanceColor::new(0.0, 0.0, 0.0));
        let needs_gs = self.opacity.is_some() || matches!(self.subtype, AnnotSubtype::Highlight);

        if needs_gs {
            builder.save_state();
            builder.ops_push_raw(lopdf::content::Operation::new(
                "gs",
                vec![Object::Name(b"GS0".to_vec())],
            ));
        }

        match self.subtype {
            AnnotSubtype::Square => {
                if let Some(ref fill) = self.interior_color {
                    builder.filled_stroked_rect(fill, &stroke, self.border_width);
                } else {
                    builder.stroked_rect(&stroke, self.border_width);
                }
            }
            AnnotSubtype::Circle => {
                builder.save_state();
                if let Some(ref fill) = self.interior_color {
                    builder.set_fill_color(fill);
                }
                builder.set_stroke_color(&stroke);
                builder.set_line_width(self.border_width);
                builder.ellipse();
                if self.interior_color.is_some() {
                    builder.fill_and_stroke();
                } else {
                    builder.stroke();
                }
                builder.restore_state();
            }
            AnnotSubtype::Line => {
                builder.save_state();
                builder.set_stroke_color(&stroke);
                builder.set_line_width(self.border_width);
                if let Some(ref dash) = self.dash_pattern {
                    builder.set_dash_pattern(dash, 0.0);
                }
                // Convert page coordinates to local form XObject coordinates.
                if let Some(ref l) = self.line_endpoints {
                    let lx1 = l[0] - self.rect.x0;
                    let ly1 = l[1] - self.rect.y0;
                    let lx2 = l[2] - self.rect.x0;
                    let ly2 = l[3] - self.rect.y0;
                    builder.line(lx1, ly1, lx2, ly2);
                } else {
                    builder.line(0.0, h / 2.0, w, h / 2.0);
                }
                builder.stroke();
                builder.restore_state();
            }
            AnnotSubtype::Highlight => {
                let fill = self.color.unwrap_or(AppearanceColor::new(1.0, 1.0, 0.0));
                builder.filled_rect(&fill);
            }
            AnnotSubtype::Underline => {
                builder.save_state();
                builder.set_stroke_color(&stroke);
                builder.set_line_width(self.border_width.max(0.5));
                builder.line(0.0, 0.0, w, 0.0);
                builder.stroke();
                builder.restore_state();
            }
            AnnotSubtype::StrikeOut => {
                builder.save_state();
                builder.set_stroke_color(&stroke);
                builder.set_line_width(self.border_width.max(0.5));
                builder.line(0.0, h / 2.0, w, h / 2.0);
                builder.stroke();
                builder.restore_state();
            }
            AnnotSubtype::Squiggly => {
                // Simplified squiggly: zigzag line at bottom.
                builder.save_state();
                builder.set_stroke_color(&stroke);
                builder.set_line_width(self.border_width.max(0.5));
                let step = 4.0;
                let amp = 2.0;
                builder.move_to(0.0, amp);
                let mut x = 0.0;
                let mut up = false;
                while x < w {
                    x += step;
                    let y = if up { amp } else { 0.0 };
                    builder.line_to(x.min(w), y);
                    up = !up;
                }
                builder.stroke();
                builder.restore_state();
            }
            AnnotSubtype::Ink => {
                builder.save_state();
                builder.set_stroke_color(&stroke);
                builder.set_line_width(self.border_width);
                if let Some(ref ink) = self.ink_list {
                    for path in ink {
                        if path.len() >= 2 {
                            let x0 = path[0] - self.rect.x0;
                            let y0 = path[1] - self.rect.y0;
                            builder.move_to(x0, y0);
                            let mut i = 2;
                            while i + 1 < path.len() {
                                let x = path[i] - self.rect.x0;
                                let y = path[i + 1] - self.rect.y0;
                                builder.line_to(x, y);
                                i += 2;
                            }
                            builder.stroke();
                        }
                    }
                }
                builder.restore_state();
            }
            AnnotSubtype::Polygon | AnnotSubtype::PolyLine => {
                builder.save_state();
                if let Some(ref fill) = self.interior_color {
                    builder.set_fill_color(fill);
                }
                builder.set_stroke_color(&stroke);
                builder.set_line_width(self.border_width);
                if let Some(ref dash) = self.dash_pattern {
                    builder.set_dash_pattern(dash, 0.0);
                }
                if let Some(ref verts) = self.vertices {
                    if verts.len() >= 2 {
                        let x0 = verts[0] - self.rect.x0;
                        let y0 = verts[1] - self.rect.y0;
                        builder.move_to(x0, y0);
                        let mut i = 2;
                        while i + 1 < verts.len() {
                            let x = verts[i] - self.rect.x0;
                            let y = verts[i + 1] - self.rect.y0;
                            builder.line_to(x, y);
                            i += 2;
                        }
                    }
                }
                let is_polygon = matches!(self.subtype, AnnotSubtype::Polygon);
                if is_polygon {
                    builder.close_path();
                    if self.interior_color.is_some() {
                        builder.fill_and_stroke();
                    } else {
                        builder.stroke();
                    }
                } else {
                    builder.stroke();
                }
                builder.restore_state();
            }
            AnnotSubtype::FreeText => {
                // White background with border.
                let white = AppearanceColor::new(1.0, 1.0, 1.0);
                builder.filled_stroked_rect(&white, &stroke, self.border_width);
                // Text is rendered via /DA by the viewer; the appearance stream
                // provides the background rectangle.
                if let Some(ref text) = self.contents {
                    let text_color = self.color.unwrap_or(AppearanceColor::new(0.0, 0.0, 0.0));
                    let margin = self.border_width + 2.0;
                    builder.text(text, "Helv", 12.0, margin, h - margin - 12.0, &text_color);
                }
            }
            AnnotSubtype::Text => {
                // Sticky note icon — simplified as a filled square with a border.
                let fill = AppearanceColor::new(1.0, 1.0, 0.6); // Light yellow
                builder.filled_stroked_rect(&fill, &stroke, self.border_width);
            }
            AnnotSubtype::Stamp => {
                // Stamp: red border with text.
                let red = AppearanceColor::new(1.0, 0.0, 0.0);
                builder.stroked_rect(&red, 2.0);
                if let Some(ref name) = self.icon_name {
                    builder.text(name, "Helv", 18.0, 4.0, h / 2.0 - 9.0, &red);
                }
            }
            AnnotSubtype::Link => {
                // Links are typically invisible — no appearance needed.
                // Empty appearance stream (viewer draws the link area).
            }
        }

        if needs_gs {
            builder.restore_state();
        }
    }
}

#[cfg(feature = "write")]
enum AnnotsAction {
    SetArray(Vec<Object>),
    AppendIndirect(ObjectId),
}

/// Add an annotation reference to a page's /Annots array.
///
/// Handles both inline arrays and indirect (referenced) arrays.  When the
/// indirect Annots array cannot be resolved (e.g. it lives in a compressed
/// ObjStm that lopdf failed to expand), falls back to replacing the /Annots
/// entry with a new inline array so the annotation is never silently dropped.
#[cfg(feature = "write")]
pub fn add_annotation_to_page(
    doc: &mut Document,
    page_num: u32,
    annot_id: ObjectId,
) -> Result<(), AnnotBuildError> {
    let pages = doc.get_pages();
    let page_count = pages.len();
    let page_id = *pages
        .get(&page_num)
        .ok_or(AnnotBuildError::PageOutOfRange(page_num, page_count))?;

    // Read the current /Annots entry using get_dictionary (follows indirect
    // refs, handles compressed-stream pages).
    let annots_action = {
        match doc.get_dictionary(page_id) {
            Ok(page_dict) => match page_dict.get(b"Annots").ok() {
                Some(Object::Array(arr)) => {
                    let mut new_arr = arr.clone();
                    new_arr.push(Object::Reference(annot_id));
                    AnnotsAction::SetArray(new_arr)
                }
                Some(Object::Reference(r)) => AnnotsAction::AppendIndirect(*r),
                _ => AnnotsAction::SetArray(vec![Object::Reference(annot_id)]),
            },
            // Page dict not accessible: still attempt to set an inline Annots.
            Err(_) => AnnotsAction::SetArray(vec![Object::Reference(annot_id)]),
        }
    };

    match annots_action {
        AnnotsAction::SetArray(arr) => {
            if let Ok(page_dict) = doc.get_dictionary_mut(page_id) {
                page_dict.set("Annots", Object::Array(arr));
            }
        }
        AnnotsAction::AppendIndirect(annots_ref) => {
            // Attempt to mutate the indirect array in place.
            let appended = {
                if let Ok(Object::Array(ref mut arr)) = doc.get_object_mut(annots_ref) {
                    arr.push(Object::Reference(annot_id));
                    true
                } else {
                    false
                }
            };

            if !appended {
                // Fallback: indirect array not accessible (e.g. lives in a
                // compressed ObjStm that was skipped or not yet decompressed).
                // Replace /Annots with a new inline array.  Existing annots
                // referenced from the unresolvable array are already
                // inaccessible, so we only lose what was already broken.
                if let Ok(page_dict) = doc.get_dictionary_mut(page_id) {
                    page_dict.set("Annots", Object::Array(vec![Object::Reference(annot_id)]));
                }
            }
        }
    }

    Ok(())
}

#[cfg(all(test, feature = "write"))]
mod tests {
    use super::*;

    fn make_test_doc() -> Document {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();

        let content_data = b"BT /F1 12 Tf (Test) Tj ET".to_vec();
        let content_stream = Stream::new(dictionary! {}, content_data);
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

        let pages_dict = dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(1),
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
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
    fn build_square_annotation() {
        let mut doc = make_test_doc();
        let rect = AnnotRect::new(100.0, 200.0, 300.0, 400.0);
        let annot_id = AnnotationBuilder::new(AnnotSubtype::Square, rect)
            .color(1.0, 0.0, 0.0)
            .border_width(2.0)
            .contents("Red square")
            .build(&mut doc)
            .unwrap();

        let annot = doc.get_object(annot_id).unwrap();
        if let Object::Dictionary(d) = annot {
            assert_eq!(
                d.get(b"Subtype").unwrap(),
                &Object::Name(b"Square".to_vec())
            );
            assert!(d.get(b"AP").is_ok());
            assert!(d.get(b"C").is_ok());
        } else {
            panic!("Expected dictionary");
        }
    }

    #[test]
    fn build_circle_annotation() {
        let mut doc = make_test_doc();
        let rect = AnnotRect::new(50.0, 50.0, 150.0, 150.0);
        let annot_id = AnnotationBuilder::new(AnnotSubtype::Circle, rect)
            .color(0.0, 0.0, 1.0)
            .interior_color(0.8, 0.8, 1.0)
            .build(&mut doc)
            .unwrap();

        let annot = doc.get_object(annot_id).unwrap();
        if let Object::Dictionary(d) = annot {
            assert_eq!(
                d.get(b"Subtype").unwrap(),
                &Object::Name(b"Circle".to_vec())
            );
            assert!(d.get(b"IC").is_ok());
        } else {
            panic!("Expected dictionary");
        }
    }

    #[test]
    fn build_with_opacity() {
        let mut doc = make_test_doc();
        let rect = AnnotRect::new(0.0, 0.0, 100.0, 100.0);
        let annot_id = AnnotationBuilder::new(AnnotSubtype::Square, rect)
            .opacity(0.5)
            .build(&mut doc)
            .unwrap();

        let annot = doc.get_object(annot_id).unwrap();
        if let Object::Dictionary(d) = annot {
            let ca = d.get(b"CA").unwrap();
            assert_eq!(ca, &Object::Real(0.5));
        } else {
            panic!("Expected dictionary");
        }
    }

    #[test]
    fn reject_zero_area_rect() {
        let mut doc = make_test_doc();
        let rect = AnnotRect::new(100.0, 200.0, 100.0, 400.0); // zero width
        let result = AnnotationBuilder::new(AnnotSubtype::Square, rect).build(&mut doc);
        assert!(result.is_err());
    }

    #[test]
    fn add_annotation_to_page_creates_annots() {
        let mut doc = make_test_doc();
        let rect = AnnotRect::new(10.0, 10.0, 50.0, 50.0);
        let annot_id = AnnotationBuilder::new(AnnotSubtype::Square, rect)
            .build(&mut doc)
            .unwrap();

        add_annotation_to_page(&mut doc, 1, annot_id).unwrap();

        // Verify page now has /Annots.
        let pages = doc.get_pages();
        let page_id = pages[&1];
        if let Object::Dictionary(d) = doc.get_object(page_id).unwrap() {
            let annots = d.get(b"Annots").unwrap();
            if let Object::Array(arr) = annots {
                assert_eq!(arr.len(), 1);
                assert_eq!(arr[0], Object::Reference(annot_id));
            } else {
                panic!("Expected array");
            }
        }
    }

    #[test]
    fn add_annotation_appends_to_existing_annots() {
        let mut doc = make_test_doc();
        let rect1 = AnnotRect::new(10.0, 10.0, 50.0, 50.0);
        let rect2 = AnnotRect::new(60.0, 60.0, 100.0, 100.0);

        let id1 = AnnotationBuilder::new(AnnotSubtype::Square, rect1)
            .build(&mut doc)
            .unwrap();
        let id2 = AnnotationBuilder::new(AnnotSubtype::Circle, rect2)
            .build(&mut doc)
            .unwrap();

        add_annotation_to_page(&mut doc, 1, id1).unwrap();
        add_annotation_to_page(&mut doc, 1, id2).unwrap();

        let pages = doc.get_pages();
        let page_id = pages[&1];
        if let Object::Dictionary(d) = doc.get_object(page_id).unwrap() {
            if let Object::Array(arr) = d.get(b"Annots").unwrap() {
                assert_eq!(arr.len(), 2);
            } else {
                panic!("Expected array");
            }
        }
    }

    #[test]
    fn invalid_page_returns_error() {
        let mut doc = make_test_doc();
        let rect = AnnotRect::new(10.0, 10.0, 50.0, 50.0);
        let annot_id = AnnotationBuilder::new(AnnotSubtype::Square, rect)
            .build(&mut doc)
            .unwrap();

        let result = add_annotation_to_page(&mut doc, 99, annot_id);
        assert!(result.is_err());
    }

    #[test]
    fn highlight_annotation() {
        let mut doc = make_test_doc();
        let rect = AnnotRect::new(72.0, 700.0, 400.0, 712.0);
        let annot_id = AnnotationBuilder::new(AnnotSubtype::Highlight, rect)
            .color(1.0, 1.0, 0.0)
            .opacity(0.4)
            .build(&mut doc)
            .unwrap();

        let annot = doc.get_object(annot_id).unwrap();
        if let Object::Dictionary(d) = annot {
            assert_eq!(
                d.get(b"Subtype").unwrap(),
                &Object::Name(b"Highlight".to_vec())
            );
        } else {
            panic!("Expected dictionary");
        }
    }

    #[test]
    fn custom_appearance() {
        let mut doc = make_test_doc();
        let rect = AnnotRect::new(0.0, 0.0, 100.0, 100.0);
        let annot_id = AnnotationBuilder::new(AnnotSubtype::Square, rect)
            .appearance(|b| {
                let red = AppearanceColor::new(1.0, 0.0, 0.0);
                b.filled_rect(&red);
            })
            .build(&mut doc)
            .unwrap();

        assert!(doc.get_object(annot_id).is_ok());
    }

    // --- Issue #303 tests: Markup annotations ---

    #[test]
    fn highlight_with_quad_points() {
        let mut doc = make_test_doc();
        let rect = AnnotRect::new(72.0, 700.0, 400.0, 712.0);
        let annot_id = AnnotationBuilder::highlight(rect)
            .quad_points_from_rect(&rect)
            .build(&mut doc)
            .unwrap();

        let annot = doc.get_object(annot_id).unwrap();
        if let Object::Dictionary(d) = annot {
            assert_eq!(
                d.get(b"Subtype").unwrap(),
                &Object::Name(b"Highlight".to_vec())
            );
            // Check QuadPoints present.
            let qp = d.get(b"QuadPoints").unwrap();
            if let Object::Array(arr) = qp {
                assert_eq!(arr.len(), 8); // Single quad = 8 values.
            } else {
                panic!("Expected QuadPoints array");
            }
            // Check opacity is set.
            assert!(d.get(b"CA").is_ok());
        } else {
            panic!("Expected dictionary");
        }
    }

    #[test]
    fn highlight_has_multiply_blend() {
        let mut doc = make_test_doc();
        let rect = AnnotRect::new(72.0, 700.0, 400.0, 712.0);
        let annot_id = AnnotationBuilder::highlight(rect).build(&mut doc).unwrap();

        // Get the appearance stream and verify its resources contain BM /Multiply.
        let annot = doc.get_object(annot_id).unwrap();
        if let Object::Dictionary(d) = annot {
            let ap = d.get(b"AP").unwrap();
            if let Object::Dictionary(ap_dict) = ap {
                let n_ref = ap_dict.get(b"N").unwrap();
                if let Object::Reference(stream_id) = n_ref {
                    let stream = doc.get_object(*stream_id).unwrap();
                    if let Object::Stream(s) = stream {
                        let res = s.dict.get(b"Resources").unwrap();
                        if let Object::Dictionary(res_dict) = res {
                            let gs = res_dict.get(b"ExtGState").unwrap();
                            if let Object::Dictionary(gs_dict) = gs {
                                let gs0_ref = gs_dict.get(b"GS0").unwrap();
                                if let Object::Reference(gs0_id) = gs0_ref {
                                    let gs0 = doc.get_object(*gs0_id).unwrap();
                                    if let Object::Dictionary(gs0_dict) = gs0 {
                                        assert_eq!(
                                            gs0_dict.get(b"BM").unwrap(),
                                            &Object::Name(b"Multiply".to_vec())
                                        );
                                        return;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        panic!("Could not find BM /Multiply in ExtGState");
    }

    #[test]
    fn underline_convenience() {
        let mut doc = make_test_doc();
        let rect = AnnotRect::new(72.0, 700.0, 400.0, 712.0);
        let annot_id = AnnotationBuilder::underline(rect)
            .quad_points_from_rect(&rect)
            .build(&mut doc)
            .unwrap();

        let annot = doc.get_object(annot_id).unwrap();
        if let Object::Dictionary(d) = annot {
            assert_eq!(
                d.get(b"Subtype").unwrap(),
                &Object::Name(b"Underline".to_vec())
            );
            assert!(d.get(b"QuadPoints").is_ok());
        } else {
            panic!("Expected dictionary");
        }
    }

    #[test]
    fn strikeout_convenience() {
        let mut doc = make_test_doc();
        let rect = AnnotRect::new(72.0, 700.0, 400.0, 712.0);
        let annot_id = AnnotationBuilder::strikeout(rect).build(&mut doc).unwrap();

        let annot = doc.get_object(annot_id).unwrap();
        if let Object::Dictionary(d) = annot {
            assert_eq!(
                d.get(b"Subtype").unwrap(),
                &Object::Name(b"StrikeOut".to_vec())
            );
        } else {
            panic!("Expected dictionary");
        }
    }

    #[test]
    fn squiggly_convenience() {
        let mut doc = make_test_doc();
        let rect = AnnotRect::new(72.0, 700.0, 400.0, 712.0);
        let annot_id = AnnotationBuilder::squiggly(rect).build(&mut doc).unwrap();

        let annot = doc.get_object(annot_id).unwrap();
        if let Object::Dictionary(d) = annot {
            assert_eq!(
                d.get(b"Subtype").unwrap(),
                &Object::Name(b"Squiggly".to_vec())
            );
        } else {
            panic!("Expected dictionary");
        }
    }

    #[test]
    fn multi_quad_points() {
        let mut doc = make_test_doc();
        let rect = AnnotRect::new(72.0, 688.0, 400.0, 712.0);
        // Two quads for two lines of text.
        let qp = vec![
            72.0, 712.0, 400.0, 712.0, 72.0, 700.0, 400.0, 700.0, // line 1
            72.0, 700.0, 300.0, 700.0, 72.0, 688.0, 300.0, 688.0, // line 2
        ];
        let annot_id = AnnotationBuilder::highlight(rect)
            .quad_points(qp)
            .build(&mut doc)
            .unwrap();

        let annot = doc.get_object(annot_id).unwrap();
        if let Object::Dictionary(d) = annot {
            if let Object::Array(arr) = d.get(b"QuadPoints").unwrap() {
                assert_eq!(arr.len(), 16); // 2 quads × 8 values.
            } else {
                panic!("Expected QuadPoints array");
            }
        }
    }

    // --- Issue #304 tests: Geometric annotations ---

    #[test]
    fn square_convenience() {
        let mut doc = make_test_doc();
        let rect = AnnotRect::new(100.0, 100.0, 200.0, 200.0);
        let annot_id = AnnotationBuilder::square(rect)
            .color(0.0, 0.0, 1.0)
            .interior_color(0.9, 0.9, 1.0)
            .border_width(2.0)
            .build(&mut doc)
            .unwrap();

        let annot = doc.get_object(annot_id).unwrap();
        if let Object::Dictionary(d) = annot {
            assert_eq!(
                d.get(b"Subtype").unwrap(),
                &Object::Name(b"Square".to_vec())
            );
            assert!(d.get(b"IC").is_ok());
        } else {
            panic!("Expected dictionary");
        }
    }

    #[test]
    fn circle_convenience() {
        let mut doc = make_test_doc();
        let rect = AnnotRect::new(50.0, 50.0, 150.0, 150.0);
        let annot_id = AnnotationBuilder::circle(rect)
            .color(1.0, 0.0, 0.0)
            .build(&mut doc)
            .unwrap();

        let annot = doc.get_object(annot_id).unwrap();
        if let Object::Dictionary(d) = annot {
            assert_eq!(
                d.get(b"Subtype").unwrap(),
                &Object::Name(b"Circle".to_vec())
            );
        } else {
            panic!("Expected dictionary");
        }
    }

    #[test]
    fn line_annotation_with_endpoints() {
        let mut doc = make_test_doc();
        let annot_id = AnnotationBuilder::line(100.0, 200.0, 400.0, 600.0)
            .color(1.0, 0.0, 0.0)
            .border_width(2.0)
            .build(&mut doc)
            .unwrap();

        let annot = doc.get_object(annot_id).unwrap();
        if let Object::Dictionary(d) = annot {
            assert_eq!(d.get(b"Subtype").unwrap(), &Object::Name(b"Line".to_vec()));
            // Check /L array present.
            let l = d.get(b"L").unwrap();
            if let Object::Array(arr) = l {
                assert_eq!(arr.len(), 4);
            } else {
                panic!("Expected /L array");
            }
        } else {
            panic!("Expected dictionary");
        }
    }

    #[test]
    fn line_with_endings() {
        let mut doc = make_test_doc();
        let annot_id = AnnotationBuilder::line(100.0, 300.0, 500.0, 300.0)
            .line_endings(LineEnding::ClosedArrow, LineEnding::OpenArrow)
            .build(&mut doc)
            .unwrap();

        let annot = doc.get_object(annot_id).unwrap();
        if let Object::Dictionary(d) = annot {
            let le = d.get(b"LE").unwrap();
            if let Object::Array(arr) = le {
                assert_eq!(arr.len(), 2);
                assert_eq!(arr[0], Object::Name(b"ClosedArrow".to_vec()));
                assert_eq!(arr[1], Object::Name(b"OpenArrow".to_vec()));
            } else {
                panic!("Expected /LE array");
            }
        } else {
            panic!("Expected dictionary");
        }
    }

    #[test]
    fn ink_annotation() {
        let mut doc = make_test_doc();
        let rect = AnnotRect::new(50.0, 50.0, 200.0, 200.0);
        let strokes = vec![
            vec![60.0, 60.0, 100.0, 150.0, 180.0, 80.0],
            vec![70.0, 70.0, 120.0, 160.0],
        ];
        let annot_id = AnnotationBuilder::ink(rect, strokes)
            .color(0.0, 0.5, 0.0)
            .border_width(3.0)
            .build(&mut doc)
            .unwrap();

        let annot = doc.get_object(annot_id).unwrap();
        if let Object::Dictionary(d) = annot {
            assert_eq!(d.get(b"Subtype").unwrap(), &Object::Name(b"Ink".to_vec()));
            let ink = d.get(b"InkList").unwrap();
            if let Object::Array(arr) = ink {
                assert_eq!(arr.len(), 2); // Two strokes.
            } else {
                panic!("Expected InkList array");
            }
        } else {
            panic!("Expected dictionary");
        }
    }

    #[test]
    fn polygon_annotation() {
        let mut doc = make_test_doc();
        let rect = AnnotRect::new(100.0, 100.0, 300.0, 300.0);
        let verts = vec![100.0, 100.0, 300.0, 100.0, 200.0, 300.0];
        let annot_id = AnnotationBuilder::polygon(rect, verts)
            .color(0.0, 0.0, 1.0)
            .interior_color(0.8, 0.8, 1.0)
            .build(&mut doc)
            .unwrap();

        let annot = doc.get_object(annot_id).unwrap();
        if let Object::Dictionary(d) = annot {
            assert_eq!(
                d.get(b"Subtype").unwrap(),
                &Object::Name(b"Polygon".to_vec())
            );
            let v = d.get(b"Vertices").unwrap();
            if let Object::Array(arr) = v {
                assert_eq!(arr.len(), 6);
            } else {
                panic!("Expected Vertices array");
            }
        } else {
            panic!("Expected dictionary");
        }
    }

    #[test]
    fn polyline_annotation() {
        let mut doc = make_test_doc();
        let rect = AnnotRect::new(50.0, 50.0, 400.0, 200.0);
        let verts = vec![50.0, 100.0, 200.0, 180.0, 350.0, 60.0, 400.0, 150.0];
        let annot_id = AnnotationBuilder::polyline(rect, verts)
            .color(1.0, 0.5, 0.0)
            .build(&mut doc)
            .unwrap();

        let annot = doc.get_object(annot_id).unwrap();
        if let Object::Dictionary(d) = annot {
            assert_eq!(
                d.get(b"Subtype").unwrap(),
                &Object::Name(b"PolyLine".to_vec())
            );
        } else {
            panic!("Expected dictionary");
        }
    }

    #[test]
    fn dashed_line_annotation() {
        let mut doc = make_test_doc();
        let annot_id = AnnotationBuilder::line(72.0, 400.0, 540.0, 400.0)
            .dash(vec![3.0, 2.0])
            .build(&mut doc)
            .unwrap();

        let annot = doc.get_object(annot_id).unwrap();
        if let Object::Dictionary(d) = annot {
            let bs = d.get(b"BS").unwrap();
            if let Object::Dictionary(bs_dict) = bs {
                assert_eq!(bs_dict.get(b"S").unwrap(), &Object::Name(b"D".to_vec()));
                assert!(bs_dict.get(b"D").is_ok());
            } else {
                panic!("Expected BS dictionary");
            }
        } else {
            panic!("Expected dictionary");
        }
    }

    // --- Issue #305 tests: Text annotations ---

    #[test]
    fn free_text_annotation() {
        let mut doc = make_test_doc();
        let rect = AnnotRect::new(72.0, 700.0, 300.0, 730.0);
        let annot_id = AnnotationBuilder::free_text(rect, "Hello World", 14.0)
            .alignment(1) // Center
            .build(&mut doc)
            .unwrap();

        let annot = doc.get_object(annot_id).unwrap();
        if let Object::Dictionary(d) = annot {
            assert_eq!(
                d.get(b"Subtype").unwrap(),
                &Object::Name(b"FreeText".to_vec())
            );
            assert!(d.get(b"DA").is_ok());
            assert_eq!(d.get(b"Q").unwrap(), &Object::Integer(1));
        } else {
            panic!("Expected dictionary");
        }
    }

    #[test]
    fn sticky_note_annotation() {
        let mut doc = make_test_doc();
        let rect = AnnotRect::new(500.0, 700.0, 524.0, 724.0);
        let annot_id = AnnotationBuilder::sticky_note(rect, TextIcon::Comment)
            .contents("This is a comment")
            .color(1.0, 1.0, 0.0)
            .build(&mut doc)
            .unwrap();

        let annot = doc.get_object(annot_id).unwrap();
        if let Object::Dictionary(d) = annot {
            assert_eq!(d.get(b"Subtype").unwrap(), &Object::Name(b"Text".to_vec()));
            assert_eq!(d.get(b"Name").unwrap(), &Object::Name(b"Comment".to_vec()));
            assert!(d.get(b"Contents").is_ok());
        } else {
            panic!("Expected dictionary");
        }
    }

    #[test]
    fn stamp_annotation() {
        let mut doc = make_test_doc();
        let rect = AnnotRect::new(72.0, 600.0, 250.0, 650.0);
        let annot_id = AnnotationBuilder::stamp(rect, StampName::Approved)
            .build(&mut doc)
            .unwrap();

        let annot = doc.get_object(annot_id).unwrap();
        if let Object::Dictionary(d) = annot {
            assert_eq!(d.get(b"Subtype").unwrap(), &Object::Name(b"Stamp".to_vec()));
            assert_eq!(d.get(b"Name").unwrap(), &Object::Name(b"Approved".to_vec()));
        } else {
            panic!("Expected dictionary");
        }
    }

    #[test]
    fn stamp_custom_name() {
        let mut doc = make_test_doc();
        let rect = AnnotRect::new(72.0, 500.0, 250.0, 550.0);
        let annot_id = AnnotationBuilder::stamp_custom(rect, "ReviewNeeded")
            .build(&mut doc)
            .unwrap();

        let annot = doc.get_object(annot_id).unwrap();
        if let Object::Dictionary(d) = annot {
            assert_eq!(
                d.get(b"Name").unwrap(),
                &Object::Name(b"ReviewNeeded".to_vec())
            );
        } else {
            panic!("Expected dictionary");
        }
    }

    #[test]
    fn link_uri_annotation() {
        let mut doc = make_test_doc();
        let rect = AnnotRect::new(72.0, 700.0, 200.0, 712.0);
        let annot_id = AnnotationBuilder::link_uri(rect, "https://example.com")
            .color(0.0, 0.0, 1.0)
            .build(&mut doc)
            .unwrap();

        let annot = doc.get_object(annot_id).unwrap();
        if let Object::Dictionary(d) = annot {
            assert_eq!(d.get(b"Subtype").unwrap(), &Object::Name(b"Link".to_vec()));
            let action = d.get(b"A").unwrap();
            if let Object::Dictionary(a) = action {
                assert_eq!(a.get(b"S").unwrap(), &Object::Name(b"URI".to_vec()));
            } else {
                panic!("Expected action dictionary");
            }
        } else {
            panic!("Expected dictionary");
        }
    }

    #[test]
    fn link_destination_annotation() {
        let mut doc = make_test_doc();
        let rect = AnnotRect::new(72.0, 650.0, 200.0, 662.0);
        let annot_id = AnnotationBuilder::link_dest(rect, "chapter1")
            .build(&mut doc)
            .unwrap();

        let annot = doc.get_object(annot_id).unwrap();
        if let Object::Dictionary(d) = annot {
            assert!(d.get(b"Dest").is_ok());
        } else {
            panic!("Expected dictionary");
        }
    }
}
