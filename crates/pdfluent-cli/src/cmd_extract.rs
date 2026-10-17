//! `extract`: structured extraction to Markdown, JSON, chunks, or text.
//!
//! Thin over [`pdfluent::PdfDocument::extract_markdown`],
//! [`extract_blocks`](pdfluent::PdfDocument::extract_blocks) +
//! [`to_json`](pdfluent::to_json),
//! [`extract_docling`](pdfluent::PdfDocument::extract_docling), and
//! [`extract_text_pages`](pdfluent::PdfDocument::extract_text_pages).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::path::{Path, PathBuf};

use clap::Args;
use serde_json::json;

use crate::input::{self, InputKind};
use crate::output::{self, Ctx};

/// Cap on inline `text` in `--json` stdout mode; past it a preview + warning.
const JSON_TEXT_INLINE_CAP: usize = 64 * 1024;

/// Rayon's own pool-size override. When set, it always wins over `--jobs`.
const RAYON_THREADS_ENV: &str = "RAYON_NUM_THREADS";

/// Debug probe: when `1`/`true`, `extract` reports its rayon pool decision
/// and the effective pool size on stderr (`pdfluent-pool: ...`). Stderr-only
/// and unset by default, so data output and `-q` silence are untouched.
const POOL_PROBE_ENV: &str = "PDFFLUENT_POOL_PROBE";

/// Extraction output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    /// Markdown with headings, lists, and tables.
    Md,
    /// Block-model JSON (`to_json` over `extract_blocks`).
    Json,
    /// Docling JSON chunks for RAG ingestion.
    Chunks,
    /// Plain text, one form feed per page.
    Text,
    /// Every word with its box, one JSON object per line.
    Words,
}

impl Format {
    fn extension(self) -> &'static str {
        match self {
            Format::Md => "md",
            Format::Json => "json",
            Format::Chunks => "chunks.json",
            Format::Text => "txt",
            Format::Words => "words.jsonl",
        }
    }
}

/// Extraction quality. `fast` is the shipping engine; `balanced` runs the
/// layout model (needs the `ort-layout` build and `models install layout`);
/// `best` needs models that have not shipped and fails loudly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Quality {
    /// The shipping engine.
    Fast,
    /// The layout model tier; fails naming the install until it is there.
    Balanced,
    /// Needs unshipped models; errors until they land.
    Best,
}

/// What `extract` does with figures: the `--images` flag, Markdown only.
///
/// Unset, references are markers naming the figure. `none` writes no
/// reference at all; `embed` inlines the figure's pixels as a base64 data
/// URL; `dir:<path>` writes one PNG per figure and references it. JSON
/// blocks always carry the figures themselves -- kind and box, never
/// pixels -- whatever this says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Images {
    /// No figure references in the Markdown.
    None,
    /// Figures embedded as `data:image/png;base64` URLs.
    Embed,
    /// Figures written as PNGs under the directory and referenced there.
    Dir(PathBuf),
}

/// Parse `--images none|embed|dir:<path>`.
fn parse_images(value: &str) -> Result<Images, String> {
    if value == "none" {
        return Ok(Images::None);
    }
    if value == "embed" {
        return Ok(Images::Embed);
    }
    if let Some(dir) = value.strip_prefix("dir:") {
        if dir.is_empty() {
            return Err("--images dir: needs a path, as in --images dir:img/".to_string());
        }
        return Ok(Images::Dir(PathBuf::from(dir)));
    }
    Err(format!(
        "unknown --images {value:?}; use none, embed, or dir:<path>"
    ))
}

#[derive(Debug, Args)]
pub struct ExtractArgs {
    /// Input PDFs, globs, directories, or `-` for stdin (alone).
    #[arg(required = true)]
    pub inputs: Vec<String>,

    /// Output format.
    #[arg(long, value_enum, default_value = "md")]
    pub format: Format,

    /// Extraction quality. `balanced` needs the `ort-layout` build and the
    /// layout model (`pdfluent models install layout`).
    #[arg(long, value_enum, default_value = "fast")]
    pub quality: Quality,

    /// Where the model tier looks for its weights. Default: the
    /// `PDFLUENT_MODEL_DIR` environment variable, else the per-user model
    /// directory. Read only with `--quality balanced`.
    #[arg(long)]
    pub model_dir: Option<PathBuf>,

    /// Where the TATR table model lives: `tatr-v1.1-all/tatr-v1.1-all.onnx`
    /// under this directory structures the balanced tier's kept table
    /// regions first (track B, P4). Needs the `ort-tables` build; without
    /// it extraction refuses. Unset, `balanced` runs as it always has.
    #[arg(long)]
    pub table_model_dir: Option<PathBuf>,

    /// Pages to extract, e.g. `1-3,5`. Default: the whole document.
    #[arg(long)]
    pub pages: Option<String>,

    /// What the Markdown carries for figures: `none` writes no reference,
    /// `embed` inlines each figure's pixels as a base64 data URL, and
    /// `dir:<path>` writes one PNG per figure and references it. Unset,
    /// references are markers naming the figure. With `dir:`, each input's
    /// files land in `<path>/<stem>/`, so batch inputs never share names.
    #[arg(long, value_parser = parse_images)]
    pub images: Option<Images>,

    /// Output file, directory, or `-`/stdout. Directories are created.
    /// Multiple inputs need a directory.
    #[arg(long, short = 'o')]
    pub out: Option<String>,

    /// Descend into subdirectories (symlinks are never followed).
    #[arg(long)]
    pub recursive: bool,

    /// Parallel inputs when writing files (stdout stays sequential).
    #[arg(long, default_value = "1")]
    pub jobs: usize,

    /// Keep going after a failed input. Default stops at the first error,
    /// keeping earlier outputs and cancelling the rest; with this every
    /// input is attempted, later successes are still written, and every
    /// error is reported (a re-run then needs --force for those).
    #[arg(long)]
    pub keep_going: bool,

    /// Overwrite existing output files.
    #[arg(long)]
    pub force: bool,

    /// Carry each block's word lines in the JSON output (`--format json`).
    /// Without the flag the JSON is exactly what it was: no word lines.
    #[arg(long)]
    pub words: bool,
}

/// One planned input: where its bytes come from and where its text goes.
/// Planning (stems, targets, collisions, guards) runs before the first byte
/// is read; workers then stream items one at a time.
struct WorkItem {
    display: String,
    source: WorkSource,
    stem: String,
    /// `None` writes to stdout.
    target: Option<PathBuf>,
}

enum WorkSource {
    Path(PathBuf),
    Stdin(Vec<u8>),
}

struct JobResult {
    display: String,
    page_count: usize,
    char_count: usize,
    output_path: Option<String>,
    /// Set in stdout mode only.
    text: Option<String>,
}

/// What one worker produces: the rendered text plus anything the commit
/// step must place. Workers never touch final paths: every write happens
/// in the main thread, in input order, so a failure stops (or is recorded)
/// deterministically no matter how the renders overlapped.
struct Rendered {
    display: String,
    page_count: usize,
    char_count: usize,
    text: String,
    /// `None` writes to stdout.
    target: Option<PathBuf>,
}

/// One failed input, with its name on it for the batch report.
struct BatchError {
    display: String,
    code: String,
    msg: String,
}

impl BatchError {
    fn new(display: &str, code: &str, msg: String) -> Self {
        BatchError {
            display: display.to_string(),
            code: code.to_string(),
            msg,
        }
    }
}

/// Threads to size the global rayon pool with, or `None` to leave rayon's
/// default alone. `--jobs 1` asks for sequential work, so it gets a
/// single-threaded pool: pages render one at a time with no idle workers
/// spinning. Anything above 1 keeps the default pool exactly as before, and
/// an explicit `RAYON_NUM_THREADS` always wins over the flag. The raw
/// variable value is injected so tests pin the mapping without touching
/// process state.
fn resolve_rayon_threads(jobs: usize, rayon_env: Option<&str>) -> Option<usize> {
    if rayon_env.is_some() {
        return None;
    }
    if jobs == 1 {
        Some(1)
    } else {
        None
    }
}

/// Whether the pool probe reports on stderr.
fn pool_probe_enabled() -> bool {
    std::env::var(POOL_PROBE_ENV)
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Size the global rayon pool from `--jobs` (see [`resolve_rayon_threads`]).
/// A pool that is already initialised — an in-process second run — keeps
/// going on the existing pool rather than failing the batch.
fn init_rayon_pool(jobs: usize) {
    let rayon_env = std::env::var(RAYON_THREADS_ENV).ok();
    let requested = resolve_rayon_threads(jobs, rayon_env.as_deref());
    if let Some(threads) = requested {
        let _ = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build_global();
    }
    if pool_probe_enabled() {
        let requested = match requested {
            Some(threads) => threads.to_string(),
            None => "default".to_string(),
        };
        eprintln!(
            "pdfluent-pool: requested={requested} effective={}",
            rayon::current_num_threads()
        );
    }
}

pub(crate) fn run(ctx: &Ctx, args: &ExtractArgs) -> u8 {
    if matches!(args.quality, Quality::Best) {
        return output::error(
            "extract",
            ctx,
            "MODEL_NOT_SHIPPED",
            "quality `best` needs models that have not shipped; use `fast`, \
             or `balanced` with the layout model (`pdfluent models install layout`)",
        );
    }
    // Asked for TATR on a build without it: the engine's own
    // `TatrNeedsOrtTables` refusal, up front, before anything is planned or
    // read. The tier would refuse the same way where it is reached, but the
    // balanced path reads the layout model first and never gets there.
    #[cfg(not(feature = "ort-tables"))]
    if args.table_model_dir.is_some() {
        return output::error(
            "extract",
            ctx,
            "IO",
            &pdfluent::OptionsError::TatrNeedsOrtTables.to_string(),
        );
    }
    if args.jobs == 0 {
        return output::error("extract", ctx, "BAD_USAGE", "--jobs must be at least 1");
    }
    // Size the global rayon pool before the first render can use it: the
    // engine extracts pages over that pool, and `--jobs 1` must not wake an
    // oversized one whose workers only spin. Must precede every rayon call:
    // the global pool initialises once per process.
    init_rayon_pool(args.jobs);
    let mut warnings = Vec::new();
    // Resolve inputs: stdin alone, or the file/glob/dir batch.
    let stdin_mode = args.inputs.iter().any(|i| i == "-");
    if stdin_mode && args.inputs.len() != 1 {
        return output::error(
            "extract",
            ctx,
            "BAD_USAGE",
            "mixing `-` (stdin) with batch inputs is not supported; extract stdin on its own",
        );
    }
    let paths: Vec<PathBuf> = if stdin_mode {
        Vec::new()
    } else {
        match input::expand_inputs("extract", ctx, &args.inputs, args.recursive) {
            Ok(paths) => paths,
            Err(code) => return code,
        }
    };
    if !stdin_mode && paths.is_empty() {
        return output::success("extract", ctx, json!({ "results": [] }), warnings, |ctx| {
            output::note(ctx, "no inputs matched; nothing to do")
        });
    }

    // Output plan.
    let multi = !stdin_mode && paths.len() > 1;
    let out_mode = match plan_output(args.out.as_deref(), multi) {
        Ok(mode) => mode,
        Err((code, msg)) => return output::error("extract", ctx, &code, &msg),
    };

    // Plan every input and output before reading anything: stems, targets,
    // collision refusal, and output guards all run here, so a batch that
    // cannot be written fails before the first byte is read.
    let mut items = Vec::new();
    if stdin_mode {
        let target = match &out_mode {
            OutMode::Stdout => None,
            OutMode::File(path) => Some(path.clone()),
            OutMode::Dir(dir) => Some(dir.join(format!("stdin.{}", args.format.extension()))),
        };
        match input::load("extract", ctx, "-") {
            Ok(loaded) => items.push(WorkItem {
                display: loaded.display,
                source: WorkSource::Stdin(loaded.bytes),
                stem: "stdin".to_string(),
                target,
            }),
            Err(code) => return code,
        }
    } else {
        for path in &paths {
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "out".to_string());
            let target = match &out_mode {
                OutMode::Stdout => None,
                OutMode::File(file) => Some(file.clone()),
                OutMode::Dir(dir) => Some(dir.join(format!("{stem}.{}", args.format.extension()))),
            };
            items.push(WorkItem {
                display: path.to_string_lossy().to_string(),
                source: WorkSource::Path(path.clone()),
                stem,
                target,
            });
        }
    }

    // Same-named inputs share one output file (2024/invoice.pdf and
    // 2025/invoice.pdf both become out/invoice.md): refuse with both names
    // on it, before any write — never race the workers for the file.
    // Compared NFC-normalised and casefolded: A/Invoice.pdf and
    // B/invoice.pdf are distinct paths on Linux but the same file on the
    // case-insensitive filesystems of macOS and Windows, where the second
    // would overwrite the first — and macOS spells é decomposed (NFD) in
    // readdir, so a composed and a decomposed target are one file there.
    {
        use unicode_normalization::UnicodeNormalization;
        let mut seen: std::collections::BTreeMap<String, &str> = std::collections::BTreeMap::new();
        for item in &items {
            if let Some(target) = item.target.as_ref() {
                let folded = target
                    .to_string_lossy()
                    .nfc()
                    .collect::<String>()
                    .to_lowercase();
                if let Some(first) = seen.insert(folded, &item.display) {
                    return output::error(
                        "extract",
                        ctx,
                        "BAD_USAGE",
                        &format!(
                            "inputs {first} and {} both map to {}; extract them in separate runs",
                            item.display,
                            target.display()
                        ),
                    );
                }
            }
        }
    }

    // Existing outputs and the input itself fail the batch before the first
    // byte is read.
    let input_paths: Vec<PathBuf> = items
        .iter()
        .filter_map(|item| match &item.source {
            WorkSource::Path(path) => Some(path.clone()),
            WorkSource::Stdin(_) => None,
        })
        .collect();
    for item in &items {
        if let Some(target) = item.target.as_ref() {
            if let Err(code) = input::guard_output("extract", ctx, &input_paths, target, args.force)
            {
                return code;
            }
        }
    }

    // Renders stream through a bounded pool of `--jobs` workers in both
    // modes; the main thread commits them in input order, so what lands on
    // disk is deterministic however the renders overlapped. Default stops
    // at the first error: earlier outputs are kept and inputs that have
    // not started are cancelled. --keep-going commits every input and
    // reports every error together with every success.
    let render = |item: &WorkItem| render_one(args, item);
    let on_panic = |item: &WorkItem| {
        Err(BatchError {
            display: item.display.clone(),
            code: "FAILED".to_string(),
            msg: "worker thread panicked".to_string(),
        })
    };
    let mut results: Vec<JobResult> = Vec::with_capacity(items.len());
    if args.keep_going {
        let mut errors: Vec<BatchError> = Vec::new();
        run_bounded_ordered(&items, args.jobs, &render, &on_panic, |_, rendered| {
            match rendered {
                Ok(rendered) => match commit_one(ctx, rendered) {
                    Ok(job) => results.push(job),
                    Err(error) => errors.push(error),
                },
                Err(error) => errors.push(error),
            }
            true
        });
        if !errors.is_empty() {
            return report_batch("extract", ctx, &results, &errors);
        }
    } else {
        let mut first_error: Option<BatchError> = None;
        run_bounded_ordered(&items, args.jobs, &render, &on_panic, |_, rendered| {
            let outcome = match rendered {
                Ok(rendered) => commit_one(ctx, rendered).map(|job| {
                    results.push(job);
                }),
                Err(error) => Err(error),
            };
            match outcome {
                Ok(()) => true,
                Err(error) => {
                    first_error = Some(error);
                    false
                }
            }
        });
        if let Some(error) = first_error {
            return output::error("extract", ctx, &error.code, &error.msg);
        }
    }

    let to_stdout = matches!(out_mode, OutMode::Stdout);
    if to_stdout {
        let only = &results[0];
        let text = only.text.clone().unwrap_or_default();
        if ctx.json {
            let (data, mut extra) = json_data_single(only, &text);
            warnings.append(&mut extra);
            return output::success("extract", ctx, data, warnings, |_| {});
        }
        if !ctx.quiet {
            for warning in &warnings {
                eprintln!("warning: {warning}");
            }
        }
        print!("{text}");
        return output::exit::OK;
    }

    let files: Vec<serde_json::Value> = results
        .iter()
        .map(|r| {
            json!({
                "input": r.display,
                "page_count": r.page_count,
                "char_count": r.char_count,
                "output_path": r.output_path,
            })
        })
        .collect();
    let wrote: Vec<String> = results
        .iter()
        .filter_map(|r| r.output_path.clone())
        .collect();
    output::success(
        "extract",
        ctx,
        json!({ "results": files }),
        warnings,
        |ctx| {
            for path in &wrote {
                output::note(ctx, &format!("Wrote {path}"));
            }
        },
    )
}

/// The --keep-going verdict when at least one input failed: every success
/// AND every error is reported, instead of the first error hiding the
/// rest. JSON keeps the first error (in input order) as the envelope
/// error and carries all successes under `results` with all failures
/// under `errors`; human mode prints `Wrote` notes plus one error line
/// naming its input per failed input. The exit code is the first error's.
fn report_batch(command: &str, ctx: &Ctx, results: &[JobResult], errors: &[BatchError]) -> u8 {
    let first = &errors[0];
    let files: Vec<serde_json::Value> = results
        .iter()
        .map(|r| {
            json!({
                "input": r.display,
                "page_count": r.page_count,
                "char_count": r.char_count,
                "output_path": r.output_path,
            })
        })
        .collect();
    let failures: Vec<serde_json::Value> = errors
        .iter()
        .map(|e| {
            json!({
                "input": e.display,
                "code": e.code,
                "message": e.msg,
            })
        })
        .collect();
    let wrote: Vec<String> = results
        .iter()
        .filter_map(|r| r.output_path.clone())
        .collect();
    if !ctx.json {
        // Human mode names the file on every error line: several failures
        // share one stderr, and messages like `Invalid PDF` carry no name
        // of their own. (JSON already carries each input beside its error.)
        for path in &wrote {
            output::note(ctx, &format!("Wrote {path}"));
        }
        for error in errors {
            output::error_line(&error.code, &format!("{}: {}", error.display, error.msg));
        }
        return output::exit_for(&first.code);
    }
    // Warnings stay dropped on error, as in every other error path.
    output::error_with_data(
        command,
        ctx,
        &first.code,
        &first.msg,
        Some(json!({ "results": files, "errors": failures })),
        || {},
    )
}

enum OutMode {
    Stdout,
    File(PathBuf),
    Dir(PathBuf),
}

fn plan_output(out: Option<&str>, multi: bool) -> Result<OutMode, (String, String)> {
    let bad = |msg: String| ("BAD_USAGE".to_string(), msg);
    match out {
        None | Some("-") => {
            if multi {
                return Err(bad("multiple inputs need -o <dir>".to_string()));
            }
            Ok(OutMode::Stdout)
        }
        Some(dir) if Path::new(dir).is_dir() => Ok(OutMode::Dir(PathBuf::from(dir))),
        Some(path) => {
            // A trailing slash names a directory even for a single input.
            let want_dir = multi || path.ends_with('/') || path.ends_with('\\');
            if want_dir {
                let dir = PathBuf::from(path);
                std::fs::create_dir_all(&dir).map_err(|e| {
                    (
                        "IO".to_string(),
                        format!("could not create directory {}: {e}", dir.display()),
                    )
                })?;
                Ok(OutMode::Dir(dir))
            } else {
                Ok(OutMode::File(PathBuf::from(path)))
            }
        }
    }
}

/// Run `work` over `items` with at most `jobs` threads in flight,
/// committing each result in input order through `commit` as soon as its
/// predecessors have committed. `commit` returns whether to continue: when
/// it says stop, nothing further is fed or committed — workers finish
/// their current item, then exit, and buffered later results are dropped
/// uncommitted. Items are fed a window at a time (one per commit past the
/// initial window), so at most a window of renders waits however the batch
/// sizes — memory stays bounded no matter the batch size. A panicking item
/// becomes `on_panic(item)` instead of killing the batch — the pool keeps
/// its other workers.
fn run_bounded_ordered<T, R>(
    items: &[T],
    jobs: usize,
    work: &(dyn Fn(&T) -> R + Sync),
    on_panic: &(dyn Fn(&T) -> R + Sync),
    mut commit: impl FnMut(usize, R) -> bool,
) where
    T: Sync,
    R: Send,
{
    use std::collections::BTreeMap;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    };
    if items.is_empty() {
        return;
    }
    let jobs = jobs.max(1).min(items.len());
    let stop = Arc::new(AtomicBool::new(false));
    let (job_tx, job_rx) = mpsc::channel::<(usize, &T)>();
    let job_rx = Arc::new(Mutex::new(job_rx));
    let (res_tx, res_rx) = mpsc::channel::<(usize, R)>();
    std::thread::scope(|scope| {
        for _ in 0..jobs {
            let job_rx = Arc::clone(&job_rx);
            let res_tx = res_tx.clone();
            let stop = Arc::clone(&stop);
            scope.spawn(move || {
                loop {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let next = match job_rx.lock() {
                        Ok(guard) => guard.recv().ok(),
                        Err(_) => None,
                    };
                    let Some((index, item)) = next else {
                        break;
                    };
                    // One panicking file must not take down the batch or
                    // strand the others: the panic is caught per item.
                    let result =
                        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(item)))
                        {
                            Ok(result) => result,
                            Err(_) => on_panic(item),
                        };
                    // The pool threads are joined below, so the only way
                    // this send fails is a commit panic unwinding the scope
                    // — and then the batch is already lost.
                    let _ = res_tx.send((index, result));
                }
            });
        }
        drop(res_tx);
        // Commit-driven feeding: a window to prime the workers, then one
        // item per commit. `job_tx` is dropped once everything is fed (and
        // on the stop), so workers blocked in `recv` exit and the scope
        // joins; while it lives, a `recv` that finds nothing waits.
        let mut job_tx = Some(job_tx);
        let mut fed = 0usize;
        let window = jobs * 2;
        feed_window(items, &mut job_tx, &mut fed, window);
        let mut pending: BTreeMap<usize, R> = BTreeMap::new();
        let mut next = 0usize;
        let mut stopped = false;
        for (index, result) in res_rx {
            if stopped {
                // Drain: workers finishing their current item after the
                // stop must find a receiver, or the scope never joins.
                continue;
            }
            pending.insert(index, result);
            while let Some(result) = pending.remove(&next) {
                if commit(next, result) {
                    next += 1;
                    feed_window(items, &mut job_tx, &mut fed, window + next);
                } else {
                    stopped = true;
                    stop.store(true, Ordering::SeqCst);
                    job_tx = None;
                    pending.clear();
                    break;
                }
            }
        }
    });
}

/// Feed items to the pool up to `limit`, dropping the sender once every
/// item is fed so idle workers exit and the scope joins.
fn feed_window<'a, T>(
    items: &'a [T],
    job_tx: &mut Option<std::sync::mpsc::Sender<(usize, &'a T)>>,
    fed: &mut usize,
    limit: usize,
) {
    while *fed < items.len().min(limit) {
        let send = match job_tx.as_ref() {
            Some(tx) => tx.send((*fed, &items[*fed])),
            None => break,
        };
        if send.is_err() {
            break;
        }
        *fed += 1;
    }
    if *fed == items.len() {
        *job_tx = None;
    }
}

/// `--table-model-dir` as the library's [`TatrOptions`][pdfluent::TatrOptions]:
/// the flag's directory over the defaults (4 intra threads, resolved paths),
/// `None` without the flag. Pure, so the unit tests pin the mapping on every
/// build; the tiers read it from the options the render passes on.
fn tatr_options_from(args: &ExtractArgs) -> Option<pdfluent::TatrOptions> {
    args.table_model_dir.clone().map(|dir| {
        let mut tatr = pdfluent::TatrOptions::default();
        tatr.model_dir = Some(dir);
        tatr
    })
}

/// Render one planned input: load, extract, hold. Pure w.r.t. final
/// paths: every write happens later, in input order, in [`commit_one`].
fn render_one(args: &ExtractArgs, item: &WorkItem) -> Result<Rendered, BatchError> {
    let fail = |code: &str, msg: String| BatchError::new(&item.display, code, msg);
    let (bytes, path) = match &item.source {
        WorkSource::Path(path) => {
            let raw = path.to_string_lossy().to_string();
            match input::load_pair(&raw) {
                Ok(loaded) => (loaded.bytes, loaded.path),
                Err((code, msg)) => return Err(fail(&code, msg)),
            }
        }
        WorkSource::Stdin(bytes) => (bytes.clone(), None),
    };
    if let InputKind::FacadeGap { label } = input::kind_of(path.as_deref(), &bytes) {
        return Err(fail(
            "UNSUPPORTED_INPUT",
            format!(
                "{label} extraction is not exposed by the `pdfluent` facade ({} in {}); only PDF is supported by `extract` today",
                item.display, "facade gap, reported rather than re-implemented",
            ),
        ));
    }
    let doc = pdfluent::PdfDocument::from_bytes(&bytes)
        .map_err(|e| fail(&cli_code(&e), e.to_string()))?;
    let page_count = doc.page_count();

    let mut opts = pdfluent::ExtractOptions::default();
    opts.quality = match args.quality {
        Quality::Fast => pdfluent::Quality::Fast,
        Quality::Balanced => pdfluent::Quality::Balanced,
        // Refused in `run` before anything is planned, so rendering never
        // sees it.
        Quality::Best => unreachable!("`--quality best` is refused before planning"),
    };
    opts.model_dir.clone_from(&args.model_dir);
    // The TATR opt-in rides into whichever tier runs: `None` -- the flag
    // unset -- leaves the options exactly as before, so the default reads
    // byte for byte as without the flag.
    if let Some(tatr) = tatr_options_from(args) {
        opts = opts.with_tatr(tatr);
    }
    if let Some(spec) = args.pages.as_deref() {
        let list = crate::pagespec::parse_list(spec, page_count).map_err(|e| match e {
            crate::pagespec::SpecError::Syntax(msg) => fail("BAD_USAGE", msg),
            crate::pagespec::SpecError::OutOfRange(msg) => fail("PAGE_RANGE", msg),
        })?;
        opts.pages = Some(list.into_iter().map(|p| p - 1).collect());
    }
    // Figures ride the Markdown only; other formats never read these.
    match args.images.as_ref() {
        None => {}
        Some(Images::None) => {
            opts.include_figures = false;
        }
        Some(Images::Embed) => {
            opts.embed_figures = true;
        }
        Some(Images::Dir(dir)) => {
            opts.figure_image_dir = Some(dir.join(&item.stem));
        }
    }
    // The word boxes travel only when asked: `--format words` is the word
    // list itself, and `--words` carries the word lines beside the blocks.
    opts.word_boxes = args.words || args.format == Format::Words;
    let text = render_format(&doc, args, &opts, &item.stem, page_count)
        .map_err(|(code, msg)| fail(&code, msg))?;
    let char_count = text.chars().count();

    Ok(Rendered {
        display: item.display.clone(),
        page_count,
        char_count,
        text,
        target: item.target.clone(),
    })
}

/// Commit one render onto its final paths, in input order on the main
/// thread. The Markdown is staged to a temp file first and renamed onto
/// its target.
fn commit_one(ctx: &Ctx, rendered: Rendered) -> Result<JobResult, BatchError> {
    let Rendered {
        display,
        page_count,
        char_count,
        text,
        target,
    } = rendered;
    let fail = |code: &str, msg: String| BatchError::new(&display, code, msg);
    let Some(target) = target else {
        return Ok(JobResult {
            display,
            page_count,
            char_count,
            output_path: None,
            text: Some(text),
        });
    };
    let tmp =
        input::stage_file_pair(&target, text.as_bytes()).map_err(|(code, msg)| fail(&code, msg))?;
    if let Err(e) = std::fs::rename(&tmp, &target) {
        // Past the guards this rename is a sibling move that cannot fail;
        // if it does anyway (a concurrent hand on the directory), remove
        // the temp file.
        let _ = std::fs::remove_file(&tmp);
        return Err(fail(
            "IO",
            format!("could not write {}: {e}", target.display()),
        ));
    }
    crate::output::verbose(ctx, &format!("extract: {display} -> {}", target.display()));
    Ok(JobResult {
        display,
        page_count,
        char_count,
        output_path: Some(target.display().to_string()),
        text: None,
    })
}

fn render_format(
    doc: &pdfluent::PdfDocument,
    args: &ExtractArgs,
    opts: &pdfluent::ExtractOptions,
    stem: &str,
    page_count: usize,
) -> Result<String, (String, String)> {
    match args.format {
        Format::Md => doc
            .extract_markdown(opts)
            .map_err(|e| (cli_code(&e), e.to_string())),
        Format::Json => {
            let blocks = doc
                .extract_blocks(opts)
                .map_err(|e| (cli_code(&e), e.to_string()))?;
            if args.words {
                Ok(pdfluent::to_json_with_words(&blocks))
            } else {
                Ok(pdfluent::to_json(&blocks))
            }
        }
        Format::Words => {
            let words = doc
                .extract_words(opts)
                .map_err(|e| (cli_code(&e), e.to_string()))?;
            Ok(pdfluent::to_words_json_lines(&words))
        }
        Format::Chunks => doc
            .extract_docling(stem, opts)
            .map_err(|e| (cli_code(&e), e.to_string())),
        Format::Text => {
            let pages = doc
                .extract_text_pages()
                .map_err(|e| (cli_code(&e), e.to_string()))?;
            // The page-text call takes no window, so `--pages` selects after
            // the fact. Past-the-end pages were already rejected above.
            if let Some(spec) = args.pages.as_deref() {
                let list = crate::pagespec::parse_list(spec, page_count).map_err(|e| match e {
                    crate::pagespec::SpecError::Syntax(msg) => ("BAD_USAGE".to_string(), msg),
                    crate::pagespec::SpecError::OutOfRange(msg) => ("PAGE_RANGE".to_string(), msg),
                })?;
                let keep: Vec<&str> = list
                    .into_iter()
                    .filter_map(|p| pages.get(p - 1).map(String::as_str))
                    .collect();
                Ok(pdfluent::join_page_texts(keep))
            } else {
                Ok(pdfluent::join_page_texts(pages.iter().map(String::as_str)))
            }
        }
    }
}

/// The CLI code for a facade error (mirrors `output::facade_error`, whose
/// messages this path reuses verbatim via `Display`).
fn cli_code(err: &pdfluent::Error) -> String {
    use pdfluent::Error as E;
    // A model failure carries its own stable code (`MODEL_NOT_AVAILABLE` and
    // its siblings), not a generic I/O one; the message names the install.
    if let Some(model) = err.model_error() {
        return model.kind.code().to_string();
    }
    match err {
        E::FileNotFound { .. } => "FILE_NOT_FOUND",
        E::InvalidPdf { .. } => "INVALID_PDF",
        E::UnsupportedPdfVersion { .. } => "UNSUPPORTED_VERSION",
        E::DecryptionFailed { reason } => match reason {
            pdfluent::error::DecryptionFailureReason::PasswordRequired => "PASSWORD_REQUIRED",
            pdfluent::error::DecryptionFailureReason::WrongPassword => "BAD_PASSWORD",
            _ => "FAILED",
        },
        E::PageOutOfRange { .. } | E::PageRangeInvalid { .. } => "PAGE_RANGE",
        E::PdfaValidationFailed { .. } => "NOT_COMPLIANT",
        E::Io { .. } => "IO",
        _ => "FAILED",
    }
    .to_string()
}

fn json_data_single(only: &JobResult, text: &str) -> (serde_json::Value, Vec<String>) {
    let mut warnings = Vec::new();
    let mut data = json!({
        "input": only.display,
        "page_count": only.page_count,
        "char_count": only.char_count,
        "output_path": serde_json::Value::Null,
    });
    if text.len() <= JSON_TEXT_INLINE_CAP {
        data["text"] = json!(text);
    } else {
        data["preview"] = json!(text.chars().take(2000).collect::<String>());
        warnings.push(format!(
            "text omitted from JSON (>{JSON_TEXT_INLINE_CAP} bytes); use -o <file>"
        ));
    }
    (data, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn pool_never_exceeds_jobs_and_commits_in_input_order() {
        let live = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let items: Vec<u32> = (0..8).collect();
        let mut committed = Vec::new();
        run_bounded_ordered(
            &items,
            2,
            &|item| {
                live.fetch_add(1, Ordering::SeqCst);
                peak.fetch_max(live.load(Ordering::SeqCst), Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_millis(20));
                live.fetch_sub(1, Ordering::SeqCst);
                item * 10
            },
            &|_| 0,
            |index, result| {
                committed.push((index, result));
                true
            },
        );
        let order: Vec<usize> = committed.iter().map(|(index, _)| *index).collect();
        assert_eq!(order, vec![0, 1, 2, 3, 4, 5, 6, 7]);
        let values: Vec<u32> = committed.iter().map(|(_, value)| *value).collect();
        assert_eq!(values, vec![0, 10, 20, 30, 40, 50, 60, 70]);
        assert!(
            peak.load(Ordering::SeqCst) <= 2,
            "more than 2 jobs ran at once"
        );
    }

    #[test]
    fn pool_turns_a_panicking_item_into_the_panic_marker() {
        let items: Vec<u32> = (0..4).collect();
        let mut committed = Vec::new();
        run_bounded_ordered(
            &items,
            2,
            &|item| {
                if *item == 1 {
                    panic!("boom");
                }
                item * 10
            },
            &|item| item * 1000 + 999,
            |_, result| {
                committed.push(result);
                true
            },
        );
        assert_eq!(committed, vec![0, 1999, 20, 30]);
    }

    #[test]
    fn pool_with_no_items_runs_nothing() {
        let items: Vec<u32> = Vec::new();
        let mut commits = 0;
        run_bounded_ordered(&items, 4, &|item| item * 10, &|_| 0, |_, _| {
            commits += 1;
            true
        });
        assert_eq!(commits, 0);
    }

    #[test]
    fn pool_stop_cancels_items_that_have_not_started() {
        // One worker, stop after the first commit: only the first item
        // commits, and at most the already-fed second is attempted — the
        // tail is never fed, let alone started.
        let items: Vec<u32> = (0..8).collect();
        let attempted = AtomicUsize::new(0);
        let mut committed = Vec::new();
        run_bounded_ordered(
            &items,
            1,
            &|item| {
                attempted.fetch_add(1, Ordering::SeqCst);
                item * 10
            },
            &|_| 0,
            |index, result| {
                committed.push((index, result));
                false
            },
        );
        assert_eq!(committed, vec![(0, 0)]);
        assert!(
            attempted.load(Ordering::SeqCst) <= 2,
            "the unfed tail is never started"
        );
    }

    fn args_with(table_model_dir: Option<PathBuf>) -> ExtractArgs {
        ExtractArgs {
            inputs: vec!["in.pdf".to_string()],
            format: Format::Md,
            quality: Quality::Balanced,
            model_dir: None,
            table_model_dir,
            pages: None,
            images: None,
            out: None,
            recursive: false,
            jobs: 1,
            keep_going: false,
            force: false,
            words: false,
        }
    }

    #[test]
    fn table_model_dir_unset_leaves_tatr_off() {
        // The default half of the parity: without the flag the render passes
        // options with no TATR opt-in, so `balanced` reads byte for byte as
        // without the flag on every build.
        assert_eq!(tatr_options_from(&args_with(None)), None);
    }

    #[test]
    fn table_model_dir_maps_to_the_library_tatr_options() {
        // The flag's directory over the library defaults, exactly as a
        // library caller builds them: equal options into the same
        // `extract_markdown` are equal markdown, so the CLI's balanced output
        // with the flag is the library's balanced-with-TATR output. The fake
        // seam rides the same value the output tests run with: set it on both
        // and they are still equal.
        let dir = PathBuf::from("/models/tatr");
        let mut expected = pdfluent::TatrOptions::default();
        expected.model_dir = Some(dir.clone());
        let mapped = tatr_options_from(&args_with(Some(dir))).expect("the flag maps");
        assert_eq!(mapped, expected);
        let mut mapped_hooked = mapped.clone();
        mapped_hooked.test_hook = Some(pdfluent::TatrTestHook::Declined);
        let mut expected_hooked = expected.clone();
        expected_hooked.test_hook = Some(pdfluent::TatrTestHook::Declined);
        assert_eq!(mapped_hooked, expected_hooked);
    }

    #[test]
    fn rayon_pool_jobs_one_requests_a_single_thread() {
        // `--jobs 1` asks for sequential work: one pool thread, no idle
        // workers spinning while pages render one at a time.
        assert_eq!(resolve_rayon_threads(1, None), Some(1));
    }

    #[test]
    fn rayon_pool_honours_an_explicit_rayon_setting() {
        // An explicit `RAYON_NUM_THREADS` always wins over the flag — even
        // an unparseable one, which then falls back to rayon's own default
        // exactly as without this code.
        assert_eq!(resolve_rayon_threads(1, Some("8")), None);
        assert_eq!(resolve_rayon_threads(1, Some("not-a-number")), None);
        assert_eq!(resolve_rayon_threads(1, Some("")), None);
    }

    #[test]
    fn rayon_pool_leaves_jobs_above_one_alone() {
        // `--jobs N>1` keeps the default pool, byte for byte as before.
        assert_eq!(resolve_rayon_threads(2, None), None);
        assert_eq!(resolve_rayon_threads(4, None), None);
    }
}
