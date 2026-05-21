//! Dev-only fidelity quantifier for the `RasterQuality` opt-in.
//!
//! Renders the same page(s) twice — once with `RasterQuality::Quality` (default,
//! f32 pipeline) and once with `RasterQuality::Speed` (u8 pipeline) — and reports
//! how much the two outputs differ: percentage of differing pixels and the
//! maximum per-channel absolute delta. This quantifies the precision cost of the
//! opt-in Speed mode so the trade-off is data-backed rather than asserted.
//!
//! Usage:
//!   cargo run -p pdf-engine --example render_quality_diff --release -- <pdf> [max_pages]
//!
//! Output: JSON lines on stdout. No files written. No private inputs embedded.

use pdf_engine::render::RenderOptions;
use pdf_engine::{PdfDocument, RasterQuality};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = match args.get(1) {
        Some(p) => p.clone(),
        None => {
            eprintln!("usage: render_quality_diff <pdf> [max_pages]");
            std::process::exit(2);
        }
    };
    let max_pages: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(3);

    let data = match std::fs::read(&path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{{\"error\":\"read failed: {e}\"}}");
            std::process::exit(3);
        }
    };

    let doc = match PdfDocument::open(data) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{{\"error\":\"open failed: {e:?}\"}}");
            std::process::exit(4);
        }
    };
    let pages = max_pages.min(doc.page_count());

    for page in 0..pages {
        for &scale in &[1.0_f64, 1.5, 2.0] {
            let q_opts = RenderOptions {
                dpi: 72.0 * scale,
                quality: RasterQuality::Quality,
                ..Default::default()
            };
            let s_opts = RenderOptions {
                dpi: 72.0 * scale,
                quality: RasterQuality::Speed,
                ..Default::default()
            };
            let (q, s) = match (
                doc.render_page(page, &q_opts),
                doc.render_page(page, &s_opts),
            ) {
                (Ok(q), Ok(s)) => (q, s),
                _ => continue,
            };
            if q.pixels.len() != s.pixels.len() {
                println!("{{\"page\":{page},\"scale\":{scale},\"error\":\"dim mismatch\"}}");
                continue;
            }
            let total_px = (q.width as usize) * (q.height as usize);
            let mut diff_px = 0usize;
            let mut max_delta = 0u8;
            let mut sum_delta = 0u64;
            for (a, b) in q.pixels.chunks_exact(4).zip(s.pixels.chunks_exact(4)) {
                let mut pixel_differs = false;
                for c in 0..4 {
                    let d = a[c].abs_diff(b[c]);
                    if d != 0 {
                        pixel_differs = true;
                        max_delta = max_delta.max(d);
                        sum_delta += d as u64;
                    }
                }
                if pixel_differs {
                    diff_px += 1;
                }
            }
            let diff_pct = if total_px > 0 {
                100.0 * diff_px as f64 / total_px as f64
            } else {
                0.0
            };
            let mean_delta_over_diff = if diff_px > 0 {
                sum_delta as f64 / (diff_px as f64 * 4.0)
            } else {
                0.0
            };
            println!(
                "{{\"page\":{page},\"scale\":{scale},\"w\":{},\"h\":{},\"total_px\":{total_px},\"diff_px\":{diff_px},\"diff_pct\":{diff_pct:.3},\"max_channel_delta\":{max_delta},\"mean_channel_delta_over_diff_px\":{mean_delta_over_diff:.3}}}",
                q.width, q.height
            );
        }
    }
}
