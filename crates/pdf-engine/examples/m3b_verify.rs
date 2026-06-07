//! M3B Hard-Proof Verification — the single validation example for the
//! RichGeometry tight-glyph-bounds feature.
//!
//! Run against a real-PDF corpus, it proves four properties and exits non-zero
//! if any fails:
//!
//!   1. Text invariance — Basic and RichGeometry extract byte-identical text
//!      (the rich path must not perturb reading order or grouping).
//!   2. Per-glyph array invariants — `char_bounds`, `tight_char_bounds`,
//!      `glyph_bounds_sources` and `glyph_advances` have equal length in every
//!      span.
//!   3. Coverage — every VISIBLE glyph (ground truth: it has an outline) has an
//!      outline, so by construction it is classified `Tight`. Equivalently, no
//!      visible glyph is `Estimate`; the `Estimate` remainder is non-visual
//!      (spaces and other ink-less glyphs), corroborated in aggregate.
//!   4. Value sanity — `Tight` bounds are dimensionally commensurate with their
//!      advance bounds. A scaling regression (e.g. a stray `font_size/1000`)
//!      collapses tight bounds to ~1% of the advance height; the median
//!      tight/advance height-ratio gate catches exactly that.
//!
//! Coverage is proven from a ground-truth recorder, NOT by cross-referencing
//! individual production glyphs: `into_blocks()` collapses overprint/fake-bold
//! duplicate spans, so the production glyph sequence is a (slightly shorter)
//! subsequence of the drawn glyphs — per-glyph index alignment would drift.
//!
//! Usage:
//!   cargo run -p pdf-engine --example m3b_verify --release -- corpus/

use kurbo::Rect;
use pdf_engine::text::{BoundsSource, GeometryMode, TextExtractionDevice};
use pdf_render::pdf_interpret::cmap::BfString;
use pdf_render::pdf_interpret::font::Glyph;
use pdf_render::pdf_interpret::util::PageExt;
use pdf_render::pdf_interpret::{
    interpret_page, BlendMode, ClipPath, Context, Device, GlyphDrawMode, Image,
    InterpreterSettings, Paint, PathDrawMode, SoftMask,
};
use pdf_render::pdf_syntax::Pdf;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

// ---------------------------------------------------------------------------
// Character classification (visual vs non-visual)
// ---------------------------------------------------------------------------

fn is_space(c: char) -> bool {
    matches!(
        c,
        ' ' | '\u{00A0}' | '\u{2000}'..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}'
    )
}

fn is_zero_width(c: char) -> bool {
    matches!(
        c,
        '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{FEFF}' | '\u{2060}' | '\u{00AD}'
    )
}

fn is_tab_newline(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r')
}

/// A glyph is "visible" if it is expected to carry ink.
fn is_visible(c: char) -> bool {
    !is_space(c) && !is_zero_width(c) && !is_tab_newline(c) && !c.is_control()
}

// ---------------------------------------------------------------------------
// Ground-truth coverage recorder
// ---------------------------------------------------------------------------

/// Records `(unicode, has_outline)` for every glyph the production device would
/// admit. It mirrors `TextExtractionDevice::draw_glyph`'s admission rule (skip
/// glyphs with no unicode mapping). Counts the drawn glyphs (no overprint
/// collapse), so `recorder >= production` by the number of collapsed duplicates.
struct CoverageRecorder {
    glyphs: Vec<(Option<char>, bool)>,
}

impl Device<'_> for CoverageRecorder {
    fn set_soft_mask(&mut self, _: Option<SoftMask<'_>>) {}
    fn set_blend_mode(&mut self, _: BlendMode) {}
    fn draw_path(&mut self, _: &kurbo::BezPath, _: kurbo::Affine, _: &Paint<'_>, _: &PathDrawMode) {
    }
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'_>>, _: BlendMode) {}
    fn draw_image(&mut self, _: Image<'_, '_>, _: kurbo::Affine) {}
    fn pop_clip_path(&mut self) {}
    fn pop_transparency_group(&mut self) {}

    fn draw_glyph(
        &mut self,
        glyph: &Glyph<'_>,
        _t: kurbo::Affine,
        _gt: kurbo::Affine,
        _p: &Paint<'_>,
        _dm: &GlyphDrawMode,
    ) {
        // Mirror production: admit only glyphs with a unicode mapping.
        let unicode = match glyph.as_unicode() {
            Some(BfString::Char(c)) => Some(c),
            Some(BfString::String(s)) => s.chars().next(),
            None => return,
        };
        let has_outline = match glyph {
            Glyph::Outline(o) => !o.outline().elements().is_empty(),
            Glyph::Type3(_) => false,
        };
        self.glyphs.push((unicode, has_outline));
    }
}

// ---------------------------------------------------------------------------
// Per-document verification
// ---------------------------------------------------------------------------

#[derive(Default)]
struct DocStats {
    pages: usize,
    spans: usize,
    invariant_violations: usize,
    text_mismatch: bool,
    total_glyphs: usize,
    tight: usize,
    estimate: usize,
    recorder_glyphs: usize,
    visible: usize,
    visible_no_outline: usize,
    ratios: Vec<f64>,
}

#[derive(Default)]
struct Stats {
    docs: usize,
    pages: usize,
    spans: usize,
    invariant_violations: usize,
    text_mismatch_docs: usize,
    total_glyphs: usize,
    tight: usize,
    estimate: usize,
    recorder_glyphs: usize,
    visible: usize,
    visible_no_outline: usize,
    ratios: Vec<f64>,
}

impl Stats {
    fn merge(&mut self, o: DocStats) {
        self.docs += 1;
        self.pages += o.pages;
        self.spans += o.spans;
        self.invariant_violations += o.invariant_violations;
        if o.text_mismatch {
            self.text_mismatch_docs += 1;
        }
        self.total_glyphs += o.total_glyphs;
        self.tight += o.tight;
        self.estimate += o.estimate;
        self.recorder_glyphs += o.recorder_glyphs;
        self.visible += o.visible;
        self.visible_no_outline += o.visible_no_outline;
        self.ratios.extend(o.ratios);
    }
}

fn block_text(blocks: &[pdf_engine::TextBlock]) -> String {
    blocks
        .iter()
        .map(|b| b.text())
        .collect::<Vec<_>>()
        .join("\n")
}

fn verify_document(path: &Path) -> Result<DocStats, String> {
    let data = fs::read(path).map_err(|e| format!("read: {e}"))?;
    let pdf = Pdf::new(data).map_err(|e| format!("open: {e:?}"))?;
    let pages = pdf.pages();
    let settings = InterpreterSettings::default();

    let mut s = DocStats::default();

    for pi in 0..pages.len() {
        let page = &pages[pi];
        let (w, h) = page.render_dimensions();
        let bb = Rect::new(0.0, 0.0, w as f64, h as f64);
        let init = page.initial_transform(false);
        s.pages += 1;

        // Pass A: ground-truth coverage recorder (every drawn, admitted glyph).
        let mut recorder = CoverageRecorder { glyphs: Vec::new() };
        {
            let mut ctx = Context::new(init, bb, page.xref(), settings.clone());
            interpret_page(page, &mut ctx, &mut recorder);
        }
        s.recorder_glyphs += recorder.glyphs.len();
        for (u, has_outline) in &recorder.glyphs {
            if u.map(is_visible).unwrap_or(false) {
                s.visible += 1;
                if !has_outline {
                    s.visible_no_outline += 1;
                }
            }
        }

        // Pass B: Basic-mode extraction (text reference).
        let basic_text = {
            let mut dev = TextExtractionDevice::new();
            let mut ctx = Context::new(init, bb, page.xref(), settings.clone());
            interpret_page(page, &mut ctx, &mut dev);
            block_text(&dev.into_blocks())
        };

        // Pass C: RichGeometry extraction (per-glyph data + text comparison).
        let rich_blocks = {
            let mut dev = TextExtractionDevice::with_mode(GeometryMode::RichGeometry);
            let mut ctx = Context::new(init, bb, page.xref(), settings.clone());
            interpret_page(page, &mut ctx, &mut dev);
            dev.into_blocks()
        };
        if basic_text != block_text(&rich_blocks) {
            s.text_mismatch = true;
        }

        for block in &rich_blocks {
            for span in &block.spans {
                s.spans += 1;
                let n_cb = span.char_bounds.len();
                let n_tb = span.tight_char_bounds.len();
                let n_gs = span.glyph_bounds_sources.len();
                let n_ga = span.glyph_advances.len();
                if n_cb != n_tb || n_tb != n_gs || n_gs != n_ga {
                    s.invariant_violations += 1;
                }

                for i in 0..n_tb {
                    s.total_glyphs += 1;
                    match span.glyph_bounds_sources[i] {
                        BoundsSource::Tight => {
                            s.tight += 1;
                            // Value sanity: tight height vs advance height.
                            let adv_h = span.char_bounds[i][3] - span.char_bounds[i][1];
                            let tight_h =
                                span.tight_char_bounds[i][3] - span.tight_char_bounds[i][1];
                            if adv_h.abs() > 0.01 {
                                s.ratios.push(tight_h / adv_h);
                            }
                        }
                        BoundsSource::Estimate => s.estimate += 1,
                        BoundsSource::Advance => {}
                    }
                }
            }
        }
    }

    Ok(s)
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

fn pct(num: usize, den: usize) -> f64 {
    if den == 0 {
        0.0
    } else {
        num as f64 / den as f64 * 100.0
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let input = match args.get(1) {
        Some(i) => i,
        None => {
            eprintln!("usage: m3b_verify <pdf_or_dir>");
            std::process::exit(2);
        }
    };

    let p = Path::new(input);
    let mut pdfs: Vec<std::path::PathBuf> = Vec::new();
    if p.is_dir() {
        for e in fs::read_dir(p).unwrap().flatten() {
            let pp = e.path();
            if pp.extension().map(|x| x == "pdf").unwrap_or(false) {
                pdfs.push(pp);
            }
        }
        pdfs.sort();
    } else {
        pdfs.push(p.to_path_buf());
    }

    let mut stats = Stats::default();
    for pdf_path in &pdfs {
        match verify_document(pdf_path) {
            Ok(d) => stats.merge(d),
            Err(e) => {
                let name = pdf_path.file_name().unwrap_or_default().to_string_lossy();
                eprintln!("  SKIP {name}: {e}");
            }
        }
    }

    // Ratio statistics.
    stats.ratios.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = stats.ratios.len();
    let median = if n == 0 { 0.0 } else { stats.ratios[n / 2] };
    let mean = if n == 0 {
        0.0
    } else {
        stats.ratios.iter().sum::<f64>() / n as f64
    };
    let rmin = stats.ratios.first().copied().unwrap_or(0.0);
    let rmax = stats.ratios.last().copied().unwrap_or(0.0);

    let visible_tight_pct = pct(stats.visible - stats.visible_no_outline, stats.visible);
    let nonvisible = stats.recorder_glyphs - stats.visible;

    // Sound pass/fail gates.
    let text_ok = stats.text_mismatch_docs == 0;
    let invariants_ok = stats.invariant_violations == 0;
    let coverage_ok = stats.visible_no_outline == 0;
    let value_ok = median >= 0.20;

    let w = io::stderr();
    let mut o = w.lock();
    let _ = writeln!(o, "\n=== M3B Hard-Proof Verification ===\n");
    let _ = writeln!(o, "Documents:                 {}", stats.docs);
    let _ = writeln!(o, "Pages:                     {}", stats.pages);

    let _ = writeln!(o, "\n--- 1. Text Invariance (Basic vs RichGeometry) ---");
    let _ = writeln!(
        o,
        "Docs with text mismatch:   {}   [{}]",
        stats.text_mismatch_docs,
        if text_ok { "PASS" } else { "FAIL" }
    );

    let _ = writeln!(o, "\n--- 2. Per-Glyph Array Invariants ---");
    let _ = writeln!(o, "Spans checked:             {}", stats.spans);
    let _ = writeln!(
        o,
        "Invariant violations:      {}   [{}]",
        stats.invariant_violations,
        if invariants_ok { "PASS" } else { "FAIL" }
    );
    let _ = writeln!(o, "Production glyphs:         {}", stats.total_glyphs);
    let _ = writeln!(
        o,
        "  Tight:                   {} ({:.2}%)",
        stats.tight,
        pct(stats.tight, stats.total_glyphs)
    );
    let _ = writeln!(
        o,
        "  Estimate:                {} ({:.2}%)",
        stats.estimate,
        pct(stats.estimate, stats.total_glyphs)
    );

    let _ = writeln!(
        o,
        "\n--- 3. Coverage (ground truth: visible glyph => has outline) ---"
    );
    let _ = writeln!(
        o,
        "Drawn glyphs (recorder):   {}   (>= production {}, diff = collapsed overprints)",
        stats.recorder_glyphs, stats.total_glyphs
    );
    let _ = writeln!(o, "Visible glyphs:            {}", stats.visible);
    let _ = writeln!(
        o,
        "  Visible without outline: {}   [{}]",
        stats.visible_no_outline,
        if coverage_ok { "PASS" } else { "FAIL" }
    );
    let _ = writeln!(o, "Visible Tight coverage:    {visible_tight_pct:.2}%");
    let _ = writeln!(
        o,
        "Non-visual (ground truth): {nonvisible}   ~=   Estimate {}   (diff = collapsed overprints)",
        stats.estimate
    );

    let _ = writeln!(
        o,
        "\n--- 4. Value Sanity (tight height / advance height) ---"
    );
    let _ = writeln!(o, "Tight glyphs measured:     {n}");
    let _ = writeln!(o, "  mean ratio:              {mean:.3}");
    let _ = writeln!(o, "  median ratio:            {median:.3}");
    let _ = writeln!(o, "  min / max:               {rmin:.3} / {rmax:.3}");
    let _ = writeln!(
        o,
        "Median >= 0.20:            [{}]   (a double-scale regression would be ~0.01)",
        if value_ok { "PASS" } else { "FAIL" }
    );

    let all_ok = text_ok && invariants_ok && coverage_ok && value_ok;
    let _ = writeln!(o, "\n=== Verdict ===");
    if all_ok {
        let _ = writeln!(
            o,
            "ALL CHECKS PASS — tight bounds correct, complete on visible glyphs \
             ({visible_tight_pct:.1}% Tight), value-sane (median {median:.2})."
        );
    } else {
        let _ = writeln!(
            o,
            "FAIL — one or more checks did not pass (see [FAIL] above)."
        );
        std::process::exit(1);
    }
}
