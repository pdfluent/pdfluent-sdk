//! Golf 1 field validation: `transform`, `render_mode`, and font metadata.
//!
//! - `transform_captures_rotation` — synthetic ground-truth: a glyph drawn under
//!   a 90°-rotated text matrix yields a vertical x-basis, proving the affine
//!   `transform` captures rotation that the legacy `(x, y, font_size)` discards.
//! - `golf1_fields_consistent_on_real_pdf` — on a real fixture the transform is
//!   self-consistent with `(x, y, font_size)` and `render_mode ∈ {0, 1, 3}`.
//! - `golf1_coverage_scorecard` — machine-generated per-field population report
//!   across the committed corpus-mini fixtures (printed as JSON).

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::{PdfDocument, TextSpan};

/// One-page PDF drawing "Hi" in 24 pt Helvetica under text matrix `tm`.
/// Mirrors the proven external lopdf build pattern in `xfa-wasm/tests`.
fn pdf_with_text_matrix(tm: [f64; 6]) -> Vec<u8> {
    let ops = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 24.into()]),
        Operation::new("Tm", tm.iter().map(|&v| Object::Real(v as f32)).collect()),
        Operation::new(
            "Tj",
            vec![Object::String(b"Hi".to_vec(), lopdf::StringFormat::Literal)],
        ),
        Operation::new("ET", vec![]),
    ];
    let content = Content { operations: ops }
        .encode()
        .expect("encode content");

    let mut doc = Document::with_version("1.7");
    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    }));
    let resources = dictionary! {
        "Font" => Object::Dictionary(dictionary! { "F1" => Object::Reference(font_id) }),
    };
    let stream_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
    let page = dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![
            Object::Integer(0), Object::Integer(0),
            Object::Integer(612), Object::Integer(792),
        ]),
        "Contents" => Object::Reference(stream_id),
        "Resources" => Object::Dictionary(resources),
    };
    let page_id = doc.add_object(Object::Dictionary(page));
    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![Object::Reference(page_id)]),
        "Count" => Object::Integer(1),
    }));
    if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
        d.set("Parent", Object::Reference(pages_id));
    }
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));

    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("save synthetic pdf");
    buf
}

fn spans_of(bytes: Vec<u8>) -> Vec<TextSpan> {
    PdfDocument::open(bytes)
        .expect("open pdf")
        .extract_text_blocks(0)
        .expect("extract text blocks")
        .into_iter()
        .flat_map(|b| b.spans)
        .collect()
}

fn corpus_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/corpus-mini")
        .join(name)
}

#[test]
fn transform_captures_rotation() {
    // Upright control: the composed x-basis is (near) horizontal -> |b| << |a|.
    let upright = spans_of(pdf_with_text_matrix([1.0, 0.0, 0.0, 1.0, 100.0, 400.0]));
    assert!(!upright.is_empty(), "no text extracted from upright pdf");
    let t = upright[0].transform.expect("upright transform present");
    assert!(
        t[1].abs() < t[0].abs() * 0.1,
        "upright x-basis is not horizontal: {t:?}"
    );

    // 90° rotation: the composed x-basis is (near) vertical -> |a| << |b|.
    let rotated = spans_of(pdf_with_text_matrix([0.0, 1.0, -1.0, 0.0, 100.0, 400.0]));
    assert!(!rotated.is_empty(), "no text extracted from rotated pdf");
    let r = rotated[0].transform.expect("rotated transform present");
    assert!(
        r[0].abs() < r[1].abs() * 0.1,
        "rotated x-basis is not vertical: {r:?}"
    );
}

#[test]
fn golf1_fields_consistent_on_real_pdf() {
    let bytes = std::fs::read(corpus_path("simple.pdf")).expect("read simple.pdf");
    let spans = spans_of(bytes);
    assert!(!spans.is_empty(), "simple.pdf yielded no spans");
    for s in &spans {
        let t = s.transform.expect("transform populated for a glyph span");
        // Translation equals the legacy origin.
        assert!((t[4] - s.x).abs() < 1e-6, "transform.e != span.x");
        assert!((t[5] - s.y).abs() < 1e-6, "transform.f != span.y");
        // The x-basis magnitude reconstructs font_size (same source coeffs).
        let scale = (t[0] * t[0] + t[1] * t[1]).sqrt() * 1000.0;
        assert!(
            (scale - s.font_size).abs() < 1e-6 * (s.font_size + 1.0),
            "transform scale {scale} inconsistent with font_size {}",
            s.font_size
        );
        // render_mode is always one of the three representable codes.
        let rm = s.render_mode.expect("render_mode populated");
        assert!(matches!(rm, 0 | 1 | 3), "render_mode {rm} not in {{0,1,3}}");
    }
}

fn pct(num: usize, den: usize) -> f64 {
    if den == 0 {
        0.0
    } else {
        (num as f64 / den as f64 * 1000.0).round() / 10.0
    }
}

#[test]
fn golf1_coverage_scorecard() {
    // Text-bearing fixtures that open without a password.
    let docs = [
        "simple.pdf",
        "multi-page.pdf",
        "acroform.pdf",
        "pdfa-2b.pdf",
        "zugferd.pdf",
        "signed-rsa.pdf",
    ];
    let mut rows = Vec::new();
    let (mut total, mut tf, mut rmode) = (0usize, 0usize, 0usize);

    for name in docs {
        let Ok(bytes) = std::fs::read(corpus_path(name)) else {
            continue;
        };
        let Ok(doc) = PdfDocument::open(bytes) else {
            continue;
        };
        let Ok(blocks) = doc.extract_text_blocks(0) else {
            continue;
        };
        let spans: Vec<_> = blocks.into_iter().flat_map(|b| b.spans).collect();
        if spans.is_empty() {
            continue;
        }
        let n = spans.len();
        let c = |f: &dyn Fn(&TextSpan) -> bool| spans.iter().filter(|s| f(s)).count();
        let with_tf = c(&|s| s.transform.is_some());
        let with_rm = c(&|s| s.render_mode.is_some());
        total += n;
        tf += with_tf;
        rmode += with_rm;
        rows.push(serde_json::json!({
            "doc": name,
            "spans": n,
            "transform_pct": pct(with_tf, n),
            "render_mode_pct": pct(with_rm, n),
            "font_weight_pct": pct(c(&|s| s.font_weight.is_some()), n),
            "is_serif_pct": pct(c(&|s| s.is_serif.is_some()), n),
            "is_monospace_pct": pct(c(&|s| s.is_monospace.is_some()), n),
        }));
    }

    let scorecard = serde_json::json!({
        "metric": "golf1_field_coverage",
        "scope": "page 0 of each fixture",
        "docs": rows,
        "totals": {
            "spans": total,
            "transform_pct": pct(tf, total),
            "render_mode_pct": pct(rmode, total),
        }
    });
    println!(
        "GOLF1_SCORECARD {}",
        serde_json::to_string_pretty(&scorecard).unwrap()
    );

    // Gate: transform + render_mode are populated for every glyph span.
    assert!(total > 0, "no spans extracted across corpus-mini");
    assert_eq!(tf, total, "transform must be 100% populated");
    assert_eq!(rmode, total, "render_mode must be 100% populated");
}

/// On-demand probe of embedded-font metadata coverage over the full `corpus/`
/// set. Ignored by default (slow; the committed corpus-mini fixtures use only
/// non-embedded standard-14 fonts, so `font_weight`/`is_serif`/`is_monospace`
/// are legitimately `None` there). Run with:
/// `cargo test -p pdf-engine --test golf1_extraction -- --ignored --nocapture`.
#[test]
#[ignore]
fn embedded_font_coverage_full_corpus() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    let mut pdfs: Vec<_> = std::fs::read_dir(&dir)
        .expect("read corpus dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "pdf"))
        .collect();
    pdfs.sort();
    pdfs.truncate(60);

    let (mut total, mut weight, mut serif, mut mono, mut scanned) = (0usize, 0, 0, 0, 0);
    for p in &pdfs {
        let Ok(bytes) = std::fs::read(p) else {
            continue;
        };
        let Ok(doc) = PdfDocument::open(bytes) else {
            continue;
        };
        let Ok(blocks) = doc.extract_text_blocks(0) else {
            continue;
        };
        let spans: Vec<_> = blocks.into_iter().flat_map(|b| b.spans).collect();
        if spans.is_empty() {
            continue;
        }
        scanned += 1;
        total += spans.len();
        weight += spans.iter().filter(|s| s.font_weight.is_some()).count();
        serif += spans.iter().filter(|s| s.is_serif.is_some()).count();
        mono += spans.iter().filter(|s| s.is_monospace.is_some()).count();
    }
    println!(
        "EMBEDDED_FONT_COVERAGE docs={scanned} spans={total} \
         font_weight={:.1}% is_serif={:.1}% is_monospace={:.1}%",
        pct(weight, total),
        pct(serif, total),
        pct(mono, total)
    );
    assert!(total > 0, "no spans across corpus sample");
    assert!(
        weight > 0,
        "font_weight never populated across {scanned} docs — embedded-font path may be broken"
    );
}
