use std::path::{Path, PathBuf};

use clap::Parser;
use image::GenericImageView;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

const SSIM_PASS_THRESHOLD: f64 = 0.85;

#[derive(Parser)]
#[command(name = "xfa-pdfrest-compare")]
#[command(about = "Compare pre-rendered XFA PNGs against Adobe pdfrest ground truth")]
struct Cli {
    #[arg(long)]
    golden_dir: PathBuf,

    #[arg(long)]
    output: PathBuf,
}

fn hash_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

fn load_png(path: &Path) -> anyhow::Result<(Vec<u8>, u32, u32)> {
    let img = image::open(path)?;
    let (w, h) = img.dimensions();
    let rgba = img.to_rgba8();
    Ok((rgba.into_raw(), w, h))
}

fn to_grayscale(rgba: &[u8], width: u32, height: u32) -> Vec<f64> {
    let len = (width * height) as usize;
    let mut gray = Vec::with_capacity(len);
    for i in 0..len {
        let idx = i * 4;
        let r = rgba[idx] as f64;
        let g = rgba[idx + 1] as f64;
        let b = rgba[idx + 2] as f64;
        gray.push(0.299 * r + 0.587 * g + 0.114 * b);
    }
    gray
}

fn window_stats(
    a: &[f64],
    stride_a: usize,
    b: &[f64],
    stride_b: usize,
    x: usize,
    y: usize,
) -> (f64, f64, f64, f64, f64) {
    let n = 64.0;
    let mut sum_a = 0.0;
    let mut sum_b = 0.0;
    let mut sum_a2 = 0.0;
    let mut sum_b2 = 0.0;
    let mut sum_ab = 0.0;
    for dy in 0..8 {
        for dx in 0..8 {
            let va = a[(y + dy) * stride_a + (x + dx)];
            let vb = b[(y + dy) * stride_b + (x + dx)];
            sum_a += va;
            sum_b += vb;
            sum_a2 += va * va;
            sum_b2 += vb * vb;
            sum_ab += va * vb;
        }
    }
    let mean_a = sum_a / n;
    let mean_b = sum_b / n;
    let var_a = sum_a2 / n - mean_a * mean_a;
    let var_b = sum_b2 / n - mean_b * mean_b;
    let covar = sum_ab / n - mean_a * mean_b;
    (mean_a, mean_b, var_a, var_b, covar)
}

fn find_png(dir: &Path, prefix: &str, page: u32) -> Option<PathBuf> {
    let p1 = dir.join(format!("{}-{}.png", prefix, page));
    let p2 = dir.join(format!("{}-{:02}.png", prefix, page));
    let p3 = dir.join(format!("{}-{:03}.png", prefix, page));
    if p1.exists() {
        Some(p1)
    } else if p2.exists() {
        Some(p2)
    } else if p3.exists() {
        Some(p3)
    } else {
        None
    }
}

fn compute_ssim(img_a: &[u8], w_a: u32, h_a: u32, img_b: &[u8], w_b: u32, h_b: u32) -> f64 {
    let w = w_a.min(w_b) as usize;
    let h = h_a.min(h_b) as usize;
    if w < 8 || h < 8 {
        return 1.0;
    }
    let gray_a = to_grayscale(img_a, w_a, h_a);
    let gray_b = to_grayscale(img_b, w_b, h_b);
    let c1: f64 = (0.01 * 255.0_f64).powi(2);
    let c2: f64 = (0.03 * 255.0_f64).powi(2);
    let mut total_ssim = 0.0;
    let mut window_count = 0usize;
    let step = 4;
    let mut y = 0;
    while y + 8 <= h {
        let mut x = 0;
        while x + 8 <= w {
            let (mean_a, mean_b, var_a, var_b, covar) =
                window_stats(&gray_a, w_a as usize, &gray_b, w_b as usize, x, y);
            let numerator = (2.0 * mean_a * mean_b + c1) * (2.0 * covar + c2);
            let denominator = (mean_a.powi(2) + mean_b.powi(2) + c1) * (var_a + var_b + c2);
            total_ssim += numerator / denominator;
            window_count += 1;
            x += step;
        }
        y += step;
    }
    if window_count == 0 {
        return 1.0;
    }
    total_ssim / window_count as f64
}

fn extract_page_number(filename: &str) -> Option<u32> {
    let re = regex::Regex::new(r"page-(\d+)").ok()?;
    let caps = re.captures(filename)?;
    caps.get(1)?.as_str().parse().ok()
}

fn find_matching_pages(dir: &Path) -> (Vec<(u32, PathBuf, PathBuf)>, usize, usize) {
    let mut matches = Vec::new();
    let mut our_pages = 0usize;
    let mut pdfrest_pages = 0usize;

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return (matches, 0, 0),
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let filename = entry.file_name();
        let filename_str = filename.to_string_lossy();
        if filename_str.starts_with("page-") && filename_str.ends_with(".png") {
            if extract_page_number(&filename_str).is_some() {
                our_pages += 1;
            }
        } else if filename_str.starts_with("pdfrest_page-") && filename_str.ends_with(".png") {
            pdfrest_pages += 1;
        }
    }

    for entry in std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
    {
        let filename = entry.file_name();
        let filename_str = filename.to_string_lossy();
        if !filename_str.starts_with("page-") || !filename_str.ends_with(".png") {
            continue;
        }
        let page_num = match extract_page_number(&filename_str) {
            Some(n) => n,
            None => continue,
        };
        let our_path = entry.path();
        if let Some(pdfrest_path) = find_png(dir, "pdfrest_page", page_num) {
            matches.push((page_num, our_path, pdfrest_path));
        }
    }

    matches.sort_by_key(|m| m.0);
    (matches, our_pages, pdfrest_pages)
}

fn open_db(path: &Path) -> anyhow::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS results (
            hash TEXT PRIMARY KEY,
            ssim_score REAL,
            page_count INTEGER,
            status TEXT,
            our_pages INTEGER,
            pdfrest_pages INTEGER,
            pdfrest_truncated INTEGER DEFAULT 0,
            timestamp TEXT DEFAULT (datetime('now'))
        )",
    )?;
    Ok(conn)
}

fn process_directory(dir: &Path) -> Option<(String, f64, usize, String, usize, usize, bool)> {
    let input_path = dir.join("input.pdf");
    let input_data = std::fs::read(&input_path).ok()?;
    let hash = hash_bytes(&input_data);

    if pdf_xfa::is_pdf_encrypted(&input_data) {
        return Some((hash, 0.0, 0, "encrypted_skip".to_string(), 0, 0, false));
    }

    let (page_matches, our_pages, pdfrest_pages) = find_matching_pages(dir);

    if page_matches.is_empty() {
        return None;
    }

    let mut total_ssim = 0.0;
    let page_count = page_matches.len();

    for (_, our_path, pdfrest_path) in &page_matches {
        let (our_pixels, our_w, our_h) = match load_png(our_path) {
            Ok(p) => p,
            Err(_) => return None,
        };
        let (ref_pixels, ref_w, ref_h) = match load_png(pdfrest_path) {
            Ok(p) => p,
            Err(_) => return None,
        };
        total_ssim += compute_ssim(&our_pixels, our_w, our_h, &ref_pixels, ref_w, ref_h);
    }

    let avg_ssim = total_ssim / page_count as f64;
    let pdfrest_truncated = pdfrest_pages == 3 && our_pages > 3;
    let status = if avg_ssim >= SSIM_PASS_THRESHOLD {
        "pass"
    } else {
        "fail"
    };

    Some((
        hash,
        avg_ssim,
        page_count,
        status.to_string(),
        our_pages,
        pdfrest_pages,
        pdfrest_truncated,
    ))
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let mut entries = Vec::new();
    for entry in WalkDir::new(&cli.golden_dir)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let dir = entry.path();
        if dir.is_dir() {
            entries.push(dir.to_path_buf());
        }
    }

    if entries.is_empty() {
        anyhow::bail!("No entries found in {}", cli.golden_dir.display());
    }

    println!("Found {} directories", entries.len());

    let conn = open_db(&cli.output)?;

    let mut has_comparable = 0usize;
    let mut pass_count = 0usize;
    let mut fail_count = 0usize;
    let mut total_ssim = 0.0;
    let mut valid_count = 0usize;
    let mut no_render = 0usize;
    let mut encrypted_skip = 0usize;
    let mut pdfrest_truncated_count = 0usize;
    let mut worst_cases = Vec::new();

    for dir in &entries {
        let dir_name = dir
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let result = process_directory(dir);

        let (hash, ssim, page_count, status, our_pages, pdfrest_pages, pdfrest_truncated) =
            match result {
                Some(r) => r,
                None => {
                    no_render += 1;
                    let input_path = dir.join("input.pdf");
                    let hash = std::fs::read(&input_path)
                        .map(|d| hash_bytes(&d))
                        .unwrap_or_default();
                    if let Err(e) = conn.execute(
                    "INSERT OR REPLACE INTO results (hash, ssim_score, page_count, status, our_pages, pdfrest_pages, pdfrest_truncated)
                     VALUES (?1, NULL, 0, 'no_render', 0, 0, 0)",
                    params![hash],
                ) {
                    eprintln!("DB error: {}", e);
                }
                    continue;
                }
            };

        if status == "encrypted_skip" {
            encrypted_skip += 1;
            if let Err(e) = conn.execute(
                "INSERT OR REPLACE INTO results (hash, ssim_score, page_count, status, our_pages, pdfrest_pages, pdfrest_truncated)
                 VALUES (?1, NULL, 0, 'encrypted_skip', 0, 0, 0)",
                params![hash],
            ) {
                eprintln!("DB error: {}", e);
            }
            println!("[{}] SKIP: encrypted PDF", dir_name);
            continue;
        }

        has_comparable += 1;
        let ssim_val = ssim;

        if status == "pass" {
            pass_count += 1;
        } else {
            fail_count += 1;
        }

        if pdfrest_truncated {
            pdfrest_truncated_count += 1;
        }

        total_ssim += ssim_val;
        valid_count += 1;
        worst_cases.push((dir_name.clone(), ssim_val));
        worst_cases.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        if worst_cases.len() > 10 {
            worst_cases.pop();
        }

        if let Err(e) = conn.execute(
            "INSERT OR REPLACE INTO results (hash, ssim_score, page_count, status, our_pages, pdfrest_pages, pdfrest_truncated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![hash, ssim_val, page_count as i64, status, our_pages as i64, pdfrest_pages as i64, pdfrest_truncated as i64],
        ) {
            eprintln!("DB error: {}", e);
        }

        println!(
            "[{}] ssim={:.4} pages={} status={}{}",
            dir_name,
            ssim_val,
            page_count,
            status,
            if pdfrest_truncated {
                " (pdfrest_truncated)"
            } else {
                ""
            }
        );
    }

    let avg_ssim = if valid_count > 0 {
        total_ssim / valid_count as f64
    } else {
        0.0
    };

    println!();
    println!("=== Summary ===");
    println!("Total directories: {}", entries.len());
    println!("No render (skipped): {}", no_render);
    println!("Encrypted (skipped): {}", encrypted_skip);
    println!("Have both PNGs: {}", has_comparable);
    println!("Pass (≥{:.2}): {}", SSIM_PASS_THRESHOLD, pass_count);
    println!("Fail: {}", fail_count);
    println!(
        "pdfrest truncated (max 3 pages): {}",
        pdfrest_truncated_count
    );
    println!("Average SSIM: {:.4}", avg_ssim);
    println!();
    println!("=== Worst 10 Cases ===");
    for (name, ssim) in worst_cases.iter().rev() {
        println!("  {}: {:.4}", name, ssim);
    }
    println!("Results written to: {}", cli.output.display());

    Ok(())
}
