// Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
//
// This software is proprietary. The PDFluent application is free to use,
// including for commercial purposes. Redistribution, or extraction or reuse
// of its components (including the embedded PDF engine), requires a licence.
// See https://pdfluent.com/license for terms.

//! Would the PDF/A fix passes that nothing calls actually change anything?
//!
//! Eight public passes in `pdfa_fonts.rs` -- 883 lines -- are called from no
//! production code, no test, no example. Each is either superseded by a
//! successor or was never wired up, and the two want opposite treatment:
//! delete, or connect. Guessing which is which from the names is how dead code
//! survives audits.
//!
//! This runs each pass over a corpus and reports how often it claims a fix. A
//! pass that reports nothing across thousands of documents is superseded. One
//! that reports steadily is repairing something that currently ships unrepaired.
//!
//! CAVEAT, and it matters: this runs each pass in isolation on an untouched
//! document. `ensure_truetype_encoding` documents that it must run after
//! `embed_fonts` and `fix_font_width_mismatches`, so a zero here is weaker
//! evidence for that one than for the others -- it may simply have nothing to
//! act on yet. A non-zero is conclusive either way.
//!
//! WHAT THIS MEASURED, 22-08-2026 — AND WHY THE ANSWER IS "LEAVE THEM ALONE"
//!
//! On 797 untouched documents three passes looked like missing repair capability:
//!
//!     fix_embedded_font_metrics      342 documents (43%)   2193 fixes
//!     fix_simple_truetype_widths     191 documents (24%)   1084 fixes
//!     fix_type1_widths               167 documents (21%)   1180 fixes
//!
//! On documents that had already been through the full PDF/A conversion it looked
//! stronger still: fix_type1_widths reported work on 200 of 200, and it converges
//! (a second run reports zero), so it is not counting non-changes -- it really
//! does rewrite something the pipeline leaves behind.
//!
//! Then veraPDF was asked. 40 documents per pass, flavour 2b, with and without:
//!
//!     fix_embedded_font_metrics              improved 0   WORSE 6   same 34
//!     fix_type1_widths                       improved 0   WORSE 5   same 35
//!     fix_simple_truetype_widths             improved 0   WORSE 1   same 39
//!     fix_truetype_macroman_unicode_aliases  improved 0   worse 0   same 40
//!
//! Not one verdict improved. Three of the four turn conforming documents into
//! non-conforming ones. These passes are not disconnected by neglect.
//!
//! The lesson is worth more than the verdict: a fix pass's own counter is not a
//! measure of its value. "200 of 200 documents, 1420 repairs" reads like an
//! obvious gap and was damage. Only measuring the outcome could tell the two
//! apart.
//!
//! Caveat kept honest: this applies each pass *after* conversion, not at the
//! position in the pipeline it may once have been written for. A pass can be
//! sound there and harmful at the end. What is established is that wiring them
//! in naively hurts.
//!
//! Usage: unwired_pass_probe <corpus-dir> [max-documents]

use lopdf::Document;
use std::collections::BTreeMap;
use std::path::PathBuf;

type Pass = (&'static str, fn(&mut Document) -> usize);

fn passes() -> Vec<Pass> {
    use pdf_manip::pdfa_fonts as f;
    vec![
        (
            "ensure_truetype_encoding",
            f::ensure_truetype_encoding as fn(&mut Document) -> usize,
        ),
        ("fix_embedded_font_metrics", f::fix_embedded_font_metrics),
        ("fix_simple_truetype_widths", f::fix_simple_truetype_widths),
        (
            "fix_truetype_macroman_unicode_aliases",
            f::fix_truetype_macroman_unicode_aliases,
        ),
        (
            "fix_type0_cmap_cidsysteminfo",
            f::fix_type0_cmap_cidsysteminfo,
        ),
        ("fix_type1_widths", f::fix_type1_widths),
    ]
}

/// `--apply <pass> <in.pdf> <out.pdf>`: run one pass and write the result.
///
/// Used for the A/B that actually decides: convert a document with the normal
/// pipeline, apply one unwired pass to that output, and let veraPDF judge both.
/// That answers "does this repair anything the pipeline leaves behind", which
/// the bare counter cannot.
fn apply_one(naam: &str, inp: &str, outp: &str) -> ! {
    let Some((_, pass)) = passes().into_iter().find(|(n, _)| *n == naam) else {
        eprintln!("unknown pass: {naam}");
        std::process::exit(64);
    };
    let mut doc = match Document::load(inp) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("cannot load {inp}: {e}");
            std::process::exit(2);
        }
    };
    let n = pass(&mut doc);
    if let Err(e) = doc.save(outp) {
        eprintln!("cannot write {outp}: {e}");
        std::process::exit(3);
    }
    println!("{n}");
    std::process::exit(0);
}

fn main() {
    let mut args = std::env::args().skip(1);
    {
        let all: Vec<String> = std::env::args().skip(1).collect();
        if all.first().map(String::as_str) == Some("--apply") {
            if all.len() < 4 {
                eprintln!("usage: --apply <pass> <in.pdf> <out.pdf>");
                std::process::exit(64);
            }
            apply_one(&all[1], &all[2], &all[3]);
        }
    }
    let dir: PathBuf = match args.next() {
        Some(d) => d.into(),
        None => {
            eprintln!("usage: unwired_pass_probe <corpus-dir> [max-documents]");
            std::process::exit(64);
        }
    };
    let limit: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(1000);

    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| {
            eprintln!("SKIPPED (not a pass): cannot read {}: {e}", dir.display());
            std::process::exit(1);
        })
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "pdf"))
        .collect();
    files.sort();
    files.truncate(limit);

    // documents where the pass claimed at least one fix, and the total claimed
    let mut docs: BTreeMap<&str, usize> = BTreeMap::new();
    let mut total: BTreeMap<&str, usize> = BTreeMap::new();
    let mut snapshot_restored = 0usize;
    let mut read = 0usize;

    for path in &files {
        let Ok(base) = Document::load(path) else {
            continue;
        };
        read += 1;
        for (name, pass) in passes() {
            let mut doc = base.clone();
            let n = pass(&mut doc);
            if n > 0 {
                *docs.entry(name).or_default() += 1;
                *total.entry(name).or_default() += n;
            }
        }
        // The snapshot/restore pair only does anything if something removed an
        // encoding in between, so on an untouched document it must report zero.
        // Running it anyway is the point: a non-zero would mean it fires when it
        // should not.
        let mut doc = base.clone();
        let snap = pdf_manip::pdfa_fonts::snapshot_font_encodings(&doc);
        snapshot_restored += pdf_manip::pdfa_fonts::restore_stripped_encodings(&mut doc, &snap);
    }

    println!("documents read: {read} of {} found", files.len());
    println!();
    println!("{:<40} {:>10} {:>10}", "pass", "documents", "fixes");
    for (name, _) in passes() {
        println!(
            "{:<40} {:>10} {:>10}",
            name,
            docs.get(name).copied().unwrap_or(0),
            total.get(name).copied().unwrap_or(0)
        );
    }
    println!(
        "{:<40} {:>10} {:>10}",
        "snapshot+restore (must be 0)", "-", snapshot_restored
    );
}
