//! Report, per CFF font, whether the missing-space repair can act — and if
//! not, which precondition stopped it.
//!
//! ```text
//! cargo run -p pdf-manip --example pdfa_space_probe -- input.pdf
//! ```

use lopdf::{Document, Object};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: pdfa_space_probe <pdf>");
    let data = std::fs::read(&path).expect("read");
    let mut doc = Document::load_mem(&data).expect("load");

    // Run the pipeline first: the repair acts on the converted document, not
    // the source, and earlier passes change both the program and the widths.
    let opts = pdf_manip::pdfa::PdfAConvertOptions::default();
    pdf_manip::pdfa::convert_document(&mut doc, &opts).expect("convert");

    for (font_id, obj) in &doc.objects {
        let Object::Dictionary(font) = obj else {
            continue;
        };
        if font.get(b"Type").ok().and_then(|o| o.as_name().ok()) != Some(b"Font") {
            continue;
        }
        let base = font
            .get(b"BaseFont")
            .ok()
            .and_then(|o| o.as_name().ok())
            .map(|n| String::from_utf8_lossy(n).to_string())
            .unwrap_or_default();
        let subtype = font
            .get(b"Subtype")
            .ok()
            .and_then(|o| o.as_name().ok())
            .map(|n| String::from_utf8_lossy(n).to_string())
            .unwrap_or_default();

        let fd_id = match font.get(b"FontDescriptor") {
            Ok(Object::Reference(id)) => *id,
            _ => {
                println!("{base:<28} {subtype:<10} no FontDescriptor");
                continue;
            }
        };
        let Some(Object::Dictionary(fd)) = doc.objects.get(&fd_id) else {
            println!("{base:<28} {subtype:<10} descriptor unresolvable");
            continue;
        };
        let ff3 = match fd.get(b"FontFile3") {
            Ok(Object::Reference(id)) => Some(*id),
            _ => None,
        };
        let Some(ff_id) = ff3 else {
            let which = ["FontFile", "FontFile2", "FontFile3"]
                .iter()
                .find(|k| fd.has(k.as_bytes()))
                .copied()
                .unwrap_or("none");
            println!("{base:<28} {subtype:<10} program={which} (repair is CFF-only)");
            continue;
        };

        let Some(program) = doc.objects.get(&ff_id).and_then(|o| match o {
            Object::Stream(s) => {
                let mut s = s.clone();
                let _ = s.decompress();
                Some(s.content)
            }
            _ => None,
        }) else {
            println!("{base:<28} {subtype:<10} FontFile3 unreadable");
            continue;
        };

        let outcome = match pdf_manip::cff_append::try_append_blank_glyph(&program, 0.0) {
            Ok(_) => "OK".to_string(),
            Err(reason) => format!("{reason:?}"),
        };
        println!("{base:<28} {subtype:<10} obj={font_id:?} cff_append={outcome}");
    }
}
