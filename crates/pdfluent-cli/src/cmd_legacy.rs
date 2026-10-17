//! Kept commands: `info`, `inspect`, `extract-text`, `doctor`,
//! `completions`, and the experimental `xfa flatten` stub.
//!
//! `extract-text` stays byte-compatible for the extraction rig
//! (`scripts/extraction/extraction_axes.py` drives `[binary, "extract-text",
//! pdf]`); new work should prefer `extract --format text`.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use clap::Args;
use serde_json::json;

use crate::input;
use crate::output::{self, Ctx, CLI_VERSION};

/// XFA caveat reused wherever XFA is mentioned.
const XFA_CAVEAT: &str =
    "XFA support is EXPERIMENTAL and feature-gated — not production-supported, \
no Adobe Reader parity claim. The `fresh-merge` policy is opt-in and never the default.";

/// Cap on inline `text` returned in `extract-text --json` (without `--out`).
const JSON_TEXT_INLINE_CAP: usize = 64 * 1024;

#[derive(Debug, Args)]
pub struct InfoArgs {
    /// Input PDF file, or `-` for stdin.
    pub input: String,
}

#[derive(Debug, Args)]
pub struct ExtractTextArgs {
    /// Input PDF file, or `-` for stdin.
    pub input: String,

    /// Write extracted text to this path instead of stdout.
    #[arg(long)]
    pub out: Option<std::path::PathBuf>,

    /// Join pages with a blank line instead of ending each with a form
    /// feed (U+000C).
    ///
    /// Page boundaries are lost; for a consumer that chunks or cites by
    /// page they are the point. Here for pipelines written against the
    /// pre-1.0 output.
    #[arg(long)]
    pub no_page_breaks: bool,

    /// Overwrite the existing `--out` file.
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct DoctorArgs {
    /// PDF file to inspect for decode-leniency events (optional).
    pub input: Option<String>,
}

#[derive(Debug, Args)]
pub struct CompletionsArgs {
    /// Shell to generate completions for.
    #[arg(value_enum)]
    pub shell: clap_complete::Shell,
}

#[derive(Debug, Args)]
pub struct XfaFlattenArgs {
    /// Input PDF file.
    pub input: std::path::PathBuf,

    /// Output PDF file.
    #[arg(long, short = 'o')]
    pub out: std::path::PathBuf,

    /// XFA rendering policy: `saved-state` (default) or `fresh-merge` (experimental, opt-in).
    #[arg(long, default_value = "saved-state")]
    pub policy: String,

    /// Acknowledge that you are using an experimental, opt-in code path.
    #[arg(long)]
    pub experimental: bool,
}

/// `info` / `inspect`.
pub(crate) fn run_info(ctx: &Ctx, args: &InfoArgs) -> u8 {
    let loaded = match input::load("info", ctx, &args.input) {
        Ok(loaded) => loaded,
        Err(code) => return code,
    };
    let doc = match input::open("info", ctx, &loaded) {
        Ok(doc) => doc,
        Err(code) => return code,
    };
    let file_size = loaded
        .path
        .as_ref()
        .and_then(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .unwrap_or(loaded.bytes.len() as u64);
    let page_count = doc.page_count();
    let version = doc.version().to_string();
    let diagnostics = doc.diagnostics();
    let report = pdfluent::LeniencyReport::from_diagnostics(&diagnostics);
    let leniency_json = if report.is_clean() {
        json!({ "clean": true, "events": [] })
    } else {
        let codes: Vec<&str> = report.events.iter().map(|e| e.code).collect();
        json!({
            "clean": false,
            "warning_count": report.warning_count,
            "critical_count": report.critical_count,
            "events": codes,
        })
    };
    output::success(
        "info",
        ctx,
        json!({
            "page_count": page_count,
            "pdf_version": version,
            "file_size": file_size,
            "leniency": leniency_json,
        }),
        vec![],
        |_| {
            println!("Pages:       {page_count}");
            println!("PDF version: {version}");
            println!("Size:        {file_size} bytes");
            if report.is_clean() {
                println!("Leniency:    clean");
            } else {
                println!("Leniency:    {} event(s)", report.unique_event_count);
                for event in &report.events {
                    println!("  {}", event.code);
                }
            }
        },
    )
}

/// `extract-text`. The plain-text form ends every page with a form feed
/// (U+000C), which is what `pdftotext` and `mutool` emit and what a consumer
/// chunking or citing by page splits on.
pub(crate) fn run_extract_text(ctx: &Ctx, args: &ExtractTextArgs) -> u8 {
    let loaded = match input::load("extract-text", ctx, &args.input) {
        Ok(loaded) => loaded,
        Err(code) => return code,
    };
    let doc = match input::open("extract-text", ctx, &loaded) {
        Ok(doc) => doc,
        Err(code) => return code,
    };
    let page_count = doc.page_count();
    let pages = match doc.extract_text_pages() {
        Ok(p) => p,
        Err(e) => return output::facade_error("extract-text", ctx, &e),
    };
    let text = if args.no_page_breaks {
        pages.join("\n\n")
    } else {
        pdfluent::join_page_texts(pages.iter().map(String::as_str))
    };
    let char_count = text.chars().count();

    if let Some(path) = args.out.as_deref() {
        if let Err(code) = input::write_bytes(
            "extract-text",
            ctx,
            text.as_bytes(),
            path,
            loaded.path.as_slice(),
            args.force,
        ) {
            return code;
        }
        return output::success(
            "extract-text",
            ctx,
            json!({ "page_count": page_count, "char_count": char_count, "output_path": path.display().to_string() }),
            vec![],
            |ctx| {
                output::note(
                    ctx,
                    &format!("Wrote {char_count} chars to {}", path.display()),
                )
            },
        );
    }

    if ctx.json {
        // Include inline text only when reasonably bounded; otherwise a preview.
        // The `pages` array is always present: its boundaries are the answer to
        // "which page is this on", and that answer survives the size cap even
        // when the text itself does not.
        let inline = text.len() <= JSON_TEXT_INLINE_CAP;
        let pages_json: Vec<serde_json::Value> = pages
            .iter()
            .enumerate()
            .map(|(idx, page_text)| {
                let mut entry = json!({
                    "page": idx + 1,
                    "char_count": page_text.chars().count(),
                });
                if inline {
                    entry["text"] = json!(page_text);
                }
                entry
            })
            .collect();
        let mut data = json!({ "page_count": page_count, "char_count": char_count, "output_path": serde_json::Value::Null, "pages": pages_json });
        let mut warnings = vec![];
        if inline {
            data["text"] = json!(text);
        } else {
            let preview: String = text.chars().take(2000).collect();
            data["preview"] = json!(preview);
            warnings.push(format!(
                "text omitted from JSON (>{JSON_TEXT_INLINE_CAP} bytes); use --out"
            ));
        }
        output::success("extract-text", ctx, data, warnings, |_| {})
    } else {
        print!("{text}");
        output::exit::OK
    }
}

/// `doctor`, with and without a file.
pub(crate) fn run_doctor(ctx: &Ctx, args: &DoctorArgs) -> u8 {
    if let Some(raw) = args.input.as_deref() {
        return run_doctor_file(ctx, raw);
    }
    let data = json!({
        "cli_version": CLI_VERSION,
        "sdk": format!("pdfluent facade (={})", pdfluent::api_version()),
        "target_os": std::env::consts::OS,
        "target_arch": std::env::consts::ARCH,
        "license": "AGPL-3.0-only OR LicenseRef-PDFluent-Commercial",
        "xfa": "experimental/feature-gated",
        "binary": "pdfluent (alias: pdfluent-cli)",
    });
    output::success("doctor", ctx, data, vec![], |_| {
        println!("pdfluent CLI doctor");
        println!("  cli_version:  {CLI_VERSION}");
        println!(
            "  sdk:          pdfluent facade (={})",
            pdfluent::api_version()
        );
        println!("  target_os:    {}", std::env::consts::OS);
        println!("  target_arch:  {}", std::env::consts::ARCH);
        println!("  license:      AGPL-3.0 or a commercial licence, at your option");
        println!("  note:         XFA is experimental/feature-gated.");
    })
}

/// `doctor <INPUT>` — open a PDF and report its decode-leniency events.
fn run_doctor_file(ctx: &Ctx, raw: &str) -> u8 {
    let loaded = match input::load("doctor", ctx, raw) {
        Ok(loaded) => loaded,
        Err(code) => return code,
    };
    let doc = match input::open("doctor", ctx, &loaded) {
        Ok(doc) => doc,
        Err(code) => return code,
    };
    let diagnostics = doc.diagnostics();
    let report = pdfluent::LeniencyReport::from_diagnostics(&diagnostics);
    let events_json: Vec<serde_json::Value> = report
        .events
        .iter()
        .map(|e| {
            json!({
                "code": e.code,
                "severity": format!("{:?}", e.severity),
                "message": e.message,
            })
        })
        .collect();
    output::success(
        "doctor",
        ctx,
        json!({
            "file": loaded.display,
            "leniency": {
                "clean": report.is_clean(),
                "unique_event_count": report.unique_event_count,
                "warning_count": report.warning_count,
                "critical_count": report.critical_count,
                "events": events_json,
            },
        }),
        vec![],
        |_| {
            println!("File: {}", loaded.display);
            if report.is_clean() {
                println!("Leniency: clean (no decode or repair events)");
            } else {
                println!("Leniency events ({}):", report.unique_event_count);
                for event in &report.events {
                    println!("  [{:?}] {}: {}", event.severity, event.code, event.message);
                }
            }
        },
    )
}

/// `completions`.
pub(crate) fn run_completions(args: &CompletionsArgs) -> u8 {
    let mut cmd = crate::cli_command();
    clap_complete::generate(args.shell, &mut cmd, "pdfluent", &mut std::io::stdout());
    output::exit::OK
}

/// `xfa flatten` — scaffold stub; enforces experimental/opt-in contract.
pub(crate) fn run_xfa_flatten(ctx: &Ctx, args: &XfaFlattenArgs) -> u8 {
    eprintln!("note: {XFA_CAVEAT}");
    match args.policy.as_str() {
        "saved-state" => output::error(
            "xfa flatten",
            ctx,
            "NOT_IMPLEMENTED",
            "xfa flatten is not implemented in the public CLI yet; use the SDK or internal tooling. XFA is experimental",
        ),
        "fresh-merge" => {
            if !args.experimental {
                return output::error(
                    "xfa flatten",
                    ctx,
                    "EXPERIMENTAL_OPT_IN_REQUIRED",
                    "policy `fresh-merge` is experimental and opt-in; re-run with --experimental to acknowledge (still not implemented in the public CLI)",
                );
            }
            output::error(
                "xfa flatten",
                ctx,
                "NOT_IMPLEMENTED",
                "xfa flatten (fresh-merge) is experimental and not implemented in the public CLI yet",
            )
        }
        other => output::error(
            "xfa flatten",
            ctx,
            "BAD_POLICY",
            &format!("unknown --policy `{other}` (expected `saved-state` or `fresh-merge`)"),
        ),
    }
}
