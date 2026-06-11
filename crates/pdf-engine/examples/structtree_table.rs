//! Throwaway prototype: StructTree -> Table -> TR -> TD -> cell spans -> TSV.
//! Proves table extraction is now feasible via the (page_index, MCID) -> text
//! architecture. Run: cargo run -q -p pdf-engine --example structtree_table -- <pdf> [max_tables]

use std::collections::HashMap;

use kurbo::{Affine, BezPath, Rect};
use pdf_compliance::tagged::{self, StructElement};
use pdf_engine::PdfDocument;
use pdf_render::pdf_interpret::cmap::BfString;
use pdf_render::pdf_interpret::font::Glyph;
use pdf_render::pdf_interpret::PageExt;
use pdf_render::pdf_interpret::{
    interpret_page, BlendMode, ClipPath, Context, Device, GlyphDrawMode, Image,
    InterpreterSettings, Paint, PathDrawMode, SoftMask,
};

struct Dev {
    stack: Vec<Option<i32>>,
    by_mcid: HashMap<i32, String>,
}
impl Dev {
    fn new() -> Self {
        Self {
            stack: Vec::new(),
            by_mcid: HashMap::new(),
        }
    }
    fn cur(&self) -> Option<i32> {
        self.stack.iter().rev().copied().flatten().next()
    }
}
impl Device<'_> for Dev {
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
        if let Some(m) = self.cur() {
            self.by_mcid.entry(m).or_default().push_str(&t);
        }
    }
    fn begin_marked_content(&mut self, _: &[u8], mcid: Option<i32>) {
        self.stack.push(mcid);
    }
    fn end_marked_content(&mut self) {
        self.stack.pop();
    }
}

/// Collect all text under a struct element's subtree (its own MCIDs + descendants').
fn collect_text(e: &StructElement, map: &HashMap<(usize, i32), String>) -> String {
    let mut s = String::new();
    if let Some(p) = e.page_index {
        for m in &e.mcids {
            if let Some(t) = map.get(&(p, *m)) {
                s.push_str(t);
                s.push(' ');
            }
        }
    }
    for c in &e.children {
        let ct = collect_text(c, map);
        if !ct.is_empty() {
            s.push_str(&ct);
            s.push(' ');
        }
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn find_tables<'a>(e: &'a StructElement, out: &mut Vec<&'a StructElement>) {
    if e.standard_type == "Table" {
        out.push(e);
    }
    for c in &e.children {
        find_tables(c, out);
    }
}

/// Rows = TR descendants (descending through THead/TBody/TFoot wrappers).
fn rows_of(tbl: &StructElement) -> Vec<&StructElement> {
    fn walk<'a>(e: &'a StructElement, rows: &mut Vec<&'a StructElement>) {
        if e.standard_type == "TR" {
            rows.push(e);
        } else {
            for c in &e.children {
                walk(c, rows);
            }
        }
    }
    let mut rows = Vec::new();
    for c in &tbl.children {
        walk(c, &mut rows);
    }
    rows
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: <pdf> [max_tables]");
    let max_tables: usize = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(3);
    let data = std::fs::read(&path).expect("read");
    let doc = PdfDocument::open(data).expect("open");
    let pdf = doc.pdf();
    let n = pdf.pages().len();

    let mut map: HashMap<(usize, i32), String> = HashMap::new();
    for pi in 0..n {
        let page = &pdf.pages()[pi];
        let (w, h) = page.render_dimensions();
        let mut dev = Dev::new();
        let mut ctx = Context::new(
            page.initial_transform(false),
            Rect::new(0.0, 0.0, w as f64, h as f64),
            page.xref(),
            InterpreterSettings::default(),
        );
        interpret_page(page, &mut ctx, &mut dev);
        for (m, t) in dev.by_mcid {
            map.entry((pi, m)).or_default().push_str(&t);
        }
    }

    let Some(tree) = tagged::parse(pdf) else {
        println!("no struct tree");
        return;
    };
    let mut tables = Vec::new();
    for root in &tree.root_elements {
        find_tables(root, &mut tables);
    }
    println!("== {} : {} Table elements ==", path, tables.len());

    let mut well_formed = 0usize;
    for (ti, tbl) in tables.iter().enumerate() {
        let rows = rows_of(tbl);
        if rows.is_empty() {
            continue;
        }
        let grid: Vec<Vec<String>> = rows
            .iter()
            .map(|tr| {
                tr.children
                    .iter()
                    .filter(|c| c.standard_type == "TD" || c.standard_type == "TH")
                    .map(|cell| collect_text(cell, &map))
                    .collect()
            })
            .collect();
        let cols: Vec<usize> = grid.iter().map(|r| r.len()).collect();
        let max_c = *cols.iter().max().unwrap_or(&0);
        let rectangular =
            max_c > 0 && cols.iter().filter(|&&c| c == max_c).count() >= rows.len() / 2;
        if rectangular {
            well_formed += 1;
        }
        if ti < max_tables {
            println!(
                "\n--- Table {} : {} rows x up-to {} cols ---",
                ti + 1,
                rows.len(),
                max_c
            );
            for r in grid.iter().take(8) {
                println!(
                    "{}",
                    r.iter()
                        .map(|c| {
                            let c = c.chars().take(24).collect::<String>();
                            format!("{c:<24}")
                        })
                        .collect::<Vec<_>>()
                        .join(" | ")
                );
            }
            if grid.len() > 8 {
                println!("... ({} more rows)", grid.len() - 8);
            }
        }
    }
    println!(
        "\n[summary] tables={} with-TR-rows={} rectangular(>=half rows full width)={}",
        tables.len(),
        tables.iter().filter(|t| !rows_of(t).is_empty()).count(),
        well_formed,
    );
}
