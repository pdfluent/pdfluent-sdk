//! Write one `pdfluent.features/18` record per page, as JSONL.
//!
//!     feature_dump <pdf> [--page-id <id>]
//!     feature_dump [--jobs N] [--out FILE] [--list FILE|-] [<pdf>...]
//!
//! A debug tool for the learned decisions of #598, not a public API. The
//! blocks are those `extract_blocks` returns with default options -- the same
//! blocks `structured_markdown --json` prints -- so a model trained on this
//! sees what the markdown path sees.
//!
//! BATCH MODE. One process per page made extraction the slow step: 69k
//! DocLayNet pages took about an hour, and the table lanes need more than a
//! million. So many PDFs go through one process, in parallel over `--jobs`
//! threads, and the output is written in INPUT order, so a batch is
//! byte-identical to the same PDFs dumped one call at a time
//! (`tests/batch_is_per_page.rs`). `--list` reads one PDF per line, optionally
//! followed by a tab and its page id; `-` is stdin. A PDF that cannot be read
//! is named on stderr and skipped, and the exit status is then 1.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_learned::{dump_pdf, dump_pdf_refined};
use rayon::prelude::*;
use std::io::{BufRead, Write};

fn usage() -> ! {
    eprintln!("usage: feature_dump <pdf> [--page-id <id>]");
    eprintln!("       feature_dump [--jobs N] [--out FILE] [--list FILE|-] [<pdf>...]");
    std::process::exit(2);
}

fn main() {
    let mut inputs: Vec<(String, Option<String>)> = Vec::new();
    let mut page_id = None;
    let mut jobs = 0usize;
    let mut out_path = None;
    // WHICH LEARNED SWITCHES RAN, for measuring what two of them are worth
    // together (#599, #602). Empty means none of them, which is the only
    // record training may read.
    let mut refine: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--page-id" => page_id = Some(args.next().unwrap_or_else(|| usage())),
            "--jobs" => {
                jobs = args
                    .next()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or_else(|| usage())
            }
            "--out" => out_path = Some(args.next().unwrap_or_else(|| usage())),
            "--refine" => refine = Some(args.next().unwrap_or_else(|| usage())),
            "--list" => {
                let from = args.next().unwrap_or_else(|| usage());
                let reader: Box<dyn BufRead> = if from == "-" {
                    Box::new(std::io::stdin().lock())
                } else {
                    match std::fs::File::open(&from) {
                        Ok(f) => Box::new(std::io::BufReader::new(f)),
                        Err(e) => {
                            eprintln!("read {from}: {e}");
                            std::process::exit(2);
                        }
                    }
                };
                // Every line or a refusal: `map_while(Result::ok)` stopped at
                // the first unreadable line and dumped the prefix as if it
                // were the batch (codex, #1961).
                for line in reader.lines() {
                    let line = match line {
                        Ok(l) => l,
                        Err(e) => {
                            eprintln!("read {from}: {e}");
                            std::process::exit(2);
                        }
                    };
                    if line.trim().is_empty() {
                        continue;
                    }
                    let mut parts = line.splitn(2, '\t');
                    let path = parts.next().unwrap_or_default().to_string();
                    let id = parts
                        .next()
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty());
                    inputs.push((path, id));
                }
            }
            other if other.starts_with("--") => usage(),
            other => inputs.push((other.to_string(), None)),
        }
    }
    if inputs.is_empty() {
        usage();
    }
    // THE LOCAL COPY OF A BENCHMARK IS NEVER DUMPED (#623, owner decision
    // 29-09-2026). The ParseBench tuning half may sit on this Mac under
    // `pdfluent/pb-tuning` to debug reading order against; a dump is what
    // training reads, and a page scored by the rig must never be trained on.
    if let Some((path, _)) = inputs
        .iter()
        .find(|(path, _)| path.contains("pdfluent/pb-tuning"))
    {
        eprintln!("{path}: a local benchmark copy (pdfluent/pb-tuning) is never dumped");
        std::process::exit(2);
    }
    if page_id.is_some() {
        if inputs.len() != 1 {
            eprintln!("--page-id names one PDF; in a batch, give ids in --list");
            std::process::exit(2);
        }
        inputs[0].1 = page_id;
    }
    if jobs > 0 {
        let _ = rayon::ThreadPoolBuilder::new()
            .num_threads(jobs)
            .build_global();
    }

    let results: Vec<Result<String, String>> = inputs
        .par_iter()
        .map(|(path, id)| {
            let bytes = std::fs::read(path).map_err(|e| format!("{path}: read: {e}"))?;
            match &refine {
                None => dump_pdf(bytes, id.as_deref()).map_err(|e| format!("{path}: {e}")),
                Some(names) => {
                    let on = |what: &str| names.split(',').any(|n| n.trim() == what);
                    let switches = pdf_engine::learned::Switches {
                        list_item: on("list_item"),
                        list_opens: on("list_opens"),
                        heading_lines: on("heading_lines"),
                        captions: on("captions"),
                        line_gnn: on("line_gnn"),
                        line_image: on("line_image"),
                        line_gnn_v2: on("line_gnn_v2"),
                        demote_per_cell: on("demote_per_cell"),
                        table_region: on("table_region"),
                        table_structure: on("table_structure"),
                        table_judge: on("table_judge"),
                        table_lattice: on("table_lattice"),
                        table_stitch_header: on("table_stitch_header"),
                        table_stitch_rows: on("table_stitch_rows"),
                        table_stitch_merge: on("table_stitch_merge"),
                        table_is_v2: on("table_is_v2"),
                        table_drop_empty_columns: on("table_drop_empty_columns"),
                        table_cell_text_by_line: on("table_cell_text_by_line"),
                        table_v2_release_rejoin: on("table_v2_release_rejoin"),
                        table_v2_release_column_order: on("table_v2_release_column_order"),
                        table_split_side_by_side: on("table_split_side_by_side"),
                        table_split_stacked: on("table_split_stacked"),
                        table_runs_on_figures: on("table_runs_on_figures"),
                        markdown_header_block_figures: on("markdown_header_block_figures"),
                        table_corridor_split: on("table_corridor_split"),
                        markdown_header_block_rowspan_crossing: on(
                            "markdown_header_block_rowspan_crossing",
                        ),
                        markdown_header_block_word_table: on("markdown_header_block_word_table"),
                        markdown_header_block_mixed_body: on("markdown_header_block_mixed_body"),
                        markdown_header_block_stub_years: on("markdown_header_block_stub_years"),
                        markdown_headerless_html: on("markdown_headerless_html"),
                    };
                    dump_pdf_refined(bytes, id.as_deref(), switches, names)
                        .map_err(|e| format!("{path}: {e}"))
                }
            }
        })
        .collect();

    let mut out: Box<dyn Write> = match &out_path {
        Some(p) => match std::fs::File::create(p) {
            Ok(f) => Box::new(std::io::BufWriter::new(f)),
            Err(e) => {
                eprintln!("write {p}: {e}");
                std::process::exit(2);
            }
        },
        None => Box::new(std::io::BufWriter::new(std::io::stdout().lock())),
    };
    let mut failed = 0;
    for r in results {
        match r {
            Ok(text) => {
                // Exit 2, not 1: 1 means "some PDFs unreadable, each named on
                // stderr", and a caller takes that chunk as whole (codex, #1961).
                if let Err(e) = out.write_all(text.as_bytes()) {
                    eprintln!("write: {e}");
                    std::process::exit(2);
                }
            }
            Err(e) => {
                eprintln!("{e}");
                failed += 1;
            }
        }
    }
    if let Err(e) = out.flush() {
        eprintln!("write: {e}");
        std::process::exit(2);
    }
    if failed > 0 {
        std::process::exit(1);
    }
}
