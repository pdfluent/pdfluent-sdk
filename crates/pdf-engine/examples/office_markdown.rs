//! Office files as markdown, for the Office benchmark (O3).
//!
//! An example rather than a subcommand, for the reason `structured_markdown`
//! next door gives: the benchmark needs our Office output in the format every
//! other column publishes in, and what the command line should offer is a
//! separate question.
//!
//! Run: `office_markdown <file>`. Dispatches on the extension -- `docx`,
//! `pptx`, `xlsx`, `html`/`htm`/`md`/`txt`, `epub` -- to the five extractors
//! and prints the engine's Markdown: the same writer the PDF path uses, so
//! Office files and PDFs serialize through one code path.
//!
//! `office_markdown --json <file>` prints the engine JSON
//! (`pdf_engine::to_json`) of the same blocks instead.
//!
//! `office_markdown --batch <list> --out-dir <dir>` reads every file the list
//! names, one path per line, in one process, and writes
//! `<dir>/<file name>.md` for each plus `<dir>/timings.tsv` (name, ms,
//! `ok`|`crash`), like `structured_markdown`'s batch. Sequentially: Office
//! files are small and the extractors are single-threaded, so there is no
//! `--jobs`.
//!
//! Exit 1 on a read error or any other failure, with the message on stderr;
//! exit 2 on a wrong invocation. `main` is only the exit: [`run_with_args`]
//! holds the parsing and the batch loop so the integration test drives the
//! real thing in-process.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::io::Write;
use std::time::Instant;

/// The program, minus the exit: `args` without the argv[0], output to `out`,
/// diagnostics to `err`. Returns the exit code.
pub fn run_with_args(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    if args.first().map(String::as_str) == Some("--batch") {
        let mut list = None;
        let mut out_dir = None;
        let mut it = args.iter();
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--batch" => list = it.next().cloned(),
                "--out-dir" => out_dir = it.next().cloned(),
                _ => {}
            }
        }
        let (Some(list), Some(out_dir)) = (list, out_dir) else {
            let _ = writeln!(err, "usage: office_markdown --batch <list> --out-dir <dir>");
            return 2;
        };
        return batch(&list, &out_dir, err);
    }
    let mut json = false;
    let mut path = None;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            other => {
                path = Some(other);
                break;
            }
        }
    }
    let Some(path) = path else {
        let _ = writeln!(err, "usage: office_markdown [--json] <file>");
        return 2;
    };
    single(path, json, out, err)
}

/// One file to `out`: the engine Markdown, or the engine JSON with `--json`.
fn single(path: &str, json: bool, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            let _ = writeln!(err, "read {path}: {error}");
            return 1;
        }
    };
    let ext = extension_of(path);
    if json {
        match blocks_of(&bytes, &ext) {
            Ok(blocks) => {
                let _ = write!(out, "{}", pdf_engine::to_json(&blocks));
                0
            }
            Err(error) => {
                let _ = writeln!(err, "{path}: {error}");
                1
            }
        }
    } else {
        match markdown_of(&bytes, &ext) {
            Ok(markdown) => {
                let _ = write!(out, "{markdown}");
                0
            }
            Err(error) => {
                let _ = writeln!(err, "{path}: {error}");
                1
            }
        }
    }
}

/// Every file of the list in this one process: `<name>.md` each, then the
/// timings. A file that cannot be read, extracted or written is a `crash`
/// line, not an abort: the run reports every input, like the sibling's.
fn batch(list: &str, out_dir: &str, err: &mut dyn Write) -> i32 {
    let listed = match std::fs::read_to_string(list) {
        Ok(text) => text,
        Err(error) => {
            let _ = writeln!(err, "read {list}: {error}");
            return 1;
        }
    };
    let paths: Vec<String> = listed
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(String::from)
        .collect();
    let out = std::path::Path::new(out_dir);
    if let Err(error) = std::fs::create_dir_all(out) {
        let _ = writeln!(err, "create {out_dir}: {error}");
        return 1;
    }
    let mut tsv = String::new();
    let mut failed = false;
    for path in &paths {
        let started = Instant::now();
        let name = std::path::Path::new(path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let outcome = match std::fs::read(path) {
            Err(error) => {
                let _ = writeln!(err, "read {path}: {error}");
                "crash"
            }
            Ok(bytes) => match markdown_of(&bytes, &extension_of(path)) {
                Err(error) => {
                    let _ = writeln!(err, "{path}: {error}");
                    "crash"
                }
                Ok(markdown) => match std::fs::write(out.join(format!("{name}.md")), markdown) {
                    Ok(()) => "ok",
                    Err(error) => {
                        let _ = writeln!(err, "write {name}: {error}");
                        "crash"
                    }
                },
            },
        };
        failed |= outcome != "ok";
        tsv.push_str(&format!(
            "{name}\t{}\t{outcome}\n",
            started.elapsed().as_millis()
        ));
    }
    if std::fs::write(out.join("timings.tsv"), tsv).is_err() {
        let _ = writeln!(err, "write the batch's timings to {out_dir}");
        return 1;
    }
    i32::from(failed)
}

/// The dispatch key of a path: its lowercase extension without the dot, or
/// the empty string where it has none.
pub fn extension_of(path: &str) -> String {
    std::path::Path::new(path)
        .extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// The engine blocks of one file's bytes, by its [`extension_of`]: each
/// Office format through its extractor, markup through its reader.
pub fn blocks_of(bytes: &[u8], ext: &str) -> Result<Vec<pdf_engine::Block>, String> {
    match ext {
        "docx" => docx_extract::extract_blocks(bytes).map_err(|error| error.to_string()),
        "pptx" => pptx_extract::extract_blocks(bytes).map_err(|error| error.to_string()),
        "xlsx" => xlsx_extract::extract_blocks(bytes).map_err(|error| error.to_string()),
        "epub" => epub_extract::extract_engine_blocks(bytes).map_err(|error| error.to_string()),
        "html" | "htm" => text_of(bytes).map(|text| markup_extract::html::extract_blocks(&text)),
        "md" => text_of(bytes).map(|text| markup_extract::markdown::extract_blocks(&text)),
        "txt" => text_of(bytes).map(|text| markup_extract::text::extract_blocks(&text)),
        "" => Err("no extension to dispatch on".to_string()),
        unknown => Err(format!("unsupported extension: .{unknown}")),
    }
}

/// The Markdown of one file's bytes: [`blocks_of`] through the engine's
/// writer, the same one the PDF path uses.
pub fn markdown_of(bytes: &[u8], ext: &str) -> Result<String, String> {
    Ok(pdf_engine::to_markdown(&blocks_of(bytes, ext)?))
}

/// Markup bytes as text, or why they are not.
fn text_of(bytes: &[u8]) -> Result<String, String> {
    String::from_utf8(bytes.to_vec()).map_err(|_| "not valid UTF-8".to_string())
}

/// Absent under `cfg(test)`, when the integration test includes this file as
/// a module to drive [`run_with_args`] in-process.
#[cfg(not(test))]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(run_with_args(
        &args,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    ));
}
