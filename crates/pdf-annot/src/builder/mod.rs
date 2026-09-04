//! Annotation builder for creating PDF annotations via lopdf.
//!
//! Provides `AnnotationBuilder` for constructing annotation dictionaries
//! with appearance streams and adding them to a PDF document.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

mod appearance;
mod page_attach;
pub mod types;

#[cfg(feature = "write")]
use lopdf::{dictionary, Document, Object, ObjectId, Stream};

#[cfg(feature = "write")]
use crate::appearance_writer::{AppearanceColor, AppearanceStreamBuilder};
#[cfg(feature = "write")]
use crate::error::AnnotBuildError;

// Re-export the unified StampName from the stamp module so that
// `pdf_annot::builder::StampName` keeps working for existing call sites.
#[cfg(feature = "write")]
pub use crate::StampName;

#[cfg(feature = "write")]
pub use page_attach::add_annotation_to_page;

#[cfg(feature = "write")]
pub use types::*;

/// Type alias for a custom appearance builder closure.
#[cfg(feature = "write")]
type AppearanceFn = Box<dyn FnOnce(&mut AppearanceStreamBuilder)>;

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
        b.icon_name = Some(name.to_pdf_name().to_string());
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
                    .expect("guarded by has_dash which checks is_some()")
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
}

#[cfg(all(test, feature = "write"))]
mod tests;
