// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use clap::Parser;
use image::{GenericImageView, ImageBuffer, Rgb, RgbImage};
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

const SSIM_PASS_THRESHOLD: f64 = 0.85;
const SSIM_HIGH_THRESHOLD: f64 = 0.99;

#[derive(Parser)]
#[command(name = "xfa-pdfrest-compare")]
#[command(about = "Compare pre-rendered XFA PNGs against Adobe pdfrest ground truth")]
struct Cli {
    #[arg(long)]
    golden_dir: PathBuf,

    #[arg(long)]
    output: PathBuf,

    #[arg(long, short)]
    analyze_fonts: bool,

    #[arg(long, short)]
    generate_diffs: bool,

    #[arg(long, short)]
    dpi_investigation: bool,

    #[arg(long, default_value_t = 15)]
    top_low_ssim: usize,
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
            pages_compared INTEGER,
            diff_path TEXT,
            font_analysis TEXT,
            root_cause TEXT,
            timestamp TEXT DEFAULT (datetime('now'))
        )",
    )?;
    // Add pages_compared column to existing databases that lack it.
    let _ = conn.execute_batch("ALTER TABLE results ADD COLUMN pages_compared INTEGER");
    Ok(conn)
}

#[derive(Debug, Clone)]
struct FontInfo {
    typeface: String,
    on_system: bool,
}

fn extract_fonts_from_xfa_template(template_xml: &str) -> Vec<String> {
    let mut fonts = Vec::new();
    let re = regex::Regex::new(r#"typeface="([^"]+)""#).unwrap();
    for cap in re.captures_iter(template_xml) {
        if let Some(m) = cap.get(1) {
            let typeface = m.as_str().to_string();
            if !fonts.contains(&typeface) {
                fonts.push(typeface);
            }
        }
    }
    fonts
}

fn check_system_font(typeface: &str, system_fonts: &HashMap<String, PathBuf>) -> bool {
    let lower = typeface.to_lowercase();
    system_fonts.contains_key(&lower)
}

fn analyze_fonts_for_pdf(
    pdf_data: &[u8],
    system_fonts: &HashMap<String, PathBuf>,
) -> Vec<FontInfo> {
    let mut fonts = Vec::new();

    let xfa_result = pdf_xfa::extract::extract_xfa_from_bytes(pdf_data.to_vec());
    let template_xml = match &xfa_result {
        Ok(packets) => packets.template(),
        Err(_) => None,
    };

    let template_fonts: Vec<String> = match template_xml {
        Some(xml) => extract_fonts_from_xfa_template(xml),
        None => Vec::new(),
    };

    for typeface in template_fonts {
        let on_system = check_system_font(&typeface, system_fonts);
        fonts.push(FontInfo {
            typeface,
            on_system,
        });
    }

    fonts
}

fn get_system_fonts() -> HashMap<String, PathBuf> {
    let mut fonts = HashMap::new();
    for dir in system_font_dirs() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if matches!(ext.as_str(), "ttf" | "otf" | "ttc" | "otc") {
                    if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                        fonts.insert(name.to_lowercase(), path);
                    }
                }
            }
        }
    }
    fonts
}

fn system_font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from("/System/Library/Fonts"));
        dirs.push(PathBuf::from("/Library/Fonts"));
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(PathBuf::from(format!("{home}/Library/Fonts")));
        }
    }
    #[cfg(target_os = "linux")]
    {
        dirs.push(PathBuf::from("/usr/share/fonts"));
        dirs.push(PathBuf::from("/usr/local/share/fonts"));
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(PathBuf::from(format!("{home}/.local/share/fonts")));
            dirs.push(PathBuf::from(format!("{home}/.fonts")));
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(windir) = std::env::var("WINDIR") {
            dirs.push(PathBuf::from(format!("{windir}\\Fonts")));
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            dirs.push(PathBuf::from(format!("{local}\\Microsoft\\Windows\\Fonts")));
        }
    }
    dirs
}

fn generate_diff_image(
    our_pixels: &[u8],
    our_w: u32,
    our_h: u32,
    ref_pixels: &[u8],
    ref_w: u32,
    ref_h: u32,
    output_path: &Path,
) -> anyhow::Result<()> {
    let w = our_w.min(ref_w);
    let h = our_h.min(ref_h);

    let mut diff_img: RgbImage = ImageBuffer::new(w, h);

    for y in 0..h {
        for x in 0..w {
            let idx = ((y * w + x) * 4) as usize;
            let our_r = our_pixels[idx] as i32;
            let our_g = our_pixels[idx + 1] as i32;
            let our_b = our_pixels[idx + 2] as i32;

            let ref_r = ref_pixels[idx] as i32;
            let ref_g = ref_pixels[idx + 1] as i32;
            let ref_b = ref_pixels[idx + 2] as i32;

            let diff_r = (our_r - ref_r).unsigned_abs() as u8;
            let diff_g = (our_g - ref_g).unsigned_abs() as u8;
            let diff_b = (our_b - ref_b).unsigned_abs() as u8;
            let max_diff = diff_r.max(diff_g).max(diff_b);

            if max_diff < 10 {
                diff_img.put_pixel(x, y, Rgb([0, 180, 0]));
            } else {
                diff_img.put_pixel(x, y, Rgb([max_diff, 0, 0]));
            }
        }
    }

    diff_img.save(output_path)?;
    Ok(())
}

fn investigate_dpi(dir: &Path) -> Option<(u32, u32, f64)> {
    let pdfrest_path = find_png(dir, "pdfrest_page", 1)?;
    let _our_path = find_png(dir, "page", 1)?;

    let pdfrest_img = image::open(&pdfrest_path).ok()?;

    let pdfrest_dims = pdfrest_img.dimensions();

    let (pdfrest_w, pdfrest_h) = pdfrest_dims;

    let pdfrest_dpi = (pdfrest_w as f64 / 8.5) * 72.0 / pdfrest_h as f64;

    Some((pdfrest_w, pdfrest_h, pdfrest_dpi))
}

fn categorize_failure(
    ssim: f64,
    our_pages: usize,
    pdfrest_pages: usize,
    diff_path: Option<&Path>,
) -> String {
    if our_pages != pdfrest_pages {
        if our_pages > pdfrest_pages {
            return "page_mismatch_we_gt".to_string();
        } else {
            return "page_mismatch_we_lt".to_string();
        }
    }

    if ssim >= SSIM_HIGH_THRESHOLD {
        return "pass".to_string();
    }

    if let Some(path) = diff_path {
        if path.exists() {
            return "visual_diff".to_string();
        }
    }

    "low_ssim".to_string()
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

    let mut skipped = 0usize;
    for (_, our_path, pdfrest_path) in &page_matches {
        let (our_pixels, our_w, our_h) = match load_png(our_path) {
            Ok(p) => p,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        let (ref_pixels, ref_w, ref_h) = match load_png(pdfrest_path) {
            Ok(p) => p,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        total_ssim += compute_ssim(&our_pixels, our_w, our_h, &ref_pixels, ref_w, ref_h);
    }
    let page_count = page_count - skipped;
    if page_count == 0 {
        return None;
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

struct SsimHistogram {
    buckets: [usize; 12],
}

impl SsimHistogram {
    fn new() -> Self {
        Self { buckets: [0; 12] }
    }

    fn add(&mut self, ssim: f64) {
        let idx = if ssim >= 0.99 {
            11
        } else if ssim >= 0.95 {
            10
        } else if ssim >= 0.90 {
            9
        } else if ssim >= 0.80 {
            8
        } else if ssim >= 0.70 {
            7
        } else if ssim >= 0.60 {
            6
        } else if ssim >= 0.50 {
            5
        } else if ssim >= 0.40 {
            4
        } else if ssim >= 0.30 {
            3
        } else if ssim >= 0.20 {
            2
        } else if ssim >= 0.10 {
            1
        } else {
            0
        };
        self.buckets[idx] += 1;
    }

    fn print(&self) {
        let labels = [
            "0.00-0.10",
            "0.10-0.20",
            "0.20-0.30",
            "0.30-0.40",
            "0.40-0.50",
            "0.50-0.60",
            "0.60-0.70",
            "0.70-0.80",
            "0.80-0.90",
            "0.90-0.95",
            "0.95-0.99",
            "0.99-1.00",
        ];

        println!();
        println!("=== SSIM Histogram ===");
        for (i, label) in labels.iter().enumerate() {
            let count = self.buckets[i];
            let bar = "█".repeat(count.min(50));
            println!("  {} │ {} {}", label, bar, count);
        }
    }
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

    let system_fonts = if cli.analyze_fonts {
        Some(get_system_fonts())
    } else {
        None
    };

    let mut has_comparable = 0usize;
    let mut pass_count = 0usize;
    let mut fail_count = 0usize;
    let mut total_ssim = 0.0;
    let mut valid_count = 0usize;
    let mut no_render = 0usize;
    let mut encrypted_skip = 0usize;
    let mut pdfrest_truncated_count = 0usize;
    let mut worst_cases = Vec::new();

    let mut histogram = SsimHistogram::new();

    let mut ssim_bucket_095_099 = 0usize;
    let mut ssim_bucket_085_095 = 0usize;
    let mut ssim_bucket_080_085 = 0usize;
    let mut ssim_bucket_050_080 = 0usize;
    let mut ssim_bucket_000_050 = 0usize;
    let mut page_mismatch_we_gt = 0usize;
    let mut page_mismatch_we_lt = 0usize;

    let mut font_usage: HashMap<String, usize> = HashMap::new();
    let mut total_fonts_checked = 0usize;
    let mut fonts_on_system = 0usize;

    let mut dpi_data: Vec<(String, u32, u32, f64)> = Vec::new();

    for dir in &entries {
        let dir_name = dir
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        if cli.dpi_investigation {
            if let Some((w, h, dpi)) = investigate_dpi(dir) {
                dpi_data.push((dir_name.clone(), w, h, dpi));
                println!(
                    "[{}] pdfrest: {}x{} px, estimated DPI: {:.1}",
                    dir_name, w, h, dpi
                );
                continue;
            }
        }

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

        let mut diff_path: Option<PathBuf> = None;
        let root_cause;
        let mut font_analysis: Option<String> = None;

        if pdfrest_truncated {
            pdfrest_truncated_count += 1;
            let ssim_val = ssim;
            let partial_status = if ssim_val >= SSIM_PASS_THRESHOLD {
                "partial_match"
            } else {
                "partial_fail"
            };

            histogram.add(ssim_val);
            total_ssim += ssim_val;
            valid_count += 1;
            has_comparable += 1;

            if ssim_val >= SSIM_PASS_THRESHOLD {
                pass_count += 1;
            } else {
                fail_count += 1;
            }

            if ssim_val >= SSIM_HIGH_THRESHOLD {
                ssim_bucket_095_099 += 1;
            } else if ssim_val >= 0.95 {
                ssim_bucket_085_095 += 1;
            } else if ssim_val >= 0.80 {
                ssim_bucket_080_085 += 1;
            } else if ssim_val >= 0.50 {
                ssim_bucket_050_080 += 1;
            } else {
                ssim_bucket_000_050 += 1;
            }

            worst_cases.push((dir_name.clone(), ssim_val));
            worst_cases.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            if worst_cases.len() > cli.top_low_ssim {
                worst_cases.pop();
            }

            if let Err(e) = conn.execute(
                "INSERT OR REPLACE INTO results (hash, ssim_score, page_count, status, our_pages, pdfrest_pages, pdfrest_truncated, pages_compared, root_cause)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![hash, ssim_val, page_count as i64, partial_status, our_pages as i64, pdfrest_pages as i64, pdfrest_truncated as i64, page_count as i64, partial_status],
            ) {
                eprintln!("DB error: {}", e);
            }
            println!(
                "[{}] ssim={:.4} pages_compared={}/{} status={} (pdfrest_truncated)",
                dir_name, ssim_val, page_count, our_pages, partial_status
            );
        } else {
            has_comparable += 1;
            let ssim_val = ssim;

            if status == "pass" {
                pass_count += 1;
            } else {
                fail_count += 1;
            }

            if ssim_val >= SSIM_HIGH_THRESHOLD {
                ssim_bucket_095_099 += 1;
            } else if ssim_val >= 0.95 {
                ssim_bucket_085_095 += 1;
            } else if ssim_val >= 0.80 {
                ssim_bucket_080_085 += 1;
            } else if ssim_val >= 0.50 {
                ssim_bucket_050_080 += 1;
            } else {
                ssim_bucket_000_050 += 1;
            }

            histogram.add(ssim_val);

            if our_pages != pdfrest_pages {
                if our_pages > pdfrest_pages {
                    page_mismatch_we_gt += 1;
                } else {
                    page_mismatch_we_lt += 1;
                }
            }

            if cli.analyze_fonts {
                let input_data = fs::read(dir.join("input.pdf")).unwrap_or_default();
                let font_infos = analyze_fonts_for_pdf(&input_data, system_fonts.as_ref().unwrap());
                let mut font_details = Vec::new();
                for fi in &font_infos {
                    total_fonts_checked += 1;
                    if fi.on_system {
                        fonts_on_system += 1;
                    }
                    *font_usage.entry(fi.typeface.clone()).or_insert(0) += 1;
                    font_details.push(format!("{} (system:{})", fi.typeface, fi.on_system));
                }
                font_analysis = Some(font_details.join("; "));
            }

            if cli.generate_diffs && ssim_val < SSIM_HIGH_THRESHOLD {
                let (page_matches, _, _) = find_matching_pages(dir);
                for (_, our_path, pdfrest_path) in page_matches {
                    let (our_pixels, our_w, our_h) = match load_png(&our_path) {
                        Ok(p) => p,
                        Err(_) => continue,
                    };
                    let (ref_pixels, ref_w, ref_h) = match load_png(&pdfrest_path) {
                        Ok(p) => p,
                        Err(_) => continue,
                    };

                    let our_name = our_path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("page");
                    let diff_file = dir.join(format!("{}_diff.png", our_name));

                    if generate_diff_image(
                        &our_pixels,
                        our_w,
                        our_h,
                        &ref_pixels,
                        ref_w,
                        ref_h,
                        &diff_file,
                    )
                    .is_ok()
                    {
                        diff_path = Some(diff_file);
                        break;
                    }
                }
            }

            root_cause =
                categorize_failure(ssim_val, our_pages, pdfrest_pages, diff_path.as_deref());

            total_ssim += ssim_val;
            valid_count += 1;
            worst_cases.push((dir_name.clone(), ssim_val));
            worst_cases.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            if worst_cases.len() > cli.top_low_ssim {
                worst_cases.pop();
            }

            if let Err(e) = conn.execute(
                "INSERT OR REPLACE INTO results (hash, ssim_score, page_count, status, our_pages, pdfrest_pages, pdfrest_truncated, pages_compared, diff_path, font_analysis, root_cause)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![hash, ssim_val, page_count as i64, status, our_pages as i64, pdfrest_pages as i64, pdfrest_truncated as i64, page_count as i64, diff_path.as_ref().and_then(|p| p.to_str()), font_analysis.as_deref(), root_cause],
            ) {
                eprintln!("DB error: {}", e);
            }

            let diff_str = diff_path
                .as_ref()
                .map(|p| format!(" diff={}", p.display()))
                .unwrap_or_default();
            println!(
                "[{}] ssim={:.4} pages={} status={}{}",
                dir_name, ssim_val, page_count, status, diff_str
            );
        }
    }

    if cli.dpi_investigation {
        println!();
        println!("=== DPI Investigation Summary ===");
        let mut sorted_dpi: Vec<_> = dpi_data.clone();
        sorted_dpi.sort_by(|a, b| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal));
        if !sorted_dpi.is_empty() {
            let avg_dpi: f64 =
                sorted_dpi.iter().map(|(_, _, _, dpi)| dpi).sum::<f64>() / sorted_dpi.len() as f64;
            println!("Average pdfrest DPI: {:.1}", avg_dpi);
            let min_dpi = sorted_dpi.first().map(|(_, _, _, d)| *d).unwrap_or(0.0);
            let max_dpi = sorted_dpi.last().map(|(_, _, _, d)| *d).unwrap_or(0.0);
            println!("DPI range: {:.1} - {:.1}", min_dpi, max_dpi);
        }
        println!("Results written to: {}", cli.output.display());
        return Ok(());
    }

    let avg_ssim = if valid_count > 0 {
        total_ssim / valid_count as f64
    } else {
        0.0
    };

    histogram.print();

    println!();
    println!("=== Summary ===");
    println!("Total directories: {}", entries.len());
    println!("No render (skipped): {}", no_render);
    println!("Encrypted (skipped): {}", encrypted_skip);
    println!(
        "pdfrest truncated (partial compare on first {} pages): {}",
        3, pdfrest_truncated_count
    );
    println!("Comparable entries: {}", has_comparable);
    let pass_rate = if has_comparable > 0 {
        (pass_count as f64 / has_comparable as f64) * 100.0
    } else {
        0.0
    };
    let fail_rate = if has_comparable > 0 {
        (fail_count as f64 / has_comparable as f64) * 100.0
    } else {
        0.0
    };
    println!(
        "Pass (≥{:.2}): {} ({:.1}%)",
        SSIM_PASS_THRESHOLD, pass_count, pass_rate
    );
    println!("Fail: {} ({:.1}%)", fail_count, fail_rate);
    println!("Average SSIM: {:.4}", avg_ssim);
    println!();
    println!("=== SSIM Breakdown ===");
    println!(
        "  SSIM ≥ {:.2} (high quality): {}",
        SSIM_HIGH_THRESHOLD, ssim_bucket_095_099
    );
    println!(
        "  SSIM 0.95-{:.2} (acceptable):  {}",
        SSIM_HIGH_THRESHOLD - 0.01,
        ssim_bucket_085_095
    );
    println!("  SSIM 0.80-0.95 (marginal):   {}", ssim_bucket_080_085);
    println!("  SSIM 0.50-0.80 (low):       {}", ssim_bucket_050_080);
    println!("  SSIM < 0.50 (very low):     {}", ssim_bucket_000_050);
    println!();
    println!("=== Failure Category Breakdown ===");
    if page_mismatch_we_gt > 0 {
        println!("  page_mismatch (we > pdfrest): {}", page_mismatch_we_gt);
    }
    if page_mismatch_we_lt > 0 {
        println!("  page_mismatch (we < pdfrest): {}", page_mismatch_we_lt);
    }
    let total_page_mismatch = page_mismatch_we_gt + page_mismatch_we_lt;
    let total_low_ssim =
        ssim_bucket_000_050 + ssim_bucket_050_080 + ssim_bucket_080_085 + ssim_bucket_085_095;
    println!("  total page_mismatch: {}", total_page_mismatch);
    println!(
        "  total low_ssim (< {:.2}): {}",
        SSIM_HIGH_THRESHOLD, total_low_ssim
    );
    println!("  encrypted_skip: {}", encrypted_skip);
    println!("  pdfrest_truncated: {}", pdfrest_truncated_count);

    if cli.analyze_fonts && total_fonts_checked > 0 {
        println!();
        println!("=== Font Usage Analysis ===");
        println!("Total fonts checked: {}", total_fonts_checked);
        println!(
            "Fonts on system: {} ({:.1}%)",
            fonts_on_system,
            (fonts_on_system as f64 / total_fonts_checked as f64) * 100.0
        );
        println!();
        println!("Top fonts across gold set:");
        let mut sorted_fonts: Vec<_> = font_usage.iter().collect();
        sorted_fonts.sort_by(|a, b| b.1.cmp(a.1));
        for (name, count) in sorted_fonts.iter().take(20) {
            println!("  {}: {} occurrences", name, count);
        }
    }

    println!();
    println!("=== Worst {} Cases ===", cli.top_low_ssim);
    for (name, ssim) in worst_cases.iter().rev() {
        println!("  {}: {:.4}", name, ssim);
    }
    println!("Results written to: {}", cli.output.display());

    Ok(())
}
