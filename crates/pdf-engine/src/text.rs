//! Text extraction via a custom Device implementation.

use kurbo::{Affine, BezPath};
use pdf_render::pdf_interpret::cmap::BfString;
use pdf_render::pdf_interpret::font::Glyph;
use pdf_render::pdf_interpret::{
    BlendMode, ClipPath, Device, GlyphDrawMode, Image, Paint, PathDrawMode, SoftMask,
};

/// A single text span at a specific position.
#[derive(Debug, Clone)]
pub struct TextSpan {
    /// The extracted text.
    pub text: String,
    /// X position in user space.
    pub x: f64,
    /// Y position in user space.
    pub y: f64,
    /// Font size (approximate, from transform).
    pub font_size: f64,
}

/// A block of text (grouped by vertical proximity).
#[derive(Debug, Clone)]
pub struct TextBlock {
    /// Spans within this block, sorted by position.
    pub spans: Vec<TextSpan>,
}

impl TextBlock {
    /// Concatenate all spans into a single string, space-separated.
    pub fn text(&self) -> String {
        self.spans
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// A Device implementation that captures text from draw_glyph calls.
pub struct TextExtractionDevice {
    spans: Vec<TextSpan>,
}

impl Default for TextExtractionDevice {
    fn default() -> Self {
        Self::new()
    }
}

impl TextExtractionDevice {
    /// Create a new text extraction device.
    pub fn new() -> Self {
        Self { spans: Vec::new() }
    }

    /// Consume the device and return extracted text as a single string.
    pub fn into_text(self) -> String {
        let blocks = group_spans_into_blocks(self.spans);
        blocks
            .iter()
            .map(|b| b.text())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Consume the device and return text blocks.
    pub fn into_blocks(self) -> Vec<TextBlock> {
        group_spans_into_blocks(self.spans)
    }

    /// Consume the device and return raw spans.
    pub fn into_spans(self) -> Vec<TextSpan> {
        self.spans
    }
}

impl Device<'_> for TextExtractionDevice {
    fn set_soft_mask(&mut self, _: Option<SoftMask<'_>>) {}
    fn set_blend_mode(&mut self, _: BlendMode) {}
    fn draw_path(&mut self, _: &BezPath, _: Affine, _: &Paint<'_>, _: &PathDrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'_>>, _: BlendMode) {}
    fn draw_image(&mut self, _: Image<'_, '_>, _: Affine) {}
    fn pop_clip_path(&mut self) {}
    fn pop_transparency_group(&mut self) {}

    fn draw_glyph(
        &mut self,
        glyph: &Glyph<'_>,
        transform: Affine,
        _glyph_transform: Affine,
        _paint: &Paint<'_>,
        _draw_mode: &GlyphDrawMode,
    ) {
        let text = match glyph.as_unicode() {
            Some(BfString::Char(c)) => c.to_string(),
            Some(BfString::String(s)) => s,
            None => return,
        };

        let coeffs = transform.as_coeffs();
        let x = coeffs[4];
        let y = coeffs[5];
        // Approximate font size from the transform matrix.
        let font_size = (coeffs[0].powi(2) + coeffs[1].powi(2)).sqrt().abs();

        self.spans.push(TextSpan {
            text,
            x,
            y,
            font_size,
        });
    }
}

/// Group spans into blocks by vertical proximity.
fn group_spans_into_blocks(mut spans: Vec<TextSpan>) -> Vec<TextBlock> {
    if spans.is_empty() {
        return Vec::new();
    }

    // Sort by y descending (PDF y-up), then x ascending.
    spans.sort_by(|a, b| {
        b.y.partial_cmp(&a.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut blocks: Vec<TextBlock> = Vec::new();
    let mut current_spans: Vec<TextSpan> = vec![spans.remove(0)];

    for span in spans {
        let last_y = current_spans.last().map(|s| s.y).unwrap_or(0.0);
        let threshold = current_spans
            .last()
            .map(|s| s.font_size * 1.5)
            .unwrap_or(12.0);

        if (last_y - span.y).abs() <= threshold {
            current_spans.push(span);
        } else {
            blocks.push(TextBlock {
                spans: std::mem::take(&mut current_spans),
            });
            current_spans.push(span);
        }
    }

    if !current_spans.is_empty() {
        blocks.push(TextBlock {
            spans: current_spans,
        });
    }

    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_device_produces_empty_text() {
        let dev = TextExtractionDevice::new();
        assert!(dev.into_text().is_empty());
    }

    #[test]
    fn spans_group_into_blocks() {
        let spans = vec![
            TextSpan {
                text: "Hello".into(),
                x: 10.0,
                y: 700.0,
                font_size: 12.0,
            },
            TextSpan {
                text: "World".into(),
                x: 60.0,
                y: 700.0,
                font_size: 12.0,
            },
            TextSpan {
                text: "Next".into(),
                x: 10.0,
                y: 680.0,
                font_size: 12.0,
            },
            TextSpan {
                text: "Far".into(),
                x: 10.0,
                y: 500.0,
                font_size: 12.0,
            },
        ];
        let blocks = group_spans_into_blocks(spans);
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].spans.len(), 2); // Hello, World (same y)
        assert_eq!(blocks[1].spans.len(), 1); // Next (y=680, gap > 1.5*12)
        assert_eq!(blocks[2].spans.len(), 1); // Far (y=500)
    }

    #[test]
    fn text_block_concatenation() {
        let block = TextBlock {
            spans: vec![
                TextSpan {
                    text: "A".into(),
                    x: 0.0,
                    y: 0.0,
                    font_size: 12.0,
                },
                TextSpan {
                    text: "B".into(),
                    x: 20.0,
                    y: 0.0,
                    font_size: 12.0,
                },
            ],
        };
        assert_eq!(block.text(), "A B");
    }

    #[test]
    fn empty_spans_no_blocks() {
        let blocks = group_spans_into_blocks(Vec::new());
        assert!(blocks.is_empty());
    }
}
