//\! Core annotation types shared across all annotation categories.

use pdf_syntax::object::dict::keys::*;
use pdf_syntax::object::{Array, Dict, Name, Rect};

/// The type of a PDF annotation (ISO 32000-2 Table 170).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationType {
    /// A text annotation (sticky note).
    Text,
    /// A link annotation.
    Link,
    /// A free text annotation.
    FreeText,
    /// A line annotation.
    Line,
    /// A square annotation.
    Square,
    /// A circle annotation.
    Circle,
    /// A polygon annotation.
    Polygon,
    /// A polyline annotation.
    PolyLine,
    /// A highlight annotation.
    Highlight,
    /// An underline annotation.
    Underline,
    /// A squiggly underline annotation.
    Squiggly,
    /// A strikeout annotation.
    StrikeOut,
    /// A rubber stamp annotation.
    Stamp,
    /// A caret annotation.
    Caret,
    /// An ink (freehand) annotation.
    Ink,
    /// A popup annotation.
    Popup,
    /// A file attachment annotation.
    FileAttachment,
    /// A sound annotation.
    Sound,
    /// A widget annotation (form field).
    Widget,
    /// A watermark annotation.
    Watermark,
    /// A redaction annotation.
    Redact,
    /// A movie annotation.
    Movie,
    /// An unknown annotation type.
    Unknown,
}

impl AnnotationType {
    /// Parse an annotation type from a PDF `/Subtype` name.
    pub fn from_name(name: &[u8]) -> Self {
        match name {
            b"Text" => Self::Text,
            b"Link" => Self::Link,
            b"FreeText" => Self::FreeText,
            b"Line" => Self::Line,
            b"Square" => Self::Square,
            b"Circle" => Self::Circle,
            b"Polygon" => Self::Polygon,
            b"PolyLine" => Self::PolyLine,
            b"Highlight" => Self::Highlight,
            b"Underline" => Self::Underline,
            b"Squiggly" => Self::Squiggly,
            b"StrikeOut" => Self::StrikeOut,
            b"Stamp" => Self::Stamp,
            b"Caret" => Self::Caret,
            b"Ink" => Self::Ink,
            b"Popup" => Self::Popup,
            b"FileAttachment" => Self::FileAttachment,
            b"Sound" => Self::Sound,
            b"Widget" => Self::Widget,
            b"Watermark" => Self::Watermark,
            b"Redact" => Self::Redact,
            b"Movie" => Self::Movie,
            _ => Self::Unknown,
        }
    }
}

/// Annotation flags (ISO 32000-2 Table 175).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnnotationFlags(pub u32);

impl AnnotationFlags {
    /// Bit 1: Invisible.
    pub fn invisible(&self) -> bool { self.0 & (1 << 0) \!= 0 }
    /// Bit 2: Hidden.
    pub fn hidden(&self) -> bool { self.0 & (1 << 1) \!= 0 }
    /// Bit 3: Print.
    pub fn print(&self) -> bool { self.0 & (1 << 2) \!= 0 }
    /// Bit 4: No zoom.
    pub fn no_zoom(&self) -> bool { self.0 & (1 << 3) \!= 0 }
    /// Bit 5: No rotate.
    pub fn no_rotate(&self) -> bool { self.0 & (1 << 4) \!= 0 }
    /// Bit 6: No view.
    pub fn no_view(&self) -> bool { self.0 & (1 << 5) \!= 0 }
    /// Bit 7: Read only.
    pub fn read_only(&self) -> bool { self.0 & (1 << 6) \!= 0 }
    /// Bit 8: Locked.
    pub fn locked(&self) -> bool { self.0 & (1 << 7) \!= 0 }
    /// Bit 9: Toggle no view.
    pub fn toggle_no_view(&self) -> bool { self.0 & (1 << 8) \!= 0 }
    /// Bit 10: Locked contents.
    pub fn locked_contents(&self) -> bool { self.0 & (1 << 9) \!= 0 }
}

/// Border style (ISO 32000-2 Table 168).
#[derive(Debug, Clone)]
pub struct BorderStyle {
    /// Border width in points.
    pub width: f32,
    /// Border style type.
    pub style: BorderStyleType,
    /// Dash pattern.
    pub dash_pattern: Vec<f32>,
}

/// Border style types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderStyleType {
    /// Solid border.
    Solid,
    /// Dashed border.
    Dashed,
    /// Beveled border.
    Beveled,
    /// Inset border.
    Inset,
    /// Underline border.
    Underline,
}

impl BorderStyle {
    /// Parse a border style from a `/BS` dictionary.
    pub fn from_dict(dict: &Dict<'_>) -> Self {
        let width = dict.get::<f32>(W).unwrap_or(1.0);
        let style = dict
            .get::<Name>(S)
            .map(|n| match n.as_ref() {
                b"D" => BorderStyleType::Dashed,
                b"B" => BorderStyleType::Beveled,
                b"I" => BorderStyleType::Inset,
                b"U" => BorderStyleType::Underline,
                _ => BorderStyleType::Solid,
            })
            .unwrap_or(BorderStyleType::Solid);
        let dash_pattern = dict
            .get::<Array<'_>>(D)
            .map(|arr| arr.iter::<f32>().collect())
            .unwrap_or_else(|| vec\![3.0]);
        Self { width, style, dash_pattern }
    }
}

/// A color value.
#[derive(Debug, Clone)]
pub enum Color {
    /// Transparent.
    Transparent,
    /// Grayscale.
    Gray(f32),
    /// RGB.
    Rgb(f32, f32, f32),
    /// CMYK.
    Cmyk(f32, f32, f32, f32),
}

impl Color {
    /// Parse a color from an annotation color array.
    pub fn from_array(arr: &Array<'_>) -> Self {
        let values: Vec<f32> = arr.iter::<f32>().collect();
        match values.len() {
            0 => Self::Transparent,
            1 => Self::Gray(values[0]),
            3 => Self::Rgb(values[0], values[1], values[2]),
            4 => Self::Cmyk(values[0], values[1], values[2], values[3]),
            _ => Self::Transparent,
        }
    }
}

/// Border effect.
#[derive(Debug, Clone)]
pub struct BorderEffect {
    /// The effect style.
    pub style: BorderEffectStyle,
    /// Effect intensity.
    pub intensity: f32,
}

/// Border effect style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderEffectStyle {
    /// No effect.
    None,
    /// Cloudy border.
    Cloudy,
}

impl BorderEffect {
    /// Parse from a `/BE` dictionary.
    pub fn from_dict(dict: &Dict<'_>) -> Self {
        let style = dict
            .get::<Name>(S)
            .map(|n| match n.as_ref() {
                b"C" => BorderEffectStyle::Cloudy,
                _ => BorderEffectStyle::None,
            })
            .unwrap_or(BorderEffectStyle::None);
        let intensity = dict.get::<f32>(I).unwrap_or(0.0);
        Self { style, intensity }
    }
}

/// Quadrilateral points for text markup annotations.
#[derive(Debug, Clone)]
pub struct QuadPoints {
    /// Raw coordinate values.
    pub points: Vec<f32>,
}

impl QuadPoints {
    /// Parse quad points from an array.
    pub fn from_array(arr: &Array<'_>) -> Self {
        Self { points: arr.iter::<f32>().collect() }
    }

    /// Return bounding rectangles.
    pub fn bounding_rects(&self) -> Vec<Rect> {
        self.points.chunks_exact(8).map(|chunk| {
            let xs = [chunk[0], chunk[2], chunk[4], chunk[6]];
            let ys = [chunk[1], chunk[3], chunk[5], chunk[7]];
            let x0 = xs.iter().copied().reduce(f32::min).unwrap_or(0.0) as f64;
            let y0 = ys.iter().copied().reduce(f32::min).unwrap_or(0.0) as f64;
            let x1 = xs.iter().copied().reduce(f32::max).unwrap_or(0.0) as f64;
            let y1 = ys.iter().copied().reduce(f32::max).unwrap_or(0.0) as f64;
            Rect::new(x0, y0, x1, y1)
        }).collect()
    }
}

/// Line ending styles (ISO 32000-2 Table 179).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    /// No line ending.
    None,
    /// A square.
    Square,
    /// A circle.
    Circle,
    /// A diamond.
    Diamond,
    /// An open arrowhead.
    OpenArrow,
    /// A closed arrowhead.
    ClosedArrow,
    /// Butt.
    Butt,
    /// Reverse open arrowhead.
    ROpenArrow,
    /// Reverse closed arrowhead.
    RClosedArrow,
    /// A slash.
    Slash,
}

impl LineEnding {
    /// Parse from a PDF name.
    pub fn from_name(name: &[u8]) -> Self {
        match name {
            b"Square" => Self::Square,
            b"Circle" => Self::Circle,
            b"Diamond" => Self::Diamond,
            b"OpenArrow" => Self::OpenArrow,
            b"ClosedArrow" => Self::ClosedArrow,
            b"Butt" => Self::Butt,
            b"ROpenArrow" => Self::ROpenArrow,
            b"RClosedArrow" => Self::RClosedArrow,
            b"Slash" => Self::Slash,
            _ => Self::None,
        }
    }
}
