//! Bounded per-document glyph outline cache (speed round R2).
//!
//! The renderer memoizes glyph outlines (paths in glyph space) per
//! (font identity, glyph id) and reuses them for repeated glyphs by
//! transforming the cached path instead of rebuilding it. These tests pin
//! the contract through the public render path:
//!
//! - cache on, cache off, and the default `render()` produce byte-identical
//!   pixmaps (the cache only avoids rebuilds; it never changes a path or a
//!   transform);
//! - repeated glyphs are served from the cache (hits), each distinct glyph
//!   is built once (misses);
//! - one cache shared across pages reuses the first page's glyphs on the
//!   second page (the per-document win);
//! - a tiny cache stays within its bound and still renders byte-identical
//!   output despite evictions.

use pdf_render::pdf_interpret::InterpreterSettings;
use pdf_render::pdf_syntax::Pdf;
use pdf_render::{GlyphOutlineCache, RenderSettings, render, render_with_glyph_cache};

/// One Helvetica line per entry, one page per group, on a 300x100 pt page.
fn text_pdf(pages: &[&[&str]]) -> Vec<u8> {
    use lopdf::{Document, Object, Stream, dictionary};

    let mut doc = Document::with_version("1.4");
    let pages_id = doc.new_object_id();
    let font_id = doc.new_object_id();
    doc.objects.insert(
        font_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Font".to_vec()),
            "Subtype" => Object::Name(b"Type1".to_vec()),
            "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        }),
    );

    let mut kids = Vec::with_capacity(pages.len());
    for lines in pages {
        let page_id = doc.new_object_id();
        let mut content = b"BT /F1 12 Tf 10 80 Td ".to_vec();
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                content.extend_from_slice(b"0 -14 Td ");
            }
            content.extend_from_slice(b"(");
            content.extend_from_slice(line.as_bytes());
            content.extend_from_slice(b") Tj ");
        }
        content.extend_from_slice(b"ET");
        let content_id = doc.add_object(Stream::new(dictionary! {}, content));
        doc.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => Object::Name(b"Page".to_vec()),
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => Object::Array(vec![
                    Object::Integer(0), Object::Integer(0),
                    Object::Integer(300), Object::Integer(100),
                ]),
                "Contents" => Object::Reference(content_id),
                "Resources" => Object::Dictionary(dictionary! {
                    "Font" => Object::Dictionary(dictionary! {
                        "F1" => Object::Reference(font_id),
                    }),
                }),
            }),
        );
        kids.push(Object::Reference(page_id));
    }

    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(kids),
            "Count" => Object::Integer(pages.len() as i64),
        }),
    );
    let catalog_id = doc.new_object_id();
    doc.objects.insert(
        catalog_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "Pages" => Object::Reference(pages_id),
        }),
    );
    doc.trailer.set("Root", Object::Reference(catalog_id));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("lopdf save should succeed");
    bytes
}

fn render_settings() -> RenderSettings {
    RenderSettings {
        x_scale: 2.0,
        y_scale: 2.0,
        ..Default::default()
    }
}

/// The cache must be output-neutral: the same page rendered through the
/// default path (fresh per-page cache), through a caller-owned enabled
/// cache, and through a disabled cache (every outline rebuilt) must agree
/// byte for byte. The counters prove the seam actually toggled: the enabled
/// run served hits, the disabled run served none and stored nothing.
#[test]
fn cache_on_off_and_default_renders_are_byte_identical() {
    let text = format!("abcdefghijklmnopqrstuvwxyz{}", "a".repeat(200));
    let bytes = text_pdf(&[&[text.as_str()]]);
    let pdf = Pdf::new(bytes).expect("fixture PDF should load");
    let pages = pdf.pages();
    let interp = InterpreterSettings::default();
    let settings = render_settings();

    let plain = render(&pages[0], &interp, &settings);

    let mut enabled = GlyphOutlineCache::new();
    let cached = render_with_glyph_cache(&pages[0], &interp, &settings, &mut enabled);

    let mut disabled = GlyphOutlineCache::new();
    disabled.set_enabled(false);
    let uncached = render_with_glyph_cache(&pages[0], &interp, &settings, &mut disabled);

    for (name, pixmap) in [("enabled", &cached), ("disabled", &uncached)] {
        assert_eq!(
            (pixmap.width(), pixmap.height()),
            (plain.width(), plain.height()),
            "{name} render dimensions must match the default path"
        );
        assert_eq!(
            pixmap.data_as_u8_slice(),
            plain.data_as_u8_slice(),
            "{name} render bytes must match the default path"
        );
    }

    assert!(
        enabled.hits() > 0,
        "repeated glyphs must hit the enabled cache"
    );
    assert_eq!(disabled.hits(), 0, "disabled cache must serve no hits");
    assert!(
        disabled.misses() > 0,
        "disabled cache must count every build as a miss"
    );
    assert_eq!(disabled.len(), 0, "disabled cache must store nothing");
}

/// 26 distinct glyphs drawn 226 times: at most 26 outline builds, the rest
/// hits. If memoization broke (every draw rebuilt), hits would be 0; if keys
/// were unstable per occurrence, misses would approach the draw count.
#[test]
fn repeated_glyphs_are_served_from_cache() {
    let text = format!("abcdefghijklmnopqrstuvwxyz{}", "a".repeat(200));
    let bytes = text_pdf(&[&[text.as_str()]]);
    let pdf = Pdf::new(bytes).expect("fixture PDF should load");
    let pages = pdf.pages();

    let mut cache = GlyphOutlineCache::new();
    render_with_glyph_cache(
        &pages[0],
        &InterpreterSettings::default(),
        &render_settings(),
        &mut cache,
    );

    assert!(
        (1..=26).contains(&cache.misses()),
        "each distinct glyph is built once; misses = {}",
        cache.misses()
    );
    assert!(
        cache.hits() >= 100,
        "the 200 repeated draws must hit; hits = {}",
        cache.hits()
    );
}

/// The per-document win: a cache shared across pages serves the second
/// page's glyphs from the first page's entries — no new builds.
#[test]
fn second_page_reuses_first_page_glyphs() {
    let bytes = text_pdf(&[&["abcdefgh"], &["abcdefgh"]]);
    let pdf = Pdf::new(bytes).expect("fixture PDF should load");
    let pages = pdf.pages();
    assert_eq!(pages.len(), 2);
    let interp = InterpreterSettings::default();
    let settings = render_settings();

    let mut cache = GlyphOutlineCache::new();
    render_with_glyph_cache(&pages[0], &interp, &settings, &mut cache);
    let (hits_after_first, misses_after_first) = (cache.hits(), cache.misses());
    assert!(
        (1..=8).contains(&misses_after_first),
        "first page builds each distinct glyph once; misses = {misses_after_first}"
    );

    render_with_glyph_cache(&pages[1], &interp, &settings, &mut cache);
    assert_eq!(
        cache.misses(),
        misses_after_first,
        "second page must add no outline builds"
    );
    assert!(
        cache.hits() > hits_after_first,
        "second page must hit the first page's entries"
    );
}

/// Eight distinct glyphs through a capacity-4 cache: the bound holds and
/// eviction is output-neutral (byte-identical to an unbounded run).
#[test]
fn tiny_cache_stays_bounded_and_byte_identical() {
    let bytes = text_pdf(&[&["abcdefgh"]]);
    let pdf = Pdf::new(bytes).expect("fixture PDF should load");
    let pages = pdf.pages();
    let interp = InterpreterSettings::default();
    let settings = render_settings();

    let mut tiny = GlyphOutlineCache::with_capacity(4);
    let evicted = render_with_glyph_cache(&pages[0], &interp, &settings, &mut tiny);
    assert!(
        tiny.len() <= 4,
        "cache must stay within capacity; len = {}",
        tiny.len()
    );

    let mut full = GlyphOutlineCache::new();
    let reference = render_with_glyph_cache(&pages[0], &interp, &settings, &mut full);
    assert_eq!(
        (evicted.width(), evicted.height()),
        (reference.width(), reference.height())
    );
    assert_eq!(
        evicted.data_as_u8_slice(),
        reference.data_as_u8_slice(),
        "eviction must not change a single pixel"
    );
}
