//! PDFluent public command-line interface (scaffold).
//!
//! This is the public-facing CLI built on the `pdfluent` SDK facade. It exposes
//! a deliberately small, release-aligned surface. XFA support is **experimental
//! and feature-gated** — there is no Adobe Reader parity claim, and the
//! `fresh-merge` policy is opt-in and never the default.
//!
//! Internal/dev/corpus tooling (measure, debug-xfa, flatten-check, demo,
//! collectors) lives in the separate `xfa-cli` crate and is intentionally NOT
//! exposed here.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{CommandFactory, Parser, Subcommand};

/// Stable CLI version string (RC line).
const CLI_VERSION: &str = "1.0.0-beta.8";

/// The XFA caveat reused wherever XFA is mentioned.
const XFA_CAVEAT: &str =
    "XFA support is EXPERIMENTAL and feature-gated — not production-supported, \
no Adobe Reader parity claim. The `fresh-merge` policy is opt-in and never the default.";

/// Exit-code contract (see docs/cli). 0 success; non-zero mapped by class.
mod exit {
    pub const OK: u8 = 0;
    pub const USAGE: u8 = 2;
    pub const IO: u8 = 3;
    pub const INVALID_PDF: u8 = 4;
    pub const UNSUPPORTED: u8 = 6; // experimental/not-yet-implemented or opt-in required
}

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
    /// Show document information (page count, file size).
    Info {
        /// Input PDF file.
        input: PathBuf,
        /// Emit a single JSON object on stdout.
        #[arg(long)]
        json: bool,
    },
    /// Inspect a document and emit JSON (alias of `info --json`).
    Inspect {
        /// Input PDF file.
        input: PathBuf,
    },
    /// Extract text from a PDF (not yet implemented in this scaffold).
    ExtractText {
        /// Input PDF file.
        input: PathBuf,
        /// Write extracted text to this path instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Validate a PDF against a compliance profile (not yet implemented in this scaffold).
    Validate {
        /// Input PDF file.
        input: PathBuf,
        /// Emit a single JSON object on stdout.
        #[arg(long)]
        json: bool,
    },
    /// Check environment / installation and print build info.
    Doctor,
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
    /// Flatten an XFA/AcroForm document (not yet implemented in this scaffold).
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
        Commands::Inspect { input } => cmd_info(&input, true),
        Commands::ExtractText { .. } => not_implemented("extract-text"),
        Commands::Validate { .. } => not_implemented("validate"),
        Commands::Doctor => cmd_doctor(),
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

/// `info` / `inspect`: real implementation via the public `pdfluent` facade.
fn cmd_info(input: &std::path::Path, json: bool) -> u8 {
    if !input.exists() {
        return fail(
            exit::IO,
            "info",
            "FILE_NOT_FOUND",
            &format!("no such file: {}", input.display()),
        );
    }
    let file_size = std::fs::metadata(input).map(|m| m.len()).unwrap_or(0);
    let doc = match pdfluent::PdfDocument::open(input) {
        Ok(d) => d,
        Err(e) => {
            return fail(
                exit::INVALID_PDF,
                "info",
                "INVALID_PDF",
                &format!("could not open PDF: {e}"),
            );
        }
    };
    let page_count = doc.page_count();
    if json {
        let env = serde_json::json!({
            "ok": true,
            "command": "info",
            "version": CLI_VERSION,
            "data": { "path": input.display().to_string(), "file_size": file_size, "page_count": page_count },
            "error": serde_json::Value::Null,
        });
        println!("{}", serde_json::to_string_pretty(&env).unwrap());
    } else {
        println!("File:       {}", input.display());
        println!("Size:       {file_size} bytes");
        println!("Pages:      {page_count}");
    }
    exit::OK
}

/// `doctor`: environment / build info. Always exits 0.
fn cmd_doctor() -> u8 {
    println!("pdfluent CLI doctor");
    println!("  cli_version:  {CLI_VERSION}");
    println!("  sdk:          pdfluent facade (=1.0.0-beta.8)");
    println!("  target_os:    {}", std::env::consts::OS);
    println!("  target_arch:  {}", std::env::consts::ARCH);
    println!("  license:      PDFluent Commercial License (evaluation use permitted)");
    println!("  note:         XFA is experimental/feature-gated; this is a scaffold CLI.");
    exit::OK
}

/// `completions`: emit shell completions for the `pdfluent` command.
fn cmd_completions(shell: clap_complete::Shell) -> u8 {
    let mut cmd = Cli::command();
    let bin = cmd.get_name().to_string();
    clap_complete::generate(shell, &mut cmd, bin, &mut std::io::stdout());
    exit::OK
}

/// `xfa flatten`: scaffold stub. Enforces the experimental/opt-in contract.
fn cmd_xfa_flatten(policy: &str, experimental: bool) -> u8 {
    eprintln!("note: {XFA_CAVEAT}");
    match policy {
        "saved-state" => not_implemented("xfa flatten (saved-state)"),
        "fresh-merge" => {
            if !experimental {
                return fail(
                    exit::UNSUPPORTED,
                    "xfa flatten",
                    "EXPERIMENTAL_OPT_IN_REQUIRED",
                    "policy `fresh-merge` is experimental and opt-in; re-run with --experimental to acknowledge",
                );
            }
            not_implemented("xfa flatten (fresh-merge, experimental)")
        }
        other => fail(
            exit::USAGE,
            "xfa flatten",
            "BAD_POLICY",
            &format!("unknown --policy `{other}` (expected `saved-state` or `fresh-merge`)"),
        ),
    }
}

/// Uniform "not implemented in this scaffold" path.
fn not_implemented(what: &str) -> u8 {
    fail(
        exit::UNSUPPORTED,
        what,
        "NOT_IMPLEMENTED",
        "not yet implemented in this CLI scaffold (see the PDFluent CLI implementation roadmap)",
    )
}

/// Print a structured error to stderr and return the exit code.
fn fail(code: u8, command: &str, err_code: &str, message: &str) -> u8 {
    let _ = writeln!(
        std::io::stderr(),
        "[{err_code}] {message} — Docs: https://docs.pdfluent.dev/cli/errors/{}",
        err_code.to_lowercase()
    );
    let _ = command; // command name reserved for future structured JSON error output
    code
}
