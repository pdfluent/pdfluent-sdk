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
    pub default_font: String,
    pub default_font_size: f64,
    pub border_width: f64,
    pub border_color: [f64; 3],
    pub background_color: Option<[f64; 3]>,
    pub text_color: [f64; 3],
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
    pub content: Vec<u8>,
    pub bbox: [f64; 4],
    pub font_resources: Vec<(String, String)>,
}

/// Appearance stream cache with invalidation.
pub struct AppearanceCache {
    cache: HashMap<(usize, u64), AppearanceStream>,
}

impl AppearanceCache {
    pub fn new() -> Self {
        Self { cache: HashMap::new() }
    }

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

    pub fn invalidate(&mut self, node_id: FormNodeId) {
        self.cache.retain(|(id, _), _| *id != node_id.0);
    }

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

#[derive(Debug)]
pub struct PageAppearances {
    pub width: f64,
    pub height: f64,
    pub entries: Vec<AppearanceEntry>,
}

#[derive(Debug)]
pub struct AppearanceEntry {
    pub name: String,
    pub abs_x: f64,
    pub abs_y: f64,
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
            LayoutContent::Field { value } => Some(field_appearance(value, w, h, config)),
            LayoutContent::Text(text) => Some(draw_appearance(text, w, h, config)),
            LayoutContent::WrappedText { lines, font_size } => {
                Some(multiline_appearance(lines, *font_size, font_size * 1.2, w, h, config))
            }
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

pub fn field_appearance(value: &str, width: f64, height: f64, config: &AppearanceConfig) -> AppearanceStream {
    let mut ops = Vec::new();
    if let Some(bg) = &config.background_color {
        let _ = write!(ops, "{:.3} {:.3} {:.3} rg\n{:.2} {:.2} {:.2} {:.2} re\nf\n",
            bg[0], bg[1], bg[2], 0.0, 0.0, width, height);
    }
    if config.border_width > 0.0 {
        let _ = write!(ops, "{:.2} w\n{:.3} {:.3} {:.3} RG\n{:.2} {:.2} {:.2} {:.2} re\nS\n",
            config.border_width, config.border_color[0], config.border_color[1],
            config.border_color[2], 0.0, 0.0, width, height);
    }
    if !value.is_empty() {
        let fs = config.default_font_size;
        let p = config.text_padding;
        let _ = write!(ops, "BT\n{:.3} {:.3} {:.3} rg\n/F1 {:.1} Tf\n{:.2} {:.2} Td\n({}) Tj\nET\n",
            config.text_color[0], config.text_color[1], config.text_color[2],
            fs, p, height - fs - p, pdf_escape(value));
        AppearanceStream { content: ops, bbox: [0.0, 0.0, width, height],
            font_resources: vec![("F1".to_string(), config.default_font.clone())] }
    } else {
        AppearanceStream { content: ops, bbox: [0.0, 0.0, width, height], font_resources: vec![] }
    }
}

pub fn draw_appearance(text: &str, width: f64, height: f64, config: &AppearanceConfig) -> AppearanceStream {
    let mut ops = Vec::new();
    if let Some(bg) = &config.background_color {
        let _ = write!(ops, "{:.3} {:.3} {:.3} rg\n{:.2} {:.2} {:.2} {:.2} re\nf\n",
            bg[0], bg[1], bg[2], 0.0, 0.0, width, height);
    }
    if !text.is_empty() {
        let fs = config.default_font_size;
        let p = config.text_padding;
        let _ = write!(ops, "BT\n{:.3} {:.3} {:.3} rg\n/F1 {:.1} Tf\n{:.2} {:.2} Td\n({}) Tj\nET\n",
            config.text_color[0], config.text_color[1], config.text_color[2],
            fs, p, height - fs - p, pdf_escape(text));
        AppearanceStream { content: ops, bbox: [0.0, 0.0, width, height],
            font_resources: vec![("F1".to_string(), config.default_font.clone())] }
    } else {
        AppearanceStream { content: ops, bbox: [0.0, 0.0, width, height], font_resources: vec![] }
    }
}

pub fn multiline_appearance(lines: &[String], font_size: f64, line_height: f64, width: f64, height: f64, config: &AppearanceConfig) -> AppearanceStream {
    let mut ops = Vec::new();
    if let Some(bg) = &config.background_color {
        let _ = write!(ops, "{:.3} {:.3} {:.3} rg\n{:.2} {:.2} {:.2} {:.2} re\nf\n",
            bg[0], bg[1], bg[2], 0.0, 0.0, width, height);
    }
    if config.border_width > 0.0 {
        let _ = write!(ops, "{:.2} w\n{:.3} {:.3} {:.3} RG\n{:.2} {:.2} {:.2} {:.2} re\nS\n",
            config.border_width, config.border_color[0], config.border_color[1],
            config.border_color[2], 0.0, 0.0, width, height);
    }
    if !lines.is_empty() {
        let p = config.text_padding;
        let _ = write!(ops, "BT\n{:.3} {:.3} {:.3} rg\n/F1 {:.1} Tf\n",
            config.text_color[0], config.text_color[1], config.text_color[2], font_size);
        let start_y = height - font_size - p;
        for (i, line) in lines.iter().enumerate() {
            let ay = start_y - (i as f64 * line_height);
            if ay < 0.0 { break; }
            if i == 0 {
                let _ = write!(ops, "{:.2} {:.2} Td\n", p, ay);
            } else {
                let _ = write!(ops, "{:.2} {:.2} Td\n", 0.0, -line_height);
            }
            let _ = write!(ops, "({}) Tj\n", pdf_escape(line));
        }
        ops.extend_from_slice(b"ET\n");
        AppearanceStream { content: ops, bbox: [0.0, 0.0, width, height],
            font_resources: vec![("F1".to_string(), config.default_font.clone())] }
    } else {
        AppearanceStream { content: ops, bbox: [0.0, 0.0, width, height], font_resources: vec![] }
    }
}

pub fn checkbox_appearance(checked: bool, width: f64, height: f64) -> AppearanceStream {
    let mut ops = Vec::new();
    let size = width.min(height);
    let _ = write!(ops, "0.50 w\n0.000 0.000 0.000 RG\n0.00 0.00 {:.2} {:.2} re\nS\n", size, size);
    if checked {
        let pad = size * 0.2;
        let _ = write!(ops,
            "1.50 w\n0.000 0.000 0.000 RG\n{:.2} {:.2} m\n{:.2} {:.2} l\nS\n{:.2} {:.2} m\n{:.2} {:.2} l\nS\n",
            pad, pad, size - pad, size - pad, size - pad, pad, pad, size - pad);
    }
    AppearanceStream { content: ops, bbox: [0.0, 0.0, size, size], font_resources: vec![] }
}

/// Apply XFA formatting patterns to a value.
pub fn format_value(value: &str, pattern: Option<&str>) -> String {
    let Some(pattern) = pattern else { return value.to_string(); };
    if pattern.starts_with("num{") && pattern.ends_with('}') {
        if let Ok(num) = value.parse::<f64>() {
            if num == num.floor() { format!("{}", num as i64) } else { format!("{:.2}", num) }
        } else { value.to_string() }
    } else { value.to_string() }
}

fn pdf_escape(s: &str) -> String {
    let mut r = String::with_capacity(s.len());
    for c in s.chars() {
        match c { '(' => r.push_str("\\("), ')' => r.push_str("\\)"), '\\' => r.push_str("\\\\"), _ => r.push(c) }
    }
    r
}

fn simple_hash(s: &str) -> u64 {
    let mut h: u64 = 5381;
    for b in s.bytes() { h = h.wrapping_mul(33).wrapping_add(b as u64); }
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
    }

    #[test]
    fn checkbox_checked() {
        let ap = checkbox_appearance(true, 12.0, 12.0);
        let content = String::from_utf8_lossy(&ap.content);
        assert!(content.contains("re\nS"));
        assert!(content.contains("m\n"));
    }
}
