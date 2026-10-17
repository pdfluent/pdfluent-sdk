//! A document as markdown, for the structured benchmark (#568 to #572).
//!
//! An example rather than a subcommand, for the reason `paragraph_blocks` next
//! door gives: the benchmark needs our structure in the format every other
//! column publishes in, and what the command line should offer is a separate
//! question (#573).
//!
//! Run: `structured_markdown <pdf>`. The whole document on stdout, headings as
//! `#`..`###`, list items as `-` and `<n>.` indented by their level, tables as
//! a pipe table or as HTML, every figure as an `<!-- image -->` comment of
//! its own paragraph, a page change as an HTML comment, and paragraphs
//! separated by a blank line. The other serialisation of the same blocks is
//! `pdf_engine::to_json`; see `docs/STRUCTURED_EXTRACTION.md`.
//!
//! `structured_markdown --json <pdf>` writes that second serialisation
//! instead, which is how the rig asks what each block was SET IN -- the size,
//! the weight and the face beside the text (#578). It is a second call and
//! never the measured one: the round times the markdown, because that is what
//! the benchmark grades, and a flag that changed what is timed would change
//! what is compared.
//!
//! `structured_markdown --batch <list> --out-dir <dir> [--jobs N]` reads every
//! PDF the list names in ONE process, the way the competitors' batches are run,
//! and writes `<dir>/<file name>.md` for each, `<dir>/timings.tsv` (name, ms,
//! outcome -- the adapters' format) and `<dir>/batch.json` (pages, wall ms,
//! jobs). With `--jobs N` the pages are shared out over N threads; the per-page
//! ms is then each page's own wall time, and throughput is `wall_ms / pages`.
//! Why it exists: on 26-09-2026 our clock was one process per page with its
//! start-up, against competitors batched, and the table had to say the two
//! were not the same measurement (#623).
//!
//! `structured_markdown --region x0,y0,x1,y1 [--page N] <pdf>` prints the
//! engine's table inside one region through the same markdown writer -- HTML
//! as the writer emits it -- and nothing else. The box is in PDF user space,
//! y up; the page counts from zero. A teacher adapter calls this or the
//! `table_in_region` API it exercises.
//!
//! `structured_markdown --header-switches <spec> <pdf>` renders the markdown
//! with the header switches set absolutely for one measurement arm: the four
//! header switches start off and the comma-separated spec names the ones on
//! (`rowspan-crossing`, `word-table`, `mixed-body`, `headerless-html`,
//! `stub-years`, or `all` for all five and `none` for none). The blocks are the production
//! ones; only the markdown writer reads the arm, so one build measures every
//! combination (#636). It combines with nothing else.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_engine::{to_json, ExtractOptions, PdfDocument};

/// Every PDF of the list in this one process, over `jobs` threads.
fn batch(list: &str, out_dir: &str, jobs: usize) -> i32 {
    use std::time::Instant;

    let listed = match std::fs::read_to_string(list) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("read {list}: {err}");
            return 2;
        }
    };
    let paths: Vec<String> = listed
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect();
    let out = std::path::Path::new(out_dir);
    if let Err(err) = std::fs::create_dir_all(out) {
        eprintln!("create {out_dir}: {err}");
        return 2;
    }
    let started = Instant::now();
    let items = pdf_engine::structured_batch::batch_markdown(paths.len(), jobs, |i| {
        std::fs::read(&paths[i]).map_err(|err| format!("read {}: {err}", paths[i]))
    });
    let wall = started.elapsed().as_millis();
    let mut tsv = String::new();
    let mut failed = false;
    for (path, item) in paths.iter().zip(&items) {
        let name = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let outcome = match &item.markdown {
            Ok(markdown) => match std::fs::write(out.join(format!("{name}.md")), markdown) {
                Ok(()) => "ok",
                Err(err) => {
                    eprintln!("write {name}: {err}");
                    "crash"
                }
            },
            Err(err) => {
                eprintln!("{path}: {err}");
                "crash"
            }
        };
        failed |= outcome != "ok";
        tsv.push_str(&format!("{name}\t{}\t{outcome}\n", item.ms));
    }
    let summary = format!(
        "{{\"pages\": {}, \"wall_ms\": {wall}, \"jobs\": {}}}\n",
        paths.len(),
        jobs.max(1)
    );
    if std::fs::write(out.join("timings.tsv"), tsv).is_err()
        || std::fs::write(out.join("batch.json"), summary).is_err()
    {
        eprintln!("write the batch's timings to {out_dir}");
        return 2;
    }
    i32::from(failed)
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if raw.first().map(String::as_str) == Some("--batch") {
        let mut list = None;
        let mut out_dir = None;
        let mut jobs = 1usize;
        let mut it = raw.iter();
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--batch" => list = it.next().cloned(),
                "--out-dir" => out_dir = it.next().cloned(),
                "--jobs" => jobs = it.next().and_then(|n| n.parse().ok()).unwrap_or(0),
                _ => {}
            }
        }
        let (Some(list), Some(out_dir), true) = (list, out_dir, jobs >= 1) else {
            eprintln!("usage: structured_markdown --batch <list> --out-dir <dir> [--jobs N]");
            std::process::exit(2);
        };
        std::process::exit(batch(&list, &out_dir, jobs));
    }
    let mut args = raw.into_iter().peekable();
    let mut json = false;
    let mut region: Option<String> = None;
    let mut page = 0usize;
    let mut header_switches: Option<String> = None;
    let mut path = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--header-switches" => {
                let Some(value) = args.next() else {
                    eprintln!("usage: structured_markdown --header-switches <spec> <pdf>");
                    std::process::exit(2);
                };
                header_switches = Some(value);
            }
            "--region" => {
                let Some(value) = args.next() else {
                    eprintln!("usage: structured_markdown --region x0,y0,x1,y1 [--page N] <pdf>");
                    std::process::exit(2);
                };
                region = Some(value);
            }
            "--page" => {
                let Some(value) = args.next().and_then(|n| n.parse().ok()) else {
                    eprintln!("usage: structured_markdown --region x0,y0,x1,y1 [--page N] <pdf>");
                    std::process::exit(2);
                };
                page = value;
            }
            other if other.starts_with("--region=") => {
                region = Some(other["--region=".len()..].to_string());
            }
            other if other.starts_with("--page=") => {
                let Ok(value) = other["--page=".len()..].parse() else {
                    eprintln!("usage: structured_markdown --region x0,y0,x1,y1 [--page N] <pdf>");
                    std::process::exit(2);
                };
                page = value;
            }
            other => {
                path = Some(other.to_string());
                break;
            }
        }
    }
    if region.is_some() && path.is_none() {
        path = args.next();
    }
    let Some(path) = path else {
        if region.is_some() {
            eprintln!("usage: structured_markdown --region x0,y0,x1,y1 [--page N] <pdf>");
        } else {
            eprintln!("usage: structured_markdown [--json] <pdf>");
        }
        std::process::exit(2);
    };
    if region.is_some() && json {
        eprintln!("usage: structured_markdown --region x0,y0,x1,y1 [--page N] <pdf>");
        std::process::exit(2);
    }
    if header_switches.is_some() && (json || region.is_some()) {
        eprintln!("usage: structured_markdown --header-switches <spec> <pdf>");
        std::process::exit(2);
    }
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("read {path}: {err}");
            std::process::exit(2);
        }
    };
    if let Some(spec) = region {
        return region_of(&path, bytes, &spec, page);
    }
    if let Some(spec) = header_switches {
        return markdown_with_header_switches(&path, bytes, &spec);
    }
    if let Ok(file) = std::env::var("PDFLUENT_TABLE_SIGNALS") {
        write_table_signals(&path, bytes.clone(), &file);
    }
    if json {
        return json_of(&path, bytes);
    }
    // No fallback to plain text when the read fails. A document that could not
    // be read has to LOOK unread in the run file: a page of text under a column
    // that claims structure is scored as structure we did not produce, which is
    // a worse answer than the page the harness counts as unanswered.
    match pdf_engine::structured_batch::markdown_of_bytes(bytes) {
        Ok(markdown) => print!("{markdown}"),
        Err(err) => {
            eprintln!("{path}: {err}");
            std::process::exit(1);
        }
    }
}

/// One region's table through the markdown writer, and nothing else: HTML as
/// the writer emits it. `spec` is `x0,y0,x1,y1` in PDF user space, y up.
fn region_of(path: &str, bytes: Vec<u8>, spec: &str, page: usize) {
    let numbers: Option<Vec<f64>> = spec
        .split(',')
        .map(|part| part.trim().parse().ok())
        .collect();
    let Some(numbers) = numbers.filter(|n| n.len() == 4) else {
        eprintln!("usage: structured_markdown --region x0,y0,x1,y1 [--page N] <pdf>");
        std::process::exit(2);
    };
    let bbox = [numbers[0], numbers[1], numbers[2], numbers[3]];
    let doc = match PdfDocument::open(bytes) {
        Ok(doc) => doc,
        Err(err) => {
            eprintln!("open {path}: {err}");
            std::process::exit(1);
        }
    };
    match doc.table_in_region(page, bbox) {
        Ok(blocks) => print!("{}", pdf_engine::to_markdown(&blocks)),
        Err(err) => {
            eprintln!("region {path}: {err}");
            std::process::exit(1);
        }
    }
}

/// The whole document with the header switches set absolutely for one arm:
/// the four start off and the spec names the ones on. Unknown names refuse,
/// so a mistyped arm never measures the wrong combination.
fn markdown_with_header_switches(path: &str, bytes: Vec<u8>, spec: &str) {
    use pdf_engine::learned::{Switches, SWITCHES};
    let mut switches = Switches {
        markdown_header_block_rowspan_crossing: false,
        markdown_header_block_word_table: false,
        markdown_header_block_mixed_body: false,
        markdown_headerless_html: false,
        markdown_header_block_stub_years: false,
        ..SWITCHES
    };
    if spec != "none" {
        for name in spec.split(',').map(str::trim) {
            match name {
                "all" => {
                    switches.markdown_header_block_rowspan_crossing = true;
                    switches.markdown_header_block_word_table = true;
                    switches.markdown_header_block_mixed_body = true;
                    switches.markdown_headerless_html = true;
                    switches.markdown_header_block_stub_years = true;
                }
                "rowspan-crossing" => switches.markdown_header_block_rowspan_crossing = true,
                "word-table" => switches.markdown_header_block_word_table = true,
                "mixed-body" => switches.markdown_header_block_mixed_body = true,
                "headerless-html" => switches.markdown_headerless_html = true,
                "stub-years" => switches.markdown_header_block_stub_years = true,
                _ => {
                    eprintln!("unknown header switch: {name}");
                    std::process::exit(2);
                }
            }
        }
    }
    let doc = match PdfDocument::open(bytes) {
        Ok(doc) => doc,
        Err(err) => {
            eprintln!("open {path}: {err}");
            std::process::exit(1);
        }
    };
    match doc.extract_blocks(&ExtractOptions::default()) {
        Ok(blocks) => print!(
            "{}",
            pdf_engine::to_markdown_with_switches(&blocks, switches)
        ),
        Err(err) => {
            eprintln!("blocks {path}: {err}");
            std::process::exit(1);
        }
    }
}

/// The same blocks as JSON: what each block was SET IN for the rig's second
/// questions, never the measured serialisation.
fn json_of(path: &str, bytes: Vec<u8>) {
    let doc = match PdfDocument::open(bytes) {
        Ok(doc) => doc,
        Err(err) => {
            eprintln!("open {path}: {err}");
            std::process::exit(1);
        }
    };
    match doc.extract_blocks_and_rules(&ExtractOptions::default()) {
        Ok((blocks, _)) => print!("{}", to_json(&blocks)),
        Err(err) => {
            eprintln!("blocks {path}: {err}");
            std::process::exit(1);
        }
    }
}

/// Table signals for calibration, one JSON line per page appended to `file`.
/// `None` writes as an absent field, never as 0.
fn write_table_signals(path: &str, bytes: Vec<u8>, file: &str) {
    let doc = match PdfDocument::open(bytes) {
        Ok(doc) => doc,
        Err(err) => {
            eprintln!("open {path}: {err}");
            std::process::exit(1);
        }
    };
    let signals = match doc.table_signals() {
        Ok(signals) => signals,
        Err(err) => {
            eprintln!("signals {path}: {err}");
            std::process::exit(1);
        }
    };
    let out = pdf_engine::learned::tables::table_signals_jsonl(&signals);
    use std::io::Write;
    let mut handle = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)
    {
        Ok(handle) => handle,
        Err(err) => {
            eprintln!("open {file}: {err}");
            std::process::exit(1);
        }
    };
    if handle.write_all(out.as_bytes()).is_err() {
        eprintln!("write {file}");
        std::process::exit(1);
    }
}
