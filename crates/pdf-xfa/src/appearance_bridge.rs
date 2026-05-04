//! FormCalc results -> PDF appearance streams.
//!
//! Connects FormCalc execution to appearance stream generation.
//! Handles value formatting, conditional visibility, and caching.

use crate::error::Result;
use std::collections::HashMap;
use std::io::Write as _;
use xfa_layout_engine::form::FormNodeId;
use xfa_layout_engine::layout::{LayoutContent, LayoutDom, LayoutNode};

/// Configuration for appearance stream generation.
#[derive(Debug, Clone)]
pub struct AppearanceConfig {
    /// default_font.
    pub default_font: String,
    /// default_font_size.
    pub default_font_size: f64,
    /// border_width.
    pub border_width: f64,
    /// border_color.
    pub border_color: [f64; 3],
    /// background_color.
    pub background_color: Option<[f64; 3]>,
    /// text_color.
    pub text_color: [f64; 3],
    /// text_padding.
    pub text_padding: f64,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            default_font: "Helvetica".to_string(),
            default_font_size: 10.0,
            border_width: 0.5,
            border_color: [0.0, 0.0, 0.0],
            background_color: Some([1.0, 1.0, 1.0]),
            text_color: [0.0, 0.0, 0.0],
            text_padding: 2.0,
        }
    }
}

/// A generated appearance stream.
#[derive(Debug, Clone)]
pub struct AppearanceStream {
    /// content.
    pub content: Vec<u8>,
    /// bbox.
    pub bbox: [f64; 4],
    /// font_resources.
    pub font_resources: Vec<(String, String)>,
}

/// Appearance stream cache with invalidation.
pub struct AppearanceCache {
    cache: HashMap<(usize, u64), AppearanceStream>,
}

impl AppearanceCache {
    /// new.
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }
    /// get_or_generate.
    pub fn get_or_generate(
        &mut self,
        node_id: FormNodeId,
        value: &str,
        width: f64,
        height: f64,
        config: &AppearanceConfig,
    ) -> &AppearanceStream {
        let key = (node_id.0, simple_hash(value));
        self.cache
            .entry(key)
            .or_insert_with(|| field_appearance(value, width, height, config))
    }
    /// invalidate.
    pub fn invalidate(&mut self, node_id: FormNodeId) {
        self.cache.retain(|(id, _), _| *id != node_id.0);
    }
    /// clear.
    pub fn clear(&mut self) {
        self.cache.clear();
    }
}

impl Default for AppearanceCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate appearance streams for an entire layout.
pub fn generate_appearances(
    layout: &LayoutDom,
    config: &AppearanceConfig,
) -> Result<Vec<PageAppearances>> {
    let mut pages = Vec::new();
    for page in &layout.pages {
        let mut entries = Vec::new();
        collect_appearances(&page.nodes, 0.0, 0.0, config, &mut entries);
        pages.push(PageAppearances {
            width: page.width,
            height: page.height,
            entries,
        });
    }
    Ok(pages)
}
/// PageAppearances.

#[derive(Debug)]
pub struct PageAppearances {
    /// width.
    pub width: f64,
    /// height.
    pub height: f64,
    /// entries.
    pub entries: Vec<AppearanceEntry>,
}
/// AppearanceEntry.

#[derive(Debug)]
pub struct AppearanceEntry {
    /// name.
    pub name: String,
    /// abs_x.
    pub abs_x: f64,
    /// abs_y.
    pub abs_y: f64,
    /// appearance.
    pub appearance: AppearanceStream,
}

fn collect_appearances(
    nodes: &[LayoutNode],
    parent_x: f64,
    parent_y: f64,
    config: &AppearanceConfig,
    result: &mut Vec<AppearanceEntry>,
) {
    for node in nodes {
        let abs_x = node.rect.x + parent_x;
        let abs_y = node.rect.y + parent_y;
        let w = node.rect.width;
        let h = node.rect.height;

        let ap = match &node.content {
            LayoutContent::Field { value, .. } => Some(field_appearance(value, w, h, config)),
            LayoutContent::Text(text) => Some(draw_appearance(text, w, h, config)),
            LayoutContent::WrappedText {
                lines, font_size, ..
            } => Some(multiline_appearance(
                lines,
                *font_size,
                font_size * 1.2,
                w,
                h,
                config,
            )),
            LayoutContent::Image { .. } => None,
            LayoutContent::Draw(_) => None,
            LayoutContent::None => None,
        };
        if let Some(ap) = ap {
            result.push(AppearanceEntry {
                name: node.name.clone(),
                abs_x,
                abs_y,
                appearance: ap,
            });
        }
        if !node.children.is_empty() {
            collect_appearances(&node.children, abs_x, abs_y, config, result);
        }
    }
}
/// field_appearance.
pub fn field_appearance(
    value: &str,
    width: f64,
    height: f64,
    config: &AppearanceConfig,
) -> AppearanceStream {
    let mut ops = Vec::new();
    if let Some(bg) = &config.background_color {
        let _ = write!(
            ops,
            "{:.3} {:.3} {:.3} rg\n{:.2} {:.2} {:.2} {:.2} re\nf\n",
            bg[0], bg[1], bg[2], 0.0, 0.0, width, height
        );
    }
    if config.border_width > 0.0 {
        let _ = write!(
            ops,
            "{:.2} w\n{:.3} {:.3} {:.3} RG\n{:.2} {:.2} {:.2} {:.2} re\nS\n",
            config.border_width,
            config.border_color[0],
            config.border_color[1],
            config.border_color[2],
            0.0,
            0.0,
            width,
            height
        );
    }
    if !value.is_empty() {
        let fs = config.default_font_size;
        let p = config.text_padding;
        let _ = write!(
            ops,
            "BT\n{:.3} {:.3} {:.3} rg\n/F1 {:.1} Tf\n{:.2} {:.2} Td\n({}) Tj\nET\n",
            config.text_color[0],
            config.text_color[1],
            config.text_color[2],
            fs,
            p,
            height - fs - p,
            pdf_escape(value)
        );
        AppearanceStream {
            content: ops,
            bbox: [0.0, 0.0, width, height],
            font_resources: vec![("F1".to_string(), config.default_font.clone())],
        }
    } else {
        AppearanceStream {
            content: ops,
            bbox: [0.0, 0.0, width, height],
            font_resources: vec![],
        }
    }
}
/// draw_appearance.
pub fn draw_appearance(
    text: &str,
    width: f64,
    height: f64,
    config: &AppearanceConfig,
) -> AppearanceStream {
    let mut ops = Vec::new();
    if let Some(bg) = &config.background_color {
        let _ = write!(
            ops,
            "{:.3} {:.3} {:.3} rg\n{:.2} {:.2} {:.2} {:.2} re\nf\n",
            bg[0], bg[1], bg[2], 0.0, 0.0, width, height
        );
    }
    if !text.is_empty() {
        let fs = config.default_font_size;
        let p = config.text_padding;
        let _ = write!(
            ops,
            "BT\n{:.3} {:.3} {:.3} rg\n/F1 {:.1} Tf\n{:.2} {:.2} Td\n({}) Tj\nET\n",
            config.text_color[0],
            config.text_color[1],
            config.text_color[2],
            fs,
            p,
            height - fs - p,
            pdf_escape(text)
        );
        AppearanceStream {
            content: ops,
            bbox: [0.0, 0.0, width, height],
            font_resources: vec![("F1".to_string(), config.default_font.clone())],
        }
    } else {
        AppearanceStream {
            content: ops,
            bbox: [0.0, 0.0, width, height],
            font_resources: vec![],
        }
    }
}
/// multiline_appearance.
pub fn multiline_appearance(
    lines: &[String],
    font_size: f64,
    line_height: f64,
    width: f64,
    height: f64,
    config: &AppearanceConfig,
) -> AppearanceStream {
    let mut ops = Vec::new();
    if let Some(bg) = &config.background_color {
        let _ = write!(
            ops,
            "{:.3} {:.3} {:.3} rg\n{:.2} {:.2} {:.2} {:.2} re\nf\n",
            bg[0], bg[1], bg[2], 0.0, 0.0, width, height
        );
    }
    if config.border_width > 0.0 {
        let _ = write!(
            ops,
            "{:.2} w\n{:.3} {:.3} {:.3} RG\n{:.2} {:.2} {:.2} {:.2} re\nS\n",
            config.border_width,
            config.border_color[0],
            config.border_color[1],
            config.border_color[2],
            0.0,
            0.0,
            width,
            height
        );
    }
    if !lines.is_empty() {
        let p = config.text_padding;
        let _ = write!(
            ops,
            "BT\n{:.3} {:.3} {:.3} rg\n/F1 {:.1} Tf\n",
            config.text_color[0], config.text_color[1], config.text_color[2], font_size
        );
        let start_y = height - font_size - p;
        for (i, line) in lines.iter().enumerate() {
            let ay = start_y - (i as f64 * line_height);
            if ay < 0.0 {
                break;
            }
            if i == 0 {
                let _ = writeln!(ops, "{:.2} {:.2} Td", p, ay);
            } else {
                let _ = writeln!(ops, "{:.2} {:.2} Td", 0.0, -line_height);
            }
            let _ = writeln!(ops, "({}) Tj", pdf_escape(line));
        }
        ops.extend_from_slice(b"ET\n");
        AppearanceStream {
            content: ops,
            bbox: [0.0, 0.0, width, height],
            font_resources: vec![("F1".to_string(), config.default_font.clone())],
        }
    } else {
        AppearanceStream {
            content: ops,
            bbox: [0.0, 0.0, width, height],
            font_resources: vec![],
        }
    }
}
/// checkbox_appearance.
pub fn checkbox_appearance(checked: bool, width: f64, height: f64) -> AppearanceStream {
    let mut ops = Vec::new();
    let size = width.min(height);
    let _ = write!(
        ops,
        "0.50 w\n0.000 0.000 0.000 RG\n0.00 0.00 {:.2} {:.2} re\nS\n",
        size, size
    );
    if checked {
        let pad = size * 0.2;
        let _ = write!(ops,
            "1.50 w\n0.000 0.000 0.000 RG\n{:.2} {:.2} m\n{:.2} {:.2} l\nS\n{:.2} {:.2} m\n{:.2} {:.2} l\nS\n",
            pad, pad, size - pad, size - pad, size - pad, pad, pad, size - pad);
    }
    AppearanceStream {
        content: ops,
        bbox: [0.0, 0.0, size, size],
        font_resources: vec![],
    }
}

/// Apply XFA formatting patterns to a value.
///
/// Supports `num{...}` patterns per XFA Spec 3.3 §17.6:
/// - `z` = digit, suppress leading zeros
/// - `9` = digit, show leading zeros
/// - `.` = decimal separator
/// - `,` = grouping separator
/// - Literal text is passed through
pub fn format_value(value: &str, pattern: Option<&str>) -> String {
    let Some(pattern) = pattern else {
        return value.to_string();
    };
    if pattern.starts_with("num{") && pattern.ends_with('}') {
        let inner = &pattern[4..pattern.len() - 1];
        if let Ok(num) = value.parse::<f64>() {
            format_numeric(num, inner)
        } else {
            value.to_string()
        }
    } else {
        value.to_string()
    }
}

/// Default formatting for `numericEdit` fields without an explicit
/// `<format><picture>` pattern.  Strips trailing fractional zeros so
/// that data values like `"3.00000000"` display as `"3"` and
/// `"1.50"` displays as `"1.5"`.
pub fn format_numeric_default(value: &str) -> String {
    let trimmed = value.trim();
    let Ok(num) = trimmed.parse::<f64>() else {
        return value.to_string();
    };
    if num.fract() == 0.0 {
        // Integer — drop all decimals
        format!("{}", num as i64)
    } else {
        // Has meaningful decimals — strip trailing zeros
        let s = format!("{}", num);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Format a number according to an XFA numeric picture pattern.
fn format_numeric(num: f64, pattern: &str) -> String {
    let is_negative = num < 0.0;
    let abs_num = num.abs();

    // Split pattern on decimal point
    let (int_pat, dec_pat) = match pattern.find('.') {
        Some(pos) => (&pattern[..pos], Some(&pattern[pos + 1..])),
        None => (pattern, None),
    };

    // Determine decimal places from pattern
    let decimal_places = dec_pat
        .map(|d| d.chars().filter(|c| *c == '9' || *c == 'z').count())
        .unwrap_or(0);

    // Round the number to the required decimal places
    let factor = 10f64.powi(decimal_places as i32);
    let rounded = (abs_num * factor).round() / factor;

    // Split number into integer and fractional parts
    let int_part = rounded.trunc() as u64;
    let frac_part = ((rounded - rounded.trunc()) * factor).round() as u64;

    // Format integer part: get raw digits
    let int_str = int_part.to_string();

    // Collect digit positions from pattern
    let pat_digit_slots: Vec<char> = int_pat.chars().filter(|c| *c == 'z' || *c == '9').collect();
    let num_slots = pat_digit_slots.len();

    // Right-align digits into slots
    let padded_len = num_slots.max(int_str.len());
    let mut digits = vec![0u8; padded_len];
    for (i, b) in int_str.bytes().rev().enumerate() {
        digits[padded_len - 1 - i] = b - b'0';
    }

    // Build output by walking the pattern left-to-right
    let mut int_result = String::new();
    let mut seen_significant = false;

    // First emit any extra digits that overflow the pattern
    for d in digits.iter().take(padded_len.saturating_sub(num_slots)) {
        int_result.push((b'0' + d) as char);
        seen_significant = true;
    }

    let mut di = padded_len.saturating_sub(num_slots);
    for ch in int_pat.chars() {
        match ch {
            'z' => {
                let d = digits[di];
                di += 1;
                if d != 0 || seen_significant {
                    int_result.push((b'0' + d) as char);
                    seen_significant = true;
                }
            }
            '9' => {
                let d = digits[di];
                di += 1;
                int_result.push((b'0' + d) as char);
                seen_significant = true;
            }
            ',' => {
                // Only emit comma if we have seen significant digits
                // and there are more digits to come
                if seen_significant {
                    int_result.push(',');
                }
            }
            _ => int_result.push(ch),
        }
    }

    // Format decimal part
    let result_dec = if let Some(dp) = dec_pat {
        let frac_str = format!("{:0>width$}", frac_part, width = decimal_places);
        let frac_bytes: Vec<u8> = frac_str.bytes().map(|b| b - b'0').collect();
        let mut dec_result = String::new();
        let mut fi = 0;
        for ch in dp.chars() {
            match ch {
                '9' | 'z' => {
                    if fi < frac_bytes.len() {
                        dec_result.push((b'0' + frac_bytes[fi]) as char);
                        fi += 1;
                    } else {
                        dec_result.push('0');
                    }
                }
                _ => dec_result.push(ch),
            }
        }
        Some(dec_result)
    } else {
        None
    };

    // Assemble final result
    let mut result = String::new();
    if is_negative {
        result.push('-');
    }
    if int_result.is_empty() {
        result.push('0');
    } else {
        result.push_str(&int_result);
    }
    if let Some(dec) = result_dec {
        result.push('.');
        result.push_str(&dec);
    }
    result
}

fn pdf_escape(s: &str) -> String {
    let mut r = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '(' => r.push_str("\\("),
            ')' => r.push_str("\\)"),
            '\\' => r.push_str("\\\\"),
            _ => r.push(c),
        }
    }
    r
}

fn simple_hash(s: &str) -> u64 {
    let mut h: u64 = 5381;
    for b in s.bytes() {
        h = h.wrapping_mul(33).wrapping_add(b as u64);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_appearance_basic() {
        let config = AppearanceConfig::default();
        let ap = field_appearance("Hello", 100.0, 20.0, &config);
        let content = String::from_utf8_lossy(&ap.content);
        assert!(content.contains("(Hello) Tj"));
    }

    #[test]
    fn field_appearance_empty() {
        let config = AppearanceConfig::default();
        let ap = field_appearance("", 100.0, 20.0, &config);
        assert!(!String::from_utf8_lossy(&ap.content).contains("BT"));
    }

    #[test]
    fn cache_hit() {
        let mut cache = AppearanceCache::new();
        let config = AppearanceConfig::default();
        let _ = cache.get_or_generate(FormNodeId(0), "Hello", 100.0, 20.0, &config);
        let _ = cache.get_or_generate(FormNodeId(0), "Hello", 100.0, 20.0, &config);
        assert_eq!(cache.cache.len(), 1);
    }

    #[test]
    fn cache_invalidate() {
        let mut cache = AppearanceCache::new();
        let config = AppearanceConfig::default();
        let _ = cache.get_or_generate(FormNodeId(0), "A", 100.0, 20.0, &config);
        let _ = cache.get_or_generate(FormNodeId(1), "B", 100.0, 20.0, &config);
        cache.invalidate(FormNodeId(0));
        assert_eq!(cache.cache.len(), 1);
    }

    #[test]
    fn format_value_numeric() {
        assert_eq!(format_value("42.5", Some("num{zzz.99}")), "42.50");
        assert_eq!(format_value("hello", None), "hello");
        // Integer formatting: suppress trailing decimals
        assert_eq!(format_value("1.00000000", Some("num{z,zzz}")), "1");
        assert_eq!(format_value("2.00000000", Some("num{z,zzz}")), "2");
        assert_eq!(format_value("1234", Some("num{z,zzz}")), "1,234");
        assert_eq!(format_value("0", Some("num{z,zzz}")), "0");
        // Decimal formatting
        assert_eq!(format_value("3.14159", Some("num{z.99}")), "3.14");
        assert_eq!(format_value("0.5", Some("num{z.99}")), "0.50");
        // Leading-zero patterns
        assert_eq!(format_value("5", Some("num{999}")), "005");
        assert_eq!(format_value("42", Some("num{999}")), "042");
        // Negative
        assert_eq!(format_value("-7.5", Some("num{z.99}")), "-7.50");
        // Non-numeric value with num pattern
        assert_eq!(format_value("abc", Some("num{z,zzz}")), "abc");
    }

    #[test]
    fn checkbox_checked() {
        let ap = checkbox_appearance(true, 12.0, 12.0);
        let content = String::from_utf8_lossy(&ap.content);
        assert!(content.contains("re\nS"));
        assert!(content.contains("m\n"));
    }
}
