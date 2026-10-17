//! What ships in a published crate cannot point at a host we do not run or at
//! a file that only existed on one developer's machine.
//!
//! Three shapes reached the 1.1.0 candidate and are ruled out here:
//!
//! * `docs.pdfluent.dev` error links. That domain was never registered, so
//!   every engine and CLI error printed a link that resolved nowhere. Error
//!   links point at `https://pdfluent.com/errors/#<anchor>`.
//! * Hardcoded `/tmp/...` corpus paths in tests and examples (`gen-`, `cs-`,
//!   `c4k-`, `xrechnung`). They pass or fail depending on what happens to be
//!   in one machine's scratch directory. Fixture directories come from an
//!   environment variable, and a missing fixture fails the test and names
//!   the variable.
//! * A hardcoded `anaconda3` OpenSSL path. The oracle is `$OPENSSL_BIN` or
//!   `openssl` on `PATH`.
//!
//! The scan covers every file a published crate's tarball carries from
//! `src/`, `examples/`, `tests/` and `benches/`: crates marked
//! `publish = false` are skipped, and so are the directories a crate lists in
//! its `exclude`.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::path::{Path, PathBuf};

/// Substrings that must not occur in shipped source, with the reason shown
/// when one does.
const FORBIDDEN: &[(&str, &str)] = &[
    (
        "pdfluent.dev",
        "unregistered docs host; link to https://pdfluent.com/errors/#<anchor>",
    ),
    (
        "/tmp/gen-",
        "machine-local corpus path; read it from a fixture env var",
    ),
    (
        "/tmp/cs-",
        "machine-local corpus path; read it from a fixture env var",
    ),
    (
        "/tmp/c4k-",
        "machine-local corpus path; read it from a fixture env var",
    ),
    (
        "/tmp/xrechnung",
        "machine-local sample path; take it as an argument",
    ),
    (
        "anaconda3",
        "machine-local tool path; resolve via an env var, then PATH",
    ),
];

const SCANNED_DIRS: &[&str] = &["src", "examples", "tests", "benches"];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

/// `true` when the manifest says the crate never reaches a registry.
fn is_unpublished(manifest: &str) -> bool {
    manifest
        .lines()
        .any(|l| l.split_whitespace().collect::<String>() == "publish=false")
}

/// The top-level directories the manifest's `exclude` drops from the tarball.
fn excluded_dirs(manifest: &str) -> Vec<String> {
    let Some(line) = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("exclude"))
    else {
        return Vec::new();
    };
    line.split('"')
        .skip(1)
        .step_by(2)
        .map(|entry| entry.trim_end_matches('/').to_string())
        .collect()
}

/// Every file below `dir`. A crate without that directory contributes nothing,
/// which is not a skip: there is simply nothing there to ship.
fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[test]
fn shipped_source_has_no_retired_hosts_or_scratch_paths() {
    let root = workspace_root();
    let this_file = Path::new(file!())
        .file_name()
        .expect("own file name")
        .to_owned();
    let mut scanned_crates = 0usize;
    let mut scanned_files = 0usize;
    let mut hits = Vec::new();

    let mut crates: Vec<PathBuf> = std::fs::read_dir(root.join("crates"))
        .expect("crates/")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("Cargo.toml").is_file())
        .collect();
    crates.sort();

    for krate in &crates {
        let manifest = std::fs::read_to_string(krate.join("Cargo.toml")).expect("manifest");
        if is_unpublished(&manifest) {
            continue;
        }
        scanned_crates += 1;
        let excluded = excluded_dirs(&manifest);
        for dir in SCANNED_DIRS {
            if excluded.iter().any(|e| e == dir) {
                continue;
            }
            let mut files = Vec::new();
            collect_files(&krate.join(dir), &mut files);
            for file in files {
                if file.file_name() == Some(this_file.as_os_str()) {
                    continue;
                }
                // Binary fixtures are not source; read what decodes as text.
                let Ok(text) = std::fs::read_to_string(&file) else {
                    continue;
                };
                scanned_files += 1;
                for (lineno, line) in text.lines().enumerate() {
                    for (needle, why) in FORBIDDEN {
                        if line.contains(needle) {
                            let rel = file.strip_prefix(&root).unwrap_or(&file);
                            hits.push(format!(
                                "{}:{}: `{needle}`: {why}",
                                rel.display(),
                                lineno + 1
                            ));
                        }
                    }
                }
            }
        }
    }

    // A scan that read nothing would pass vacuously.
    assert!(
        scanned_crates >= 10 && scanned_files >= 100,
        "the scan read {scanned_crates} crates and {scanned_files} files; the workspace layout moved"
    );
    assert!(
        hits.is_empty(),
        "shipped source carries {} forbidden reference(s):\n{}",
        hits.len(),
        hits.join("\n")
    );
}

#[test]
fn manifest_parsing_reads_publish_and_exclude() {
    assert!(is_unpublished("[package]\npublish = false\n"));
    assert!(!is_unpublished("[package]\npublish = true\n"));
    assert_eq!(
        excluded_dirs("exclude = [\"tests/\", \"examples/\"]\n"),
        vec!["tests".to_string(), "examples".to_string()]
    );
    assert!(excluded_dirs("[package]\nname = \"x\"\n").is_empty());
}
