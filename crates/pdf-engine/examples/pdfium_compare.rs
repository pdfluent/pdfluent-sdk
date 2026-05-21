//! Dev-only pdfium reference comparator (render-profiling milestone).
//!
//! Times the SAME pages/scales through (a) our `pdf_engine` render path and
//! (b) pdfium (via `pdfium-render` bound to a local libpdfium), in one Rust
//! process, to produce an apples-to-apples ms/megapixel ratio. pdfium is a
//! mature C++ rasterizer used as a performance *reference floor*, not a target
//! to match 1:1.
//!
//! The libpdfium shared library is NOT committed; provide it at runtime:
//!   PDFIUM_LIB=/path/to/libpdfium.dylib \
//!   cargo run -p pdf-engine --example pdfium_compare --release -- <pdf> [runs] [max_pages]
//!
//! Output: JSON lines on stdout. No files written.

use std::time::Instant;

use pdf_engine::render::RenderOptions;
use pdf_engine::PdfDocument;
use pdfium_render::prelude::*;

fn p50(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).cloned().unwrap_or_else(|| {
        eprintln!("usage: PDFIUM_LIB=<libpdfium> pdfium_compare <pdf> [runs] [max_pages]");
        std::process::exit(2);
    });
    let runs: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
    let max_pages: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(3);

    let lib = std::env::var("PDFIUM_LIB").unwrap_or_default();
    let bindings = Pdfium::bind_to_library(&lib)
        .or_else(|_| Pdfium::bind_to_system_library())
        .unwrap_or_else(|e| {
            eprintln!("{{\"error\":\"pdfium bind failed (set PDFIUM_LIB): {e:?}\"}}");
            std::process::exit(3);
        });
    let pdfium = Pdfium::new(bindings);

    let scales = [1.0_f64, 1.5, 2.0];

    // --- ours ---
    let data = std::fs::read(&path).expect("read pdf");
    let ours = PdfDocument::open(data).expect("our engine open");
    let pages = max_pages.min(ours.page_count());
    for page in 0..pages {
        for &scale in &scales {
            let opts = RenderOptions {
                dpi: 72.0 * scale,
                ..Default::default()
            };
            let mut t = Vec::new();
            let (mut w, mut h) = (0u32, 0u32);
            for _ in 0..runs {
                let s = Instant::now();
                if let Ok(r) = ours.render_page(page, &opts) {
                    t.push(s.elapsed().as_secs_f64() * 1000.0);
                    w = r.width;
                    h = r.height;
                }
            }
            let ms = p50(t);
            let mpx = (w as f64 * h as f64) / 1e6;
            println!(
                "{{\"engine\":\"pdfluent\",\"page\":{page},\"scale\":{scale},\"w\":{w},\"h\":{h},\"mpx\":{mpx:.3},\"render_p50_ms\":{ms:.2},\"ms_per_mpx\":{:.2}}}",
                if mpx > 0.0 { ms / mpx } else { 0.0 }
            );
        }
    }

    // --- pdfium ---
    let doc = pdfium.load_pdf_from_file(&path, None).expect("pdfium open");
    let pcount = doc.pages().len() as usize;
    let ppages = max_pages.min(pcount);
    for page_idx in 0..ppages {
        let page = doc.pages().get(page_idx as u16).expect("pdfium page");
        let pts_w = page.width().value as f64; // points
        let pts_h = page.height().value as f64;
        for &scale in &scales {
            let tw = (pts_w * scale).round() as i32;
            let th = (pts_h * scale).round() as i32;
            let cfg = PdfRenderConfig::new()
                .set_target_width(tw)
                .set_target_height(th);
            let mut t = Vec::new();
            let (mut w, mut h) = (0u32, 0u32);
            for _ in 0..runs {
                let s = Instant::now();
                if let Ok(bmp) = page.render_with_config(&cfg) {
                    t.push(s.elapsed().as_secs_f64() * 1000.0);
                    w = bmp.width() as u32;
                    h = bmp.height() as u32;
                }
            }
            let ms = p50(t);
            let mpx = (w as f64 * h as f64) / 1e6;
            println!(
                "{{\"engine\":\"pdfium\",\"page\":{page_idx},\"scale\":{scale},\"w\":{w},\"h\":{h},\"mpx\":{mpx:.3},\"render_p50_ms\":{ms:.2},\"ms_per_mpx\":{:.2}}}",
                if mpx > 0.0 { ms / mpx } else { 0.0 }
            );
        }
    }
}
