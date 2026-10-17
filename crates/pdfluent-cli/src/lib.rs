//! PDFluent public command-line interface.
//!
//! A thin layer over the `pdfluent` SDK facade: every command maps to public
//! facade calls, and anything the facade does not expose is reported as a
//! facade gap rather than re-implemented here.
//!
//! Ships as two binaries from one entry point: `pdfluent` and the
//! compatibility alias `pdfluent-cli` (see `src/main.rs` and
//! `src/bin/pdfluent-cli.rs`).
//!
//! XFA support is **experimental and feature-gated** — no Adobe Reader parity
//! claim. Internal/dev/corpus tooling lives in the separate `xfa-cli` crate
//! (binary `xfa-runner`) and is intentionally NOT exposed here.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

mod cmd_compress;
mod cmd_convert;
mod cmd_extract;
mod cmd_legacy;
mod cmd_manip;
mod cmd_models;
mod cmd_protect;
mod cmd_redact;
mod cmd_sign;
mod cmd_validate;
mod cmd_watermark;
mod imagedims;
mod input;
mod manpages;
mod output;
mod pagespec;
mod password;

use std::process::ExitCode;

use clap::{ArgAction, CommandFactory, Parser, Subcommand};

use output::{Ctx, CLI_VERSION};

#[derive(Parser)]
#[command(
    name = "pdfluent",
    version = CLI_VERSION,
    about = "PDFluent — PDF toolkit: extract, merge, split, compress, protect, watermark, convert, redact, sign, validate. AGPL-3.0 or a commercial licence, at your option.",
    long_about = None,
    propagate_version = true
)]
struct Cli {
    /// Machine-readable JSON envelope on stdout.
    #[arg(long, global = true)]
    json: bool,

    /// Quiet: human mode prints data only (no warnings or progress notes).
    #[arg(short = 'q', long, global = true)]
    quiet: bool,

    /// Verbose stderr diagnostics (repeat for more).
    #[arg(short = 'v', long, global = true, action = ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Extract a document as Markdown, JSON, chunks, or text.
    Extract(cmd_extract::ExtractArgs),
    /// Combine several PDFs into one, in the order given.
    Merge(cmd_manip::MergeArgs),
    /// Divide a document into separate files.
    Split(cmd_manip::SplitArgs),
    /// Rotate, delete, reorder, or extract pages.
    Pages(cmd_manip::PagesArgs),
    /// Reduce file size.
    Compress(cmd_compress::CompressArgs),
    /// Set a password and restrict permissions.
    Protect(cmd_protect::ProtectArgs),
    /// Remove password protection.
    Unprotect(cmd_protect::UnprotectArgs),
    /// Add a text or image watermark across all pages.
    Watermark(cmd_watermark::WatermarkArgs),
    /// Convert to Word, Excel, PowerPoint, image, or PDF/A.
    Convert(cmd_convert::ConvertArgs),
    /// Permanently remove text or regions, not just cover them.
    Redact(cmd_redact::RedactArgs),
    /// Apply a PAdES signature with a PKCS#12 identity.
    Sign(cmd_sign::SignArgs),
    /// Validate: parse check, PDF/A conformance, or (unexposed) PDF/UA.
    Validate(cmd_validate::ValidateArgs),
    /// Show document information (page count, PDF version, file size).
    Info(cmd_legacy::InfoArgs),
    /// Inspect a document and emit JSON (alias of `info --json`).
    Inspect(cmd_legacy::InfoArgs),
    /// Extract plain text (kept for compatibility; prefer `extract`).
    ExtractText(cmd_legacy::ExtractTextArgs),
    /// Check environment / installation and print build info.
    Doctor(cmd_legacy::DoctorArgs),
    /// Install and inspect the opt-in model tiers' files.
    Models(cmd_models::ModelsArgs),
    /// Generate shell completions.
    Completions(cmd_legacy::CompletionsArgs),
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
    Flatten(cmd_legacy::XfaFlattenArgs),
}

/// The clap command tree, for completions and (in tests) help snapshots.
pub(crate) fn cli_command() -> clap::Command {
    Cli::command()
}

/// Parse argv and run the command. Shared by both binaries.
/// Deliberately exercised end to end by spawning the binary in `tests/`,
/// which the call-graph reachability check cannot see by construction.
pub fn run_cli() -> ExitCode {
    // Internal docs generator, deliberately outside the clap tree: handled
    // here so it never appears in `--help`, completions, or the pages it
    // writes. `docs/cli/man/*.1` are its output (see `tests/cli_man.rs`).
    let mut argv = std::env::args_os();
    argv.next();
    if argv.next().as_deref() == Some(std::ffi::OsStr::new("generate-man-pages")) {
        let rest: Vec<std::ffi::OsString> = argv.collect();
        return ExitCode::from(manpages::run(&rest));
    }
    // Usage errors exit 1 here, not clap's default 2: the CLI contract is
    // 0 ok / 1 usage / 2 input error / 3 processing error. Help and version
    // still exit 0.
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            use clap::error::ErrorKind;
            use std::io::Write;
            let code = match e.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => 0,
                _ => 1,
            };
            let _ = e.print();
            // `process::exit` runs no destructors: without an explicit flush
            // the printed help/error is lost whenever stdout is a pipe.
            let _ = std::io::stdout().flush();
            let _ = std::io::stderr().flush();
            std::process::exit(code);
        }
    };
    let ctx = Ctx {
        json: cli.json,
        quiet: cli.quiet,
        verbose: cli.verbose,
    };
    let code = match &cli.command {
        Commands::Extract(args) => cmd_extract::run(&ctx, args),
        Commands::Merge(args) => cmd_manip::run_merge(&ctx, args),
        Commands::Split(args) => cmd_manip::run_split(&ctx, args),
        Commands::Pages(args) => cmd_manip::run_pages(&ctx, args),
        Commands::Compress(args) => cmd_compress::run(&ctx, args),
        Commands::Protect(args) => cmd_protect::run_protect(&ctx, args),
        Commands::Unprotect(args) => cmd_protect::run_unprotect(&ctx, args),
        Commands::Watermark(args) => cmd_watermark::run(&ctx, args),
        Commands::Convert(args) => cmd_convert::run(&ctx, args),
        Commands::Redact(args) => cmd_redact::run(&ctx, args),
        Commands::Sign(args) => cmd_sign::run(&ctx, args),
        Commands::Validate(args) => cmd_validate::run(&ctx, args),
        Commands::Info(args) => cmd_legacy::run_info(&ctx, args),
        // `inspect` always emits JSON; the global --json flag is accepted
        // for compatibility and changes nothing.
        Commands::Inspect(args) => {
            let json_ctx = Ctx { json: true, ..ctx };
            cmd_legacy::run_info(&json_ctx, args)
        }
        Commands::ExtractText(args) => cmd_legacy::run_extract_text(&ctx, args),
        Commands::Doctor(args) => cmd_legacy::run_doctor(&ctx, args),
        Commands::Models(args) => cmd_models::run(&ctx, args),
        Commands::Completions(args) => cmd_legacy::run_completions(args),
        Commands::Xfa { command } => match command {
            XfaCommands::Flatten(args) => cmd_legacy::run_xfa_flatten(&ctx, args),
        },
    };
    ExitCode::from(code)
}
