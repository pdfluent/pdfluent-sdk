use std::path::{Path, PathBuf};
use std::sync::Mutex;

use clap::Parser;
use rayon::prelude::*;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

const SSIM_PASS_THRESHOLD: f64 = 0.85;
const RENDER_DPI: f64 = 150.0;

#[derive(Parser)]
#[command(name = "xfa-pdfrest-compare")]
#[command(about = "Compare XFA flatten output against Adobe pdfrest ground truth")]
struct Cli {
    #[arg(long)]
    golden_dir: PathBuf,

    #[arg(long)]
    output: PathBuf,
}

struct GoldenEntry {
    dir: PathBuf,
    input_path: PathBuf,
    reference_path: PathBuf,
    itext_path: PathBuf,
}

fn discover_golden_entries(golden_dir: &Path) -> Vec<GoldenEntry> {
    let mut entries = Vec::new();
    for entry in WalkDir::new(golden_dir)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let input_path = dir.join("input.pdf");
        let reference_path = dir.join("pdfrest_flat.pdf");
        let itext_path = dir.join("itext_flat.pdf");
        if input_path.exists() && reference_path.exists() {
            entries.push(GoldenEntry {
                dir: dir.to_path_buf(),
                input_path,
                reference_path,
                itext_path,
            });
        }
    }
    entries
}

fn flatten_and_render(input_data: &[u8]) -> anyhow::Result<(Vec<u8>, usize)> {
    let flattened = pdf_xfa::flatten_xfa_to_pdf(input_data)
        .map_err(|e| anyhow::anyhow!("flatten failed: {e:?}"))?;

    let doc = pdf_engine::PdfDocument::open(flattened.clone())?;
    let page_count = doc.page_count();
    Ok((flattened, page_count))
}

fn render_pdf_to_rgba(data: &[u8], dpi: f64) -> anyhow::Result<(Vec<u8>, u32, u32)> {
    let doc = pdf_engine::PdfDocument::open(data.to_vec())?;
    let page_count = doc.page_count();
    if page_count == 0 {
        anyhow::bail!("PDF has no pages");
    }
    let opts = pdf_engine::RenderOptions {
        dpi,
        ..Default::default()
    };
    let rendered = doc.render_page(0, &opts)?;
    Ok((rendered.pixels, rendered.width, rendered.height))
}

fn compute_ssim(img_a: &[u8], w_a: u32, h_a: u32, img_b: &[u8], w_b: u32, h_b: u32) -> f64 {
    let w = w_a.min(w_a).min(w_b).min(w_b) as usize;
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

fn hash_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    fn open(path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS results (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                hash TEXT NOT NULL,
                ssim_score REAL NOT NULL,
                page_count INTEGER NOT NULL,
                status TEXT NOT NULL,
                dir_name TEXT NOT NULL,
                timestamp TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_hash ON results(hash);
            CREATE INDEX IF NOT EXISTS idx_status ON results(status);",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn insert(
        &self,
        hash: &str,
        ssim_score: f64,
        page_count: i64,
        status: &str,
        dir_name: &str,
    ) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO results (hash, ssim_score, page_count, status, dir_name, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))",
            params![hash, ssim_score, page_count, status, dir_name],
        )?;
        Ok(())
    }
}

struct CompareResult {
    dir_name: String,
    hash: String,
    ssim_score: f64,
    page_count: i64,
    status: String,
}

fn process_entry(entry: &GoldenEntry) -> CompareResult {
    let dir_name = entry
        .dir
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    let input_data = match std::fs::read(&entry.input_path) {
        Ok(d) => d,
        Err(e) => {
            return CompareResult {
                dir_name,
                hash: String::new(),
                ssim_score: 0.0,
                page_count: 0,
                status: format!("read_error: {}", e),
            };
        }
    };

    let hash = hash_bytes(&input_data);

    let (our_flat, page_count) = match flatten_and_render(&input_data) {
        Ok((flat, pc)) => (flat, pc as i64),
        Err(e) => {
            return CompareResult {
                dir_name,
                hash,
                ssim_score: 0.0,
                page_count: 0,
                status: format!("flatten_error: {}", e),
            };
        }
    };

    let reference_data = match std::fs::read(&entry.reference_path) {
        Ok(d) => d,
        Err(e) => {
            return CompareResult {
                dir_name,
                hash,
                ssim_score: 0.0,
                page_count,
                status: format!("reference_read_error: {}", e),
            };
        }
    };

    let (our_pixels, our_w, our_h) = match render_pdf_to_rgba(&our_flat, RENDER_DPI) {
        Ok(p) => p,
        Err(e) => {
            return CompareResult {
                dir_name,
                hash,
                ssim_score: 0.0,
                page_count,
                status: format!("our_render_error: {}", e),
            };
        }
    };

    let (ref_pixels, ref_w, ref_h) = match render_pdf_to_rgba(&reference_data, RENDER_DPI) {
        Ok(p) => p,
        Err(e) => {
            return CompareResult {
                dir_name,
                hash,
                ssim_score: 0.0,
                page_count,
                status: format!("reference_render_error: {}", e),
            };
        }
    };

    let ssim_score = compute_ssim(&our_pixels, our_w, our_h, &ref_pixels, ref_w, ref_h);

    let status = if ssim_score >= SSIM_PASS_THRESHOLD {
        "pass".to_string()
    } else {
        "fail".to_string()
    };

    CompareResult {
        dir_name,
        hash,
        ssim_score,
        page_count,
        status,
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let entries = discover_golden_entries(&cli.golden_dir);
    if entries.is_empty() {
        anyhow::bail!("No golden entries found in {}", cli.golden_dir.display());
    }

    println!("Found {} golden entries", entries.len());

    let db = Db::open(&cli.output)?;

    let results: Vec<CompareResult> = entries
        .par_iter()
        .map(|entry| {
            let result = process_entry(entry);
            eprintln!(
                "[{}] ssim={:.4} pages={} status={}",
                result.dir_name, result.ssim_score, result.page_count, result.status
            );
            result
        })
        .collect();

    let mut pass_count = 0;
    let mut fail_count = 0;
    let mut total_ssim = 0.0;
    let mut valid_count = 0;

    for result in &results {
        let ssim = result.ssim_score;
        total_ssim += ssim;
        valid_count += 1;
        if result.status == "pass" {
            pass_count += 1;
        } else if result.status.starts_with("fail") {
            fail_count += 1;
        }
        if let Err(e) = db.insert(
            &result.hash,
            result.ssim_score,
            result.page_count,
            &result.status,
            &result.dir_name,
        ) {
            eprintln!("Failed to insert result for {}: {}", result.dir_name, e);
        }
    }

    let avg_ssim = if valid_count > 0 {
        total_ssim / valid_count as f64
    } else {
        0.0
    };

    println!();
    println!("=== Summary ===");
    println!("Total: {}", results.len());
    println!("Pass (>={:.2}): {}", SSIM_PASS_THRESHOLD, pass_count);
    println!("Fail: {}", fail_count);
    println!("Average SSIM: {:.4}", avg_ssim);
    println!("Results written to: {}", cli.output.display());

    Ok(())
}
