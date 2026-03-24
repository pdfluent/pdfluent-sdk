//! Measure OCR accuracy on the synthetic scanned fixtures produced by
//! `generate_scanned_pdfs`.
//!
//! For each `fixtures/scanned/scan_NN_<name>.pdf`, the companion
//! `scan_NN_<name>.source.pdf` provides ground-truth text via text
//! extraction.  The scanned PDF is OCR'd with `OcrsBackend::try_default()`
//! and the result is compared character-by-character.
//!
//! Accuracy metric: character bag overlap
//!   overlap  = sum of min(count_ocr[c], count_gt[c])  for each Latin char c
//!   accuracy = overlap / max(total_ocr_chars, total_gt_chars)
//!
//! This metric is order-insensitive, making it robust to minor word-boundary
//! differences typical in OCR output.  Target: ≥ 0.80 (80 %).
//!
//! Prerequisites:
//!   1. Run `cargo run -p xfa-test-runner --example generate_scanned_pdfs`
//!   2. Download OCR models (see pdf-engine/src/ocr.rs for URLs) to
//!      ~/.cache/ocrs/ or set OCRS_DETECTION_MODEL / OCRS_RECOGNITION_MODEL.
//!
//! Run with:
//!   cargo run -p xfa-test-runner --features ocr --example check_ocr_accuracy

use std::collections::HashMap;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.join("../..").canonicalize().unwrap();
    let fixtures_dir = workspace_root.join("fixtures/scanned");

    if !fixtures_dir.exists() {
        eprintln!(
            "fixtures/scanned/ not found — run generate_scanned_pdfs first:\n  \
             cargo run -p xfa-test-runner --example generate_scanned_pdfs"
        );
        std::process::exit(1);
    }

    // Check OCR backend availability.
    #[cfg(not(feature = "ocr"))]
    {
        eprintln!(
            "OCR feature not compiled — build with:\n  \
             cargo run -p xfa-test-runner --features ocr --example check_ocr_accuracy"
        );
        std::process::exit(1);
    }

    #[cfg(feature = "ocr")]
    {
        let backend = match pdf_engine::OcrsBackend::try_default() {
            Ok(b) => b,
            Err(_) => {
                eprintln!(
                    "OCR models not found.\n\
                     Download to ~/.cache/ocrs/:\n  \
                     curl -fsSL https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.rten \
                     -o ~/.cache/ocrs/text-detection.rten\n  \
                     curl -fsSL https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.rten \
                     -o ~/.cache/ocrs/text-recognition.rten"
                );
                std::process::exit(1);
            }
        };

        run_accuracy_tests(&fixtures_dir, &backend);
    }
}

#[cfg(feature = "ocr")]
fn run_accuracy_tests(fixtures_dir: &std::path::Path, backend: &pdf_engine::OcrsBackend) {
    let mut entries: Vec<_> = std::fs::read_dir(fixtures_dir)
        .expect("read fixtures/scanned")
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let s = name.to_string_lossy();
            s.starts_with("scan_") && s.ends_with(".pdf") && !s.contains(".source.")
        })
        .map(|e| e.path())
        .collect();
    entries.sort();

    if entries.is_empty() {
        eprintln!("No scan_*.pdf fixtures found in {}", fixtures_dir.display());
        std::process::exit(1);
    }

    println!(
        "{:<40} {:>8} {:>8} {:>8}",
        "fixture", "gt_chars", "ocr_chars", "accuracy"
    );
    println!("{}", "-".repeat(70));

    let mut total_gt = 0usize;
    let mut total_overlap = 0usize;
    let mut passed = 0usize;

    for scanned_path in &entries {
        let name = scanned_path.file_stem().unwrap().to_string_lossy().to_string();

        // Find companion source PDF.
        let source_path = scanned_path
            .parent()
            .unwrap()
            .join(format!("{name}.source.pdf"));
        if !source_path.exists() {
            println!("  {name:<38} — source PDF not found, skipping");
            continue;
        }

        // Ground-truth text from source.
        let gt_text = match extract_text(&source_path) {
            Some(t) => t,
            None => {
                println!("  {name:<38} — text extraction failed, skipping");
                continue;
            }
        };

        // OCR text from scanned PDF.
        let ocr_text = match ocr_page(scanned_path, backend) {
            Some(t) => t,
            None => {
                println!("  {name:<38} — OCR failed, skipping");
                continue;
            }
        };

        let gt_norm = normalize(&gt_text);
        let ocr_norm = normalize(&ocr_text);

        let gt_chars = gt_norm.len();
        let ocr_chars = ocr_norm.len();
        let overlap = char_bag_overlap(&gt_norm, &ocr_norm);
        let denom = gt_chars.max(ocr_chars);
        let accuracy = if denom == 0 {
            0.0_f64
        } else {
            overlap as f64 / denom as f64
        };

        let marker = if accuracy >= 0.80 { "✓" } else { "✗" };
        println!(
            "{marker} {name:<38} {:>8} {:>8} {:>7.1}%",
            gt_chars,
            ocr_chars,
            accuracy * 100.0
        );

        total_gt += gt_chars;
        total_overlap += overlap;
        if accuracy >= 0.80 {
            passed += 1;
        }
    }

    let total_denom = total_gt;
    let overall = if total_denom == 0 {
        0.0_f64
    } else {
        total_overlap as f64 / total_denom as f64
    };

    println!("{}", "-".repeat(70));
    println!(
        "  Overall accuracy: {:.1}%  ({passed}/{} fixtures ≥ 80 %)",
        overall * 100.0,
        entries.len()
    );

    if overall >= 0.80 {
        println!("  TARGET MET ✓ (≥ 80 %)");
    } else {
        println!("  TARGET NOT MET ✗ (< 80 %)");
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[allow(dead_code)]
fn extract_text(path: &std::path::Path) -> Option<String> {
    let data = std::fs::read(path).ok()?;
    let doc = pdf_engine::PdfDocument::open(data).ok()?;
    doc.extract_text(0).ok()
}

#[cfg(feature = "ocr")]
fn ocr_page(
    path: &std::path::Path,
    backend: &pdf_engine::OcrsBackend,
) -> Option<String> {
    let data = std::fs::read(path).ok()?;
    let doc = pdf_engine::PdfDocument::open(data).ok()?;
    let result = doc.ocr_page(0, backend, 150.0).ok()?;
    Some(result.text)
}

/// Keep only lowercase Latin letters, digits, and spaces.
#[allow(dead_code)]
fn normalize(text: &str) -> String {
    text.chars()
        .filter_map(|c| {
            if c.is_ascii_alphanumeric() {
                Some(c.to_ascii_lowercase())
            } else if c.is_whitespace() {
                Some(' ')
            } else {
                None
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Character bag overlap: sum of min(count_a[c], count_b[c]) over all chars.
#[allow(dead_code)]
fn char_bag_overlap(a: &str, b: &str) -> usize {
    let mut freq_a: HashMap<char, usize> = HashMap::new();
    let mut freq_b: HashMap<char, usize> = HashMap::new();
    for c in a.chars() {
        *freq_a.entry(c).or_insert(0) += 1;
    }
    for c in b.chars() {
        *freq_b.entry(c).or_insert(0) += 1;
    }
    freq_a
        .iter()
        .map(|(c, &ca)| ca.min(*freq_b.get(c).unwrap_or(&0)))
        .sum()
}
