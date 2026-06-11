//! Throwaway measurement: emit per-doc JSON for A-vs-B business-value analysis.
//! For each doc: A = plain extract_all_text (geometric), B = logical text,
//! and the struct-tree leaf elements {type, page, text} as ground-truth units.
//! Run: cargo run -q -p pdf-engine --example structtree_measure -- <pdf>

use std::collections::HashMap;

use kurbo::{Affine, BezPath, Rect};
use pdf_compliance::tagged;
use pdf_engine::PdfDocument;
use pdf_render::pdf_interpret::cmap::BfString;
use pdf_render::pdf_interpret::font::Glyph;
use pdf_render::pdf_interpret::PageExt;
use pdf_render::pdf_interpret::{
    interpret_page, BlendMode, ClipPath, Context, Device, GlyphDrawMode, Image,
    InterpreterSettings, Paint, PathDrawMode, SoftMask,
};

struct McidDev {
    stack: Vec<Option<i32>>,
    by_mcid: HashMap<i32, String>,
    untagged_chars: usize,
}
impl McidDev {
    fn new() -> Self {
        Self {
            stack: Vec::new(),
            by_mcid: HashMap::new(),
            untagged_chars: 0,
        }
    }
    fn cur(&self) -> Option<i32> {
        self.stack.iter().rev().copied().flatten().next()
    }
}
impl Device<'_> for McidDev {
    fn set_soft_mask(&mut self, _: Option<SoftMask<'_>>) {}
    fn set_blend_mode(&mut self, _: BlendMode) {}
    fn draw_path(&mut self, _: &BezPath, _: Affine, _: &Paint<'_>, _: &PathDrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'_>>, _: BlendMode) {}
    fn draw_image(&mut self, _: Image<'_, '_>, _: Affine) {}
    fn pop_clip_path(&mut self) {}
    fn pop_transparency_group(&mut self) {}
    fn draw_glyph(
        &mut self,
        g: &Glyph<'_>,
        _: Affine,
        _: Affine,
        _: &Paint<'_>,
        _: &GlyphDrawMode,
    ) {
        let t = match g.as_unicode() {
            Some(BfString::Char(c)) => c.to_string(),
            Some(BfString::String(s)) => s,
            None => return,
        };
        match self.cur() {
            Some(m) => self.by_mcid.entry(m).or_default().push_str(&t),
            None => self.untagged_chars += t.chars().count(),
        }
    }
    fn begin_marked_content(&mut self, _: &[u8], mcid: Option<i32>) {
        self.stack.push(mcid);
    }
    fn end_marked_content(&mut self) {
        self.stack.pop();
    }
}

fn jstr(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => {}
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => {}
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: <pdf>");
    let data = std::fs::read(&path).expect("read");
    let doc = PdfDocument::open(data).expect("open");
    let pdf = doc.pdf();
    let n = pdf.pages().len();

    // (page, mcid) -> text, plus untagged char count
    let mut map: HashMap<(usize, i32), String> = HashMap::new();
    let mut untagged_chars = 0usize;
    for pi in 0..n {
        let page = &pdf.pages()[pi];
        let (w, h) = page.render_dimensions();
        let mut dev = McidDev::new();
        let mut ctx = Context::new(
            page.initial_transform(false),
            Rect::new(0.0, 0.0, w as f64, h as f64),
            page.xref(),
            InterpreterSettings::default(),
        );
        interpret_page(page, &mut ctx, &mut dev);
        untagged_chars += dev.untagged_chars;
        for (m, t) in dev.by_mcid {
            map.entry((pi, m)).or_default().push_str(&t);
        }
    }

    let plain = doc.extract_all_text(); // A (geometric)
    let mut elements: Vec<(String, String)> = Vec::new(); // (type, text)
    let mut n_table = 0usize;
    let mut n_heading = 0usize;
    if let Some(tree) = tagged::parse(pdf) {
        for e in tree.reading_order() {
            if e.standard_type == "Table" {
                n_table += 1;
            }
            if e.is_heading() {
                n_heading += 1;
            }
            let mut text = String::new();
            if let Some(at) = &e.actual_text {
                text.push_str(at);
            } else if let Some(p) = e.page_index {
                for m in &e.mcids {
                    if let Some(t) = map.get(&(p, *m)) {
                        text.push_str(t);
                        text.push(' ');
                    }
                }
            }
            let t = text.trim();
            if !t.is_empty() {
                elements.push((e.standard_type.clone(), t.to_string()));
            }
        }
    }
    // B (logical) = production method
    let logical = doc.extract_text_logical();

    let name = std::path::Path::new(&path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let elems_json: Vec<String> = elements
        .iter()
        .map(|(ty, tx)| format!("{{\"type\":{},\"text\":{}}}", jstr(ty), jstr(tx)))
        .collect();
    println!(
        "{{\"doc\":{},\"pages\":{},\"n_table_elem\":{},\"n_heading\":{},\"untagged_chars\":{},\"plain\":{},\"logical\":{},\"elements\":[{}]}}",
        jstr(&name),
        n,
        n_table,
        n_heading,
        untagged_chars,
        jstr(&plain),
        jstr(&logical),
        elems_json.join(","),
    );
}
