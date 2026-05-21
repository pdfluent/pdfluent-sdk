//! SDK-only render benchmark harness (no editor, no canvas, no React).
//!
//! Times the pure Rust SDK render path: `PdfDocument::open` + `render_page` at
//! several DPR/scale equivalents. Emits one JSON object per (page, scale) plus a
//! summary, so render cost can be attributed to rasterization vs page pixels —
//! independent of any WASM boundary, canvas upload, or UI.
//!
//! Usage:
//!   cargo run -p pdf-engine --example render_bench --release -- <pdf> [runs] [max_pages]
//!
//! Output: JSON lines on stdout. No files written. No private inputs embedded.

use std::time::Instant;

use pdf_engine::render::RenderOptions;
use pdf_engine::PdfDocument;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = match args.get(1) {
        Some(p) => p.clone(),
        None => {
            eprintln!("usage: render_bench <pdf> [runs] [max_pages]");
            std::process::exit(2);
        }
    };
    let runs: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
    let max_pages: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(3);

    let data = match std::fs::read(&path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{{\"error\":\"read failed: {e}\"}}");
            std::process::exit(3);
        }
    };
    let file_size = data.len();

    let t_open = Instant::now();
    let doc = match PdfDocument::open(data) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{{\"error\":\"open failed: {e:?}\"}}");
            std::process::exit(4);
        }
    };
    let open_ms = t_open.elapsed().as_secs_f64() * 1000.0;
    let page_count = doc.page_count();
    let pages = max_pages.min(page_count);

    println!(
        "{{\"event\":\"open\",\"file\":\"<input>\",\"file_size\":{file_size},\"open_ms\":{open_ms:.2},\"page_count\":{page_count}}}"
    );

    let scales = [1.0_f64, 1.5, 2.0];
    for page in 0..pages {
        for &scale in &scales {
            let max_pixels = std::env::var("MAX_PIXELS")
                .ok()
                .and_then(|s| s.parse::<u32>().ok());
            // RENDER_QUALITY=speed selects the faster u8 pipeline; default = quality (f32).
            let quality = match std::env::var("RENDER_QUALITY").ok().as_deref() {
                Some("speed") | Some("Speed") => pdf_engine::RasterQuality::Speed,
                _ => pdf_engine::RasterQuality::Quality,
            };
            let opts = RenderOptions {
                dpi: 72.0 * scale,
                max_pixels,
                quality,
                ..Default::default()
            };
            let mut times = Vec::with_capacity(runs);
            let mut w = 0u32;
            let mut h = 0u32;
            let mut bytes = 0usize;
            let mut phash = 0u64; // FNV-1a of pixels — for cross-process fidelity compare
            for _ in 0..runs {
                let t = Instant::now();
                match doc.render_page(page, &opts) {
                    Ok(r) => {
                        times.push(t.elapsed().as_secs_f64() * 1000.0);
                        w = r.width;
                        h = r.height;
                        bytes = r.pixels.len();
                        let mut hsh = 0xcbf29ce484222325u64;
                        for b in &r.pixels {
                            hsh = (hsh ^ *b as u64).wrapping_mul(0x100000001b3);
                        }
                        phash = hsh;
                    }
                    Err(e) => {
                        println!(
                            "{{\"event\":\"render\",\"page\":{page},\"scale\":{scale},\"error\":\"{e:?}\"}}"
                        );
                        continue;
                    }
                }
            }
            if times.is_empty() {
                continue;
            }
            times.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let cold = times.first().copied().unwrap_or(0.0); // first run isn't necessarily cold here; see note
            let p50 = times[times.len() / 2];
            let p95 = times[((times.len() as f64 * 0.95) as usize).min(times.len() - 1)];
            let mpx = (w as f64 * h as f64) / 1.0e6;
            let ms_per_mpx = if mpx > 0.0 { p50 / mpx } else { 0.0 };
            println!(
                "{{\"event\":\"render\",\"page\":{page},\"scale\":{scale},\"w\":{w},\"h\":{h},\"megapixels\":{mpx:.3},\"render_p50_ms\":{p50:.2},\"render_p95_ms\":{p95:.2},\"render_min_ms\":{cold:.2},\"ms_per_megapixel\":{ms_per_mpx:.2},\"out_bytes\":{bytes},\"phash\":\"{phash:016x}\",\"runs\":{}}}",
                times.len()
            );
        }
    }
}
