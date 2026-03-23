fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = &args[1];
    let prefix = &args[2];
    let data = std::fs::read(path).unwrap();
    let doc = match pdf_engine::PdfDocument::open(data) {
        Ok(d) => d,
        Err(e) => { eprintln!("open error: {e}"); return; }
    };
    let dpi: f64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(72.0);
    let opts = pdf_engine::RenderOptions { dpi, ..Default::default() };
    for i in 0..doc.page_count().min(5) {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            doc.render_page(i, &opts)
        }));
        match result {
            Ok(Ok(p)) => {
                let outpath = format!("{prefix}_{}.png", i+1);
                // Encode as PNG manually using basic approach
                save_png(&outpath, p.width, p.height, &p.pixels);
                eprintln!("page {}: {}x{}", i+1, p.width, p.height);
            },
            Ok(Err(e)) => eprintln!("page {} error: {e}", i+1),
            Err(_) => eprintln!("page {} PANIC", i+1),
        }
    }
}

fn save_png(path: &str, w: u32, h: u32, rgba: &[u8]) {
    // Use image crate if available, otherwise write raw
    let _ = (path, w, h, rgba);
    // Write as PPM for simplicity
    let ppm_path = path.replace(".png", ".ppm");
    let mut out = Vec::new();
    out.extend(format!("P6\n{w} {h}\n255\n").into_bytes());
    for chunk in rgba.chunks(4) {
        out.extend([chunk[0], chunk[1], chunk[2]]);
    }
    std::fs::write(&ppm_path, &out).unwrap();
    // Convert to PNG using sips (macOS built-in)
    let _ = std::process::Command::new("sips")
        .args(["-s", "format", "png", &ppm_path, "--out", path])
        .output();
    let _ = std::fs::remove_file(&ppm_path);
}
