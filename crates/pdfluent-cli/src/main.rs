//! PDFluent public command-line interface.
//!
//! Public-facing CLI built on the `pdfluent` SDK facade. Deliberately small,
//! release-aligned, non-XFA core surface. XFA support is **experimental and
//! feature-gated** — no Adobe Reader parity claim, and the `fresh-merge` policy
//! is opt-in and never the default.
//!
//! Internal/dev/corpus tooling (measure, debug-xfa, flatten-check, demo,
//! collectors) lives in the separate `xfa-cli` crate and is intentionally NOT
//! exposed here.

mod output;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{CommandFactory, Parser, Subcommand};
use serde_json::json;

use output::{exit, CLI_VERSION};

/// XFA caveat reused wherever XFA is mentioned.
const XFA_CAVEAT: &str =
    "XFA support is EXPERIMENTAL and feature-gated — not production-supported, \
no Adobe Reader parity claim. The `fresh-merge` policy is opt-in and never the default.";

/// Cap on inline `text` returned in `extract-text --json` (without `--out`).
const JSON_TEXT_INLINE_CAP: usize = 64 * 1024;

#[derive(Parser)]
#[command(
    name = "pdfluent",
    version = CLI_VERSION,
    about = "PDFluent — PDF toolkit (info, text, validation). Commercial license; evaluation use permitted.",
    long_about = None,
    propagate_version = true
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show document information (page count, PDF version, file size).
    Info {
        /// Input PDF file.
        input: PathBuf,
        /// Emit the stable JSON envelope on stdout.
        #[arg(long)]
        json: bool,
    },
    /// Inspect a document and emit JSON (alias of `info --json`).
    Inspect {
        /// Input PDF file.
        input: PathBuf,
        /// Accepted for compatibility; `inspect` always emits JSON.
        #[arg(long)]
        json: bool,
    },
    /// Extract text from a PDF.
    ExtractText {
        /// Input PDF file.
        input: PathBuf,
        /// Write extracted text to this path instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Emit the stable JSON envelope on stdout.
        #[arg(long)]
        json: bool,
    },
    /// Validate that a PDF parses and has readable pages (NOT a PDF/A conformance check).
    Validate {
        /// Input PDF file.
        input: PathBuf,
        /// Emit the stable JSON envelope on stdout.
        #[arg(long)]
        json: bool,
    },
    /// Check environment / installation and print build info.
    ///
    /// When `INPUT` is supplied the command opens that PDF and reports its
    /// decode-leniency events instead of the standard build-info output.
    Doctor {
        /// PDF file to inspect for decode-leniency events (optional).
        input: Option<PathBuf>,
        /// Emit the stable JSON envelope on stdout.
        #[arg(long)]
        json: bool,
    },
    /// Generate shell completions.
    Completions {
        /// Shell to generate completions for.
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Experimental XFA tooling (feature-gated, not production-supported).
    Xfa {
        #[command(subcommand)]
        command: XfaCommands,
    },
}

#[derive(Subcommand)]
enum XfaCommands {
    /// Flatten an XFA/AcroForm document (not yet implemented in the public CLI).
    ///
    /// XFA support is EXPERIMENTAL and feature-gated — no Adobe Reader parity
    /// claim. `--policy saved-state` (default) honors the document's saved form
    /// state; `--policy fresh-merge` is EXPERIMENTAL, opt-in, and never the
    /// default (requires `--experimental`).
    Flatten {
        /// Input PDF file.
        input: PathBuf,
        /// Output PDF file.
        #[arg(long, short = 'o')]
        out: PathBuf,
        /// XFA rendering policy: `saved-state` (default) or `fresh-merge` (experimental, opt-in).
        #[arg(long, default_value = "saved-state")]
        policy: String,
        /// Acknowledge that you are using an experimental, opt-in code path.
        #[arg(long)]
        experimental: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let code = match cli.command {
        Commands::Info { input, json } => cmd_info(&input, json),
        Commands::Inspect { input, .. } => cmd_info(&input, true),
        Commands::ExtractText { input, out, json } => {
            cmd_extract_text(&input, out.as_deref(), json)
        }
        Commands::Validate { input, json } => cmd_validate(&input, json),
        Commands::Doctor { input, json } => match input {
            Some(path) => cmd_doctor_file(&path, json),
            None => cmd_doctor(json),
        },
        Commands::Completions { shell } => cmd_completions(shell),
        Commands::Xfa { command } => match command {
            XfaCommands::Flatten {
                policy,
                experimental,
                ..
            } => cmd_xfa_flatten(&policy, experimental),
        },
    };
    ExitCode::from(code)
}

/// Open a PDF via the public facade, mapping IO/parse failures to the contract.
fn open_doc(cmd: &str, input: &Path, json: bool) -> Result<pdfluent::PdfDocument, u8> {
    if !input.exists() {
        return Err(output::error(
            cmd,
            json,
            "FILE_NOT_FOUND",
            &format!("no such file: {}", input.display()),
        ));
    }
    pdfluent::PdfDocument::open(input).map_err(|e| {
        output::error(
            cmd,
            json,
            "INVALID_PDF",
            &format!("could not open PDF: {e}"),
        )
    })
}

/// `info` / `inspect`.
fn cmd_info(input: &Path, json: bool) -> u8 {
    let doc = match open_doc("info", input, json) {
        Ok(d) => d,
        Err(c) => return c,
    };
    let file_size = std::fs::metadata(input).map(|m| m.len()).unwrap_or(0);
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
        json,
        json!({
            "page_count": page_count,
            "pdf_version": version,
            "file_size": file_size,
            "leniency": leniency_json,
        }),
        vec![],
        || {
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

/// `extract-text`.
fn cmd_extract_text(input: &Path, out: Option<&Path>, json: bool) -> u8 {
    let doc = match open_doc("extract-text", input, json) {
        Ok(d) => d,
        Err(c) => return c,
    };
    let page_count = doc.page_count();
    let text = match doc.extract_text() {
        Ok(t) => t,
        Err(e) => {
            return output::error(
                "extract-text",
                json,
                "INVALID_PDF",
                &format!("text extraction failed: {e}"),
            )
        }
    };
    let char_count = text.chars().count();

    if let Some(path) = out {
        if let Err(e) = std::fs::write(path, &text) {
            return output::error(
                "extract-text",
                json,
                "IO",
                &format!("could not write {}: {e}", path.display()),
            );
        }
        return output::success(
            "extract-text",
            json,
            json!({ "page_count": page_count, "char_count": char_count, "output_path": path.display().to_string() }),
            vec![],
            || println!("Wrote {char_count} chars to {}", path.display()),
        );
    }

    if json {
        // Include inline text only when reasonably bounded; otherwise a preview.
        let mut data = json!({ "page_count": page_count, "char_count": char_count, "output_path": serde_json::Value::Null });
        let mut warnings = vec![];
        if text.len() <= JSON_TEXT_INLINE_CAP {
            data["text"] = json!(text);
        } else {
            let preview: String = text.chars().take(2000).collect();
            data["preview"] = json!(preview);
            warnings.push(format!(
                "text omitted from JSON (>{JSON_TEXT_INLINE_CAP} bytes); use --out"
            ));
        }
        output::success("extract-text", true, data, warnings, || {})
    } else {
        print!("{text}");
        exit::OK
    }
}

/// `validate` — parse + page-count only. NOT a PDF/A / veraPDF conformance check.
fn cmd_validate(input: &Path, json: bool) -> u8 {
    let doc = match open_doc("validate", input, json) {
        Ok(d) => d,
        Err(c) => return c, // INVALID_PDF / FILE_NOT_FOUND already emitted
    };
    let page_count = doc.page_count();
    let version = doc.version().to_string();
    output::success(
        "validate",
        json,
        json!({
            "valid": true,
            "page_count": page_count,
            "pdf_version": version,
            "checks": ["parse", "page_count"],
            "compliance": serde_json::Value::Null,
        }),
        vec![],
        || {
            println!("valid: parses, {page_count} page(s), PDF {version}");
            println!("note: this is a parse + page-count check, not a PDF/A or veraPDF conformance check.");
        },
    )
}

/// `doctor`.
fn cmd_doctor(json: bool) -> u8 {
    let data = json!({
        "cli_version": CLI_VERSION,
        "sdk": format!("pdfluent facade (={})", pdfluent::api_version()),
        "target_os": std::env::consts::OS,
        "target_arch": std::env::consts::ARCH,
        "license": "PDFluent Commercial License (evaluation use permitted)",
        "xfa": "experimental/feature-gated",
        "binary": "pdfluent-cli (public CLI; `pdfluent` binary-name takeover is a separate milestone)",
    });
    output::success("doctor", json, data, vec![], || {
        println!("pdfluent CLI doctor");
        println!("  cli_version:  {CLI_VERSION}");
        println!(
            "  sdk:          pdfluent facade (={})",
            pdfluent::api_version()
        );
        println!("  target_os:    {}", std::env::consts::OS);
        println!("  target_arch:  {}", std::env::consts::ARCH);
        println!("  license:      PDFluent Commercial License (evaluation use permitted)");
        println!("  note:         XFA is experimental/feature-gated; binary is `pdfluent-cli`.");
    })
}

/// `doctor <INPUT>` — open a PDF and report its decode-leniency events.
fn cmd_doctor_file(input: &Path, json: bool) -> u8 {
    let doc = match open_doc("doctor", input, json) {
        Ok(d) => d,
        Err(c) => return c,
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
        json,
        json!({
            "file": input.display().to_string(),
            "leniency": {
                "clean": report.is_clean(),
                "unique_event_count": report.unique_event_count,
                "warning_count": report.warning_count,
                "critical_count": report.critical_count,
                "events": events_json,
            },
        }),
        vec![],
        || {
            println!("File: {}", input.display());
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
fn cmd_completions(shell: clap_complete::Shell) -> u8 {
    let mut cmd = Cli::command();
    let bin = cmd.get_name().to_string();
    clap_complete::generate(shell, &mut cmd, bin, &mut std::io::stdout());
    exit::OK
}

/// `xfa flatten` — scaffold stub; enforces experimental/opt-in contract.
fn cmd_xfa_flatten(policy: &str, experimental: bool) -> u8 {
    eprintln!("note: {XFA_CAVEAT}");
    match policy {
        "saved-state" => output::error(
            "xfa flatten",
            false,
            "NOT_IMPLEMENTED",
            "xfa flatten is not implemented in the public CLI yet; use the SDK or internal tooling. XFA is experimental",
        ),
        "fresh-merge" => {
            if !experimental {
                return output::error(
                    "xfa flatten",
                    false,
                    "EXPERIMENTAL_OPT_IN_REQUIRED",
                    "policy `fresh-merge` is experimental and opt-in; re-run with --experimental to acknowledge (still not implemented in the public CLI)",
                );
            }
            output::error(
                "xfa flatten",
                false,
                "NOT_IMPLEMENTED",
                "xfa flatten (fresh-merge) is experimental and not implemented in the public CLI yet",
            )
        }
        other => output::error(
            "xfa flatten",
            false,
            "BAD_POLICY",
            &format!("unknown --policy `{other}` (expected `saved-state` or `fresh-merge`)"),
        ),
    }
}
