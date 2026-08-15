//! Produce a PDF whose text has been replaced with scripts the source never
//! contained, for verification with an outside tool (pdftotext, mutool,
//! veraPDF) rather than only against our own assertions.
//!
//! Usage:
//!   cargo run -p pdf-manip --example unicode_replace_demo -- <font.ttf> <out.pdf> [text]

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, Stream, StringFormat};
use pdf_manip::text_edit::{
    begin_text_edit, DocumentRevision, FitPolicy, FontFallback, ReplaceOptions, TextQuery,
};
use pdf_manip::unicode_font::UnicodeFont;

const SOURCE: &str = "REPLACE ME";

fn make_doc() -> Document {
    let mut doc = Document::with_version("1.7");
    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    }));
    let mut fonts = Dictionary::new();
    fonts.set("F1", Object::Reference(font_id));

    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), Object::Real(18.0)]),
            Operation::new("Td", vec![Object::Real(72.0), Object::Real(700.0)]),
            Operation::new(
                "Tj",
                vec![Object::String(
                    SOURCE.as_bytes().to_vec(),
                    StringFormat::Literal,
                )],
            ),
            Operation::new("ET", vec![]),
        ],
    };
    let stream_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        content.encode().unwrap(),
    )));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Page",
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => Object::Reference(stream_id),
        "Resources" => Object::Dictionary(dictionary! { "Font" => Object::Dictionary(fonts) }),
    }));
    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Pages", "Kids" => vec![Object::Reference(page_id)], "Count" => 1_i64,
    }));
    if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
        d.set("Parent", Object::Reference(pages_id));
    }
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog", "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));
    doc
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: unicode_replace_demo <font.ttf> <out.pdf> [text]");
        std::process::exit(2);
    }
    let replacement = args
        .get(3)
        .cloned()
        .unwrap_or_else(|| "zażółć gęślą jaźń — Съешь ещё — Γειά σου".to_string());

    let font_data = std::fs::read(&args[1]).expect("read font");
    let font = UnicodeFont::from_bytes(font_data).expect("parse font");

    let missing = font.missing_chars(&replacement);
    if !missing.is_empty() {
        eprintln!("font '{}' lacks: {missing:?}", font.name());
        std::process::exit(1);
    }

    let mut doc = make_doc();
    let rev = {
        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        DocumentRevision::from_source_bytes(&buf)
    };

    let mut session = begin_text_edit(&mut doc, rev).expect("begin");
    let matches = session.find_text(TextQuery::exact(SOURCE)).expect("find");
    session
        .stage_replace(
            &matches[0].id,
            &replacement,
            ReplaceOptions::default()
                .font_fallback(FontFallback::EmbedUnicode(font))
                .fit(if std::env::var("DEMO_SHRINK").is_ok() {
                    FitPolicy::ShrinkToFit
                } else {
                    FitPolicy::Exact
                }),
        )
        .expect("stage");
    let report = session.commit().expect("commit");

    doc.save(&args[2]).expect("save");
    println!("replacements applied: {}", report.replacements_applied);
    for r in &report.results {
        for d in &r.diagnostics {
            println!("  [{}] {}", d.code, d.message);
        }
    }
    println!("wrote {}", args[2]);
    println!("expected text: {replacement}");
}
