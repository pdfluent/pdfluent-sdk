//! Watermarking options.

/// Rotation in 90-degree steps for page-level rotation operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Rotation {
    /// 90° clockwise.
    D90,
    /// 180°.
    D180,
    /// 270° clockwise (= 90° counter-clockwise).
    D270,
}

/// Layer — whether the watermark goes in front of or behind content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// Behind existing content.
    Background,
    /// In front of existing content.
    Foreground,
}

/// Position of a watermark on the page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Position {
    /// Centred.
    Center,
    /// Top-left with offset from the corner.
    TopLeft(f32, f32),
    /// Top-right with offset.
    TopRight(f32, f32),
    /// Bottom-left with offset.
    BottomLeft(f32, f32),
    /// Bottom-right with offset.
    BottomRight(f32, f32),
    /// Exact position in PDF points from bottom-left.
    Exact(f32, f32),
}

/// Text alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    /// Left-aligned.
    Left,
    /// Centred.
    Center,
    /// Right-aligned.
    Right,
}

/// Options for a text watermark.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct WatermarkOptions {
    pub(crate) position: Position,
    pub(crate) rotation_degrees: f32,
    pub(crate) opacity: f32,
    pub(crate) layer: Layer,
    pub(crate) font_size: f32,
    pub(crate) color_rgb: (f32, f32, f32),
}

impl Default for WatermarkOptions {
    fn default() -> Self {
        Self {
            position: Position::Center,
            rotation_degrees: 0.0,
            opacity: 0.5,
            layer: Layer::Foreground,
            font_size: 48.0,
            color_rgb: (0.6, 0.6, 0.6),
        }
    }
}

impl WatermarkOptions {
    /// A centred watermark with default styling.
    pub fn centered() -> Self {
        Self::default()
    }

    /// Rotate the watermark by the given angle in degrees.
    pub fn rotated(mut self, degrees: f32) -> Self {
        self.rotation_degrees = degrees;
        self
    }

    /// Set opacity in `[0.0, 1.0]`.
    pub fn opacity(mut self, alpha: f32) -> Self {
        self.opacity = alpha;
        self
    }

    /// Choose foreground or background placement.
    pub fn layer(mut self, layer: Layer) -> Self {
        self.layer = layer;
        self
    }

    /// Set font size.
    pub fn font_size(mut self, pt: f32) -> Self {
        self.font_size = pt;
        self
    }

    /// Set text colour.
    pub fn color(mut self, r: f32, g: f32, b: f32) -> Self {
        self.color_rgb = (r, g, b);
        self
    }
}
