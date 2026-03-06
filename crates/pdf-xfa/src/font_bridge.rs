//! XFA font resolver: maps XFA font specifications to system/embedded fonts.
//!
//! Resolves fonts from XFA template declarations using:
//! 1. Fonts embedded in the source PDF (via extract_embedded_fonts)
//! 2. System fonts found on disk
//! 3. Common fallback fonts (Helvetica, DejaVu Sans, Liberation Sans)

use crate::error::{Result, XfaError};
use std::collections::HashMap;
use std::path::PathBuf;

/// A resolved font ready for use in PDF rendering.
#[derive(Debug, Clone)]
pub struct ResolvedFont {
    /// PostScript name or family name.
    pub name: String,
    /// Raw font data (TTF/OTF/CFF bytes).
    pub data: Vec<u8>,
    /// Font face index within a collection (TTC/OTC).
    pub face_index: u32,
    /// Units per em from the font's head table.
    pub units_per_em: u16,
    /// Ascender in font units.
    pub ascender: i16,
    /// Descender in font units (negative).
    pub descender: i16,
}

impl ResolvedFont {
    /// Measure the approximate width of a string in points at the given font size.
    pub fn measure_string(&self, text: &str, font_size: f64) -> f64 {
        if let Ok(face) = ttf_parser::Face::parse(&self.data, self.face_index) {
            let upem = face.units_per_em() as f64;
            let scale = font_size / upem;
            let mut width = 0.0;
            for ch in text.chars() {
                if let Some(gid) = face.glyph_index(ch) {
                    width += face.glyph_hor_advance(gid).unwrap_or(0) as f64 * scale;
                } else {
                    width += font_size * 0.5;
                }
            }
            width
        } else {
            text.len() as f64 * font_size * 0.5
        }
    }

    /// Line height in points at the given font size.
    pub fn line_height(&self, font_size: f64) -> f64 {
        let upem = self.units_per_em as f64;
        if upem > 0.0 {
            (self.ascender as f64 - self.descender as f64) / upem * font_size
        } else {
            font_size * 1.2
        }
    }

    /// Ascender in points at the given font size.
    pub fn ascender_pt(&self, font_size: f64) -> f64 {
        let upem = self.units_per_em as f64;
        if upem > 0.0 {
            self.ascender as f64 / upem * font_size
        } else {
            font_size * 0.8
        }
    }

    /// Descender in points at the given font size (negative value).
    pub fn descender_pt(&self, font_size: f64) -> f64 {
        let upem = self.units_per_em as f64;
        if upem > 0.0 {
            self.descender as f64 / upem * font_size
        } else {
            font_size * -0.2
        }
    }

    /// Generate PDF glyph widths array for embedding.
    pub fn pdf_glyph_widths(&self) -> (u16, Vec<u16>) {
        if let Ok(face) = ttf_parser::Face::parse(&self.data, self.face_index) {
            let upem = face.units_per_em() as f64;
            let scale = 1000.0 / upem;
            let mut widths = Vec::with_capacity(256);
            for code in 0u16..256 {
                let w = if let Some(gid) = face.glyph_index(char::from(code as u8)) {
                    (face.glyph_hor_advance(gid).unwrap_or(0) as f64 * scale) as u16
                } else {
                    0
                };
                widths.push(w);
            }
            (0, widths)
        } else {
            (0, vec![500; 256])
        }
    }
}

/// XFA font specification from the template.
#[derive(Debug, Clone)]
pub struct XfaFontSpec {
    pub typeface: String,
    pub weight: FontWeight,
    pub posture: FontPosture,
    pub size_pt: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontWeight {
    Normal,
    Bold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontPosture {
    Normal,
    Italic,
}

impl XfaFontSpec {
    /// Parse a font specification from XFA template attributes.
    pub fn from_xfa_attrs(
        typeface: &str,
        weight: Option<&str>,
        posture: Option<&str>,
        size: Option<&str>,
    ) -> Self {
        Self {
            typeface: typeface.to_string(),
            weight: match weight {
                Some("bold") => FontWeight::Bold,
                _ => FontWeight::Normal,
            },
            posture: match posture {
                Some("italic") => FontPosture::Italic,
                _ => FontPosture::Normal,
            },
            size_pt: size
                .and_then(|s| s.strip_suffix("pt").or(Some(s)))
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(10.0),
        }
    }
}

/// Resolves XFA font specifications to actual font data.
pub struct XfaFontResolver {
    embedded: HashMap<String, ResolvedFont>,
    system_fonts: HashMap<String, PathBuf>,
    cache: HashMap<String, ResolvedFont>,
}

impl XfaFontResolver {
    /// Create a new resolver with embedded fonts extracted from the PDF.
    pub fn new(embedded_fonts: Vec<(String, Vec<u8>)>) -> Self {
        let mut embedded = HashMap::new();
        for (name, data) in embedded_fonts {
            if let Some(font) = parse_font_data(&name, &data) {
                embedded.insert(name.to_lowercase(), font);
            }
        }
        let system_fonts = scan_system_fonts();
        Self {
            embedded,
            system_fonts,
            cache: HashMap::new(),
        }
    }

    /// Resolve a font specification to a usable font.
    pub fn resolve(&mut self, spec: &XfaFontSpec) -> Result<ResolvedFont> {
        let cache_key = format!("{}_{:?}_{:?}", spec.typeface, spec.weight, spec.posture);
        if let Some(cached) = self.cache.get(&cache_key) {
            return Ok(cached.clone());
        }
        let font = self
            .try_embedded(&spec.typeface)
            .or_else(|| self.try_system(&spec.typeface))
            .or_else(|| self.try_base_name(&spec.typeface))
            .or_else(|| self.try_fallbacks())
            .ok_or_else(|| {
                XfaError::FontError(format!("cannot resolve font: {}", spec.typeface))
            })?;
        self.cache.insert(cache_key, font.clone());
        Ok(font)
    }

    fn try_embedded(&self, name: &str) -> Option<ResolvedFont> {
        self.embedded.get(&name.to_lowercase()).cloned()
    }

    fn try_system(&self, name: &str) -> Option<ResolvedFont> {
        let path = self.system_fonts.get(&name.to_lowercase())?;
        load_system_font(path, name)
    }

    fn try_base_name(&self, name: &str) -> Option<ResolvedFont> {
        let base = name
            .replace("-Bold", "")
            .replace("-Italic", "")
            .replace("-BoldItalic", "")
            .replace(",Bold", "")
            .replace(",Italic", "");
        if base != name {
            self.try_embedded(&base).or_else(|| self.try_system(&base))
        } else {
            None
        }
    }

    fn try_fallbacks(&self) -> Option<ResolvedFont> {
        for name in &["Helvetica", "Arial", "DejaVuSans", "LiberationSans"] {
            if let Some(font) = self.try_system(name) {
                return Some(font);
            }
        }
        None
    }
}

fn parse_font_data(name: &str, data: &[u8]) -> Option<ResolvedFont> {
    let face = ttf_parser::Face::parse(data, 0).ok()?;
    Some(ResolvedFont {
        name: name.to_string(),
        data: data.to_vec(),
        face_index: 0,
        units_per_em: face.units_per_em(),
        ascender: face.ascender(),
        descender: face.descender(),
    })
}

fn load_system_font(path: &PathBuf, name: &str) -> Option<ResolvedFont> {
    let data = std::fs::read(path).ok()?;
    for idx in 0..ttf_parser::fonts_in_collection(&data).unwrap_or(1) {
        if let Ok(face) = ttf_parser::Face::parse(&data, idx) {
            let matches = face.names().into_iter().any(|n| {
                n.name_id == ttf_parser::name_id::FULL_NAME
                    && n.to_string().is_some_and(|s| s.eq_ignore_ascii_case(name))
            });
            if matches || idx == 0 {
                return Some(ResolvedFont {
                    name: name.to_string(),
                    data: data.clone(),
                    face_index: idx,
                    units_per_em: face.units_per_em(),
                    ascender: face.ascender(),
                    descender: face.descender(),
                });
            }
        }
    }
    None
}

fn scan_system_fonts() -> HashMap<String, PathBuf> {
    let mut fonts = HashMap::new();
    for dir in system_font_dirs() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if matches!(ext.as_str(), "ttf" | "otf" | "ttc" | "otc") {
                    if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                        fonts.insert(name.to_lowercase(), path);
                    }
                }
            }
        }
    }
    fonts
}

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
            dirs.push(PathBuf::from(format!("{home}/.local/share/fonts")));
            dirs.push(PathBuf::from(format!("{home}/.fonts")));
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(windir) = std::env::var("WINDIR") {
            dirs.push(PathBuf::from(format!("{windir}\\Fonts")));
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            dirs.push(PathBuf::from(format!("{local}\\Microsoft\\Windows\\Fonts")));
        }
    }
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_spec_parsing() {
        let spec = XfaFontSpec::from_xfa_attrs("Helvetica", Some("bold"), None, Some("12pt"));
        assert_eq!(spec.typeface, "Helvetica");
        assert_eq!(spec.weight, FontWeight::Bold);
        assert_eq!(spec.posture, FontPosture::Normal);
        assert!((spec.size_pt - 12.0).abs() < 0.001);
    }

    #[test]
    fn font_spec_defaults() {
        let spec = XfaFontSpec::from_xfa_attrs("Arial", None, None, None);
        assert_eq!(spec.weight, FontWeight::Normal);
        assert_eq!(spec.posture, FontPosture::Normal);
        assert!((spec.size_pt - 10.0).abs() < 0.001);
    }

    #[test]
    fn resolver_empty() {
        let mut resolver = XfaFontResolver::new(vec![]);
        let spec = XfaFontSpec::from_xfa_attrs("NonExistentFont12345", None, None, None);
        let _ = resolver.resolve(&spec);
    }

    #[test]
    fn system_font_dirs_not_empty() {
        let dirs = system_font_dirs();
        assert!(!dirs.is_empty());
    }
}
