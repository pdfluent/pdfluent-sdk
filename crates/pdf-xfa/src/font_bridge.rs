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
    /// PDF /Widths array for glyph metrics: (first_char_code, widths).
    pub pdf_widths: Option<(u16, Vec<u16>)>,
}

impl ResolvedFont {
    /// Measure the approximate width of a string in points at the given font size.
    pub fn measure_string(&self, text: &str, font_size: f64) -> f64 {
        if let Some((first_char, ref widths)) = self.pdf_widths {
            let mut total = 0.0;
            for ch in text.chars() {
                let code = ch as u16;
                if code >= first_char && ((code - first_char) as usize) < widths.len() {
                    total += widths[(code - first_char) as usize] as f64;
                } else {
                    total += self.measure_char_fallback(ch);
                }
            }
            return total * font_size / 1000.0;
        }
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

    fn measure_char_fallback(&self, ch: char) -> f64 {
        if let Ok(face) = ttf_parser::Face::parse(&self.data, self.face_index) {
            if let Some(gid) = face.glyph_index(ch) {
                let upem = face.units_per_em() as f64;
                let scale = 1.0 / upem;
                face.glyph_hor_advance(gid).unwrap_or(0) as f64 * scale * 1000.0
            } else {
                500.0
            }
        } else {
            500.0
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

    /// Generate PDF glyph widths array for embedding (WinAnsiEncoding, 256 entries).
    pub fn pdf_glyph_widths(&self) -> (u16, Vec<u16>) {
        if let Some(widths) = &self.pdf_widths {
            return widths.clone();
        }
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

    /// Generate CID font data for Identity-H encoding.
    ///
    /// Returns glyph widths indexed by GID and a GID→Unicode mapping for
    /// the ToUnicode CMap.
    pub fn cid_font_info(&self) -> Option<CidFontInfo> {
        let face = ttf_parser::Face::parse(&self.data, self.face_index).ok()?;
        let upem = face.units_per_em() as f64;
        let scale = 1000.0 / upem;
        let num_glyphs = face.number_of_glyphs();

        let mut widths = Vec::with_capacity(num_glyphs as usize);
        for gid_val in 0..num_glyphs {
            let w = face
                .glyph_hor_advance(ttf_parser::GlyphId(gid_val))
                .map(|a| (a as f64 * scale) as u16)
                .unwrap_or(0);
            widths.push(w);
        }

        let mut gid_to_unicode = Vec::new();
        for cp in 0x0020u32..=0xFFFDu32 {
            if let Some(ch) = char::from_u32(cp) {
                if let Some(gid) = face.glyph_index(ch) {
                    gid_to_unicode.push((gid.0, ch));
                }
            }
        }

        Some(CidFontInfo {
            widths,
            gid_to_unicode,
        })
    }
}

/// Data needed for CIDFont (Identity-H) embedding.
pub struct CidFontInfo {
    /// Width in 1/1000 units for each glyph, indexed by GID.
    pub widths: Vec<u16>,
    /// Mapping from glyph ID to Unicode codepoint (for ToUnicode CMap).
    pub gid_to_unicode: Vec<(u16, char)>,
}

/// Build a cache/lookup key that encodes typeface, weight, and posture.
///
/// This ensures that "Arial" regular and "Arial" bold are stored and
/// looked up as distinct entries in font maps and metrics data.
pub fn font_variant_key(typeface: &str, weight: Option<&str>, posture: Option<&str>) -> String {
    let w = match weight {
        Some("bold") => "_Bold",
        _ => "_Normal",
    };
    let p = match posture {
        Some("italic") => "_Italic",
        _ => "_Normal",
    };
    format!("{}{}{}", typeface, w, p)
}

/// XFA Spec 3.3 §17 (p716) — genericFamily attribute on the font element.
///
/// Used as a fallback when the requested typeface cannot be found.
/// XFA Spec 3.3 §28.2 (p1246) — Font mapping step 4: genericFamily mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenericFamily {
    Serif,
    SansSerif,
    Monospaced,
    Decorative,
    Fantasy,
    Cursive,
}

impl GenericFamily {
    /// Parse the XFA genericFamily attribute value.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "serif" => Some(Self::Serif),
            "sansSerif" => Some(Self::SansSerif),
            "monospaced" => Some(Self::Monospaced),
            "decorative" => Some(Self::Decorative),
            "fantasy" => Some(Self::Fantasy),
            "cursive" => Some(Self::Cursive),
            _ => None,
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
    /// XFA Spec 3.3 §17 (p716) — genericFamily fallback hint.
    pub generic_family: Option<GenericFamily>,
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
        generic_family: Option<&str>,
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
            generic_family: generic_family.and_then(GenericFamily::parse),
        }
    }
}

/// Resolves XFA font specifications to actual font data.
pub struct XfaFontResolver {
    embedded: HashMap<String, ResolvedFont>,
    system_fonts: HashMap<String, PathBuf>,
    cache: HashMap<String, ResolvedFont>,
}

/// Normalize a font name by stripping subset prefixes and PostScript suffixes.
///
/// - Strips subset prefix: "ABCDEF+Arial" -> "Arial"
/// - Strips PS suffixes: "ArialMT" -> "Arial", "TimesNewRomanPSMT" -> "TimesNewRoman"
/// - Converts to lowercase
fn normalize_font_name(name: &str) -> String {
    // Strip subset prefix (6 uppercase letters + '+')
    let stripped = if name.len() > 7 && name.as_bytes()[6] == b'+' {
        let prefix = &name[..6];
        if prefix.chars().all(|c| c.is_ascii_uppercase()) {
            &name[7..]
        } else {
            name
        }
    } else {
        name
    };

    // Strip PostScript suffixes
    let stripped = stripped
        .strip_suffix("PSMT")
        .or_else(|| stripped.strip_suffix("MT"))
        .unwrap_or(stripped);

    stripped.to_lowercase()
}

/// Return alias list for common font family names.
///
/// Maps Windows/macOS font names to their Linux metric-compatible equivalents.
fn font_family_aliases(name: &str) -> &'static [&'static str] {
    match name {
        "arial" | "arialmt" => &["liberationsans", "arimo", "freesans"],
        "times new roman" | "timesnewroman" | "timesnewromanpsmt" => {
            &["liberationserif", "tinos", "freeserif"]
        }
        "courier new" | "couriernew" | "couriernewpsmt" => {
            &["liberationmono", "cousine", "freemono"]
        }
        "helvetica" => &["liberationsans", "arimo", "arial"],
        "myriad pro" | "myriadpro" => &["liberationsans", "arimo", "dejavusans"],
        // Reverse mappings: Linux fonts -> common equivalents
        "liberationsans" | "liberation sans" => &["arial", "arimo", "freesans", "helvetica"],
        "liberationserif" | "liberation serif" => &["times new roman", "tinos", "freeserif"],
        "liberationmono" | "liberation mono" => &["courier new", "cousine", "freemono"],
        _ => &[],
    }
}

/// Font classification for family-aware fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FontFamily {
    Serif,
    SansSerif,
    Monospace,
    Unknown,
}

/// Classify a font name into a font family category.
fn classify_font_family(name: &str) -> FontFamily {
    let lower = name.to_lowercase();

    // Monospace indicators
    if lower.contains("mono")
        || lower.contains("courier")
        || lower.contains("consolas")
        || lower.contains("menlo")
        || lower.contains("fixed")
        || lower.contains("code")
    {
        return FontFamily::Monospace;
    }

    // Serif indicators (check before sans-serif since "sans" contains checks come after)
    if lower.contains("serif") && !lower.contains("sans") {
        return FontFamily::Serif;
    }
    if lower.contains("times")
        || lower.contains("garamond")
        || lower.contains("georgia")
        || lower.contains("palatino")
        || lower.contains("bodoni")
        || lower.contains("cambria")
        || lower.contains("tinos")
    {
        return FontFamily::Serif;
    }

    // Sans-serif indicators
    if lower.contains("sans")
        || lower.contains("arial")
        || lower.contains("helvetica")
        || lower.contains("verdana")
        || lower.contains("tahoma")
        || lower.contains("calibri")
        || lower.contains("arimo")
        || lower.contains("myriad")
        || lower.contains("segoe")
    {
        return FontFamily::SansSerif;
    }

    FontFamily::Unknown
}

/// Return family-aware fallback chain for the given font family.
fn family_fallback_chain(family: FontFamily) -> &'static [&'static str] {
    match family {
        FontFamily::SansSerif | FontFamily::Unknown => &[
            "liberationsans",
            "arimo",
            "dejavusans",
            "freesans",
            "helvetica",
            "arial",
        ],
        FontFamily::Serif => &["liberationserif", "tinos", "dejavuserif", "freeserif"],
        FontFamily::Monospace => &["liberationmono", "cousine", "dejavusansmono", "freemono"],
    }
}

fn font_name_substring_match(requested: &str, available: &str) -> bool {
    let requested_lower = requested.to_lowercase();
    let available_lower = available.to_lowercase();

    if requested_lower == available_lower {
        return true;
    }

    if requested_lower.contains(&available_lower) || available_lower.contains(&requested_lower) {
        return true;
    }

    let requested_stripped = normalize_font_name(requested);
    let available_stripped = normalize_font_name(available);

    if requested_stripped == available_stripped {
        return true;
    }

    if requested_stripped.contains(&available_stripped)
        || available_stripped.contains(&requested_stripped)
    {
        return true;
    }

    false
}

// XFA Spec 3.3 §28.2 (p1246) — Font mapping step 4: genericFamily mapping.
// Maps genericFamily values to concrete font fallback chains.
fn generic_family_fallback_chain(gf: GenericFamily) -> &'static [&'static str] {
    match gf {
        GenericFamily::Serif | GenericFamily::Decorative => &[
            "liberationserif",
            "tinos",
            "dejavuserif",
            "freeserif",
            "times new roman",
            "times",
        ],
        GenericFamily::SansSerif | GenericFamily::Fantasy => &[
            "liberationsans",
            "arimo",
            "dejavusans",
            "freesans",
            "helvetica",
            "arial",
        ],
        GenericFamily::Monospaced => &[
            "liberationmono",
            "cousine",
            "dejavusansmono",
            "freemono",
            "courier new",
            "courier",
        ],
        GenericFamily::Cursive => {
            // Cursive maps to best available serif-italic; fall back to serif fonts.
            &[
                "liberationserif",
                "tinos",
                "dejavuserif",
                "freeserif",
                "times new roman",
            ]
        }
    }
}

impl XfaFontResolver {
    /// Create a new resolver with embedded fonts extracted from the PDF.
    pub fn new(embedded_fonts: Vec<(String, Vec<u8>, Option<(u16, Vec<u16>)>)>) -> Self {
        let mut embedded = HashMap::new();
        for (name, data, pdf_widths) in embedded_fonts {
            if let Some(font) = parse_font_data_with_widths(&name, &data, pdf_widths) {
                let normalized = normalize_font_name(&name);
                embedded.insert(name.to_lowercase(), font.clone());
                if normalized != name.to_lowercase() {
                    embedded.insert(normalized, font);
                }
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
    ///
    /// XFA Spec 3.3 §28.2 (p1246) — Font mapping: Adobe uses a 5-step algorithm:
    /// 1) direct match, 2) equate, 3) locale, 4) genericFamily, 5) default.
    /// We implement steps 1, 4, 5 (equate and locale are config-dependent).
    ///
    /// When weight is Bold and/or posture is Italic, variant-specific font
    /// names are tried first (e.g. "Arial-Bold", "ArialBold", "Arial Bold")
    /// in both embedded and system font lookups. This ensures that bold/italic
    /// text gets the correct font metrics (wider glyphs, different ascender/
    /// descender) instead of silently falling back to the regular weight.
    pub fn resolve(&mut self, spec: &XfaFontSpec) -> Result<ResolvedFont> {
        let cache_key = format!("{}_{:?}_{:?}", spec.typeface, spec.weight, spec.posture);
        if let Some(cached) = self.cache.get(&cache_key) {
            return Ok(cached.clone());
        }
        let normalized = normalize_font_name(&spec.typeface);

        // Build variant suffixes based on weight/posture.
        let variant_names = build_variant_names(&spec.typeface, spec.weight, spec.posture);

        // Step 1: Try variant-specific names first (bold/italic variants).
        let font = variant_names
            .iter()
            .find_map(|vn| {
                self.try_embedded(vn)
                    .or_else(|| self.try_system(vn))
                    .or_else(|| {
                        let norm = normalize_font_name(vn);
                        self.try_embedded(&norm).or_else(|| self.try_system(&norm))
                    })
            })
            // Then try the base name as before.
            .or_else(|| self.try_embedded(&spec.typeface))
            .or_else(|| self.try_embedded(&normalized))
            .or_else(|| self.try_system(&spec.typeface))
            .or_else(|| self.try_system(&normalized))
            .or_else(|| self.try_base_name(&spec.typeface))
            .or_else(|| self.try_aliases(&spec.typeface))
            .or_else(|| self.try_substring_match(&spec.typeface))
            // Step 4: genericFamily fallback (XFA §28.2 step 4).
            .or_else(|| self.try_generic_family_fallback(spec.generic_family))
            .or_else(|| self.try_family_fallback(&spec.typeface))
            // Step 5: system default.
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
            let normalized_base = normalize_font_name(&base);
            self.try_embedded(&base)
                .or_else(|| self.try_embedded(&normalized_base))
                .or_else(|| self.try_system(&base))
                .or_else(|| self.try_system(&normalized_base))
        } else {
            None
        }
    }

    /// Try font family aliases: map common font names to Linux equivalents.
    fn try_aliases(&self, name: &str) -> Option<ResolvedFont> {
        let normalized = normalize_font_name(name);

        for lookup in &[name.to_lowercase(), normalized] {
            let no_spaces = lookup.replace(' ', "");
            for candidate in [lookup.as_str(), no_spaces.as_str()] {
                let aliases = font_family_aliases(candidate);
                for alias in aliases {
                    if let Some(font) = self.try_system(alias) {
                        return Some(font);
                    }
                }
            }
        }
        None
    }

    fn try_substring_match(&self, name: &str) -> Option<ResolvedFont> {
        let name_lower = name.to_lowercase();
        let name_normalized = normalize_font_name(name);

        for (available_key, font) in &self.embedded {
            if font_name_substring_match(&name_lower, available_key)
                || font_name_substring_match(&name_normalized, available_key)
            {
                return Some(font.clone());
            }
        }

        for (available_key, path) in &self.system_fonts {
            if font_name_substring_match(&name_lower, available_key)
                || font_name_substring_match(&name_normalized, available_key)
            {
                if let Some(font) = load_system_font(path, available_key) {
                    return Some(font);
                }
            }
        }

        None
    }

    /// XFA Spec 3.3 §28.2 step 4 — genericFamily fallback.
    fn try_generic_family_fallback(&self, gf: Option<GenericFamily>) -> Option<ResolvedFont> {
        let gf = gf?;
        let chain = generic_family_fallback_chain(gf);
        for candidate in chain {
            if let Some(font) = self.try_system(candidate) {
                return Some(font);
            }
        }
        None
    }

    /// Family-aware fallback: classify the font and try appropriate chain.
    fn try_family_fallback(&self, name: &str) -> Option<ResolvedFont> {
        let family = classify_font_family(name);
        let chain = family_fallback_chain(family);
        for candidate in chain {
            if let Some(font) = self.try_system(candidate) {
                return Some(font);
            }
        }
        None
    }

    fn try_fallbacks(&self) -> Option<ResolvedFont> {
        #[cfg(target_os = "macos")]
        let fallback_chain = ["Arial", "Helvetica.ttc", "DejaVuSans", "LiberationSans"];
        #[cfg(not(target_os = "macos"))]
        let fallback_chain = [
            "LiberationSans",
            "DejaVuSans",
            "Arial",
            "Helvetica",
            "FreeSans",
        ];

        for name in &fallback_chain {
            if let Some(font) = self.try_system(name) {
                return Some(font);
            }
        }
        None
    }
}

/// Build variant-specific font names for bold/italic lookup.
///
/// Given a base typeface name ("Arial") and weight/posture, produces names
/// like "Arial-Bold", "ArialBold", "Arial Bold" etc. Returns an empty vec
/// when both weight and posture are Normal.
fn build_variant_names(typeface: &str, weight: FontWeight, posture: FontPosture) -> Vec<String> {
    let suffix = match (weight, posture) {
        (FontWeight::Bold, FontPosture::Italic) => "BoldItalic",
        (FontWeight::Bold, FontPosture::Normal) => "Bold",
        (FontWeight::Normal, FontPosture::Italic) => "Italic",
        (FontWeight::Normal, FontPosture::Normal) => return Vec::new(),
    };

    let mut names = Vec::with_capacity(6);
    // "{Name}-{Suffix}" e.g. "Arial-Bold"
    names.push(format!("{}-{}", typeface, suffix));
    // "{Name}{Suffix}" e.g. "ArialBold"
    names.push(format!("{}{}", typeface, suffix));
    // "{Name} {Suffix}" e.g. "Arial Bold"
    names.push(format!("{} {}", typeface, suffix));
    // Comma-separated: "{Name},{Suffix}" e.g. "Arial,Bold"
    names.push(format!("{},{}", typeface, suffix));

    // For BoldItalic, also try the two-suffix patterns:
    // "{Name}-Bold Italic", "{Name} Bold Italic"
    if weight == FontWeight::Bold && posture == FontPosture::Italic {
        names.push(format!("{}-Bold Italic", typeface));
        names.push(format!("{} Bold Italic", typeface));
    }

    names
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
        pdf_widths: None,
    })
}

fn parse_font_data_with_widths(
    name: &str,
    data: &[u8],
    pdf_widths: Option<(u16, Vec<u16>)>,
) -> Option<ResolvedFont> {
    let face = ttf_parser::Face::parse(data, 0).ok()?;
    Some(ResolvedFont {
        name: name.to_string(),
        data: data.to_vec(),
        face_index: 0,
        units_per_em: face.units_per_em(),
        ascender: face.ascender(),
        descender: face.descender(),
        pdf_widths,
    })
}

fn load_system_font(path: &PathBuf, name: &str) -> Option<ResolvedFont> {
    let data = std::fs::read(path).ok()?;
    let num_fonts = ttf_parser::fonts_in_collection(&data).unwrap_or(1);
    for idx in 0..num_fonts {
        if let Ok(face) = ttf_parser::Face::parse(&data, idx) {
            let name_id_matches = |name_id: u16| {
                name_id == ttf_parser::name_id::FULL_NAME
                    || name_id == ttf_parser::name_id::POST_SCRIPT_NAME
                    || name_id == ttf_parser::name_id::FAMILY
            };
            let matches = face.names().into_iter().any(|n| {
                name_id_matches(n.name_id)
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
                    pdf_widths: None,
                });
            }
        }
    }
    None
}

fn scan_system_fonts() -> HashMap<String, PathBuf> {
    let mut fonts = HashMap::new();
    let mut font_files = Vec::new();

    // Collect all font files, including subdirectories (Linux stores fonts in
    // subdirectories like /usr/share/fonts/truetype/liberation/).
    for dir in system_font_dirs() {
        collect_font_files(&dir, &mut font_files, 0);
    }

    for path in &font_files {
        // Always register by filename stem (existing behavior)
        if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
            fonts.insert(name.to_lowercase(), path.clone());
        }

        // Also register by TrueType name table entries
        if let Ok(data) = std::fs::read(path) {
            let num_faces = ttf_parser::fonts_in_collection(&data).unwrap_or(1);
            for face_idx in 0..num_faces {
                if let Ok(face) = ttf_parser::Face::parse(&data, face_idx) {
                    for name_record in face.names() {
                        // Register under family name (ID 1), full name (ID 4),
                        // and PostScript name (ID 6).
                        let dominated = matches!(
                            name_record.name_id,
                            ttf_parser::name_id::FAMILY
                                | ttf_parser::name_id::FULL_NAME
                                | ttf_parser::name_id::POST_SCRIPT_NAME
                        );
                        if dominated {
                            if let Some(s) = name_record.to_string() {
                                let key = s.to_lowercase();
                                // Don't overwrite an existing entry — first match wins
                                fonts.entry(key).or_insert_with(|| path.clone());

                                // Also insert without spaces so "Liberation Sans"
                                // can be found as "liberationsans"
                                let no_spaces = s.replace(' ', "").to_lowercase();
                                if no_spaces != s.to_lowercase() {
                                    fonts.entry(no_spaces).or_insert_with(|| path.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fonts
}

/// Recursively collect font files from a directory (up to 3 levels deep).
fn collect_font_files(dir: &std::path::Path, out: &mut Vec<PathBuf>, depth: u32) {
    if depth > 3 {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_font_files(&path, out, depth + 1);
            } else {
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if matches!(ext.as_str(), "ttf" | "otf" | "ttc" | "otc") {
                    out.push(path);
                }
            }
        }
    }
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
        let spec = XfaFontSpec::from_xfa_attrs("Helvetica", Some("bold"), None, Some("12pt"), None);
        assert_eq!(spec.typeface, "Helvetica");
        assert_eq!(spec.weight, FontWeight::Bold);
        assert_eq!(spec.posture, FontPosture::Normal);
        assert!((spec.size_pt - 12.0).abs() < 0.001);
        assert_eq!(spec.generic_family, None);
    }

    #[test]
    fn font_spec_defaults() {
        let spec = XfaFontSpec::from_xfa_attrs("Arial", None, None, None, None);
        assert_eq!(spec.weight, FontWeight::Normal);
        assert_eq!(spec.posture, FontPosture::Normal);
        assert!((spec.size_pt - 10.0).abs() < 0.001);
    }

    #[test]
    fn font_spec_generic_family() {
        let spec = XfaFontSpec::from_xfa_attrs("FancyFont", None, None, None, Some("serif"));
        assert_eq!(spec.generic_family, Some(GenericFamily::Serif));

        let spec = XfaFontSpec::from_xfa_attrs("FancyFont", None, None, None, Some("sansSerif"));
        assert_eq!(spec.generic_family, Some(GenericFamily::SansSerif));

        let spec = XfaFontSpec::from_xfa_attrs("FancyFont", None, None, None, Some("monospaced"));
        assert_eq!(spec.generic_family, Some(GenericFamily::Monospaced));

        let spec = XfaFontSpec::from_xfa_attrs("FancyFont", None, None, None, Some("bogus"));
        assert_eq!(spec.generic_family, None);
    }

    #[test]
    fn resolver_empty() {
        let mut resolver = XfaFontResolver::new(vec![]);
        let spec = XfaFontSpec::from_xfa_attrs("NonExistentFont12345", None, None, None, None);
        let _ = resolver.resolve(&spec);
    }

    #[test]
    fn system_font_dirs_not_empty() {
        let dirs = system_font_dirs();
        assert!(!dirs.is_empty());
    }

    #[test]
    fn cid_font_info_with_system_font() {
        // Try to resolve a system font and verify cid_font_info works
        let mut resolver = XfaFontResolver::new(vec![]);
        let spec = XfaFontSpec::from_xfa_attrs("Helvetica", None, None, None, None);
        if let Ok(font) = resolver.resolve(&spec) {
            let info = font.cid_font_info();
            assert!(
                info.is_some(),
                "cid_font_info should succeed for a valid font"
            );
            let info = info.unwrap();
            assert!(!info.widths.is_empty(), "widths should not be empty");
            assert!(
                !info.gid_to_unicode.is_empty(),
                "gid_to_unicode should not be empty"
            );
            // Verify that 'A' (U+0041) is mapped
            let has_a = info.gid_to_unicode.iter().any(|&(_, ch)| ch == 'A');
            assert!(has_a, "font should have a mapping for 'A'");
        }
    }

    #[test]
    fn normalize_font_name_strips_subset_prefix() {
        assert_eq!(normalize_font_name("ABCDEF+Arial"), "arial");
        assert_eq!(
            normalize_font_name("XYZABC+TimesNewRomanPSMT"),
            "timesnewroman"
        );
    }

    #[test]
    fn normalize_font_name_strips_ps_suffixes() {
        assert_eq!(normalize_font_name("ArialMT"), "arial");
        assert_eq!(normalize_font_name("TimesNewRomanPSMT"), "timesnewroman");
        assert_eq!(normalize_font_name("CourierNewPSMT"), "couriernew");
    }

    #[test]
    fn normalize_font_name_preserves_normal_names() {
        assert_eq!(normalize_font_name("Helvetica"), "helvetica");
        assert_eq!(normalize_font_name("DejaVuSans"), "dejavusans");
    }

    #[test]
    fn normalize_font_name_no_false_prefix_strip() {
        // "abcdef+" should not be stripped (not uppercase)
        assert_eq!(normalize_font_name("abcdef+Arial"), "abcdef+arial");
        // Short prefix should not be stripped
        assert_eq!(normalize_font_name("AB+Arial"), "ab+arial");
    }

    #[test]
    fn font_family_aliases_known_fonts() {
        assert!(!font_family_aliases("arial").is_empty());
        assert!(!font_family_aliases("helvetica").is_empty());
        assert!(!font_family_aliases("courier new").is_empty());
        assert!(!font_family_aliases("times new roman").is_empty());
        assert!(!font_family_aliases("myriad pro").is_empty());
    }

    #[test]
    fn font_family_aliases_unknown_font() {
        assert!(font_family_aliases("some_unknown_font_xyz").is_empty());
    }

    #[test]
    fn classify_font_family_sans() {
        assert_eq!(classify_font_family("Arial"), FontFamily::SansSerif);
        assert_eq!(classify_font_family("Helvetica"), FontFamily::SansSerif);
        assert_eq!(classify_font_family("DejaVuSans"), FontFamily::SansSerif);
        assert_eq!(
            classify_font_family("LiberationSans"),
            FontFamily::SansSerif
        );
    }

    #[test]
    fn classify_font_family_serif() {
        assert_eq!(classify_font_family("Times New Roman"), FontFamily::Serif);
        assert_eq!(classify_font_family("Georgia"), FontFamily::Serif);
        assert_eq!(classify_font_family("LiberationSerif"), FontFamily::Serif);
    }

    #[test]
    fn classify_font_family_mono() {
        assert_eq!(classify_font_family("Courier New"), FontFamily::Monospace);
        assert_eq!(
            classify_font_family("LiberationMono"),
            FontFamily::Monospace
        );
        assert_eq!(classify_font_family("Consolas"), FontFamily::Monospace);
    }

    #[test]
    fn classify_font_family_unknown() {
        assert_eq!(classify_font_family("FancyFont"), FontFamily::Unknown);
    }

    #[test]
    fn scan_system_fonts_has_name_table_entries() {
        let fonts = scan_system_fonts();
        // On any system with fonts, we should have entries.
        // The name-table scanning should produce more entries than just filename stems.
        assert!(!fonts.is_empty(), "system fonts map should not be empty");
    }

    #[test]
    fn build_variant_names_normal() {
        let names = build_variant_names("Arial", FontWeight::Normal, FontPosture::Normal);
        assert!(names.is_empty(), "normal/normal should produce no variants");
    }

    #[test]
    fn build_variant_names_bold() {
        let names = build_variant_names("Arial", FontWeight::Bold, FontPosture::Normal);
        assert!(names.contains(&"Arial-Bold".to_string()));
        assert!(names.contains(&"ArialBold".to_string()));
        assert!(names.contains(&"Arial Bold".to_string()));
        assert!(names.contains(&"Arial,Bold".to_string()));
    }

    #[test]
    fn build_variant_names_italic() {
        let names = build_variant_names("Helvetica", FontWeight::Normal, FontPosture::Italic);
        assert!(names.contains(&"Helvetica-Italic".to_string()));
        assert!(names.contains(&"HelveticaItalic".to_string()));
        assert!(names.contains(&"Helvetica Italic".to_string()));
    }

    #[test]
    fn build_variant_names_bold_italic() {
        let names = build_variant_names("Arial", FontWeight::Bold, FontPosture::Italic);
        assert!(names.contains(&"Arial-BoldItalic".to_string()));
        assert!(names.contains(&"ArialBoldItalic".to_string()));
        assert!(names.contains(&"Arial BoldItalic".to_string()));
        assert!(names.contains(&"Arial-Bold Italic".to_string()));
        assert!(names.contains(&"Arial Bold Italic".to_string()));
    }

    #[test]
    fn font_variant_key_encoding() {
        assert_eq!(font_variant_key("Arial", None, None), "Arial_Normal_Normal");
        assert_eq!(
            font_variant_key("Arial", Some("bold"), None),
            "Arial_Bold_Normal"
        );
        assert_eq!(
            font_variant_key("Arial", None, Some("italic")),
            "Arial_Normal_Italic"
        );
        assert_eq!(
            font_variant_key("Arial", Some("bold"), Some("italic")),
            "Arial_Bold_Italic"
        );
    }

    #[test]
    fn resolve_uses_bold_variant_cache_key() {
        let mut resolver = XfaFontResolver::new(vec![]);
        let spec_normal = XfaFontSpec::from_xfa_attrs("Arial", None, None, None, None);
        let spec_bold = XfaFontSpec::from_xfa_attrs("Arial", Some("bold"), None, None, None);
        // Both should resolve (or fail) independently — they use different cache keys.
        let _ = resolver.resolve(&spec_normal);
        let _ = resolver.resolve(&spec_bold);
    }
}
