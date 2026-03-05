//! Core types for XFA layout — Box Model, measurements, and layout primitives.
//!
//! Implements XFA 3.3 §4 (Box Model) types.

/// A 2D point in layout coordinates (points, 1pt = 1/72 inch).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// A 2D size in points.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

/// An axis-aligned rectangle in layout space.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(&self) -> f64 {
        self.x + self.width
    }

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
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

impl Insets {
    pub fn uniform(value: f64) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    pub fn horizontal(&self) -> f64 {
        self.left + self.right
    }

    pub fn vertical(&self) -> f64 {
        self.top + self.bottom
    }
}

/// A measurement with a unit, parsed from XFA attributes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measurement {
    pub value: f64,
    pub unit: MeasurementUnit,
}

impl Measurement {
    /// Convert this measurement to points (the internal unit).
    pub fn to_points(&self) -> f64 {
        match self.unit {
            MeasurementUnit::Points => self.value,
            MeasurementUnit::Inches => self.value * 72.0,
            MeasurementUnit::Centimeters => self.value * 72.0 / 2.54,
            MeasurementUnit::Millimeters => self.value * 72.0 / 25.4,
            MeasurementUnit::Em => self.value * 12.0, // default 12pt font
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasurementUnit {
    Inches,
    Centimeters,
    Millimeters,
    Points,
    Em,
}

/// Layout strategy for a container.
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

/// Caption placement relative to content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaptionPlacement {
    #[default]
    Left,
    Top,
    Right,
    Bottom,
    Inline,
}

/// The XFA Box Model for a form element.
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
    pub placement: CaptionPlacement,
    /// Reserved space for the caption (None = auto).
    pub reserve: Option<f64>,
    pub text: String,
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
