//! Text placement — font metrics, text wrapping, and dimension calculation.
//!
//! Provides text measurement for layout using per-character width tables
//! derived from Adobe Font Metrics (AFM) for standard PDF fonts.

use crate::types::{Size, TextAlign};

/// Font properties for text measurement.
#[derive(Debug, Clone)]
pub struct FontMetrics {
    /// Font size in points.
    pub size: f64,
    /// Line height as a multiplier of font size (typically 1.2).
    pub line_height: f64,
    /// Average character width as a fraction of font size (fallback).
    pub avg_char_width: f64,
    /// Horizontal text alignment (from XFA `<para hAlign>`).
    pub text_align: TextAlign,
    /// Font family for per-character width lookup.
    pub typeface: FontFamily,
}

/// Font family classification for width table selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontFamily {
    /// Times New Roman, Times, Georgia, etc.
    Serif,
    /// Arial, Helvetica, Verdana, Myriad Pro, etc.
    SansSerif,
    /// Courier New, Courier, Consolas, etc.
    Monospace,
}

impl Default for FontFamily {
    fn default() -> Self {
        FontFamily::SansSerif
    }
}

impl FontFamily {
    /// Classify a typeface name into a font family.
    pub fn from_typeface(name: &str) -> Self {
        let lower = name.to_ascii_lowercase();
        if lower.contains("courier") || lower.contains("consolas") || lower.contains("mono") {
            FontFamily::Monospace
        } else if lower.contains("times")
            || lower.contains("georgia")
            || lower.contains("garamond")
            || lower.contains("palatino")
            || lower.contains("cambria")
            || lower.contains("book antiqua")
            || lower.contains("century")
            || lower.contains("serif")
        {
            FontFamily::Serif
        } else {
            // Arial, Helvetica, Verdana, Myriad Pro, Calibri, Tahoma, etc.
            FontFamily::SansSerif
        }
    }
}

impl Default for FontMetrics {
    fn default() -> Self {
        Self {
            size: 10.0,
            line_height: 1.2,
            avg_char_width: 0.50,
            text_align: TextAlign::Left,
            typeface: FontFamily::SansSerif,
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

    /// Estimated width of a string in points using per-character width tables.
    pub fn measure_width(&self, text: &str) -> f64 {
        let table = match self.typeface {
            FontFamily::Serif => &TIMES_WIDTHS,
            FontFamily::SansSerif => &HELVETICA_WIDTHS,
            FontFamily::Monospace => &COURIER_WIDTHS,
        };
        let mut width = 0.0;
        for byte in text.bytes() {
            let idx = byte as usize;
            let char_width = if idx < 128 {
                table[idx] as f64
            } else {
                // Non-ASCII: use the font's default width (same as 'n')
                table[b'n' as usize] as f64
            };
            width += char_width / 1000.0 * self.size;
        }
        width
    }
}

// ---------------------------------------------------------------------------
// Per-character width tables (Adobe Font Metrics, units per 1000 em)
// ---------------------------------------------------------------------------

/// Times-Roman character widths (ASCII 0-127).
/// Source: Adobe AFM for Times-Roman.
#[rustfmt::skip]
static TIMES_WIDTHS: [u16; 128] = [
    // 0x00-0x0F: control characters → use space width (250)
    250,250,250,250,250,250,250,250,250,250,250,250,250,250,250,250,
    // 0x10-0x1F: control characters
    250,250,250,250,250,250,250,250,250,250,250,250,250,250,250,250,
    // 0x20-0x2F: SP ! " # $ % & ' ( ) * + , - . /
    250,333,408,500,500,833,778,180,333,333,500,564,250,333,250,278,
    // 0x30-0x3F: 0 1 2 3 4 5 6 7 8 9 : ; < = > ?
    500,500,500,500,500,500,500,500,500,500,278,278,564,564,564,444,
    // 0x40-0x4F: @ A B C D E F G H I J K L M N O
    921,722,667,667,722,611,556,722,722,333,389,722,611,889,722,722,
    // 0x50-0x5F: P Q R S T U V W X Y Z [ \ ] ^ _
    556,722,667,556,611,722,722,944,722,722,611,333,278,333,469,500,
    // 0x60-0x6F: ` a b c d e f g h i j k l m n o
    333,444,500,444,500,444,333,500,500,278,278,500,278,778,500,500,
    // 0x70-0x7F: p q r s t u v w x y z { | } ~ DEL
    500,500,333,389,278,500,500,722,500,500,444,480,200,480,541,250,
];

/// Helvetica character widths (ASCII 0-127).
/// Source: Adobe AFM for Helvetica.
#[rustfmt::skip]
static HELVETICA_WIDTHS: [u16; 128] = [
    // 0x00-0x0F: control characters → use space width (278)
    278,278,278,278,278,278,278,278,278,278,278,278,278,278,278,278,
    // 0x10-0x1F: control characters
    278,278,278,278,278,278,278,278,278,278,278,278,278,278,278,278,
    // 0x20-0x2F: SP ! " # $ % & ' ( ) * + , - . /
    278,278,355,556,556,889,667,191,333,333,389,584,278,333,278,278,
    // 0x30-0x3F: 0 1 2 3 4 5 6 7 8 9 : ; < = > ?
    556,556,556,556,556,556,556,556,556,556,278,278,584,584,584,556,
    // 0x40-0x4F: @ A B C D E F G H I J K L M N O
    1015,667,667,722,722,611,556,778,722,278,500,667,556,833,722,778,
    // 0x50-0x5F: P Q R S T U V W X Y Z [ \ ] ^ _
    667,778,722,667,611,722,667,944,667,667,611,278,278,278,469,556,
    // 0x60-0x6F: ` a b c d e f g h i j k l m n o
    333,556,556,500,556,556,278,556,556,222,222,500,222,833,556,556,
    // 0x70-0x7F: p q r s t u v w x y z { | } ~ DEL
    556,556,333,500,278,556,500,722,500,500,500,334,260,334,584,278,
];

/// Courier character widths (ASCII 0-127).
/// All characters are 600 units wide (monospace).
#[rustfmt::skip]
static COURIER_WIDTHS: [u16; 128] = [
    600,600,600,600,600,600,600,600,600,600,600,600,600,600,600,600,
    600,600,600,600,600,600,600,600,600,600,600,600,600,600,600,600,
    600,600,600,600,600,600,600,600,600,600,600,600,600,600,600,600,
    600,600,600,600,600,600,600,600,600,600,600,600,600,600,600,600,
    600,600,600,600,600,600,600,600,600,600,600,600,600,600,600,600,
    600,600,600,600,600,600,600,600,600,600,600,600,600,600,600,600,
    600,600,600,600,600,600,600,600,600,600,600,600,600,600,600,600,
    600,600,600,600,600,600,600,600,600,600,600,600,600,600,600,600,
];

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
    let max_width = lines
        .iter()
        .map(|l| font.measure_width(l))
        .fold(0.0, f64::max);
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
    fn measure_width_times() {
        let f = FontMetrics {
            size: 10.0,
            typeface: FontFamily::Serif,
            ..Default::default()
        };
        // "Hello" in Times: H=722 e=444 l=278 l=278 o=500 = 2222/1000*10 = 22.22
        let w = f.measure_width("Hello");
        assert!((w - 22.22).abs() < 0.01, "Times Hello={w}, expected ~22.22");
    }

    #[test]
    fn measure_width_helvetica() {
        let f = FontMetrics {
            size: 10.0,
            typeface: FontFamily::SansSerif,
            ..Default::default()
        };
        // "Hello" in Helvetica: H=722 e=556 l=222 l=222 o=556 = 2278/1000*10 = 22.78
        let w = f.measure_width("Hello");
        assert!((w - 22.78).abs() < 0.01, "Helv Hello={w}, expected ~22.78");
    }

    #[test]
    fn measure_width_courier() {
        let f = FontMetrics {
            size: 10.0,
            typeface: FontFamily::Monospace,
            ..Default::default()
        };
        // "Hello" in Courier: 5 * 600/1000*10 = 30.0
        let w = f.measure_width("Hello");
        assert!((w - 30.0).abs() < 0.01, "Courier Hello={w}, expected 30.0");
    }

    #[test]
    fn times_narrower_than_old_avg() {
        // The old avg_char_width=0.5 would give: 5*10*0.5=25.0 for "Hello"
        // Times should be narrower (~22.22)
        let f = FontMetrics {
            size: 10.0,
            typeface: FontFamily::Serif,
            ..Default::default()
        };
        let w = f.measure_width("Hello");
        assert!(w < 25.0, "Times should be narrower than old 0.5 avg");
    }

    #[test]
    fn font_family_classification() {
        assert_eq!(FontFamily::from_typeface("Times New Roman"), FontFamily::Serif);
        assert_eq!(FontFamily::from_typeface("Arial"), FontFamily::SansSerif);
        assert_eq!(FontFamily::from_typeface("Courier New"), FontFamily::Monospace);
        assert_eq!(FontFamily::from_typeface("Myriad Pro"), FontFamily::SansSerif);
        assert_eq!(FontFamily::from_typeface("Verdana"), FontFamily::SansSerif);
        assert_eq!(FontFamily::from_typeface("Georgia"), FontFamily::Serif);
    }

    #[test]
    fn measure_text_single_line() {
        let f = FontMetrics::default();
        let s = measure_text("Hello", &f);
        assert!(s.width > 0.0);
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
    fn wrap_text_times_given_name() {
        // Regression: "Given Name (First Name)" should fit in ~140pt at 9pt Times
        let f = FontMetrics {
            size: 9.0,
            typeface: FontFamily::Serif,
            ..Default::default()
        };
        let result = wrap_text("Given Name (First Name)", 140.0, &f);
        assert_eq!(
            result.lines.len(), 1,
            "Should fit on 1 line but got: {:?}", result.lines
        );
    }
}
