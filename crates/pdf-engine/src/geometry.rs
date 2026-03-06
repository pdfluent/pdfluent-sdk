//! Page geometry: boxes (MediaBox, CropBox, TrimBox, BleedBox, ArtBox),
//! rotation, and DPI-based pixel conversions.

use pdf_render::pdf_syntax::object::dict::keys;
use pdf_render::pdf_syntax::object::Rect;
use pdf_render::pdf_syntax::page::Page;

/// A rectangle in PDF user-space points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageBox {
    /// Left edge.
    pub x0: f64,
    /// Bottom edge.
    pub y0: f64,
    /// Right edge.
    pub x1: f64,
    /// Top edge.
    pub y1: f64,
}

impl PageBox {
    /// Width in points.
    pub fn width(&self) -> f64 {
        (self.x1 - self.x0).abs()
    }

    /// Height in points.
    pub fn height(&self) -> f64 {
        (self.y1 - self.y0).abs()
    }

    /// Convert to pixel dimensions at the given DPI.
    pub fn pixels(&self, dpi: f64) -> (u32, u32) {
        let scale = dpi / 72.0;
        (
            (self.width() * scale).ceil() as u32,
            (self.height() * scale).ceil() as u32,
        )
    }
}

impl From<Rect> for PageBox {
    fn from(r: Rect) -> Self {
        Self {
            x0: r.x0,
            y0: r.y0,
            x1: r.x1,
            y1: r.y1,
        }
    }
}

/// Page rotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageRotation {
    /// No rotation (0 degrees).
    None,
    /// 90 degrees clockwise.
    Rotate90,
    /// 180 degrees.
    Rotate180,
    /// 270 degrees clockwise (90 counter-clockwise).
    Rotate270,
}

impl PageRotation {
    /// Rotation in degrees.
    pub fn degrees(&self) -> u32 {
        match self {
            Self::None => 0,
            Self::Rotate90 => 90,
            Self::Rotate180 => 180,
            Self::Rotate270 => 270,
        }
    }
}

/// Complete geometry for a single page.
#[derive(Debug, Clone)]
pub struct PageGeometry {
    /// MediaBox (required, fallback to A4).
    pub media_box: PageBox,
    /// CropBox (defaults to MediaBox).
    pub crop_box: PageBox,
    /// TrimBox if present.
    pub trim_box: Option<PageBox>,
    /// BleedBox if present.
    pub bleed_box: Option<PageBox>,
    /// ArtBox if present.
    pub art_box: Option<PageBox>,
    /// Page rotation.
    pub rotation: PageRotation,
}

impl PageGeometry {
    /// Effective visible dimensions in points, accounting for rotation.
    pub fn effective_dimensions(&self) -> (f64, f64) {
        let w = self.crop_box.width();
        let h = self.crop_box.height();
        match self.rotation {
            PageRotation::Rotate90 | PageRotation::Rotate270 => (h, w),
            _ => (w, h),
        }
    }

    /// Effective visible dimensions in pixels at the given DPI.
    pub fn pixel_dimensions(&self, dpi: f64) -> (u32, u32) {
        let (w, h) = self.effective_dimensions();
        let scale = dpi / 72.0;
        ((w * scale).ceil() as u32, (h * scale).ceil() as u32)
    }
}

/// Extract full geometry from a pdf-syntax Page.
pub fn extract_geometry(page: &Page<'_>) -> PageGeometry {
    let media_box = PageBox::from(page.media_box());
    let crop_box = PageBox::from(page.crop_box());

    let rotation = match page.rotation() {
        pdf_render::pdf_syntax::page::Rotation::None => PageRotation::None,
        pdf_render::pdf_syntax::page::Rotation::Horizontal => PageRotation::Rotate90,
        pdf_render::pdf_syntax::page::Rotation::Flipped => PageRotation::Rotate180,
        pdf_render::pdf_syntax::page::Rotation::FlippedHorizontal => PageRotation::Rotate270,
    };

    let raw = page.raw();
    let trim_box = raw.get::<Rect>(keys::TRIM_BOX).map(PageBox::from);
    let bleed_box = raw.get::<Rect>(keys::BLEED_BOX).map(PageBox::from);
    let art_box = raw.get::<Rect>(keys::ART_BOX).map(PageBox::from);

    PageGeometry {
        media_box,
        crop_box,
        trim_box,
        bleed_box,
        art_box,
        rotation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_box_dimensions() {
        let b = PageBox {
            x0: 0.0,
            y0: 0.0,
            x1: 612.0,
            y1: 792.0,
        };
        assert!((b.width() - 612.0).abs() < f64::EPSILON);
        assert!((b.height() - 792.0).abs() < f64::EPSILON);
    }

    #[test]
    fn page_box_pixels() {
        let b = PageBox {
            x0: 0.0,
            y0: 0.0,
            x1: 72.0,
            y1: 72.0,
        };
        assert_eq!(b.pixels(72.0), (72, 72));
        assert_eq!(b.pixels(144.0), (144, 144));
    }

    #[test]
    fn rotation_degrees() {
        assert_eq!(PageRotation::None.degrees(), 0);
        assert_eq!(PageRotation::Rotate90.degrees(), 90);
        assert_eq!(PageRotation::Rotate180.degrees(), 180);
        assert_eq!(PageRotation::Rotate270.degrees(), 270);
    }

    #[test]
    fn geometry_effective_dimensions() {
        let g = PageGeometry {
            media_box: PageBox {
                x0: 0.0,
                y0: 0.0,
                x1: 612.0,
                y1: 792.0,
            },
            crop_box: PageBox {
                x0: 0.0,
                y0: 0.0,
                x1: 612.0,
                y1: 792.0,
            },
            trim_box: None,
            bleed_box: None,
            art_box: None,
            rotation: PageRotation::Rotate90,
        };
        let (w, h) = g.effective_dimensions();
        assert!((w - 792.0).abs() < f64::EPSILON);
        assert!((h - 612.0).abs() < f64::EPSILON);
    }
}
