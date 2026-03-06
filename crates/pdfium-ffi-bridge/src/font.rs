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
use lopdf::Object;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The outline format of a font program.
///
/// PDF requires different font stream types depending on whether the font
/// contains TrueType (`glyf`) or CFF outlines. Using the wrong subtype
/// causes PDF/A validation failures and incorrect text rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontOutlineType {
    /// TrueType outlines (`glyf` table present).
    /// Embedded via `/FontFile2` with `/Subtype /TrueType`.
    TrueType,
    /// CFF outlines (`CFF ` or `CFF2` table present).
    /// Embedded via `/FontFile3` with `/Subtype /OpenType` (CIDFont)
    /// or `/Subtype /Type1C` (simple font).
    Cff,
}

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
    /// The outline format (TrueType vs CFF).
    pub outline_type: FontOutlineType,
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
        Self::from_data_at(data, 0)
    }

    /// Load a specific face from raw font data (for TTC/OTC collections).
    pub fn from_data_at(data: Vec<u8>, face_index: u32) -> Result<Self> {
        let face = ttf_parser::Face::parse(&data, face_index)
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

        // Detect outline type: TrueType fonts have a `glyf` table,
        // CFF/OpenType fonts have a `CFF ` or `CFF2` table instead.
        let outline_type = if face.tables().glyf.is_some() {
            FontOutlineType::TrueType
        } else {
            // CFF, CFF2, or unknown — treat as CFF for PDF embedding purposes.
            FontOutlineType::Cff
        };

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
            outline_type,
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

/// Result of font embedding validation for PDF/A compliance.
#[derive(Debug)]
pub struct FontValidationReport {
    /// Fonts that are referenced but not embedded.
    pub missing_fonts: Vec<String>,
    /// Fonts that are embedded but have missing glyphs for used text.
    pub incomplete_fonts: Vec<FontGlyphIssue>,
}

/// A font with missing glyphs.
#[derive(Debug)]
pub struct FontGlyphIssue {
    /// Font name.
    pub font_name: String,
    /// Characters that lack glyphs in the embedded font.
    pub missing_chars: Vec<char>,
}

impl FontValidationReport {
    /// Whether all fonts pass PDF/A validation.
    pub fn is_valid(&self) -> bool {
        self.missing_fonts.is_empty() && self.incomplete_fonts.is_empty()
    }
}

/// Validate font embedding for PDF/A compliance.
///
/// PDF/A requires all fonts to be embedded with all used glyphs present.
/// This function checks every font referenced in the document:
/// 1. Verifies a FontFile stream exists in the FontDescriptor
/// 2. Parses the embedded font data and checks glyph coverage
pub fn validate_font_embedding(doc: &lopdf::Document) -> FontValidationReport {
    let mut missing_fonts = Vec::new();
    let mut incomplete_fonts = Vec::new();

    for obj in doc.objects.values() {
        let dict = match obj {
            Object::Dictionary(d) => d,
            Object::Stream(s) => &s.dict,
            _ => continue,
        };

        // Look for Font dictionaries (not FontDescriptors).
        let is_font = dict
            .get(b"Type")
            .ok()
            .and_then(|t| t.as_name().ok())
            .is_some_and(|n| n == b"Font");
        if !is_font {
            continue;
        }

        let base_font = dict
            .get(b"BaseFont")
            .ok()
            .and_then(|n| n.as_name().ok())
            .map(|n| String::from_utf8_lossy(n).to_string())
            .unwrap_or_default();

        // Standard 14 fonts don't need embedding (but PDF/A technically requires it;
        // we flag them as missing so the caller can decide).
        let font_descriptor_ref = match dict.get(b"FontDescriptor") {
            Ok(Object::Reference(r)) => *r,
            _ => {
                // No FontDescriptor → likely a standard font or Type0 CID font.
                // Type1 standard fonts without descriptors are not embedded.
                let subtype = dict
                    .get(b"Subtype")
                    .ok()
                    .and_then(|s| s.as_name().ok())
                    .unwrap_or(b"");
                if (subtype == b"Type1" || subtype == b"TrueType") && !base_font.is_empty() {
                    missing_fonts.push(base_font);
                }
                continue;
            }
        };

        let descriptor = match doc.get_object(font_descriptor_ref) {
            Ok(Object::Dictionary(d)) => d,
            Ok(Object::Stream(s)) => &s.dict,
            _ => continue,
        };

        // Check for embedded font data.
        let has_font_file = [b"FontFile".as_slice(), b"FontFile2", b"FontFile3"]
            .iter()
            .any(|key| {
                matches!(
                    descriptor.get(key),
                    Ok(Object::Reference(_)) | Ok(Object::Stream(_))
                )
            });

        if !has_font_file && !base_font.is_empty() {
            missing_fonts.push(base_font.clone());
            continue;
        }

        // If embedded, try to parse and check glyph coverage.
        // We only check if there's a ToUnicode CMap (meaning we know what chars are used).
        if let Ok(Object::Reference(tounicode_ref)) = dict.get(b"ToUnicode") {
            if let Ok(Object::Stream(cmap_stream)) = doc.get_object(*tounicode_ref) {
                let chars = extract_unicode_from_cmap(&cmap_stream.content);
                if !chars.is_empty() {
                    // Try to load the embedded font and check coverage.
                    let font_data = extract_font_file(doc, descriptor);
                    if let Some(data) = font_data {
                        if let Ok(loaded) = LoadedFont::from_data(data) {
                            let missing: Vec<char> =
                                chars.into_iter().filter(|c| !loaded.has_char(*c)).collect();
                            if !missing.is_empty() {
                                incomplete_fonts.push(FontGlyphIssue {
                                    font_name: base_font,
                                    missing_chars: missing,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    FontValidationReport {
        missing_fonts,
        incomplete_fonts,
    }
}

/// Extract font file data from a FontDescriptor.
fn extract_font_file(doc: &lopdf::Document, descriptor: &lopdf::Dictionary) -> Option<Vec<u8>> {
    for key in &[b"FontFile2".as_slice(), b"FontFile3", b"FontFile"] {
        if let Ok(Object::Reference(r)) = descriptor.get(key) {
            if let Ok(Object::Stream(s)) = doc.get_object(*r) {
                let mut stream = s.clone();
                let _ = stream.decompress();
                return Some(stream.content.clone());
            }
        }
    }
    None
}

/// Extract Unicode characters from a ToUnicode CMap stream (best-effort).
///
/// Parses `beginbfchar` / `endbfchar` sections to find mapped characters.
fn extract_unicode_from_cmap(cmap_data: &[u8]) -> Vec<char> {
    let text = String::from_utf8_lossy(cmap_data);
    let mut chars = Vec::new();

    let mut in_bfchar = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.contains("beginbfchar") {
            in_bfchar = true;
            continue;
        }
        if trimmed.contains("endbfchar") {
            in_bfchar = false;
            continue;
        }
        if in_bfchar {
            // Lines look like: <0041> <0041>  (src → unicode)
            let parts: Vec<&str> = trimmed.split('>').collect();
            if parts.len() >= 2 {
                let hex = parts[1].trim().trim_start_matches('<');
                if let Ok(cp) = u32::from_str_radix(hex, 16) {
                    if let Some(ch) = char::from_u32(cp) {
                        chars.push(ch);
                    }
                }
            }
        }
    }

    chars
}

/// Font resolver — finds and loads fonts by name.
#[derive(Debug)]
pub struct FontResolver {
    /// Cached loaded fonts.
    fonts: HashMap<String, LoadedFont>,
    /// System font directories.
    font_dirs: Vec<PathBuf>,
    /// Font file index: family name → (file path, face index).
    font_index: HashMap<String, (PathBuf, u32)>,
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
        if let Some((path, face_idx)) = self.font_index.get(&key).cloned() {
            if let Ok(font) = load_font_at(&path, face_idx) {
                self.fonts.insert(key.clone(), font);
                return self.fonts.get(&key);
            }
        }

        // Try without style
        let base_key = font_cache_key(family, false, false);
        if let Some((path, face_idx)) = self.font_index.get(&base_key).cloned() {
            if let Ok(font) = load_font_at(&path, face_idx) {
                self.fonts.insert(key.clone(), font);
                return self.fonts.get(&key);
            }
        }

        // Try common fallbacks (with requested style first, then regular)
        for fallback in &["Helvetica", "Arial", "DejaVu Sans", "Liberation Sans"] {
            for (fb_bold, fb_italic) in [(bold, italic), (false, false)] {
                let fallback_key = font_cache_key(fallback, fb_bold, fb_italic);
                if self.fonts.contains_key(&fallback_key) {
                    return self.fonts.get(&fallback_key);
                }
                if let Some((path, face_idx)) = self.font_index.get(&fallback_key).cloned() {
                    if let Ok(font) = load_font_at(&path, face_idx) {
                        self.fonts.insert(fallback_key.clone(), font);
                        return self.fonts.get(&fallback_key);
                    }
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
                if is_font_collection(&path) {
                    // TTC/OTC: index all faces in the collection
                    if let Ok(data) = std::fs::read(&path) {
                        for face_index in 0..u32::MAX {
                            match ttf_parser::Face::parse(&data, face_index) {
                                Ok(face) => {
                                    let family = face
                                        .names()
                                        .into_iter()
                                        .find(|n| n.name_id == ttf_parser::name_id::FAMILY)
                                        .and_then(|n| n.to_string())
                                        .unwrap_or_else(|| "Unknown".to_string());
                                    let key =
                                        font_cache_key(&family, face.is_bold(), face.is_italic());
                                    self.font_index
                                        .entry(key)
                                        .or_insert_with(|| (path.clone(), face_index));
                                }
                                Err(_) => break,
                            }
                        }
                    }
                } else if let Ok(font) = LoadedFont::from_file(&path) {
                    let key = font_cache_key(&font.family, font.is_bold, font.is_italic);
                    self.font_index.entry(key).or_insert((path, 0));
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

/// Load a font from a file at a specific face index.
fn load_font_at(path: &Path, face_index: u32) -> Result<LoadedFont> {
    let data = std::fs::read(path)
        .map_err(|e| PdfError::FontError(format!("Failed to read font file: {e}")))?;
    LoadedFont::from_data_at(data, face_index)
}

/// Check if a font file is a TTC/OTC collection.
fn is_font_collection(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| matches!(ext.to_lowercase().as_str(), "ttc" | "otc"))
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

    fn try_test_font_data() -> Option<Vec<u8>> {
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
                                    return Some(data);
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    #[test]
    fn load_system_font() {
        let Some(data) = try_test_font_data() else {
            return;
        };
        let font = LoadedFont::from_data(data).unwrap();
        assert!(!font.family.is_empty());
        assert!(font.units_per_em > 0);
        assert!(font.glyph_count > 0);
    }

    #[test]
    fn measure_string_width() {
        let Some(data) = try_test_font_data() else {
            return;
        };
        let font = LoadedFont::from_data(data).unwrap();
        let width = font.measure_string("Hello", 12.0);
        assert!(width > 0.0, "String width should be positive");
    }

    #[test]
    fn char_advance_nonzero() {
        let Some(data) = try_test_font_data() else {
            return;
        };
        let font = LoadedFont::from_data(data).unwrap();
        let advance = font.char_advance('A');
        assert!(advance > 0, "Advance for 'A' should be non-zero");
    }

    #[test]
    fn line_height_positive() {
        let Some(data) = try_test_font_data() else {
            return;
        };
        let font = LoadedFont::from_data(data).unwrap();
        let lh = font.line_height(12.0);
        assert!(lh > 0.0, "Line height should be positive");
    }

    #[test]
    fn used_glyphs_includes_notdef() {
        let Some(data) = try_test_font_data() else {
            return;
        };
        let font = LoadedFont::from_data(data).unwrap();
        let glyphs = font.used_glyphs("AB");
        assert_eq!(glyphs[0], 0, "First glyph should be .notdef");
        assert!(glyphs.len() >= 3, "Should have .notdef + A + B");
    }

    #[test]
    fn pdf_glyph_widths_scaled() {
        let Some(data) = try_test_font_data() else {
            return;
        };
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
        let Some(data) = try_test_font_data() else {
            return;
        };
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
