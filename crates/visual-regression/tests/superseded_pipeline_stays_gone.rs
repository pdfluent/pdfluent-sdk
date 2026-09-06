// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The AVRT shell pipeline this crate replaced does not come back (#327).
//!
//! `scripts/run-avrt.sh`, `scripts/avrt-report.sh` and `avrt-config.json` drove
//! visual regression before this crate existed. They compared engine renders
//! against Adobe gold masters that have never been present on any branch, so
//! the pipeline could not produce a verdict -- and `avrt.yml` sat behind
//! `if: false` for four months while reading as coverage (#290, #293).
//!
//! Removing the files is not the same as removing the pipeline: a branch cut
//! before this change carries their presence as content, and a restore
//! conflicts with nothing. Two visual regression pipelines under one name is
//! exactly how the dead one gets invoked and believed, so their absence is
//! asserted by the suite that superseded them rather than left to reviewers.

use std::path::{Path, PathBuf};
use visual_regression::root;

/// The three files, repository-relative, as `Removes-deliberately:` named them.
const SUPERSEDED: [&str; 3] = [
    "scripts/run-avrt.sh",
    "scripts/avrt-report.sh",
    "avrt-config.json",
];

/// Directories that invoked the pipeline. A restored script is only harmful
/// once something runs it, and these are the two places that would.
const CALLERS: [&str; 2] = ["scripts", ".github/workflows"];

/// Below this the walk is broken, not clean. Both directories hold well over a
/// hundred files; an absence found by looking in the wrong place is not an
/// absence.
const MIN_FILES_SCANNED: usize = 60;

fn repository() -> PathBuf {
    // `root()` is this crate's manifest directory, so the repository is two
    // levels up. Anchored below rather than trusted: a wrong root makes every
    // assertion here pass by finding nothing, which is the one way this test
    // could go green while the pipeline is back.
    root().join("..").join("..")
}

fn text_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        // Announced rather than swallowed. An unreadable directory here means
        // this walk scanned less of the tree than the caller believes, and a
        // test that asserts "the pipeline is gone" over a tree it could not
        // read is green for the wrong reason.
        eprintln!("SKIPPED (not a pass): {} could not be read", dir.display());
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            text_files(&path, out);
        } else if path.is_file() {
            out.push(path);
        }
    }
}

#[test]
fn superseded_pipeline_stays_gone() {
    let repo = repository();

    // Anchors first. Each of these is present on every revision that could run
    // this test, so a failure here means the root is wrong and the rest of the
    // test would have been vacuous.
    for anchor in [
        "Cargo.toml",
        "crates/visual-regression/Cargo.toml",
        ".github/workflows/visual-regression.yml",
    ] {
        assert!(
            repo.join(anchor).is_file(),
            "repository root resolved to {}, which has no {anchor}. The test \
             below would report the AVRT pipeline absent from a directory that \
             is not the repository.",
            repo.display()
        );
    }

    let back: Vec<&str> = SUPERSEDED
        .iter()
        .copied()
        .filter(|p| repo.join(p).exists())
        .collect();
    assert!(
        back.is_empty(),
        "the superseded AVRT pipeline is back in the tree: {back:?}. This crate \
         replaced it in #326 and #327 removed it; it compares against gold \
         masters that do not exist, so whatever runs it cannot fail. Delete the \
         file again, or -- if it is wanted after all -- retire this crate first \
         and say so, because the two cannot both be the visual regression suite."
    );

    // A file may return under a new name while the old invocation lives on in a
    // workflow or a script, which is the form the restore actually takes.
    let mut files = Vec::new();
    for caller in CALLERS {
        text_files(&repo.join(caller), &mut files);
    }
    assert!(
        files.len() >= MIN_FILES_SCANNED,
        "scanned {} file(s) under {CALLERS:?}, expected at least \
         {MIN_FILES_SCANNED}. The walk is broken -- this is not a clean result.",
        files.len()
    );

    let mut invocations = Vec::new();
    for file in &files {
        let Ok(text) = std::fs::read_to_string(file) else {
            continue; // A binary under scripts/ invokes nothing.
        };
        for name in SUPERSEDED {
            let basename = name.rsplit('/').next().expect("non-empty path");
            if text.contains(basename) {
                invocations.push(format!("{} names {basename}", file.display()));
            }
        }
    }
    assert!(
        invocations.is_empty(),
        "removed AVRT pipeline files are still named where they used to be \
         invoked:\n  {}\nRun the replacement instead: `cargo test -p \
         visual-regression --release`, which is what the local push gate and \
         .github/workflows/visual-regression.yml already run.",
        invocations.join("\n  ")
    );
}
