//! Debug CFF glyph widths for all CIDFontType0 fonts in a PDF.
use lopdf::{Document, Object};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: debug_cff_glyph_widths <pdf>");
    let data = std::fs::read(&path).expect("read");
    let doc = Document::load_mem(&data).expect("parse");

    let mut font_ids: Vec<_> = doc.objects.keys().copied().collect();
    font_ids.sort();

    for font_id in font_ids {
        let obj = &doc.objects[&font_id];
        let dict = match obj {
            Object::Dictionary(d) => d,
            _ => continue,
        };
        let _subtype = match dict.get(b"Subtype").ok() {
            Some(Object::Name(n)) if n == b"CIDFontType0" => "CIDFontType0",
            _ => continue,
        };
        let base_font = match dict.get(b"BaseFont").ok() {
            Some(Object::Name(n)) => String::from_utf8_lossy(n).to_string(),
            _ => "?".to_string(),
        };
        let fd_id = match dict.get(b"FontDescriptor").ok() {
            Some(Object::Reference(r)) => *r,
            _ => continue,
        };
        let fd = match doc.objects.get(&fd_id) {
            Some(Object::Dictionary(d)) => d,
            _ => continue,
        };
        let ff3_id = match fd.get(b"FontFile3").ok() {
            Some(Object::Reference(r)) => *r,
            _ => continue,
        };
        let cff_data = match doc.objects.get(&ff3_id) {
            Some(Object::Stream(s)) => {
                let mut s2 = s.clone();
                let _ = s2.decompress();
                s2.content.clone()
            }
            _ => continue,
        };

        let table = match cff_parser::Table::parse(&cff_data) {
            Some(t) => t,
            None => {
                println!("{base_font}: CFF PARSE FAILED");
                continue;
            }
        };

        let n = table.number_of_glyphs();
        let matrix = table.matrix();
        let scale = if matrix.sx.abs() > f32::EPSILON {
            matrix.sx * 1000.0
        } else {
            1.0
        };

        let mut widths: Vec<(u16, i64)> = Vec::new();
        let mut none_count = 0u32;
        for i in 0..n {
            let gid = cff_parser::GlyphId(i);
            match table.glyph_width(gid) {
                Some(w) => {
                    let scaled = (w as f64 * scale as f64).round() as i64;
                    let cid = table.glyph_cid(gid).unwrap_or(i);
                    widths.push((cid, scaled));
                }
                None => none_count += 1,
            }
        }

        // Get DW and W from dict
        let dw = match dict.get(b"DW").ok() {
            Some(Object::Integer(d)) => Some(*d),
            _ => None,
        };

        println!("{base_font}: n_glyphs={n} scale={scale:.4} glyph_widths={} none={none_count} dict_DW={dw:?}",
            widths.len());
        println!(
            "  CFF widths (cid,width): {:?}",
            &widths[..widths.len().min(10)]
        );
        if widths.len() > 10 {
            println!("  ... {} more", widths.len() - 10);
        }
    }
}
