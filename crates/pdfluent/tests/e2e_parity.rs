//! Epic 5 #1246 — end-to-end parity runner.
//!
//! Verifies the website-parity contract:
//!
//! 1. Every `[[page]]` in
//!    `tools/pdfluent-snippet-extract/manifest.toml` has a
//!    matching `crates/pdfluent/tests/web_examples/<slug>.rs` file.
//! 2. Every matching file carries at least a `_compiles` test.
//! 3. Each file is explicitly classified as **runnable** (has a
//!    `_runs` test) or **not runnable** (deferred for a documented
//!    reason — see `non_runnable` table below). No silent skips.
//!
//! This runner does NOT re-execute the individual `_runs` tests —
//! those are wired through the existing `web_examples.rs` binary.
//! Running them a second time here would double test time with no
//! extra signal. Instead it **asserts every slug is categorised**,
//! which is the missing link between #1237 (compile suite) and
//! #1238 (drift guard).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR points at crates/pdfluent/. Climb two levels.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace root")
}

fn manifest_path() -> PathBuf {
    workspace_root().join("tools/pdfluent-snippet-extract/manifest.toml")
}

fn web_examples_dir() -> PathBuf {
    workspace_root().join("crates/pdfluent/tests/web_examples")
}

// ---------------------------------------------------------------------------
// Runnability classification
// ---------------------------------------------------------------------------
//
// Every slug must appear in exactly one of:
// - NATURALLY_RUNNABLE: has a `_runs` test that executes today
// - DEFERRED_RUNTIME: has a documented reason the `_runs` test is
//   currently `#[ignore]`d. The reason is asserted to match a
//   known tracker issue so the category can't drift silently.
//
// A slug that's in neither is a test-coverage gap and fails this
// runner.

/// Slugs whose `_runs` test is expected to execute unconditionally
/// on every run (no `#[ignore]` attribute).
const NATURALLY_RUNNABLE: &[&str] = &[
    "compress_pdf_rust",
    "convert_pdf_to_docx_rust",
    "encrypt_pdf_rust",
    "extract_text_pdf_rust",
    "fill_pdf_form_rust",
    "insert_image_pdf_rust",
    "merge_pdfs_rust",
    "render_pdf_to_jpeg_rust",
    "subset_fonts_rust",
];

/// Slugs whose `_runs` test is currently gated by `#[ignore]`, with
/// the blocking issue explicitly captured for audit. Empty today —
/// `render_pdf_to_png_rust_runs` is ignored but that's because of a
/// bootstrap-fixture detail, not a first-class runtime gap; see
/// comment below.
///
/// If a future runtime regresses a currently-runnable snippet, the
/// fix is to add the slug here (with the tracking issue) rather than
/// silently removing it from `NATURALLY_RUNNABLE`.
const DEFERRED_RUNTIME: &[(&str, &str)] = &[];

/// Slugs that are present as `tests/web_examples/*.rs` files but
/// NOT in `manifest.toml` yet. These are hand-authored placeholders
/// that will migrate to manifest-driven extraction once their
/// upstream how-to page ships. Listing them here keeps the parity
/// runner aware of the full universe.
const PLACEHOLDERS_NOT_IN_MANIFEST: &[&str] = &[
    "compress_pdf_rust",
    "convert_pdf_to_docx_rust",
    "insert_image_pdf_rust",
    "render_pdf_to_jpeg_rust",
    "subset_fonts_rust",
];

// `render_pdf_to_png_rust` has a legacy `#[ignore]` on its `_runs`
// test pointing to #1224 bootstrap work; the underlying method
// (`to_images`) IS wired and the companion `render_pdf_to_jpeg_rust`
// exercises it fully without the ignore gate. The png file is
// intentionally left as-is; when its bootstrap fixture lands, the
// ignore is removed in the same PR.
const LEGACY_IGNORED_WITH_COMPANION_COVERAGE: &[&str] = &["render_pdf_to_png_rust"];

// ---------------------------------------------------------------------------
// Manifest parsing (no toml dep — keep test deps minimal)
// ---------------------------------------------------------------------------

fn parse_manifest_slugs(manifest_src: &str) -> Vec<String> {
    let mut slugs = Vec::new();
    let mut in_page = false;
    for raw in manifest_src.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with("[[page]]") {
            in_page = true;
            continue;
        }
        if line.starts_with('[') {
            in_page = false;
            continue;
        }
        if !in_page {
            continue;
        }
        if let Some(rest) = line.strip_prefix("slug") {
            // e.g. `slug = "fill_pdf_form_rust"`
            let rest = rest.trim_start_matches([' ', '=']);
            let unquoted = rest.trim().trim_start_matches('"').trim_end_matches('"');
            if !unquoted.is_empty() {
                slugs.push(unquoted.to_owned());
            }
        }
    }
    slugs
}

fn file_exists_for(slug: &str) -> bool {
    web_examples_dir().join(format!("{slug}.rs")).exists()
}

fn file_body(slug: &str) -> String {
    fs::read_to_string(web_examples_dir().join(format!("{slug}.rs")))
        .expect("web_example file readable")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn every_manifest_slug_has_a_web_example_file() {
    let src = fs::read_to_string(manifest_path()).expect("manifest readable");
    let slugs = parse_manifest_slugs(&src);
    assert!(!slugs.is_empty(), "manifest parsed 0 slugs — parser broken");

    let missing: Vec<_> = slugs
        .iter()
        .filter(|s| !file_exists_for(s))
        .cloned()
        .collect();
    assert!(
        missing.is_empty(),
        "manifest entries without a matching tests/web_examples/*.rs: {missing:?}",
    );
}

#[test]
fn every_web_example_file_has_a_compile_test() {
    // Accept either `fn <slug>_compiles` (the extractor's default
    // convention) or any `_compiles`-suffixed test function. Two
    // legacy hand-written files use abbreviated function names
    // (`extract_text_rust_compiles`, `render_png_rust_compiles`)
    // that predate the manifest convention — still contract-valid.
    for slug in discover_all_slugs() {
        let body = file_body(&slug);
        assert!(
            body.contains("_compiles"),
            "{slug}.rs has no `_compiles` test — extractor contract violated",
        );
    }
}

#[test]
fn every_slug_is_explicitly_classified() {
    // A slug MUST appear in exactly one of:
    //   NATURALLY_RUNNABLE
    //   DEFERRED_RUNTIME (key)
    //   LEGACY_IGNORED_WITH_COMPANION_COVERAGE
    // Silent skips are a test-coverage bug.
    let runnable: HashSet<&str> = NATURALLY_RUNNABLE.iter().copied().collect();
    let deferred: HashSet<&str> = DEFERRED_RUNTIME.iter().map(|(s, _)| *s).collect();
    let legacy: HashSet<&str> = LEGACY_IGNORED_WITH_COMPANION_COVERAGE
        .iter()
        .copied()
        .collect();

    let mut problems: Vec<String> = Vec::new();
    for slug in discover_all_slugs() {
        let r = runnable.contains(slug.as_str());
        let d = deferred.contains(slug.as_str());
        let l = legacy.contains(slug.as_str());
        let hits = [r, d, l].into_iter().filter(|b| *b).count();
        if hits != 1 {
            problems.push(format!(
                "{slug}: classified in {hits} categories (runnable={r}, deferred={d}, legacy={l})"
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "slug classification gaps:\n{}",
        problems.join("\n")
    );
}

#[test]
fn naturally_runnable_slugs_match_master_reality() {
    // Pin: any slug in NATURALLY_RUNNABLE must NOT carry a plain
    // `#[ignore]` on its `_runs` test. Catches accidental
    // regressions where a runtime stub creeps back in.
    //
    // Accepts both the `<slug>_runs` convention and the legacy
    // abbreviated names (e.g. `extract_text_rust_runs` instead of
    // `extract_text_pdf_rust_runs`).
    for slug in NATURALLY_RUNNABLE {
        let body = file_body(slug);
        let idx = body
            .find(&format!("fn {slug}_runs"))
            .or_else(|| {
                // Legacy abbreviated convention: strip a `_pdf`
                // segment from the slug.
                let abbreviated = slug.replace("_pdf", "");
                body.find(&format!("fn {abbreviated}_runs"))
            })
            .or_else(|| body.find("_runs()"));
        // Codex #1279 P2: a slug on NATURALLY_RUNNABLE MUST have a
        // `_runs` function. Silently continuing would let CI pass
        // after a `_runs` test is accidentally deleted while the
        // slug remains in the runnable list.
        let idx = idx.unwrap_or_else(|| {
            panic!(
                "{slug} is listed in NATURALLY_RUNNABLE but has no _runs \
                 function in tests/web_examples/{slug}.rs. Either add \
                 the test or move the slug to DEFERRED_RUNTIME."
            )
        });
        // Look backwards from `fn foo_runs` for attributes on the
        // same item. `#[ignore]` typically sits 1-2 lines above.
        let window_start = idx.saturating_sub(120);
        let window = &body[window_start..idx];
        assert!(
            !window.contains("#[ignore"),
            "{slug} is in NATURALLY_RUNNABLE but its _runs test is `#[ignore]`d. \
             Either wire it (remove ignore) or move it to DEFERRED_RUNTIME.",
        );
    }
}

#[test]
fn coverage_summary() {
    // Purely informational — always passes. The stdout is surfaced
    // to CI logs so a human can eyeball coverage on every run.
    let all = discover_all_slugs();
    let runnable = NATURALLY_RUNNABLE.len();
    let deferred = DEFERRED_RUNTIME.len();
    let legacy = LEGACY_IGNORED_WITH_COMPANION_COVERAGE.len();
    let placeholders = PLACEHOLDERS_NOT_IN_MANIFEST.len();

    println!(
        "e2e-parity coverage: total={}, naturally_runnable={}, deferred={}, legacy_ignored={}, placeholders_not_in_manifest={}",
        all.len(),
        runnable,
        deferred,
        legacy,
        placeholders,
    );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn discover_all_slugs() -> Vec<String> {
    let dir = web_examples_dir();
    let mut out = Vec::new();
    for entry in fs::read_dir(&dir).expect("web_examples readable") {
        let entry = entry.expect("dir entry");
        let path: PathBuf = entry.path();
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !file_name.ends_with(".rs") {
            continue;
        }
        if file_name == "mod.rs" {
            continue;
        }
        out.push(file_name.trim_end_matches(".rs").to_owned());
    }
    out.sort();
    out
}

// ---------------------------------------------------------------------------
// Manifest parser unit tests
// ---------------------------------------------------------------------------

#[test]
fn manifest_parser_extracts_slugs() {
    let src = r#"
# comment
[[page]]
url = "https://x/a"
slug = "a_rust"

[[page]]
url = "https://x/b"
slug = "b_rust"
scope = "article"

# commented-out:
# [[page]]
# url = "https://x/c"
# slug = "c_rust"
"#;
    let got = parse_manifest_slugs(src);
    assert_eq!(got, vec!["a_rust".to_string(), "b_rust".to_string()]);
}

#[test]
fn discover_all_slugs_finds_committed_files() {
    let slugs = discover_all_slugs();
    // Sanity: the five 3C-2 placeholders must all be present.
    for expected in PLACEHOLDERS_NOT_IN_MANIFEST {
        assert!(
            slugs.iter().any(|s| s == *expected),
            "{expected} missing from tests/web_examples/",
        );
    }
    // Plus the five original hand-written ones.
    for expected in &[
        "encrypt_pdf_rust",
        "extract_text_pdf_rust",
        "fill_pdf_form_rust",
        "merge_pdfs_rust",
        "render_pdf_to_png_rust",
    ] {
        assert!(slugs.iter().any(|s| s == *expected), "{expected} missing");
    }
}

#[test]
fn manifest_path_resolves_and_is_readable() {
    let path = manifest_path();
    let src =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("manifest at {}: {e}", path.display()));
    assert!(
        src.contains("[[page]]"),
        "manifest at {} has no [[page]] entries",
        path.display(),
    );
}
