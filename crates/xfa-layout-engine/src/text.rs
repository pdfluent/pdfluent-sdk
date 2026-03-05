//! Text placement — font metrics, text wrapping, and dimension calculation.
//!
//! Provides basic text measurement for layout purposes. In the absence of
//! a full font renderer, uses configurable average character widths based
//! on font size. This module will be extended with PDFium-based font metrics
//! in a later epic.

use crate::types::Size;

/// Font properties for text measurement.
#[derive(Debug, Clone)]
pub struct FontMetrics {
    /// Font size in points.
    pub size: f64,
    /// Line height as a multiplier of font size (typically 1.2).
    pub line_height: f64,
    /// Average character width as a fraction of font size.
    /// For proportional fonts ~0.5, for monospace ~0.6.
    pub avg_char_width: f64,
}

impl Default for FontMetrics {
    fn default() -> Self {
        Self {
            size: 10.0,
            line_height: 1.2,
            avg_char_width: 0.5,
        }
    }
}

impl FontMetrics {
    pub fn new(size: f64) -> Self {
        Self {
            size,
            ..Default::default()
        }
    }

    /// The height of a single line of text.
    pub fn line_height_pt(&self) -> f64 {
        self.size * self.line_height
    }

    /// Estimated width of a string in points.
    pub fn measure_width(&self, text: &str) -> f64 {
        text.len() as f64 * self.size * self.avg_char_width
    }
}

/// Text wrapping and measurement result.
#[derive(Debug, Clone)]
pub struct TextLayout {
    /// The wrapped lines of text.
    pub lines: Vec<String>,
    /// Total size of the text block.
    pub size: Size,
}

/// Wrap text to fit within a given width, and compute the resulting size.
///
/// Uses a simple word-wrapping algorithm: breaks at whitespace boundaries.
/// Returns the lines and the total bounding box.
pub fn wrap_text(text: &str, max_width: f64, font: &FontMetrics) -> TextLayout {
    if text.is_empty() {
        return TextLayout {
            lines: vec![],
            size: Size {
                width: 0.0,
                height: 0.0,
            },
        };
    }

    let mut lines = Vec::new();
    let mut max_line_width = 0.0_f64;

    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }

        let words: Vec<&str> = paragraph.split_whitespace().collect();
        if words.is_empty() {
            lines.push(String::new());
            continue;
        }

        let mut current_line = String::new();
        let mut current_width = 0.0;

        for word in words {
            let word_width = font.measure_width(word);
            let space_width = if current_line.is_empty() {
                0.0
            } else {
                font.measure_width(" ")
            };

            if current_width + space_width + word_width > max_width && !current_line.is_empty() {
                // Wrap to new line
                max_line_width = max_line_width.max(current_width);
                lines.push(current_line);
                current_line = word.to_string();
                current_width = word_width;
            } else {
                if !current_line.is_empty() {
                    current_line.push(' ');
                    current_width += space_width;
                }
                current_line.push_str(word);
                current_width += word_width;
            }
        }

        if !current_line.is_empty() {
            max_line_width = max_line_width.max(current_width);
            lines.push(current_line);
        }
    }

    let height = lines.len() as f64 * font.line_height_pt();

    TextLayout {
        lines,
        size: Size {
            width: max_line_width,
            height,
        },
    }
}

/// Compute the bounding box of text without wrapping (single-line or multi-line via \n).
pub fn measure_text(text: &str, font: &FontMetrics) -> Size {
    if text.is_empty() {
        return Size {
            width: 0.0,
            height: 0.0,
        };
    }

    let lines: Vec<&str> = text.split('\n').collect();
    let max_width = lines.iter().map(|l| font.measure_width(l)).fold(0.0, f64::max);
    let height = lines.len() as f64 * font.line_height_pt();

    Size {
        width: max_width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_metrics_defaults() {
        let f = FontMetrics::default();
        assert_eq!(f.size, 10.0);
        assert_eq!(f.line_height_pt(), 12.0); // 10 * 1.2
    }

    #[test]
    fn measure_width_basic() {
        let f = FontMetrics::default();
        // "Hello" = 5 chars * 10pt * 0.5 = 25pt
        assert_eq!(f.measure_width("Hello"), 25.0);
    }

    #[test]
    fn measure_text_single_line() {
        let f = FontMetrics::default();
        let s = measure_text("Hello", &f);
        assert_eq!(s.width, 25.0);
        assert_eq!(s.height, 12.0);
    }

    #[test]
    fn measure_text_multiline() {
        let f = FontMetrics::default();
        let s = measure_text("Line 1\nLine 2\nLine 3", &f);
        assert_eq!(s.height, 36.0); // 3 lines * 12pt
    }

    #[test]
    fn wrap_text_no_wrap_needed() {
        let f = FontMetrics::default();
        let result = wrap_text("Short", 200.0, &f);
        assert_eq!(result.lines.len(), 1);
        assert_eq!(result.lines[0], "Short");
    }

    #[test]
    fn wrap_text_wraps_at_word_boundary() {
        let f = FontMetrics::default();
        // "Hello World" = 11 chars * 5pt = 55pt; max_width = 40pt
        // "Hello" = 25pt fits, "World" = 25pt would make 55pt → wrap
        let result = wrap_text("Hello World", 40.0, &f);
        assert_eq!(result.lines.len(), 2);
        assert_eq!(result.lines[0], "Hello");
        assert_eq!(result.lines[1], "World");
    }

    #[test]
    fn wrap_text_multiple_words() {
        let f = FontMetrics::default();
        // Each word ~15-25pt, max_width 60pt
        let result = wrap_text("The quick brown fox jumps", 60.0, &f);
        assert!(result.lines.len() > 1);
        // All words should appear across lines
        let joined: String = result.lines.join(" ");
        assert_eq!(joined, "The quick brown fox jumps");
    }

    #[test]
    fn wrap_text_preserves_newlines() {
        let f = FontMetrics::default();
        let result = wrap_text("Line 1\nLine 2", 200.0, &f);
        assert_eq!(result.lines.len(), 2);
        assert_eq!(result.lines[0], "Line 1");
        assert_eq!(result.lines[1], "Line 2");
    }

    #[test]
    fn wrap_text_empty_string() {
        let f = FontMetrics::default();
        let result = wrap_text("", 100.0, &f);
        assert_eq!(result.lines.len(), 0);
        assert_eq!(result.size.height, 0.0);
    }

    #[test]
    fn wrap_text_size_correct() {
        let f = FontMetrics::default();
        let result = wrap_text("Hello World", 40.0, &f);
        // 2 lines, each 12pt tall
        assert_eq!(result.size.height, 24.0);
        // Max line width = max("Hello", "World") = 25pt
        assert_eq!(result.size.width, 25.0);
    }

    #[test]
    fn custom_font_size() {
        let f = FontMetrics::new(14.0);
        assert_eq!(f.size, 14.0);
        assert_eq!(f.line_height_pt(), 16.8); // 14 * 1.2
        // "AB" = 2 * 14 * 0.5 = 14pt
        assert_eq!(f.measure_width("AB"), 14.0);
    }
}
