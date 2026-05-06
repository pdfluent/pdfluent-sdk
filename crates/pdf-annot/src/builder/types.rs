//! Value types used by the annotation builder: rectangles, subtypes, icons,
//! and line endings.

#[cfg(feature = "write")]
use lopdf::Object;

/// PDF annotation rectangle in page user-space coordinates.
///
/// Coordinates are PDF user units (1/72 inch) with the origin at the
/// bottom-left of the page. The rectangle is the annotation's clickable /
/// visible area on the page; for markup annotations it is also used as
/// the bounding box of the marked text region.
///
/// `(x0, y0)` is one corner and `(x1, y1)` is the opposite corner; the
/// constructor does not require a particular ordering, but a zero-area
/// rectangle (width == 0 or height == 0) is rejected by
/// [`super::AnnotationBuilder::build`].
#[cfg(feature = "write")]
#[derive(Debug, Clone, Copy)]
pub struct AnnotRect {
    /// X coordinate of the first corner, in PDF user-space points.
    pub x0: f64,
    /// Y coordinate of the first corner, in PDF user-space points.
    pub y0: f64,
    /// X coordinate of the opposite corner, in PDF user-space points.
    pub x1: f64,
    /// Y coordinate of the opposite corner, in PDF user-space points.
    pub y1: f64,
}

#[cfg(feature = "write")]
impl AnnotRect {
    /// Construct a rectangle from two opposite corners. No ordering is
    /// required between `(x0, y0)` and `(x1, y1)` — width and height are
    /// computed as absolute differences.
    pub fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self { x0, y0, x1, y1 }
    }

    /// Width in PDF user-space points (always non-negative).
    pub fn width(&self) -> f64 {
        (self.x1 - self.x0).abs()
    }

    /// Height in PDF user-space points (always non-negative).
    pub fn height(&self) -> f64 {
        (self.y1 - self.y0).abs()
    }

    pub(super) fn as_array(&self) -> Object {
        Object::Array(vec![
            Object::Real(self.x0 as f32),
            Object::Real(self.y0 as f32),
            Object::Real(self.x1 as f32),
            Object::Real(self.y1 as f32),
        ])
    }
}

/// PDF annotation subtype as defined by ISO 32000-2 §12.5.6.
///
/// Each variant maps directly to the `/Subtype` name written into the
/// annotation dictionary. Use the typed constructors on
/// [`super::AnnotationBuilder`] (`square`, `highlight`, `link_uri`, …) instead
/// of constructing this enum directly when possible — they wire up the
/// per-subtype defaults that are required for a valid annotation.
#[cfg(feature = "write")]
#[derive(Debug, Clone, Copy)]
pub enum AnnotSubtype {
    /// Filled or stroked rectangle (`/Square`). Used for emphasis or
    /// region marking. Pair with [`super::AnnotationBuilder::interior_color`]
    /// for a fill.
    Square,
    /// Filled or stroked ellipse (`/Circle`) inscribed in the
    /// rectangle. Pair with [`super::AnnotationBuilder::interior_color`] for a
    /// fill.
    Circle,
    /// Line segment (`/Line`) between two points, optionally with arrow
    /// or other end caps. See [`super::AnnotationBuilder::line`] and
    /// [`super::AnnotationBuilder::line_endings`].
    Line,
    /// Text-markup highlight (`/Highlight`) — semi-transparent yellow
    /// fill over a text range. Use [`super::AnnotationBuilder::quad_points`]
    /// to mark the actual text quadrilaterals.
    Highlight,
    /// Text-markup underline (`/Underline`) drawn beneath a text range.
    Underline,
    /// Text-markup strike-through (`/StrikeOut`) drawn through a text
    /// range.
    StrikeOut,
    /// Text-markup squiggly underline (`/Squiggly`) — typically used to
    /// flag spelling or grammar concerns.
    Squiggly,
    /// Free-text annotation (`/FreeText`) — text rendered directly on
    /// the page (vs. a popup note). See
    /// [`super::AnnotationBuilder::free_text`].
    FreeText,
    /// Sticky-note annotation (`/Text`). Despite the name, this is the
    /// "popup comment" style — a small icon on the page that opens a
    /// text bubble in the viewer. See
    /// [`super::AnnotationBuilder::sticky_note`].
    Text,
    /// Rubber stamp annotation (`/Stamp`) — APPROVED, DRAFT,
    /// CONFIDENTIAL, etc. See [`crate::StampName`] for the standard set and
    /// [`super::AnnotationBuilder::stamp`] / [`super::AnnotationBuilder::stamp_custom`].
    Stamp,
    /// Free-hand ink annotation (`/Ink`) consisting of one or more
    /// stroke paths. See [`super::AnnotationBuilder::ink`].
    Ink,
    /// Closed polygon (`/Polygon`) defined by a list of vertices.
    /// Optionally filled with [`super::AnnotationBuilder::interior_color`].
    Polygon,
    /// Open polyline (`/PolyLine`) defined by a list of vertices.
    /// Stroked but not filled.
    PolyLine,
    /// Hyperlink annotation (`/Link`) — invisible by default, the
    /// viewer renders it as a clickable region. See
    /// [`super::AnnotationBuilder::link_uri`] and
    /// [`super::AnnotationBuilder::link_dest`].
    Link,
}

#[cfg(feature = "write")]
impl AnnotSubtype {
    pub(super) fn as_str(&self) -> &'static str {
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

/// Standard icon names for `/Text` (sticky-note) annotations per
/// ISO 32000-2 §12.5.6.4 Table 180.
///
/// The icon is the small graphic that the viewer renders on the page;
/// clicking it opens the popup note containing the annotation's
/// `/Contents` text.
#[cfg(feature = "write")]
#[derive(Debug, Clone, Copy)]
pub enum TextIcon {
    /// Speech-bubble icon — the most common "this is a comment" icon.
    Comment,
    /// Key icon — typically used to flag access-control or
    /// authentication notes.
    Key,
    /// Generic note-paper icon.
    Note,
    /// Question-mark / help icon for clarification requests.
    Help,
    /// New-paragraph icon (¶ with a plus) for editorial markup.
    NewParagraph,
    /// Paragraph icon (¶) for editorial markup.
    Paragraph,
    /// Caret/insert icon for indicating insertion points in editorial
    /// review.
    Insert,
}

#[cfg(feature = "write")]
impl TextIcon {
    pub(super) fn as_str(&self) -> &'static str {
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

/// Line-ending style for `/Line`, `/PolyLine`, and free-text callout
/// annotations per ISO 32000-2 §12.5.6.7 Table 179.
///
/// Pass two values to [`super::AnnotationBuilder::line_endings`] — the first
/// applies to the start point, the second to the end point.
#[cfg(feature = "write")]
#[derive(Debug, Clone, Copy)]
pub enum LineEnding {
    /// `/None` — no end cap; the line terminates flush.
    None,
    /// `/Square` — solid square end cap.
    Square,
    /// `/Circle` — solid circle end cap.
    Circle,
    /// `/Diamond` — diamond / rhombus end cap.
    Diamond,
    /// `/OpenArrow` — open V-shaped arrowhead pointing away from the line.
    OpenArrow,
    /// `/ClosedArrow` — filled triangular arrowhead pointing away from the line.
    ClosedArrow,
    /// `/Butt` — short perpendicular tick at the end of the line.
    Butt,
    /// `/ROpenArrow` — reversed open arrow (points back along the line).
    ROpenArrow,
    /// `/RClosedArrow` — reversed filled arrow (points back along the line).
    RClosedArrow,
    /// `/Slash` — short diagonal slash at the end of the line.
    Slash,
}

#[cfg(feature = "write")]
impl LineEnding {
    pub(super) fn as_str(&self) -> &'static str {
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
