//! Core types for XFA layout — Box Model, measurements, and layout primitives.
//!
//! Implements XFA 3.3 §4 (Box Model) types.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/// Shared default horizontal text padding, applied per side when paragraph
/// margins are not explicitly set.
pub const DEFAULT_TEXT_PADDING: f64 = 0.0;

/// A 2D point in layout coordinates (points, 1pt = 1/72 inch).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    /// X coordinate.
    pub x: f64,
    /// Y coordinate.
    pub y: f64,
}

/// A 2D size in points.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

/// An axis-aligned rectangle in layout space.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    /// X coordinate.
    pub x: f64,
    /// Y coordinate.
    pub y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

impl Rect {
    /// Create a new rectangle.
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Right edge.
    pub fn right(&self) -> f64 {
        self.x + self.width
    }

    /// Bottom edge.
    pub fn bottom(&self) -> f64 {
        self.y + self.height
    }

    /// Check whether a point (px, py) lies inside this rectangle.
    pub fn contains(&self, px: f64, py: f64) -> bool {
        px >= self.x && px <= self.right() && py >= self.y && py <= self.bottom()
    }
}

/// Inset values (margins, padding) for the four sides.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Insets {
    /// Top inset.
    pub top: f64,
    /// Right inset.
    pub right: f64,
    /// Bottom inset.
    pub bottom: f64,
    /// Left inset.
    pub left: f64,
}

impl Insets {
    /// Create uniform insets.
    pub fn uniform(value: f64) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    /// Horizontal insets sum.
    pub fn horizontal(&self) -> f64 {
        self.left + self.right
    }

    /// Vertical insets sum.
    pub fn vertical(&self) -> f64 {
        self.top + self.bottom
    }
}

/// A measurement with a unit, parsed from XFA attributes.
///
/// XFA Spec 3.3 §2.2 (p36-38) — Measurements:
///   Absolute: in (inches, default), cm, mm, pt (1/72 inch).
///   Relative (XFA 2.8+): em (em width in current font), % (percentage of space width).
///   Note: bare numbers default to inches for dimensions but points for font sizes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measurement {
    /// Measurement value.
    pub value: f64,
    /// Measurement unit.
    pub unit: MeasurementUnit,
}

impl Measurement {
    /// Convert this measurement to points (the internal unit).
    ///
    /// Note: `Em` and `Percent` are relative units that depend on the current
    /// font context. Here we use a default 12pt font for em and approximate
    /// percentage as a fraction of the default space width (~3pt at 12pt).
    pub fn to_points(&self) -> f64 {
        match self.unit {
            MeasurementUnit::Points => self.value,
            MeasurementUnit::Inches => self.value * 72.0,
            MeasurementUnit::Centimeters => self.value * 72.0 / 2.54,
            MeasurementUnit::Millimeters => self.value * 72.0 / 25.4,
            MeasurementUnit::Em => self.value * 12.0, // default 12pt font
            // XFA §2.2: % = percentage of space (U+0020) width in current font.
            // Approximate: space width ≈ 25% of em → 3pt at 12pt default.
            MeasurementUnit::Percent => self.value / 100.0 * 3.0,
        }
    }

    /// Parse a measurement string like "10mm", "1in", "72pt", "2.5cm".
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        // Find where the numeric part ends
        let num_end = s
            .find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
            .unwrap_or(s.len());
        let value: f64 = s[..num_end].parse().ok()?;
        let unit_str = s[num_end..].trim();
        let unit = match unit_str {
            "" | "in" => MeasurementUnit::Inches,
            "pt" => MeasurementUnit::Points,
            "cm" => MeasurementUnit::Centimeters,
            "mm" => MeasurementUnit::Millimeters,
            "em" => MeasurementUnit::Em,
            "%" => MeasurementUnit::Percent,
            _ => return None,
        };
        Some(Measurement { value, unit })
    }
}

impl Default for Measurement {
    fn default() -> Self {
        Self {
            value: 0.0,
            unit: MeasurementUnit::Points,
        }
    }
}

/// Units for measurements in XFA.
///
/// XFA Spec 3.3 §2.2 (p37) — Absolute: in, cm, mm, pt.
/// Relative (XFA 2.8+): em, % (percentage of space width in current font).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasurementUnit {
    /// Inches.
    Inches,
    /// Centimeters.
    Centimeters,
    /// Millimeters.
    Millimeters,
    /// Points.
    Points,
    /// Em units.
    Em,
    /// Percentage of the width of a space (U+0020) in the current font.
    Percent,
}

/// Horizontal text alignment (XFA `<para hAlign>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    /// Left-aligned (default).
    #[default]
    Left,
    /// Centered.
    Center,
    /// Right-aligned.
    Right,
    /// Justified (treated as left for simple text rendering).
    Justify,
}

/// Layout strategy for a container.
///
/// XFA Spec 3.3 §2.6 (p43) — Two layout strategies:
///   Positioned: objects at fixed x,y coordinates (default for most containers).
///   Flowing: objects placed sequentially — tb, lr-tb, rl-tb, table, row.
///   pageArea always uses positioned layout only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutStrategy {
    /// Fixed x,y coordinates (default for subforms).
    #[default]
    Positioned,
    /// Top-to-bottom flow (layout="tb").
    TopToBottom,
    /// Left-to-right, top-to-bottom wrapping (layout="lr-tb").
    LeftToRightTB,
    /// Right-to-left, top-to-bottom wrapping (layout="rl-tb").
    RightToLeftTB,
    /// Table layout (layout="table").
    Table,
    /// Row within a table (layout="row").
    Row,
}

/// Vertical text alignment (XFA `<para vAlign>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerticalAlign {
    #[default]
    /// Top alignment.
    Top,
    /// Middle alignment.
    Middle,
    /// Bottom alignment.
    Bottom,
}

/// Caption placement relative to content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaptionPlacement {
    #[default]
    /// Left placement.
    Left,
    /// Top placement.
    Top,
    /// Right placement.
    Right,
    /// Bottom placement.
    Bottom,
    /// Inline placement.
    Inline,
}

/// The XFA Box Model for a form element.
///
/// XFA Spec 3.3 §2.6 (p49-50) — Nominal extent is w × h.
/// Inside: margins → border inset → caption region → content region.
/// The Nominal Content Region is the area after margins are applied.
///
/// §8 Growability (p275-276): a container is growable if it omits h and/or w:
/// - h=✓ w=✓ → fixed, not growable (minH/maxH/minW/maxW ignored)
/// - h=✓ w=∅ → growable along X only (minH/maxH ignored)
/// - h=∅ w=✓ → growable along Y only (minW/maxW ignored)
/// - h=∅ w=∅ → growable along both axes
///   Default: minH=0, minW=0, maxH=infinity, maxW=infinity.
///
/// See spec figure "Relationship between nominal extent and borders,
/// margins, captions, and content" (p50).
///
/// TODO(§2.6): border inset not modeled separately — currently merged with margins.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BoxModel {
    /// Nominal width (None = growable).
    pub width: Option<f64>,
    /// Nominal height (None = growable).
    pub height: Option<f64>,
    /// Explicit x position (for positioned layout).
    pub x: f64,
    /// Explicit y position (for positioned layout).
    pub y: f64,
    /// Margins.
    pub margins: Insets,
    /// Border thickness (simplified to uniform for now).
    pub border_width: f64,
    /// Minimum width constraint.
    pub min_width: f64,
    /// Maximum width constraint.
    pub max_width: f64,
    /// Minimum height constraint.
    pub min_height: f64,
    /// Maximum height constraint.
    pub max_height: f64,
    /// Caption region.
    pub caption: Option<Caption>,
}

/// A caption for a form field.
#[derive(Debug, Clone, PartialEq)]
pub struct Caption {
    /// Caption placement.
    pub placement: CaptionPlacement,
    /// Reserved space for the caption (None = auto).
    pub reserve: Option<f64>,
    /// Caption text.
    pub text: String,
    /// Caption's own typeface (XFA `<caption><font typeface="...">`). The caption
    /// styles its label text independently of the field's `<font>`; when present
    /// it must win, otherwise an Arial caption on a Times field renders serif.
    pub font_family: Option<String>,
    /// Caption's own font size in points, if specified on its `<font>`.
    pub font_size: Option<f64>,
}

impl BoxModel {
    /// The available content width after subtracting margins, borders, and caption.
    pub fn content_width(&self) -> f64 {
        let total = self.width.unwrap_or(self.max_width);
        let mut available = total - self.margins.horizontal() - self.border_width * 2.0;
        if let Some(ref cap) = self.caption {
            if matches!(
                cap.placement,
                CaptionPlacement::Left | CaptionPlacement::Right
            ) {
                available -= cap.reserve.unwrap_or(0.0);
            }
        }
        available.max(0.0)
    }

    /// The available content height after subtracting margins, borders, and caption.
    pub fn content_height(&self) -> f64 {
        let total = self.height.unwrap_or(self.max_height);
        let mut available = total - self.margins.vertical() - self.border_width * 2.0;
        if let Some(ref cap) = self.caption {
            if matches!(
                cap.placement,
                CaptionPlacement::Top | CaptionPlacement::Bottom
            ) {
                available -= cap.reserve.unwrap_or(0.0);
            }
        }
        available.max(0.0)
    }

    /// The outer extent (total bounding box).
    pub fn outer_size(&self, content: Size) -> Size {
        let mut w = content.width + self.margins.horizontal() + self.border_width * 2.0;
        let mut h = content.height + self.margins.vertical() + self.border_width * 2.0;
        if let Some(ref cap) = self.caption {
            match cap.placement {
                CaptionPlacement::Left | CaptionPlacement::Right => {
                    w += cap.reserve.unwrap_or(0.0);
                }
                CaptionPlacement::Top | CaptionPlacement::Bottom => {
                    h += cap.reserve.unwrap_or(0.0);
                }
                CaptionPlacement::Inline => {}
            }
        }
        // Apply min/max constraints
        if let Some(fixed_w) = self.width {
            w = fixed_w;
        } else {
            w = w.clamp(self.min_width, self.max_width);
        }
        if let Some(fixed_h) = self.height {
            h = fixed_h;
        } else {
            h = h.clamp(self.min_height, self.max_height);
        }
        Size {
            width: w,
            height: h,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measurement_parse() {
        let m = Measurement::parse("10mm").unwrap();
        assert_eq!(m.unit, MeasurementUnit::Millimeters);
        assert!((m.to_points() - 28.3464).abs() < 0.01);

        let m = Measurement::parse("72pt").unwrap();
        assert_eq!(m.to_points(), 72.0);

        let m = Measurement::parse("1in").unwrap();
        assert_eq!(m.to_points(), 72.0);

        let m = Measurement::parse("2.54cm").unwrap();
        assert!((m.to_points() - 72.0).abs() < 0.01);
    }

    #[test]
    fn box_model_content_area() {
        let bm = BoxModel {
            width: Some(200.0),
            height: Some(100.0),
            margins: Insets {
                top: 5.0,
                right: 10.0,
                bottom: 5.0,
                left: 10.0,
            },
            border_width: 1.0,
            max_width: f64::MAX,
            max_height: f64::MAX,
            ..Default::default()
        };
        // content_width = 200 - 20 (margins) - 2 (border) = 178
        assert_eq!(bm.content_width(), 178.0);
        // content_height = 100 - 10 (margins) - 2 (border) = 88
        assert_eq!(bm.content_height(), 88.0);
    }

    #[test]
    fn box_model_with_caption() {
        let bm = BoxModel {
            width: Some(200.0),
            height: Some(100.0),
            caption: Some(Caption {
                placement: CaptionPlacement::Left,
                reserve: Some(50.0),
                text: "Label".to_string(),
                font_family: None,
                font_size: None,
            }),
            max_width: f64::MAX,
            max_height: f64::MAX,
            ..Default::default()
        };
        // content_width = 200 - 0 (margins) - 0 (border) - 50 (caption) = 150
        assert_eq!(bm.content_width(), 150.0);
    }

    #[test]
    fn outer_size_applies_constraints() {
        let bm = BoxModel {
            min_width: 100.0,
            min_height: 50.0,
            max_width: 500.0,
            max_height: 300.0,
            ..Default::default()
        };
        let s = bm.outer_size(Size {
            width: 10.0,
            height: 10.0,
        });
        assert_eq!(s.width, 100.0); // clamped to min
        assert_eq!(s.height, 50.0); // clamped to min
    }

    #[test]
    fn outer_size_fixed() {
        let bm = BoxModel {
            width: Some(200.0),
            height: Some(100.0),
            max_width: f64::MAX,
            max_height: f64::MAX,
            ..Default::default()
        };
        let s = bm.outer_size(Size {
            width: 50.0,
            height: 50.0,
        });
        assert_eq!(s.width, 200.0); // fixed
        assert_eq!(s.height, 100.0); // fixed
    }

    #[test]
    fn insets_helpers() {
        let i = Insets {
            top: 1.0,
            right: 2.0,
            bottom: 3.0,
            left: 4.0,
        };
        assert_eq!(i.horizontal(), 6.0);
        assert_eq!(i.vertical(), 4.0);
    }
}
