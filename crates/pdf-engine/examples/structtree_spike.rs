//! ISOLATED FEASIBILITY SPIKE (throwaway; not committed; dev-only).
//!
//! Validates the StructTree-driven extraction foundation end-to-end using the
//! REAL `pdf_compliance::tagged::parse` (now with populated `page_index`):
//!   StructTree -> StructElement{page_index, mcids} -> per-page (page,mcid)->text -> logical order
//!
//! It does not modify any production extraction path. Run:
//!   cargo run -p pdf-engine --example structtree_spike -- <file.pdf>

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::collections::HashMap;

use kurbo::{Affine, BezPath, Rect};
use pdf_compliance::tagged;
use pdf_render::pdf_interpret::cmap::BfString;
use pdf_render::pdf_interpret::font::Glyph;
use pdf_render::pdf_interpret::PageExt;
use pdf_render::pdf_interpret::{
    interpret_page, BlendMode, ClipPath, Context, Device, GlyphDrawMode, Image,
    InterpreterSettings, Paint, PathDrawMode, SoftMask,
};
use pdf_render::pdf_syntax::Pdf;

/// Throwaway device: bucket each glyph's unicode into the innermost active MCID.
struct McidTextDevice {
    mcid_stack: Vec<Option<i32>>,
    by_mcid: HashMap<i32, String>,
    untagged: String,
    glyphs_total: usize,
    glyphs_tagged: usize,
}

impl McidTextDevice {
    fn new() -> Self {
        Self {
            mcid_stack: Vec::new(),
            by_mcid: HashMap::new(),
            untagged: String::new(),
            glyphs_total: 0,
            glyphs_tagged: 0,
        }
    }
    fn current_mcid(&self) -> Option<i32> {
        self.mcid_stack.iter().rev().copied().flatten().next()
    }
}

impl Device<'_> for McidTextDevice {
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
        glyph: &Glyph<'_>,
        _t: Affine,
        _gt: Affine,
        _p: &Paint<'_>,
        _d: &GlyphDrawMode,
    ) {
        let text = match glyph.as_unicode() {
            Some(BfString::Char(c)) => c.to_string(),
            Some(BfString::String(s)) => s,
            None => return,
        };
        self.glyphs_total += 1;
        match self.current_mcid() {
            Some(mcid) => {
                self.glyphs_tagged += 1;
                self.by_mcid.entry(mcid).or_default().push_str(&text);
            }
            None => self.untagged.push_str(&text),
        }
    }
    fn begin_marked_content(&mut self, _tag: &[u8], mcid: Option<i32>) {
        self.mcid_stack.push(mcid);
    }
    fn end_marked_content(&mut self) {
        self.mcid_stack.pop();
    }
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: structtree_spike <file.pdf>");
    let data = std::fs::read(&path).expect("read pdf");
    let pdf = Pdf::new(data).expect("parse pdf");
    let n_pages = pdf.pages().len();
    println!("== StructTree foundation spike: {path} ({n_pages} page(s)) ==");

    // --- 1. Run the MCID device on EVERY page -> (page, mcid) -> text. ---
    let mut text_by_page_mcid: HashMap<(usize, i32), String> = HashMap::new();
    let mut glyphs_total = 0usize;
    let mut glyphs_tagged = 0usize;
    let mut untagged_total = 0usize;
    for pi in 0..n_pages {
        let page = &pdf.pages()[pi];
        let (w, h) = page.render_dimensions();
        let mut dev = McidTextDevice::new();
        let mut ctx = Context::new(
            page.initial_transform(false),
            Rect::new(0.0, 0.0, w as f64, h as f64),
            page.xref(),
            InterpreterSettings::default(),
        );
        interpret_page(page, &mut ctx, &mut dev);
        glyphs_total += dev.glyphs_total;
        glyphs_tagged += dev.glyphs_tagged;
        untagged_total += dev.untagged.chars().count();
        for (mcid, t) in dev.by_mcid {
            text_by_page_mcid
                .entry((pi, mcid))
                .or_default()
                .push_str(&t);
        }
    }
    println!(
        "[device] glyphs total={glyphs_total} tagged-with-mcid={glyphs_tagged} ({:.1}%) untagged-chars={untagged_total} (page,mcid) buckets={}",
        100.0 * glyphs_tagged as f64 / glyphs_total.max(1) as f64,
        text_by_page_mcid.len(),
    );

    // --- 2. Parse StructTree (REAL parser, page_index now populated). ---
    let Some(tree) = tagged::parse(&pdf) else {
        println!(
            "[structtree] no StructTreeRoot — untagged document; would fall back to geometric"
        );
        return;
    };
    let order = tree.reading_order();
    let elems_total = order.len();
    let elems_with_page = order.iter().filter(|e| e.page_index.is_some()).count();
    let total_mcid_refs: usize = order.iter().map(|e| e.mcids.len()).sum();
    println!(
        "[structtree] reading-order elements={elems_total} with-page_index={elems_with_page} ({:.1}%) total-mcid-refs={total_mcid_refs}",
        100.0 * elems_with_page as f64 / elems_total.max(1) as f64,
    );

    // --- 3. Reconstruct text in logical (reading) order via (page_index, mcid). ---
    let mut logical = String::new();
    let mut mapped = 0usize;
    let mut unmapped = 0usize;
    let mut used_actualtext = 0usize;
    for e in &order {
        // /ActualText overrides glyph-derived text when present (authoritative).
        if let Some(at) = &e.actual_text {
            if !at.is_empty() {
                logical.push_str(at);
                logical.push(' ');
                used_actualtext += 1;
                continue;
            }
        }
        let Some(page) = e.page_index else {
            unmapped += e.mcids.len();
            continue;
        };
        for &mcid in &e.mcids {
            match text_by_page_mcid.get(&(page, mcid)) {
                Some(t) => {
                    logical.push_str(t);
                    logical.push(' ');
                    mapped += 1;
                }
                None => unmapped += 1,
            }
        }
    }
    println!(
        "[reconstruct] mcid refs mapped={mapped} unmapped={unmapped} actualtext-elements={used_actualtext} coverage={:.1}%",
        100.0 * mapped as f64 / total_mcid_refs.max(1) as f64,
    );
    println!(
        "\n[logical-order text, first 500 chars]\n{}",
        logical.chars().take(500).collect::<String>()
    );
}
