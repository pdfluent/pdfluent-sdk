// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! `Page.write_text` through the facade (#643).
//!
//! Staged writers cross the door boundary as values, rebuild exactly,
//! and flush into the target rectangle; missing parts and file fonts
//! keep loud errors.

use pdfluent::compat::{
    AppendOptions, Font, FontOptions, PageMut, Rect, TextWriter, TextWriterOptions,
    WriteTextOptions,
};
use pdfluent::prelude::*;

const SAMPLE_PDF: &[u8] = include_bytes!("../../../fixtures/sample.pdf");

fn open_sample() -> PdfDocument {
    PdfDocument::from_bytes(SAMPLE_PDF).expect("sample opens")
}

fn helvetica() -> Font {
    let mut options = FontOptions::default();
    options.fontname = Some("helv".to_string());
    Font::new(&options).expect("helvetica resolves")
}

fn staged_marker() -> TextWriter {
    let font = helvetica();
    let mut options = TextWriterOptions::default();
    options.opacity = 1.0;
    let mut writer = TextWriter::new(
        Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 200.0,
            y1: 200.0,
        },
        &options,
    );
    let mut append = AppendOptions::default();
    append.font = Some(&font);
    writer
        .append(
            pdfluent::compat::Point { x: 72.0, y: 720.0 },
            "roundtrip-marker",
            &append,
        )
        .expect("append works");
    writer
}

fn write_options(rect: Option<Rect>, writers: Option<pdfluent::compat::Value>) -> WriteTextOptions {
    let mut options = WriteTextOptions::default();
    options.rect = rect;
    options.writers = writers;
    options
}

#[test]
fn staged_writers_rebuild_and_flush() {
    let writer = staged_marker();
    let value = writer.staged_value();
    let mut doc = open_sample();
    PageMut::new(&mut doc, 0)
        .write_text(&write_options(
            Some(Rect {
                x0: 72.0,
                y0: 700.0,
                x1: 300.0,
                y1: 740.0,
            }),
            Some(pdfluent::compat::Value::List(vec![value])),
        ))
        .expect("write works");
    let bytes = doc.to_bytes().expect("save works");
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.contains("roundtrip-marker"),
        "marker survives the round trip"
    );
}

#[test]
fn missing_parts_fail_loudly() {
    let writer = staged_marker();
    let value = writer.staged_value();
    let mut doc = open_sample();

    let err = PageMut::new(&mut doc, 0)
        .write_text(&write_options(None, None))
        .expect_err("bare call fails");
    assert!(err.to_string().contains("write_text needs"), "{err}");

    let err = PageMut::new(&mut doc, 0)
        .write_text(&write_options(
            Some(Rect {
                x0: 0.0,
                y0: 0.0,
                x1: 10.0,
                y1: 10.0,
            }),
            None,
        ))
        .expect_err("rect without writers fails");
    assert!(
        err.to_string().contains("write_text needs writers"),
        "{err}"
    );

    let err = PageMut::new(&mut doc, 0)
        .write_text(&write_options(
            None,
            Some(pdfluent::compat::Value::List(vec![value])),
        ))
        .expect_err("writers without rect fail");
    assert!(err.to_string().contains("write_text needs a rect"), "{err}");
}

#[test]
fn file_fonts_keep_their_flush_error() {
    let mut doc = open_sample();
    let mut run = std::collections::HashMap::new();
    run.insert(
        "pos".to_string(),
        pdfluent::compat::Value::List(vec![
            pdfluent::compat::Value::Float(72.0),
            pdfluent::compat::Value::Float(720.0),
        ]),
    );
    run.insert(
        "text".to_string(),
        pdfluent::compat::Value::Str("x".to_string()),
    );
    run.insert(
        "font_base".to_string(),
        pdfluent::compat::Value::Str("not-a-builtin-face".to_string()),
    );
    let mut writer = std::collections::HashMap::new();
    writer.insert(
        "rect".to_string(),
        pdfluent::compat::Value::List(vec![
            pdfluent::compat::Value::Float(0.0),
            pdfluent::compat::Value::Float(0.0),
            pdfluent::compat::Value::Float(200.0),
            pdfluent::compat::Value::Float(200.0),
        ]),
    );
    writer.insert("opacity".to_string(), pdfluent::compat::Value::Float(1.0));
    writer.insert("color".to_string(), pdfluent::compat::Value::Null);
    writer.insert("union".to_string(), pdfluent::compat::Value::Null);
    writer.insert(
        "runs".to_string(),
        pdfluent::compat::Value::List(vec![pdfluent::compat::Value::Dict(run)]),
    );
    let err = PageMut::new(&mut doc, 0)
        .write_text(&write_options(
            Some(Rect {
                x0: 0.0,
                y0: 0.0,
                x1: 200.0,
                y1: 200.0,
            }),
            Some(pdfluent::compat::Value::List(vec![
                pdfluent::compat::Value::Dict(writer),
            ])),
        ))
        .expect_err("file font fails");
    assert!(
        err.to_string().contains("file fonts cannot be written yet"),
        "{err}"
    );
}
