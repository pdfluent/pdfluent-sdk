//! Shared CLI output + exit-code contract.
//!
//! Every `--json` output uses a stable envelope; every error maps to a
//! documented exit code. Human output is concise; JSON output is machine-stable.
//! Data goes to stdout, diagnostics to stderr, in both modes.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use serde_json::{json, Value};

/// Stable CLI version string.
pub const CLI_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Exit-code contract (see `docs/en/cli.md`): 0 ok, 1 usage, 2 input error,
/// 3 processing error.
pub mod exit {
    /// Success.
    pub const OK: u8 = 0;
    /// Usage error: unknown command or flag, missing required argument,
    /// malformed flag value, mutually exclusive options.
    pub const USAGE: u8 = 1;
    /// Input error: missing or unreadable input, corrupt PDF, wrong or
    /// missing password, unsupported input type, page range the document
    /// cannot satisfy.
    pub const INPUT: u8 = 2;
    /// Processing error: the request was well-formed and the input loaded,
    /// but the work failed — write errors, conversion failures, facade
    /// errors, and capabilities the facade does not expose yet.
    pub const PROCESSING: u8 = 3;
}

/// Per-invocation output context from the global flags.
#[derive(Debug, Clone, Copy)]
pub struct Ctx {
    /// `--json`: machine-readable envelope on stdout.
    pub json: bool,
    /// `-q`: human mode prints data only — no warnings, no progress notes.
    /// JSON output is unaffected: warnings there are data.
    pub quiet: bool,
    /// `-v`: stderr diagnostics. Higher counts add detail.
    pub verbose: u8,
}

/// Map a stable error code string to its exit code.
pub(crate) fn exit_for(code: &str) -> u8 {
    match code {
        "BAD_USAGE" | "BAD_POLICY" | "EXPERIMENTAL_OPT_IN_REQUIRED" | "UNKNOWN_COMMAND" => {
            exit::USAGE
        }
        "FILE_NOT_FOUND"
        | "UNREADABLE_INPUT"
        | "INVALID_PDF"
        | "UNSUPPORTED_VERSION"
        | "PASSWORD_REQUIRED"
        | "BAD_PASSWORD"
        | "PASSWORD_MISMATCH"
        | "INVALID_IDENTITY"
        | "UNSUPPORTED_INPUT"
        | "PAGE_RANGE"
        | "EMPTY_RESULT"
        | "NOT_COMPLIANT" => exit::INPUT,
        _ => exit::PROCESSING,
    }
}

/// Print a success result. In `--json` mode emits the stable envelope,
/// otherwise runs `human` to print concise output. Returns `exit::OK`.
pub(crate) fn success(
    command: &str,
    ctx: &Ctx,
    data: Value,
    warnings: Vec<String>,
    human: impl FnOnce(&Ctx),
) -> u8 {
    if ctx.json {
        let env = json!({
            "ok": true,
            "command": command,
            "version": CLI_VERSION,
            "data": data,
            "warnings": warnings,
        });
        // Serialising a `Value` to a `String` cannot fail; the fallback
        // keeps a theoretical fmt error from panicking the CLI.
        match serde_json::to_string_pretty(&env) {
            Ok(text) => println!("{text}"),
            Err(e) => eprintln!("[FAILED] could not serialise JSON output: {e}"),
        }
    } else {
        if !ctx.quiet {
            for w in &warnings {
                eprintln!("warning: {w}");
            }
        }
        human(ctx);
    }
    exit::OK
}

/// Emit an error. In `--json` mode emits the stable error envelope to stdout;
/// otherwise a concise `[CODE] message` line to stderr. Returns the exit code.
pub(crate) fn error(command: &str, ctx: &Ctx, code: &str, message: &str) -> u8 {
    error_with_data(command, ctx, code, message, None, || {})
}

/// Like [`error`], but carries machine-readable `data` in the JSON envelope
/// (additive: readers that ignore it see the standard shape) and runs
/// `human_detail` after the human error line. For verdicts — a document that
/// is fine as bytes but fails the asked standard still has findings worth
/// automating on.
pub(crate) fn error_with_data(
    command: &str,
    ctx: &Ctx,
    code: &str,
    message: &str,
    data: Option<Value>,
    human_detail: impl FnOnce(),
) -> u8 {
    let exit_code = exit_for(code);
    if ctx.json {
        let mut env = json!({
            "ok": false,
            "command": command,
            "version": CLI_VERSION,
            "error": { "code": code, "message": message, "exit_code": exit_code },
            "warnings": [],
        });
        if let Some(data) = data {
            env["data"] = data;
        }
        match serde_json::to_string_pretty(&env) {
            Ok(text) => println!("{text}"),
            Err(_) => eprintln!("[{code}] {message}"),
        }
    } else {
        error_line(code, message);
        human_detail();
    }
    exit_code
}

/// One human-mode error line. Batch commands print one per failed input;
/// [`error_with_data`] prints the first and runs the rest as detail.
pub(crate) fn error_line(code: &str, message: &str) {
    eprintln!("[{code}] {message} — Docs: {}", error_docs_url(code));
}

/// The error reference page; each CLI code has the anchor `cli-<code>` there,
/// the code lowercased with its underscores kept.
const ERROR_DOCS_BASE: &str = "https://pdfluent.com/errors/#cli-";

/// Deep link to the documentation anchor for one CLI error code.
pub(crate) fn error_docs_url(code: &str) -> String {
    format!("{ERROR_DOCS_BASE}{}", code.to_lowercase())
}

/// A human-mode progress note ("Wrote …"). Silent under `-q` and in `--json`
/// mode, where the envelope carries the same facts as data.
pub(crate) fn note(ctx: &Ctx, message: &str) {
    if !ctx.quiet && !ctx.json {
        println!("{message}");
    }
}

/// A stderr diagnostic, printed only under `-v`.
pub(crate) fn verbose(ctx: &Ctx, message: &str) {
    if ctx.verbose >= 1 {
        eprintln!("{message}");
    }
}

/// Map a facade [`pdfluent::Error`] to a CLI error. Input-shaped failures
/// (missing file, corrupt PDF, password, page range) exit 2; everything else
/// exits 3 with the facade's own message.
pub(crate) fn facade_error(command: &str, ctx: &Ctx, err: &pdfluent::Error) -> u8 {
    use pdfluent::Error as E;
    // A model failure carries its own stable code (`MODEL_NOT_AVAILABLE` and
    // its siblings), not a generic I/O one; the message names the install.
    if let Some(model) = err.model_error() {
        return error(command, ctx, model.kind.code(), &err.to_string());
    }
    let (code, message) = match err {
        E::FileNotFound { path } => (
            "FILE_NOT_FOUND",
            format!("no such file: {}", path.display()),
        ),
        E::InvalidPdf { reason, .. } => ("INVALID_PDF", format!("could not open PDF: {reason}")),
        E::UnsupportedPdfVersion {
            found,
            supported_up_to,
        } => (
            "UNSUPPORTED_VERSION",
            format!("PDF version {found} is newer than this build supports ({supported_up_to})"),
        ),
        E::DecryptionFailed { reason } => match reason {
            pdfluent::error::DecryptionFailureReason::PasswordRequired => (
                "PASSWORD_REQUIRED",
                "this document requires a password".to_string(),
            ),
            pdfluent::error::DecryptionFailureReason::WrongPassword => {
                ("BAD_PASSWORD", "wrong password".to_string())
            }
            other => ("FAILED", format!("decryption failed: {other:?}")),
        },
        E::PageOutOfRange {
            requested,
            page_count,
        } => (
            "PAGE_RANGE",
            format!(
                "page {requested} is outside this document (1–{page_count}; pages count from 1)"
            ),
        ),
        E::PageRangeInvalid {
            start,
            end,
            page_count,
        } => (
            "PAGE_RANGE",
            format!("page range {start}–{end} cannot be satisfied by {page_count} page(s)"),
        ),
        E::PdfaValidationFailed { .. } => (
            "NOT_COMPLIANT",
            "document is not compliant with the requested profile".to_string(),
        ),
        E::Io { source, path } => (
            "IO",
            match path {
                Some(p) => format!("I/O error on {}: {source}", p.display()),
                None => format!("I/O error: {source}"),
            },
        ),
        other => ("FAILED", other.to_string()),
    };
    error(command, ctx, code, &message)
}

#[cfg(test)]
mod tests {
    use super::error_docs_url;

    #[test]
    fn error_docs_url_is_the_cli_anchor_of_the_code() {
        assert_eq!(
            error_docs_url("BAD_USAGE"),
            "https://pdfluent.com/errors/#cli-bad_usage"
        );
    }
}
