//! Guard: nothing that ships inside the WASM bundle may call std APIs that
//! are compiled-but-panicking on `wasm32-unknown-unknown`.
//!
//! # Why this test exists
//!
//! `SystemTime::now()`, `Instant::now()` and `thread::spawn` all **compile**
//! for `wasm32-unknown-unknown` against std's `unsupported` stubs, and then
//! panic at runtime. Under the release profile's `panic = "abort"` that
//! reaches JavaScript as a bare `RuntimeError: unreachable` — no message, no
//! location, no stack.
//!
//! That is not theoretical. One `SystemTime::now()` on the font-embedding
//! path took down `PdfDoc.convertToPdfa` for every input and every
//! conformance level in the browser build, and it shipped, because the whole
//! native suite stayed green the entire time. Native tests structurally
//! cannot catch this class: the calls work perfectly on every other target.
//!
//! So the defence has to be a source-level rule rather than a runtime test.
//! Use [`pdf_manip::clock::unix_now_secs`] for wall-clock time; if you need a
//! genuinely native-only path, gate it on `#[cfg(not(target_arch = "wasm32"))]`
//! and add it to the allowlist below with the reason.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::path::{Path, PathBuf};

/// Crates whose source ends up inside the WASM bundle.
const WASM_CRATES: &[&str] = &[
    "pdf-manip",
    "pdf-engine",
    "pdf-compliance",
    "pdf-annot",
    "pdf-forms",
    "pdf-redact",
    "pdf-render",
    "pdf-syntax",
];

/// Calls that compile on wasm32 and panic when reached.
const FORBIDDEN: &[&str] = &["SystemTime::now(", "Instant::now(", "thread::spawn("];

/// Sites that are known and accepted, with the reason they are safe.
///
/// Keep this list short and justified. A path here is a promise that the code
/// is either unreachable from the WASM API surface or properly cfg-gated —
/// not merely that someone was in a hurry.
const ALLOWED: &[(&str, &str)] = &[
    (
        "pdf-engine/src/batch.rs",
        "batch worker pool: threads are inherently native, and no wasm-exported \
         function reaches batch processing",
    ),
    (
        "pdf-engine/src/ocr.rs",
        "OCR shells out to native engines (Tesseract/Paddle) that do not exist \
         in a browser build",
    ),
    (
        "pdf-compliance/src/pdfa.rs",
        "timing instrumentation behind an eprintln diagnostics path; see the \
         follow-up note in BUG_WASM_CONVERTTOPDFA.md",
    ),
    (
        "pdf-compliance/src/bin/profile_compliance.rs",
        "a native profiling binary, never compiled into the WASM cdylib",
    ),
];

fn workspace_crates_dir() -> PathBuf {
    // tests/ -> xfa-wasm/ -> crates/
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates dir")
        .to_path_buf()
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        eprintln!(
            "SKIPPED (not a pass): precondition not met at {}:{}",
            file!(),
            line!()
        );
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn nothing_in_the_wasm_bundle_calls_std_apis_that_panic_there() {
    let crates_dir = workspace_crates_dir();
    let mut violations: Vec<String> = Vec::new();

    for krate in WASM_CRATES {
        let src = crates_dir.join(krate).join("src");
        if !src.exists() {
            continue;
        }
        let mut files = Vec::new();
        rust_files(&src, &mut files);

        for file in files {
            let rel = file
                .strip_prefix(&crates_dir)
                .unwrap_or(&file)
                .to_string_lossy()
                .replace('\\', "/");

            if ALLOWED.iter().any(|(p, _)| rel == *p) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            // The clock module is the sanctioned home for the native call.
            if rel.ends_with("pdf-manip/src/clock.rs") {
                continue;
            }

            // Test modules never reach the cdylib. Convention in this
            // workspace puts them last, at column 0, so everything from the
            // first top-level `#[cfg(test)]` onwards is out of scope. Being
            // precise here matters: a guard that cries wolf gets ignored,
            // which is worse than no guard at all.
            let scan_until = text
                .lines()
                .position(|l| l.starts_with("#[cfg(test)]"))
                .unwrap_or(usize::MAX);

            for (lineno, line) in text.lines().enumerate().take(scan_until) {
                let trimmed = line.trim_start();
                // Comments and doc comments describe the rule; they are not it.
                if trimmed.starts_with("//") {
                    continue;
                }
                for needle in FORBIDDEN {
                    if line.contains(needle) {
                        violations.push(format!("{rel}:{} — {needle}", lineno + 1));
                    }
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "these calls compile on wasm32 and panic when reached, which surfaces \
         to JavaScript as a bare `RuntimeError: unreachable`:\n  {}\n\n\
         Use pdf_manip::clock::unix_now_secs() for wall-clock time, or gate the \
         code on #[cfg(not(target_arch = \"wasm32\"))] and add it to ALLOWED in \
         {} with the reason.",
        violations.join("\n  "),
        file!(),
    );
}
