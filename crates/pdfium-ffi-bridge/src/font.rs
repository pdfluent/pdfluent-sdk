//! Font loading and metrics — TrueType/OpenType font parsing for XFA rendering.
//!
//! Provides:
//! - Font loading from raw TrueType/OpenType data
//! - Per-glyph width measurement (replacing placeholder avg_char_width)
//! - System font directory scanning and fallback
//! - Font extraction from PDF embedded font streams
//! - Basic font subsetting (used glyphs only)
//!
//! Uses `ttf-parser` for zero-copy font parsing.

use crate::error::{PdfError, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A loaded font with metrics available for text measurement.
#[derive(Debug)]
pub struct LoadedFont {
    /// Font family name.
    pub family: String,
    /// PostScript name (used in PDF font references).
    pub postscript_name: Option<String>,
    /// Whether this is a bold variant.
    pub is_bold: bool,
    /// Whether this is an italic variant.
    pub is_italic: bool,
    /// Units per em (for scaling glyph metrics to points).
    pub units_per_em: u16,
    /// Ascender value in font units.
    pub ascender: i16,
    /// Descender value in font units (typically negative).
    pub descender: i16,
    /// Line gap in font units.
    pub line_gap: i16,
    /// Number of glyphs in the font.
    pub glyph_count: u16,
    /// Glyph advance widths indexed by glyph ID.
    glyph_widths: Vec<u16>,
    /// Character to glyph ID mapping (cmap).
    cmap: HashMap<u32, u16>,
    /// Raw font data (kept for subsetting).
    raw_data: Vec<u8>,
}

impl LoadedFont {
    /// Load a font from raw TrueType/OpenType data.
    pub fn from_data(data: Vec<u8>) -> Result<Self> {
        let face = ttf_parser::Face::parse(&data, 0)
            .map_err(|e| PdfError::FontError(format!("Failed to parse font: {e}")))?;

        let family = face
            .names()
            .into_iter()
            .find(|n| n.name_id == ttf_parser::name_id::FAMILY)
            .and_then(|n| n.to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let postscript_name = face
            .names()
            .into_iter()
            .find(|n| n.name_id == ttf_parser::name_id::POST_SCRIPT_NAME)
            .and_then(|n| n.to_string());

        let is_bold = face.is_bold();
        let is_italic = face.is_italic();
        let units_per_em = face.units_per_em();
        let ascender = face.ascender();
        let descender = face.descender();
        let line_gap = face.line_gap();
        let glyph_count = face.number_of_glyphs();

        // Extract glyph widths
        let mut glyph_widths = Vec::with_capacity(glyph_count as usize);
        for gid in 0..glyph_count {
            let width = face
                .glyph_hor_advance(ttf_parser::GlyphId(gid))
                .unwrap_or(0);
            glyph_widths.push(width);
        }

        // Build cmap (character → glyph ID)
        let mut cmap = HashMap::new();
        if let Some(subtable) = face.tables().cmap {
            for subtable in subtable.subtables {
                if !subtable.is_unicode() {
                    continue;
                }
                subtable.codepoints(|cp| {
                    if let Some(gid) = subtable.glyph_index(cp) {
                        cmap.insert(cp, gid.0);
                    }
                });
            }
        }

        Ok(Self {
            family,
            postscript_name,
            is_bold,
            is_italic,
            units_per_em,
            ascender,
            descender,
            line_gap,
            glyph_count,
            glyph_widths,
            cmap,
            raw_data: data,
        })
    }

    /// Load a font from a file path.
    pub fn from_file(path: &Path) -> Result<Self> {
        let data = std::fs::read(path)
            .map_err(|e| PdfError::FontError(format!("Failed to read font file: {e}")))?;
        Self::from_data(data)
    }

    /// Get the glyph ID for a Unicode code point.
    pub fn glyph_id(&self, codepoint: u32) -> Option<u16> {
        self.cmap.get(&codepoint).copied()
    }

    /// Get the advance width of a glyph in font units.
    pub fn glyph_advance(&self, glyph_id: u16) -> u16 {
        self.glyph_widths
            .get(glyph_id as usize)
            .copied()
            .unwrap_or(0)
    }

    /// Get the advance width of a character in font units.
    pub fn char_advance(&self, ch: char) -> u16 {
        self.glyph_id(ch as u32)
            .map(|gid| self.glyph_advance(gid))
            .unwrap_or_else(|| {
                // Fallback: use average width or notdef glyph
                self.glyph_advance(0)
            })
    }

    /// Measure the width of a string in points at the given font size.
    pub fn measure_string(&self, text: &str, font_size: f64) -> f64 {
        let total_units: u32 = text.chars().map(|ch| self.char_advance(ch) as u32).sum();
        (total_units as f64 / self.units_per_em as f64) * font_size
    }

    /// Get the line height in points at the given font size.
    pub fn line_height(&self, font_size: f64) -> f64 {
        let total = self.ascender as f64 - self.descender as f64 + self.line_gap as f64;
        (total / self.units_per_em as f64) * font_size
    }

    /// Get the ascender in points at the given font size.
    pub fn ascender_pt(&self, font_size: f64) -> f64 {
        (self.ascender as f64 / self.units_per_em as f64) * font_size
    }

    /// Get the descender in points at the given font size (negative).
    pub fn descender_pt(&self, font_size: f64) -> f64 {
        (self.descender as f64 / self.units_per_em as f64) * font_size
    }

    /// Get the set of glyph IDs used by the given text.
    pub fn used_glyphs(&self, text: &str) -> Vec<u16> {
        let mut glyphs: Vec<u16> = text
            .chars()
            .filter_map(|ch| self.glyph_id(ch as u32))
            .collect();
        glyphs.sort_unstable();
        glyphs.dedup();
        // Always include .notdef (glyph 0)
        if glyphs.first() != Some(&0) {
            glyphs.insert(0, 0);
        }
        glyphs
    }

    /// Get glyph widths scaled to 1000 units (PDF convention).
    ///
    /// PDF font descriptors use a 1000-unit em square by convention.
    /// This returns widths scaled from the font's native units_per_em.
    pub fn pdf_glyph_widths(&self, glyph_ids: &[u16]) -> Vec<u16> {
        let scale = 1000.0 / self.units_per_em as f64;
        glyph_ids
            .iter()
            .map(|&gid| {
                let w = self.glyph_advance(gid);
                (w as f64 * scale).round() as u16
            })
            .collect()
    }

    /// Get a reference to the raw font data.
    pub fn raw_data(&self) -> &[u8] {
        &self.raw_data
    }

    /// Check if the font contains a specific character.
    pub fn has_char(&self, ch: char) -> bool {
        self.cmap.contains_key(&(ch as u32))
    }
}

/// Font resolver — finds and loads fonts by name.
#[derive(Debug)]
pub struct FontResolver {
    /// Cached loaded fonts.
    fonts: HashMap<String, LoadedFont>,
    /// System font directories.
    font_dirs: Vec<PathBuf>,
    /// Font file index: family name → file path.
    font_index: HashMap<String, PathBuf>,
    /// Whether the index has been built.
    indexed: bool,
}

impl FontResolver {
    /// Create a new font resolver with platform-specific font directories.
    pub fn new() -> Self {
        let font_dirs = system_font_dirs();
        Self {
            fonts: HashMap::new(),
            font_dirs,
            font_index: HashMap::new(),
            indexed: false,
        }
    }

    /// Add a custom font directory.
    pub fn add_font_dir(&mut self, dir: PathBuf) {
        self.font_dirs.push(dir);
        self.indexed = false; // Invalidate index
    }

    /// Register a font directly from data.
    pub fn register_font(&mut self, data: Vec<u8>) -> Result<String> {
        let font = LoadedFont::from_data(data)?;
        let key = font_cache_key(&font.family, font.is_bold, font.is_italic);
        let family = font.family.clone();
        self.fonts.insert(key, font);
        Ok(family)
    }

    /// Resolve a font by family name and style.
    ///
    /// Tries (in order):
    /// 1. Already-loaded fonts cache
    /// 2. Font file index (lazy-built from system dirs)
    /// 3. Fallback to a default sans-serif font
    pub fn resolve(&mut self, family: &str, bold: bool, italic: bool) -> Option<&LoadedFont> {
        let key = font_cache_key(family, bold, italic);

        // Check cache first
        if self.fonts.contains_key(&key) {
            return self.fonts.get(&key);
        }

        // Build index if needed
        if !self.indexed {
            self.build_index();
        }

        // Try exact match in index
        if let Some(path) = self.font_index.get(&key).cloned() {
            if let Ok(font) = LoadedFont::from_file(&path) {
                self.fonts.insert(key.clone(), font);
                return self.fonts.get(&key);
            }
        }

        // Try without style
        let base_key = font_cache_key(family, false, false);
        if let Some(path) = self.font_index.get(&base_key).cloned() {
            if let Ok(font) = LoadedFont::from_file(&path) {
                self.fonts.insert(key.clone(), font);
                return self.fonts.get(&key);
            }
        }

        // Try common fallbacks
        for fallback in &["Helvetica", "Arial", "DejaVu Sans", "Liberation Sans"] {
            let fallback_key = font_cache_key(fallback, bold, italic);
            if self.fonts.contains_key(&fallback_key) {
                return self.fonts.get(&fallback_key);
            }
            if let Some(path) = self.font_index.get(&fallback_key).cloned() {
                if let Ok(font) = LoadedFont::from_file(&path) {
                    self.fonts.insert(fallback_key.clone(), font);
                    return self.fonts.get(&fallback_key);
                }
            }
        }

        None
    }

    /// Build the font file index from system directories (recursive).
    fn build_index(&mut self) {
        let dirs = self.font_dirs.clone();
        for dir in &dirs {
            self.scan_dir_recursive(dir);
        }
        self.indexed = true;
    }

    /// Recursively scan a directory for font files.
    fn scan_dir_recursive(&mut self, dir: &Path) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                self.scan_dir_recursive(&path);
            } else if is_font_file(&path) {
                if let Ok(font) = LoadedFont::from_file(&path) {
                    let key = font_cache_key(&font.family, font.is_bold, font.is_italic);
                    self.font_index.entry(key).or_insert(path);
                }
            }
        }
    }
}

impl Default for FontResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract embedded font data from a PDF document.
///
/// Scans the PDF's font resources for TrueType/OpenType font streams.
/// Returns a list of (font_name, font_data) pairs.
pub fn extract_pdf_fonts(reader: &crate::pdf_reader::PdfReader) -> Vec<(String, Vec<u8>)> {
    reader.extract_font_data()
}

/// Get platform-specific system font directories.
fn system_font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from("/System/Library/Fonts"));
        dirs.push(PathBuf::from("/Library/Fonts"));
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(PathBuf::from(format!("{home}/Library/Fonts")));
        }
    }

    #[cfg(target_os = "linux")]
    {
        dirs.push(PathBuf::from("/usr/share/fonts"));
        dirs.push(PathBuf::from("/usr/local/share/fonts"));
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(PathBuf::from(format!("{home}/.fonts")));
            dirs.push(PathBuf::from(format!("{home}/.local/share/fonts")));
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(windir) = std::env::var("WINDIR") {
            dirs.push(PathBuf::from(format!("{windir}\\Fonts")));
        }
        if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
            dirs.push(PathBuf::from(format!(
                "{localappdata}\\Microsoft\\Windows\\Fonts"
            )));
        }
    }

    dirs
}

/// Generate a cache key for a font.
fn font_cache_key(family: &str, bold: bool, italic: bool) -> String {
    let style = match (bold, italic) {
        (true, true) => "BoldItalic",
        (true, false) => "Bold",
        (false, true) => "Italic",
        (false, false) => "Regular",
    };
    format!("{}:{}", family.to_lowercase(), style)
}

/// Check if a file path is a font file.
fn is_font_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_lowercase().as_str(),
                "ttf" | "otf" | "ttc" | "otc" | "woff" | "woff2"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_font_data() -> Vec<u8> {
        // Try to find a system font that has Latin characters
        for dir in system_font_dirs() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().is_some_and(|e| e == "ttf") {
                        if let Ok(data) = std::fs::read(&path) {
                            // Verify this font has basic Latin glyphs
                            if let Ok(face) = ttf_parser::Face::parse(&data, 0) {
                                if face.glyph_index('A').is_some()
                                    && face.glyph_index('0').is_some()
                                {
                                    return data;
                                }
                            }
                        }
                    }
                }
            }
        }
        panic!("No system TrueType font with Latin glyphs found for testing");
    }

    #[test]
    fn load_system_font() {
        let data = test_font_data();
        let font = LoadedFont::from_data(data).unwrap();
        assert!(!font.family.is_empty());
        assert!(font.units_per_em > 0);
        assert!(font.glyph_count > 0);
    }

    #[test]
    fn measure_string_width() {
        let data = test_font_data();
        let font = LoadedFont::from_data(data).unwrap();
        let width = font.measure_string("Hello", 12.0);
        assert!(width > 0.0, "String width should be positive");
    }

    #[test]
    fn char_advance_nonzero() {
        let data = test_font_data();
        let font = LoadedFont::from_data(data).unwrap();
        let advance = font.char_advance('A');
        assert!(advance > 0, "Advance for 'A' should be non-zero");
    }

    #[test]
    fn line_height_positive() {
        let data = test_font_data();
        let font = LoadedFont::from_data(data).unwrap();
        let lh = font.line_height(12.0);
        assert!(lh > 0.0, "Line height should be positive");
    }

    #[test]
    fn used_glyphs_includes_notdef() {
        let data = test_font_data();
        let font = LoadedFont::from_data(data).unwrap();
        let glyphs = font.used_glyphs("AB");
        assert_eq!(glyphs[0], 0, "First glyph should be .notdef");
        assert!(glyphs.len() >= 3, "Should have .notdef + A + B");
    }

    #[test]
    fn pdf_glyph_widths_scaled() {
        let data = test_font_data();
        let font = LoadedFont::from_data(data).unwrap();
        let glyphs = font.used_glyphs("W");
        let widths = font.pdf_glyph_widths(&glyphs);
        // 'W' is typically one of the widest glyphs
        for w in &widths {
            assert!(*w <= 2000, "Width should be reasonable in 1000-unit em");
        }
    }

    #[test]
    fn has_char_basic() {
        let data = test_font_data();
        let font = LoadedFont::from_data(data).unwrap();
        assert!(font.has_char('A'), "Should have 'A'");
        assert!(font.has_char('0'), "Should have '0'");
    }

    #[test]
    fn font_cache_key_format() {
        assert_eq!(font_cache_key("Arial", false, false), "arial:Regular");
        assert_eq!(font_cache_key("Arial", true, false), "arial:Bold");
        assert_eq!(font_cache_key("Arial", false, true), "arial:Italic");
        assert_eq!(font_cache_key("Arial", true, true), "arial:BoldItalic");
    }

    #[test]
    fn system_font_dirs_not_empty() {
        let dirs = system_font_dirs();
        assert!(!dirs.is_empty(), "Should have at least one font directory");
    }

    #[test]
    fn font_resolver_new() {
        let resolver = FontResolver::new();
        assert!(!resolver.font_dirs.is_empty());
    }
}
