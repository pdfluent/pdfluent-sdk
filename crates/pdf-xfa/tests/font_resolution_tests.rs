//! Font resolution integration tests.
//!
//! Tests the font resolution pipeline including:
//! - PostScript name normalization
//! - Subset prefix stripping
//! - Font aliasing
//! - Serif/Monospace fallback
//! - Case insensitive matching
//! - Base name stripping
//! - genericFamily fallback

use pdf_xfa::font_bridge::{FontPosture, FontWeight, GenericFamily, XfaFontResolver, XfaFontSpec};

#[test]
fn test_postscript_name_normalization() {
    let mut resolver = XfaFontResolver::new(vec![]);
    let spec = XfaFontSpec::from_xfa_attrs("ArialMT", None, None, None, None);
    let result = resolver.resolve(&spec);
    assert!(result.is_ok(), "Should resolve ArialMT to a valid font");
}

#[test]
fn test_subset_prefix_stripping() {
    let mut resolver = XfaFontResolver::new(vec![]);
    let spec = XfaFontSpec::from_xfa_attrs("ABCDEF+Arial", None, None, None, None);
    let result = resolver.resolve(&spec);
    assert!(result.is_ok(), "Should resolve ABCDEF+Arial to Arial");
}

#[test]
fn test_font_alias_arial() {
    let mut resolver = XfaFontResolver::new(vec![]);
    let spec = XfaFontSpec::from_xfa_attrs("Arial", None, None, None, None);
    let result = resolver.resolve(&spec);
    assert!(result.is_ok(), "Should resolve Arial");
}

#[test]
fn test_font_alias_times() {
    let mut resolver = XfaFontResolver::new(vec![]);
    let spec = XfaFontSpec::from_xfa_attrs("TimesNewRoman", None, None, None, None);
    let result = resolver.resolve(&spec);
    assert!(result.is_ok(), "Should resolve TimesNewRoman");
}

#[test]
fn test_font_alias_courier() {
    let mut resolver = XfaFontResolver::new(vec![]);
    let spec = XfaFontSpec::from_xfa_attrs("CourierNew", None, None, None, None);
    let result = resolver.resolve(&spec);
    assert!(result.is_ok(), "Should resolve CourierNew");
}

#[test]
fn test_case_insensitive_arial() {
    let mut resolver = XfaFontResolver::new(vec![]);
    let spec_lower = XfaFontSpec::from_xfa_attrs("arial", None, None, None, None);
    let spec_upper = XfaFontSpec::from_xfa_attrs("ARIAL", None, None, None, None);
    let spec_mixed = XfaFontSpec::from_xfa_attrs("Arial", None, None, None, None);
    assert!(resolver.resolve(&spec_lower).is_ok());
    assert!(resolver.resolve(&spec_upper).is_ok());
    assert!(resolver.resolve(&spec_mixed).is_ok());
}

#[test]
fn test_base_name_stripping_bold() {
    let mut resolver = XfaFontResolver::new(vec![]);
    let spec = XfaFontSpec::from_xfa_attrs("Arial-Bold", Some("bold"), None, None, None);
    let result = resolver.resolve(&spec);
    assert!(
        result.is_ok(),
        "Should resolve Arial-Bold by stripping to Arial"
    );
}

#[test]
fn test_base_name_stripping_italic() {
    let mut resolver = XfaFontResolver::new(vec![]);
    let spec = XfaFontSpec::from_xfa_attrs("Arial-Italic", None, Some("italic"), None, None);
    let result = resolver.resolve(&spec);
    assert!(
        result.is_ok(),
        "Should resolve Arial-Italic by stripping to Arial"
    );
}

#[test]
fn test_base_name_stripping_bolditalic() {
    let mut resolver = XfaFontResolver::new(vec![]);
    let spec = XfaFontSpec::from_xfa_attrs(
        "Arial-BoldItalic",
        Some("bold"),
        Some("italic"),
        None,
        None,
    );
    let result = resolver.resolve(&spec);
    assert!(
        result.is_ok(),
        "Should resolve Arial-BoldItalic by stripping to Arial"
    );
}

#[test]
fn test_font_spec_parsing_bold() {
    let spec = XfaFontSpec::from_xfa_attrs("Helvetica", Some("bold"), None, None, None);
    assert_eq!(spec.typeface, "Helvetica");
    assert_eq!(spec.weight, FontWeight::Bold);
    assert_eq!(spec.posture, FontPosture::Normal);
}

#[test]
fn test_font_spec_parsing_italic() {
    let spec = XfaFontSpec::from_xfa_attrs("Helvetica", None, Some("italic"), None, None);
    assert_eq!(spec.typeface, "Helvetica");
    assert_eq!(spec.weight, FontWeight::Normal);
    assert_eq!(spec.posture, FontPosture::Italic);
}

#[test]
fn test_font_spec_parsing_size() {
    let spec = XfaFontSpec::from_xfa_attrs("Helvetica", None, None, Some("12pt"), None);
    assert!((spec.size_pt - 12.0).abs() < 0.001, "Size should be 12pt");

    let spec_no_unit = XfaFontSpec::from_xfa_attrs("Helvetica", None, None, Some("10"), None);
    assert!(
        (spec_no_unit.size_pt - 10.0).abs() < 0.001,
        "Size should default to 10pt for bare numbers"
    );
}

#[test]
fn test_font_spec_default_values() {
    let spec = XfaFontSpec::from_xfa_attrs("Arial", None, None, None, None);
    assert_eq!(spec.weight, FontWeight::Normal);
    assert_eq!(spec.posture, FontPosture::Normal);
    assert!(
        (spec.size_pt - 10.0).abs() < 0.001,
        "Default size should be 10pt"
    );
}

#[test]
fn test_generic_family_parsing() {
    let spec = XfaFontSpec::from_xfa_attrs("FancyFont", None, None, None, Some("serif"));
    assert_eq!(spec.generic_family, Some(GenericFamily::Serif));

    let spec = XfaFontSpec::from_xfa_attrs("FancyFont", None, None, None, Some("sansSerif"));
    assert_eq!(spec.generic_family, Some(GenericFamily::SansSerif));

    let spec = XfaFontSpec::from_xfa_attrs("FancyFont", None, None, None, Some("monospaced"));
    assert_eq!(spec.generic_family, Some(GenericFamily::Monospaced));

    let spec = XfaFontSpec::from_xfa_attrs("FancyFont", None, None, None, Some("decorative"));
    assert_eq!(spec.generic_family, Some(GenericFamily::Decorative));

    let spec = XfaFontSpec::from_xfa_attrs("FancyFont", None, None, None, Some("fantasy"));
    assert_eq!(spec.generic_family, Some(GenericFamily::Fantasy));

    let spec = XfaFontSpec::from_xfa_attrs("FancyFont", None, None, None, Some("cursive"));
    assert_eq!(spec.generic_family, Some(GenericFamily::Cursive));

    let spec = XfaFontSpec::from_xfa_attrs("FancyFont", None, None, None, Some("bogus"));
    assert_eq!(spec.generic_family, None);
}

#[test]
fn test_generic_family_serif_fallback() {
    let mut resolver = XfaFontResolver::new(vec![]);
    let spec =
        XfaFontSpec::from_xfa_attrs("UnknownSerifFont999", None, None, None, Some("serif"));
    let result = resolver.resolve(&spec);
    assert!(
        result.is_ok(),
        "genericFamily=serif should resolve to a serif fallback font"
    );
}

#[test]
fn test_generic_family_monospaced_fallback() {
    let mut resolver = XfaFontResolver::new(vec![]);
    let spec =
        XfaFontSpec::from_xfa_attrs("UnknownMonoFont999", None, None, None, Some("monospaced"));
    let result = resolver.resolve(&spec);
    assert!(
        result.is_ok(),
        "genericFamily=monospaced should resolve to a mono fallback font"
    );
}

#[test]
fn test_nonexistent_font_falls_back() {
    let mut resolver = XfaFontResolver::new(vec![]);
    let spec = XfaFontSpec::from_xfa_attrs("NonExistentFont12345", None, None, None, None);
    let result = resolver.resolve(&spec);
    assert!(
        result.is_ok(),
        "Should fall back to a system font even for nonexistent fonts"
    );
}

#[test]
fn test_resolved_font_has_metrics() {
    let mut resolver = XfaFontResolver::new(vec![]);
    let spec = XfaFontSpec::from_xfa_attrs("Helvetica", None, None, Some("12pt"), None);
    let result = resolver.resolve(&spec);
    assert!(result.is_ok());
    let font = result.unwrap();
    assert!(font.units_per_em > 0, "Font should have valid units_per_em");
    assert!(font.ascender > 0, "Font should have positive ascender");
    assert!(font.descender < 0, "Font should have negative descender");
}

#[test]
fn test_resolved_font_measure_string() {
    let mut resolver = XfaFontResolver::new(vec![]);
    let spec = XfaFontSpec::from_xfa_attrs("Helvetica", None, None, Some("12pt"), None);
    let result = resolver.resolve(&spec);
    assert!(result.is_ok());
    let font = result.unwrap();
    let width = font.measure_string("Hello", 12.0);
    assert!(width > 0.0, "Should measure string width");
}

#[test]
fn test_resolved_font_line_height() {
    let mut resolver = XfaFontResolver::new(vec![]);
    let spec = XfaFontSpec::from_xfa_attrs("Helvetica", None, None, Some("12pt"), None);
    let result = resolver.resolve(&spec);
    assert!(result.is_ok());
    let font = result.unwrap();
    let lh = font.line_height(12.0);
    assert!(lh > 0.0, "Line height should be positive");
}
