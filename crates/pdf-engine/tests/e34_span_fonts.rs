//! E3/E4 (#643): spans carry the document's font names and the descriptor's
//! font metrics, as the L1 f1040 recordings show.
//!
//! E3: `TextSpan.font_name` is the font dictionary's /BaseFont with any
//! 6-letter subset prefix stripped -- the name the page set its text in --
//! not the substitute face the renderer draws it on. f1040's embedded CFF
//! fonts used to report `Helvetica` / `Helvetica-Bold`.
//!
//! E4: `TextSpan.font_metrics` for embedded Type1/CFF fonts is the
//! FontDescriptor /Ascent /Descent (/CapHeight /XHeight), in /1000 em --
//! not the substitute face's AFM pair (0.718/-0.207 for Helvetica).
//!
//! These tests run with `ExtractOptions::document_font_names` set; the
//! default output is pinned byte-identical by
//! `default_output_on_f1040_is_byte_identical_to_cc2f37b` below -- our own
//! regression baseline, not a spec expectation. The goldens are frozen:
//! the test never rewrites them from current output; they move only by an
//! explicit commit that states why. Dumped at cc2f37b, moved once in
//! bae0eda because 08b3b6a fixes char-spaced words ("Di gi t al A s s e t s"
//! reads "Digital Assets" now).
//!
//! NOTE on `ITCFranklinGothicStd-Demi`: the document spells it `Demi` in
//! every name source (/BaseFont, descriptor /FontName, the CFF Name INDEX,
//! the XFA `psName`), while the L1 recordings spell it `Dem`. The stripped
//! document name is 25 chars, the only recorded name over 24 chars, so the
//! recording looks like a 24-char truncation on the black-box side. The
//! engine reports the document's own name exactly; matching the truncated
//! recording is a compat-side decision (see REPORT.md).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_engine::{to_json, ExtractOptions, PdfDocument, TextSpan};

/// One known span of the L1 f1040 recording: where it is (y-up user space,
/// the recording's y-down box flipped over the 792pt page), how big, and
/// what the recording shows for it.
struct Pin {
    label: &'static str,
    size: (f64, f64),
    x: Option<(f64, f64)>,
    y: Option<(f64, f64)>,
    font: &'static str,
    asc: f64,
    desc: f64,
}

/// The 8 recorded (font, ascender, descender) pairs, each pinned to a span
/// only that font sets: unique sizes where the recording has one, a recorded
/// bbox otherwise.
fn pins() -> Vec<Pin> {
    vec![
        Pin {
            label: "1040 header",
            size: (23.4, 24.6),
            x: None,
            y: None,
            font: "HelveticaNeueLTStd-BlkCn",
            asc: 0.972,
            desc: -0.230,
        },
        Pin {
            label: "year 20",
            size: (19.4, 20.6),
            x: Some((290.0, 323.3)),
            y: None,
            font: "HelveticaNeueLTStd-BdOu",
            asc: 0.992,
            desc: -0.27,
        },
        Pin {
            label: "year 25",
            size: (19.4, 20.6),
            x: Some((323.3, 355.0)),
            y: None,
            font: "HelveticaNeueLTStd-Blk",
            asc: 0.953,
            desc: -0.232,
        },
        Pin {
            label: "return title",
            size: (11.4, 12.6),
            x: None,
            y: None,
            font: "ITCFranklinGothicStd-Demi",
            asc: 0.934,
            desc: -0.25,
        },
        Pin {
            label: "Income head",
            size: (10.4, 11.6),
            x: None,
            y: None,
            font: "HelveticaNeueLTStd-BdCn",
            asc: 0.961,
            desc: -0.224,
        },
        Pin {
            label: "Filing Status head",
            size: (9.4, 10.6),
            x: Some((0.0, 200.0)),
            y: None,
            font: "HelveticaNeueLTStd-BdCn",
            asc: 0.961,
            desc: -0.224,
        },
        Pin {
            label: "bottom 1040",
            size: (9.4, 10.6),
            x: Some((400.0, 612.0)),
            y: None,
            font: "HelveticaNeueLTStd-Bd",
            asc: 0.975,
            desc: -0.218,
        },
        Pin {
            label: "OMB line",
            size: (6.4, 7.6),
            x: Some((354.0, 424.0)),
            y: Some((744.0, 757.0)),
            font: "HelveticaNeueLTStd-Roman",
            asc: 0.952,
            desc: -0.214,
        },
        Pin {
            label: "SSN line",
            size: (6.4, 7.6),
            x: Some((470.0, 570.0)),
            y: Some((696.0, 709.0)),
            font: "HelveticaNeueLTStd-Bd",
            asc: 0.975,
            desc: -0.218,
        },
        Pin {
            label: "main-home line",
            size: (6.4, 7.6),
            x: Some((468.0, 578.0)),
            y: Some((649.0, 662.0)),
            font: "HelveticaNeueLTStd-Cn",
            asc: 0.932,
            desc: -0.212,
        },
        Pin {
            label: "Presidential Election Campaign",
            size: (7.4, 8.6),
            x: Some((470.0, 574.0)),
            y: Some((624.0, 638.0)),
            font: "HelveticaNeueLTStd-BdCn",
            asc: 0.961,
            desc: -0.224,
        },
        Pin {
            label: "rotated Form",
            size: (6.4, 7.6),
            x: Some((25.0, 55.0)),
            y: Some((740.0, 772.0)),
            font: "HelveticaNeueLTStd-Roman",
            asc: 0.952,
            desc: -0.214,
        },
    ]
}

fn f1040_p0_spans() -> Vec<TextSpan> {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/f1040.pdf"
    ))
    .expect("fixture");
    let doc = PdfDocument::open(bytes).expect("open");
    let mut options = ExtractOptions::window(0, 1);
    options.learned = false;
    options.document_font_names = true;
    doc.extract_blocks(&options)
        .expect("blocks")
        .into_iter()
        .flat_map(|block| block.spans)
        .collect()
}

fn spans_matching<'a>(spans: &'a [TextSpan], pin: &Pin) -> Vec<&'a TextSpan> {
    spans
        .iter()
        .filter(|s| {
            (pin.size.0..=pin.size.1).contains(&s.font_size)
                && pin.x.is_none_or(|(x0, x1)| {
                    let cx = s.x + s.width / 2.0;
                    (x0..=x1).contains(&cx)
                })
                && pin.y.is_none_or(|(y0, y1)| {
                    let cy = s.y + s.height / 2.0;
                    (y0..=y1).contains(&cy)
                })
        })
        .collect()
}

/// E3: every pinned span reports the stripped /BaseFont, not the substitute.
#[test]
fn f1040_spans_report_the_document_font_names() {
    let spans = f1040_p0_spans();
    assert!(!spans.is_empty());
    for pin in pins() {
        let hits = spans_matching(&spans, &pin);
        assert!(
            !hits.is_empty(),
            "no spans matched pin '{}' ({} spans on page)",
            pin.label,
            spans.len()
        );
        for s in hits {
            assert_eq!(
                s.font_name.as_deref(),
                Some(pin.font),
                "pin '{}': span {s:?}",
                pin.label
            );
        }
    }
}

/// E4: every pinned span reports the recorded ascender/descender pair.
#[test]
fn f1040_spans_report_the_descriptor_metrics() {
    let spans = f1040_p0_spans();
    for pin in pins() {
        let hits = spans_matching(&spans, &pin);
        assert!(!hits.is_empty(), "no spans matched pin '{}'", pin.label);
        for s in hits {
            let m = s
                .font_metrics
                .as_ref()
                .unwrap_or_else(|| panic!("pin '{}': no metrics: {s:?}", pin.label));
            assert!(
                (m.ascent / 1000.0 - pin.asc).abs() < 1e-3,
                "pin '{}': ascent {} vs {}: {s:?}",
                pin.label,
                m.ascent / 1000.0,
                pin.asc
            );
            assert!(
                (m.descent / 1000.0 - pin.desc).abs() < 1e-3,
                "pin '{}': descent {} vs {}: {s:?}",
                pin.label,
                m.descent / 1000.0,
                pin.desc
            );
        }
    }
}

/// E3, the other direction: no f1040 p0 span reports a substitute face. The
/// recording names zero base-14 spans on this page.
#[test]
fn no_substitute_face_names_on_f1040_p0() {
    const SUBSTITUTES: [&str; 14] = [
        "Helvetica",
        "Helvetica-Bold",
        "Helvetica-Oblique",
        "Helvetica-BoldOblique",
        "Courier",
        "Courier-Bold",
        "Courier-Oblique",
        "Courier-BoldOblique",
        "Times-Roman",
        "Times-Bold",
        "Times-Italic",
        "Times-BoldItalic",
        "Symbol",
        "ZapfDingbats",
    ];
    let spans = f1040_p0_spans();
    let bad: Vec<_> = spans
        .iter()
        .filter(|s| {
            s.font_name
                .as_deref()
                .is_some_and(|n| SUBSTITUTES.contains(&n))
        })
        .take(5)
        .collect();
    assert!(bad.is_empty(), "spans with substitute names: {bad:?}");
}

// ---- The default output, pinned byte-identical ----

fn golden_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

/// Byte equality with a readable failure: the offset of the first
/// difference and the lines around it, not two megabytes of `assert_eq`.
fn assert_same_bytes(what: &str, produced: &str, expected: &str) {
    if produced == expected {
        return;
    }
    let mut at = produced
        .bytes()
        .zip(expected.bytes())
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| produced.len().min(expected.len()));
    while !produced.is_char_boundary(at) {
        at -= 1;
    }
    let line = produced[..at].matches('\n').count() + 1;
    let lo = produced[..at].rfind('\n').map_or(0, |i| i + 1);
    panic!(
        "{what} moved off the golden at byte {at} (line {line}; {} vs {} bytes):\nproduced: {}\nexpected: {}",
        produced.len(),
        expected.len(),
        produced[lo..].lines().next().unwrap_or(""),
        expected[expected[..at].rfind('\n').map_or(0, |i| i + 1)..]
            .lines()
            .next()
            .unwrap_or(""),
    );
}

/// The default output on f1040 is byte-identical to the golden: the E34 font
/// truth (E3 names, E4 descriptor metrics) is option-gated (#643
/// follow-up), and a caller that never heard of the option reads exactly
/// what it read before -- substitute-face names and metrics, and the style
/// hints those imply.
///
/// OUR OWN REGRESSION BASELINE, NOT A SPEC EXPECTATION: the goldens are
/// frozen and never rewritten from current output by this test -- they move
/// only by an explicit commit that states why. Dumped at cc2f37b, moved
/// once in bae0eda because 08b3b6a fixes char-spaced words (f1040's
/// "Di gi t al A s s e t s" reads "Digital Assets" now); the names keep
/// cc2f37b as provenance. A golden that moves here means the default path
/// moved -- read the diff first.
#[test]
fn default_output_on_f1040_is_byte_identical_to_cc2f37b() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/f1040.pdf"
    ))
    .expect("fixture");
    let doc = PdfDocument::open(bytes).expect("open");
    let blocks = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let produced = to_json(&blocks);
    // to_json carries no fontMetrics, so the per-span (name, metrics) is
    // pinned beside it: one line per span, block and span index for
    // identity.
    let mut projection = String::new();
    for block in &blocks {
        for (s, span) in block.spans.iter().enumerate() {
            use std::fmt::Write as _;
            writeln!(
                projection,
                "{} {s} {:?} {:?}",
                block.index, span.font_name, span.font_metrics
            )
            .expect("write projection");
        }
    }
    let json_path = golden_path("f1040_default_cc2f37b.json");
    let metrics_path = golden_path("f1040_default_metrics_cc2f37b.txt");
    let expected = std::fs::read_to_string(&json_path).expect("json golden");
    assert_same_bytes("default to_json", &produced, &expected);
    let expected_metrics = std::fs::read_to_string(&metrics_path).expect("metrics golden");
    assert_same_bytes("default span names/metrics", &projection, &expected_metrics);
}

// ---- The subset-prefix proof on a synthetic page ----

/// A minimal Type1 program ("TestFont", glyphs .notdef/A/B), eexec-encrypted
/// the way embedded Type1 programs are. Its internal /FontName is beside the
/// point: the span must report the dictionary's /BaseFont.
fn test_type1_program() -> Vec<u8> {
    fn number(value: i32) -> Vec<u8> {
        match value {
            -107..=107 => vec![(value + 139) as u8],
            108..=1131 => {
                let value = value - 108;
                vec![((value / 256) + 247) as u8, (value % 256) as u8]
            }
            _ => panic!("unsupported test number {value}"),
        }
    }
    fn charstring(width: i32) -> Vec<u8> {
        let mut data = number(0);
        data.extend(number(width));
        data.extend([13, 14]);
        data
    }
    fn eexec_hex(data: &[u8]) -> String {
        let mut r: u32 = 55665;
        let mut out = String::new();
        for plain in [0u8, 0, 0, 0].into_iter().chain(data.iter().copied()) {
            let cipher = plain ^ ((r >> 8) as u8);
            r = ((cipher as u32 + r).wrapping_mul(52845) + 22719) & 0xFFFF;
            use std::fmt::Write as _;
            write!(&mut out, "{cipher:02X}").expect("hex write");
        }
        out
    }

    let mut eexec = Vec::new();
    eexec.extend_from_slice(b"/Private 8 dict dup begin\n");
    eexec.extend_from_slice(b"/lenIV -1 def\n");
    eexec.extend_from_slice(b"/Subrs 0 array\n");
    eexec.extend_from_slice(b"/CharStrings 3 dict dup begin\n");
    for (name, bytes) in [
        (".notdef", charstring(500)),
        ("A", charstring(700)),
        ("B", charstring(710)),
    ] {
        eexec.extend_from_slice(format!("/{name} {} RD ", bytes.len()).as_bytes());
        eexec.extend_from_slice(&bytes);
        eexec.extend_from_slice(b" ND\n");
    }
    eexec.extend_from_slice(b"end\nend\n");
    format!(
        "%!PS-AdobeFont-1.0: TestFont 1.0\n\
         /FontName /TestFont def\n\
         /FontType 1 def\n\
         /FontMatrix [0.001 0 0 0.001 0 0] readonly def\n\
         /Encoding StandardEncoding def\n\
         currentfile eexec\n{}\ncleartomark\n",
        eexec_hex(&eexec)
    )
    .into_bytes()
}

/// E3+E4 on a page we built: the embedded Type1's subset-prefixed /BaseFont
/// comes out stripped, and its descriptor metrics (not the matched standard
/// face's AFM) land on the span.
#[test]
fn a_subset_prefixed_basefont_is_reported_stripped() {
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Document, Object, Stream};

    let program = test_type1_program();
    let mut doc = Document::with_version("1.7");
    let font_file_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, program)));
    let descriptor_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"FontDescriptor".to_vec()),
        "FontName" => Object::Name(b"ABCDEF+HelveticaNeueLTStd-Roman".to_vec()),
        "Flags" => Object::Integer(32),
        "FontBBox" => Object::Array(vec![
            Object::Integer(-166), Object::Integer(-214),
            Object::Integer(1076), Object::Integer(952),
        ]),
        "ItalicAngle" => Object::Integer(0),
        "Ascent" => Object::Integer(952),
        "Descent" => Object::Integer(-214),
        "CapHeight" => Object::Integer(714),
        "XHeight" => Object::Integer(517),
        "StemV" => Object::Integer(84),
        "FontFile" => Object::Reference(font_file_id),
    }));
    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"ABCDEF+HelveticaNeueLTStd-Roman".to_vec()),
        "FirstChar" => Object::Integer(65),
        "LastChar" => Object::Integer(66),
        "Widths" => Object::Array(vec![Object::Integer(700), Object::Integer(710)]),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
        "FontDescriptor" => Object::Reference(descriptor_id),
    }));
    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 24.into()]),
            Operation::new(
                "Tm",
                vec![
                    1.into(),
                    0.into(),
                    0.into(),
                    1.into(),
                    72.into(),
                    770.into(),
                ],
            ),
            Operation::new(
                "Tj",
                vec![Object::String(
                    vec![0x41, 0x42],
                    lopdf::StringFormat::Hexadecimal,
                )],
            ),
            Operation::new("ET", vec![]),
        ],
    }
    .encode()
    .expect("encode content");
    let stream_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![
            Object::Integer(0), Object::Integer(0),
            Object::Integer(612), Object::Integer(792),
        ]),
        "Contents" => Object::Reference(stream_id),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! { "F1" => Object::Reference(font_id) }),
        }),
    }));
    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![Object::Reference(page_id)]),
        "Count" => Object::Integer(1),
    }));
    if let Ok(Object::Dictionary(d)) = doc.get_object_mut(page_id) {
        d.set("Parent", Object::Reference(pages_id));
    }
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));
    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("save synthetic pdf");

    let text = PdfDocument::open(buf.clone())
        .expect("open")
        .extract_text(0)
        .expect("text");
    assert_eq!(text.trim(), "AB");

    let engine = PdfDocument::open(buf).expect("open");
    let mut options = ExtractOptions::window(0, 1);
    options.learned = false;
    options.document_font_names = true;
    let spans: Vec<_> = engine
        .extract_blocks(&options)
        .expect("blocks")
        .into_iter()
        .flat_map(|block| block.spans)
        .collect();
    assert!(!spans.is_empty());
    for s in &spans {
        assert_eq!(
            s.font_name.as_deref(),
            Some("HelveticaNeueLTStd-Roman"),
            "subset prefix stripped: {s:?}"
        );
        let m = s.font_metrics.as_ref().expect("descriptor metrics");
        assert_eq!((m.ascent, m.descent), (952.0, -214.0));
        assert_eq!((m.cap_height, m.x_height), (Some(714.0), Some(517.0)));
    }
}
