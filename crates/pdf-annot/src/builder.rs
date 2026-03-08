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
            custom_appearance: None,
        }
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

        // Border style.
        if (self.border_width - 1.0).abs() > f64::EPSILON {
            let bs = dictionary! {
                "W" => Object::Real(self.border_width as f32),
                "S" => "S",
            };
            annot_dict.set("BS", Object::Dictionary(bs));
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

        // If opacity is used, add ExtGState resource.
        if let Some(alpha) = self.opacity {
            let gs_dict = dictionary! {
                "Type" => "ExtGState",
                "ca" => Object::Real(alpha as f32),
                "CA" => Object::Real(alpha as f32),
            };
            let gs_id = doc.add_object(Object::Dictionary(gs_dict));

            let mut gs_res = lopdf::Dictionary::new();
            gs_res.set("GS0", Object::Reference(gs_id));
            let mut resources = lopdf::Dictionary::new();
            resources.set("ExtGState", Object::Dictionary(gs_res));
            stream_dict.set("Resources", Object::Dictionary(resources));
        }

        let stream = Stream::new(stream_dict, content_bytes);
        Ok(doc.add_object(Object::Stream(stream)))
    }

    /// Generate a default appearance based on the annotation subtype.
    fn default_appearance(&self, builder: &mut AppearanceStreamBuilder, w: f64, h: f64) {
        let stroke = self.color.unwrap_or(AppearanceColor::new(0.0, 0.0, 0.0));

        if self.opacity.is_some() {
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
                builder.line(0.0, h / 2.0, w, h / 2.0);
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
            _ => {
                // Generic: stroked rect fallback.
                builder.stroked_rect(&stroke, self.border_width);
            }
        }

        if self.opacity.is_some() {
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
/// Handles both inline arrays and indirect (referenced) arrays.
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

    // First, check what kind of /Annots the page has.
    let annots_action = {
        if let Some(Object::Dictionary(page_dict)) = doc.objects.get(&page_id) {
            match page_dict.get(b"Annots").ok() {
                Some(Object::Array(arr)) => {
                    let mut new_arr = arr.clone();
                    new_arr.push(Object::Reference(annot_id));
                    AnnotsAction::SetArray(new_arr)
                }
                Some(Object::Reference(r)) => AnnotsAction::AppendIndirect(*r),
                _ => AnnotsAction::SetArray(vec![Object::Reference(annot_id)]),
            }
        } else {
            return Ok(());
        }
    };

    match annots_action {
        AnnotsAction::SetArray(arr) => {
            if let Some(Object::Dictionary(ref mut page_dict)) = doc.objects.get_mut(&page_id) {
                page_dict.set("Annots", Object::Array(arr));
            }
        }
        AnnotsAction::AppendIndirect(annots_ref) => {
            if let Ok(Object::Array(ref mut arr)) = doc.get_object_mut(annots_ref) {
                arr.push(Object::Reference(annot_id));
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
}
