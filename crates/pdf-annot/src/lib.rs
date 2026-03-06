//! PDF annotation engine.
//!
//! Handles parsing, rendering, and creation of PDF annotations
//! (markup, stamps, links, widgets, etc.) per ISO 32000-2 §12.5.

/// The type of a PDF annotation (ISO 32000-2 Table 170).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationType {
    Text,
    Link,
    FreeText,
    Line,
    Square,
    Circle,
    Polygon,
    PolyLine,
    Highlight,
    Underline,
    Squiggly,
    StrikeOut,
    Stamp,
    Caret,
    Ink,
    Popup,
    FileAttachment,
    Sound,
    Widget,
    Watermark,
    Redact,
}

/// A single PDF annotation.
#[derive(Debug, Clone)]
pub struct Annotation {
    /// Annotation subtype.
    pub annotation_type: AnnotationType,
    /// Bounding rectangle [x1, y1, x2, y2] in page coordinates.
    pub rect: [f32; 4],
    /// Text contents / alt text.
    pub contents: Option<String>,
}

/// Opaque page reference for annotation queries.
#[derive(Debug, Clone)]
pub struct Page {
    /// Zero-based page index.
    pub index: usize,
}

/// A rendering device that annotation renderers can draw to.
pub trait Device {
    /// Draw raw appearance stream data at the given rectangle.
    fn draw_appearance(&mut self, rect: [f32; 4], data: &[u8]);
}

/// Core trait for annotation engines.
///
/// Implementors provide annotation enumeration, rendering, and creation.
pub trait AnnotationRenderer {
    /// Returns all annotations on a given page.
    fn annotations(&self, page: &Page) -> Vec<Annotation>;

    /// Renders a single annotation onto a device.
    fn render_annotation(&self, annot: &Annotation, device: &mut dyn Device);

    /// Creates a new annotation on a page.
    fn create_annotation(&mut self, page: &Page, annot: Annotation);
}
