//! End-to-end test: manifest → cached HTML → extracted `.rs`.
//!
//! Uses the synthetic fixture in `fixtures/synthetic_how_to.html` so
//! the test stays offline + deterministic. Asserts the extractor
//! picks the Rust block (not the shell or TOML blocks), decodes HTML
//! entities, writes a stable header, and is idempotent (unchanged
//! outputs don't get rewritten).

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn bin_path() -> PathBuf {
    // cargo test sets CARGO_BIN_EXE_<name>.
    PathBuf::from(env!("CARGO_BIN_EXE_pdfluent-snippet-extract"))
}

fn workdir(subdir: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("pdfluent-snippet-extract-it-{subdir}"));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    base
}

/// Seed a cache dir + manifest pointing at the in-repo synthetic
/// fixture. Returns `(cache_dir, manifest_path, out_dir)`.
fn seed_case(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
    let work = workdir(tag);
    let cache = work.join("cache");
    let out = work.join("out");
    fs::create_dir_all(&cache).unwrap();

    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/synthetic_how_to.html");
    fs::copy(&fixture, cache.join("open_pdf_rust.html")).unwrap();

    let manifest = work.join("manifest.toml");
    fs::write(
        &manifest,
        r#"
[[page]]
url = "https://pdfluent.com/how-to/open-pdf-rust"
slug = "open_pdf_rust"
scope = "article"
"#,
    )
    .unwrap();

    (cache, manifest, out)
}

#[test]
fn extracts_rust_block_from_synthetic_fixture() {
    let (cache, manifest, out) = seed_case("extract");

    let status = Command::new(bin_path())
        .args([
            "--manifest",
            manifest.to_str().unwrap(),
            "--cache-dir",
            cache.to_str().unwrap(),
            "--out-dir",
            out.to_str().unwrap(),
            "--fetched",
            "2026-04-21",
        ])
        .status()
        .expect("run extractor");
    assert!(status.success());

    let written = fs::read_to_string(out.join("open_pdf_rust.rs")).expect("output written");

    // Header contract.
    assert!(written.starts_with("//! web_examples/open_pdf_rust\n"));
    assert!(written
        .contains("Source: <https://pdfluent.com/how-to/open-pdf-rust> (fetched 2026-04-21)"));
    assert!(written.contains("Auto-extracted"));

    // Correct block picked.
    assert!(written.contains("use pdfluent::prelude::*;"));
    assert!(written.contains("PdfDocument::open"));

    // Non-Rust siblings were rejected.
    assert!(!written.contains("[dependencies]"));
    assert!(!written.contains("cargo build"));

    // HTML entities decoded.
    assert!(written.contains("-> Result<()>"));
    assert!(!written.contains("&gt;"));
    assert!(!written.contains("&quot;"));
}

#[test]
fn idempotent_re_runs_do_not_rewrite_unchanged_file() {
    let (cache, manifest, out) = seed_case("idempotent");

    let run = || {
        Command::new(bin_path())
            .args([
                "--manifest",
                manifest.to_str().unwrap(),
                "--cache-dir",
                cache.to_str().unwrap(),
                "--out-dir",
                out.to_str().unwrap(),
                "--fetched",
                "2026-04-21",
            ])
            .status()
            .expect("run extractor")
    };

    assert!(run().success());
    let target = out.join("open_pdf_rust.rs");
    let mtime_first = fs::metadata(&target).unwrap().modified().unwrap();

    // Sleep briefly so the filesystem mtime is distinct if the file
    // were rewritten.
    std::thread::sleep(std::time::Duration::from_millis(10));

    assert!(run().success());
    let mtime_second = fs::metadata(&target).unwrap().modified().unwrap();

    // Same bytes → extractor must not rewrite → mtime unchanged.
    assert_eq!(mtime_first, mtime_second);
}

#[test]
fn dry_run_does_not_write_file() {
    let (cache, manifest, out) = seed_case("dry");

    let status = Command::new(bin_path())
        .args([
            "--manifest",
            manifest.to_str().unwrap(),
            "--cache-dir",
            cache.to_str().unwrap(),
            "--out-dir",
            out.to_str().unwrap(),
            "--fetched",
            "2026-04-21",
            "--dry-run",
        ])
        .status()
        .expect("run extractor");
    assert!(status.success());

    assert!(
        !out.join("open_pdf_rust.rs").exists(),
        "dry-run must not write output",
    );
}

#[test]
fn offline_without_fetched_flag_is_rejected() {
    // Codex P1 on PR #1275: defaulting to today's date breaks the
    // drift-guard on the day after extraction. Offline runs now
    // require an explicit `--fetched`.
    let (cache, manifest, out) = seed_case("no-fetched");

    let output = Command::new(bin_path())
        .args([
            "--manifest",
            manifest.to_str().unwrap(),
            "--cache-dir",
            cache.to_str().unwrap(),
            "--out-dir",
            out.to_str().unwrap(),
        ])
        .output()
        .expect("run extractor");

    assert!(
        !output.status.success(),
        "missing --fetched on offline run must error",
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--fetched"),
        "error should name the missing flag, got: {stderr}",
    );
    assert!(
        !out.join("open_pdf_rust.rs").exists(),
        "failed run must not have written output",
    );
}
