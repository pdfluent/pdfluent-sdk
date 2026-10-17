// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Which engine segmented this dump, and which decisions wrote its model
//! outputs: THE TREE and THE DECISIONS at build time, stamped into every
//! record (#574).
//!
//! THE TREE is sha256 over the sorted (path, blob-sha) pairs of
//! `git ls-tree -r HEAD crates/pdf-engine/src`, filtered to what a dump's
//! values depend on: everything outside `src/learned/` (the segmentation),
//! plus [`LEARNED_FEATURE_FILES`]. The dump runs with the learned pass OFF,
//! but it still writes values computed in learned/ -- block features, line
//! vectors, table pairs, style, drawings -- so those files are in and the
//! decision code (`mod.rs`, the models, the judges) is out: a decision-only
//! landing changes no dumped value and reuses old dumps.
//!
//! THE DECISIONS is the same sha256 layout over everything under
//! `src/learned/`, models included: what the dumped MODEL outputs depend on.
//! A model trains on blocks, but a stacked model trains on another model's
//! dumped verdicts (`list_signal`, `line_class_probs`, `p_heading`), so a
//! landing that touches only models or decisions changes those values without
//! changing the tree -- a silent skew for the stacked trainers, which refuse
//! a record whose decisions are not the serving ones.
//!
//! `training_rules.py` holds the same list and hashes the same bytes: one
//! pair per entry, `path\0blob\0`, paths in byte order. That byte layout is
//! the contract; `test_engine_tree_scope.py` holds the two lists equal, and a
//! test in `pdf-learned` holds the two hashes equal for this tree.
//!
//! A tree with local changes stamps "<tree>-dirty", which never equals a
//! serving tree: a dump from a dirty tree trains on blocks no commit has. The
//! decisions stamp carries "-dirty" the same way when `src/learned/` has
//! local changes. The commit is stamped beside them for the human reader,
//! dirty the same way.
//!
//! All fall back to "unknown" when git is missing, which is what an exported
//! tree builds with. The trainers refuse "unknown" like any other skew unless
//! `--engine-skew-is-fine` (or `--decisions-skew-is-fine` for the stacked
//! ones) says why.
//!
//! THE DECISIONS WITHOUT is stamped beside them: for each model in
//! [`SELF_EXCLUDABLE`], the same sha256 layout over `src/learned/` minus that
//! model's directory `models/<dir>/`. A retrain of `table_is` changes THE
//! DECISIONS, which would refuse the dump the retrain itself needs -- for a
//! change that cannot have touched a dumped value, because the model's own
//! output reaches no refine-off record its trainer reads. The `-dirty` ride
//! the same way, over the same reduced scope: a byte under the excluded
//! directory moves neither the hash nor the dirt. `training_rules.py` holds
//! the same map and hashes the same bytes; `test_self_exclusion.py` holds the
//! two maps equal, and a test in `pdf-learned` holds the two hashes equal for
//! this tree.

use std::process::Command;

/// The learned/ files a dump's values depend on: THE TREE keeps these and
/// nothing else under `src/learned/`. The same list as
/// `LEARNED_FEATURE_FILES` in `training_rules.py`; `test_engine_tree_scope.py`
/// refuses drift between the two.
const LEARNED_FEATURE_FILES: [&str; 9] = [
    "alignment.rs",
    // Not on this tree yet; listed so its landing changes the hash.
    "drawings.rs",
    "features.rs",
    "lines.rs",
    "pairs.rs",
    // The image arm (/12): its values also depend on the renderer, which
    // is outside this tree -- see `training_rules.py`.
    "raster.rs",
    "splits.rs",
    // Not on this tree yet either.
    "style.rs",
    "vector_proposals.rs",
];

/// Whether repo-relative `path`, as ls-tree prints it, is in the tree.
fn tree_keeps(path: &str) -> bool {
    const PREFIX: &str = "crates/pdf-engine/src/";
    const LEARNED: &str = "crates/pdf-engine/src/learned/";
    if !path.starts_with(PREFIX) {
        return false;
    }
    if let Some(rest) = path.strip_prefix(LEARNED) {
        return !rest.contains('/') && LEARNED_FEATURE_FILES.contains(&rest);
    }
    true
}

/// Whether repo-relative `path`, as ls-tree prints it, is in the decisions.
fn decisions_keeps(path: &str) -> bool {
    path.starts_with("crates/pdf-engine/src/learned/")
}

/// The models a trainer may exclude from the decisions it is compared
/// against: (model name, directory it ships in under `src/learned/models/`).
/// The same map as `SELF_EXCLUDABLE` in `training_rules.py`;
/// `test_self_exclusion.py` refuses drift between the two.
const SELF_EXCLUDABLE: [(&str, &str); 1] = [("table_is", "table_is")];

/// THE DECISIONS minus one model's directory, for `git ls-tree` (path,
/// blob-sha) pairs: the same layout as [`decisions_hash`] over `src/learned/`
/// minus `models/<dir>/`.
fn decisions_without_hash(pairs: &[(String, String)], dir: &str) -> String {
    let prefix = format!("crates/pdf-engine/src/learned/models/{dir}/");
    let mut kept: Vec<(&str, &str)> = pairs
        .iter()
        .filter(|(path, _)| decisions_keeps(path) && !path.starts_with(&prefix))
        .map(|(path, blob)| (path.as_str(), blob.as_str()))
        .collect();
    kept.sort();
    hash_sorted(&kept)
}

/// sha256 over `path\0blob\0` per included pair, paths in byte order: THE
/// TREE with [`tree_keeps`], THE DECISIONS with [`decisions_keeps`]. The same
/// bytes `training_rules.tree_hash` and `training_rules.decisions_hash` hash.
fn hash_over(pairs: &[(String, String)], keeps: fn(&str) -> bool) -> String {
    let mut kept: Vec<(&str, &str)> = pairs
        .iter()
        .filter(|(path, _)| keeps(path))
        .map(|(path, blob)| (path.as_str(), blob.as_str()))
        .collect();
    kept.sort();
    hash_sorted(&kept)
}

/// sha256 over `path\0blob\0` per pair of a sorted slice, as lowercase hex.
fn hash_sorted(kept: &[(&str, &str)]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for (path, blob) in kept {
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update(blob.as_bytes());
        hasher.update([0]);
    }
    let mut hex = String::with_capacity(64);
    for byte in hasher.finalize() {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// THE TREE for `git ls-tree` (path, blob-sha) pairs.
fn tree_hash(pairs: &[(String, String)]) -> String {
    hash_over(pairs, tree_keeps)
}

/// THE DECISIONS for `git ls-tree` (path, blob-sha) pairs.
fn decisions_hash(pairs: &[(String, String)]) -> String {
    hash_over(pairs, decisions_keeps)
}

/// One git command, or `None` when git is missing, this is not a checkout, or
/// the command fails. Every caller treats `None` as "unknown".
fn git(dir: &str, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn main() {
    // Re-stamp when the segmentation code moves. The path is beside this
    // crate rather than inside it, which is the whole point: what the stamp
    // names is not what this crate holds.
    println!("cargo:rerun-if-changed=../pdf-engine/src");
    let (tree, decisions, without, commit) = engine_of();
    println!("cargo:rustc-env=PDFLUENT_ENGINE_TREE={tree}");
    println!("cargo:rustc-env=PDFLUENT_ENGINE_DECISIONS={decisions}");
    println!("cargo:rustc-env=PDFLUENT_ENGINE_DECISIONS_WITHOUT={without}");
    println!("cargo:rustc-env=PDFLUENT_ENGINE_COMMIT={commit}");
}

/// THE DECISIONS WITHOUT as the stamp carries it: `{"name": "hash"}` in
/// [`SELF_EXCLUDABLE`] order. Names are `[a-z0-9_]` and hashes `[0-9a-f-]`
/// (plus "-dirty"), so no escaping: the braces and quotes below are the whole
/// encoding.
fn without_json(without: &[(&str, String)]) -> String {
    let mut out = String::from("{");
    for (i, (name, hash)) in without.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('"');
        out.push_str(name);
        out.push_str("\":\"");
        out.push_str(hash);
        out.push('"');
    }
    out.push('}');
    out
}

fn engine_of() -> (String, String, String, String) {
    let unknown = || {
        (
            "unknown".to_string(),
            "unknown".to_string(),
            without_json(
                &SELF_EXCLUDABLE
                    .iter()
                    .map(|(name, _)| (*name, "unknown".to_string()))
                    .collect::<Vec<_>>(),
            ),
            "unknown".to_string(),
        )
    };
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let root = match git(&manifest, &["rev-parse", "--show-toplevel"]) {
        Some(root) if !root.is_empty() => root,
        _ => return unknown(),
    };
    let listing = match git(&root, &["ls-tree", "-r", "HEAD", "crates/pdf-engine/src"]) {
        Some(listing) if !listing.is_empty() => listing,
        _ => return unknown(),
    };
    let mut pairs = Vec::new();
    for line in listing.lines() {
        // "100644 blob <sha>\t<path>"
        let Some((meta, path)) = line.split_once('\t') else {
            continue;
        };
        let mut fields = meta.split_whitespace();
        match (fields.next(), fields.next(), fields.next()) {
            (_, Some("blob"), Some(sha)) => {
                pairs.push((path.to_string(), sha.to_string()));
            }
            _ => continue,
        }
    }
    if pairs.is_empty() {
        return unknown();
    }
    let mut tree = tree_hash(&pairs);
    let mut decisions = decisions_hash(&pairs);
    let mut without: Vec<(&str, String)> = SELF_EXCLUDABLE
        .iter()
        .map(|(name, dir)| (*name, decisions_without_hash(&pairs, dir)))
        .collect();
    let mut commit = match git(&root, &["rev-parse", "HEAD"]) {
        Some(commit) if !commit.is_empty() => commit,
        _ => return unknown(),
    };
    // Dirty is about the ENGINE, not the tree: local changes elsewhere leave
    // the blocks alone, so they leave the stamp alone too. The TREE carries
    // it, not just the commit beside it: the trainers compare the tree, and a
    // dirty stamp must never equal a serving one. The DECISIONS carry it for
    // their own scope: local changes under learned/ change the dumped model
    // outputs, while changes outside it leave them alone. And each WITHOUT
    // carries it for its own reduced scope: local changes under the excluded
    // directory move neither its hash nor its dirt.
    if git(
        &root,
        &["status", "--porcelain", "--", "crates/pdf-engine/src"],
    )
    .is_some_and(|changes| !changes.is_empty())
    {
        tree.push_str("-dirty");
        commit.push_str("-dirty");
    }
    let learned_changes = git(
        &root,
        &[
            "status",
            "--porcelain",
            "--",
            "crates/pdf-engine/src/learned",
        ],
    )
    .unwrap_or_default();
    if !learned_changes.is_empty() {
        decisions.push_str("-dirty");
    }
    for ((_, dir), (_, hash)) in SELF_EXCLUDABLE.iter().zip(without.iter_mut()) {
        if without_is_dirty(&learned_changes, dir) {
            hash.push_str("-dirty");
        }
    }
    (tree, decisions, without_json(&without), commit)
}

/// Whether `git status --porcelain` output over `src/learned/` names a change
/// outside the excluded model's directory: the `-dirty` of one WITHOUT entry.
/// Porcelain lines are `XY <path>`, or `XY <orig> -> <new>` for a rename, so
/// both sides are asked.
fn without_is_dirty(changes: &str, dir: &str) -> bool {
    let prefix = format!("crates/pdf-engine/src/learned/models/{dir}/");
    changes.lines().any(|line| {
        let paths = line.get(3..).unwrap_or("");
        paths.split(" -> ").any(|p| {
            let p = p.trim().trim_matches('"');
            !p.is_empty() && !p.starts_with(&prefix)
        })
    })
}
