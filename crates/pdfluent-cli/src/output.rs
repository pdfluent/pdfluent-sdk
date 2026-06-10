//! Shared CLI output + exit-code contract.
//!
//! Every `--json` output uses a stable envelope; every error maps to a
//! documented exit code. Human output is concise; JSON output is machine-stable.

use serde_json::{json, Value};

/// Stable CLI version string (RC line).
pub const CLI_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Exit-code contract (see `docs/en/cli.md`).
pub mod exit {
    pub const OK: u8 = 0;
    pub const GENERIC: u8 = 1;
    pub const USAGE: u8 = 2;
    pub const IO: u8 = 3;
    pub const INVALID_PDF: u8 = 4;
    pub const UNSUPPORTED: u8 = 5;
    pub const NOT_IMPLEMENTED: u8 = 6;
    pub const LICENSE: u8 = 7;
    pub const INTERNAL: u8 = 70;
}

/// Map a stable error code string to its exit code.
pub fn exit_for(code: &str) -> u8 {
    match code {
        "FILE_NOT_FOUND" | "IO" => exit::IO,
        "INVALID_PDF" => exit::INVALID_PDF,
        "BAD_USAGE" | "BAD_POLICY" => exit::USAGE,
        "UNSUPPORTED" | "EXPERIMENTAL_OPT_IN_REQUIRED" => exit::UNSUPPORTED,
        "NOT_IMPLEMENTED" => exit::NOT_IMPLEMENTED,
        "LICENSE" => exit::LICENSE,
        "INTERNAL" => exit::INTERNAL,
        _ => exit::GENERIC,
    }
}

/// Print a success result. In `--json` mode emits the stable envelope; otherwise
/// runs `human` to print concise human output. Returns `exit::OK`.
pub fn success(
    command: &str,
    json: bool,
    data: Value,
    warnings: Vec<String>,
    human: impl FnOnce(),
) -> u8 {
    if json {
        let env = json!({
            "ok": true,
            "command": command,
            "version": CLI_VERSION,
            "data": data,
            "warnings": warnings,
        });
        println!("{}", serde_json::to_string_pretty(&env).unwrap());
    } else {
        for w in &warnings {
            eprintln!("warning: {w}");
        }
        human();
    }
    exit::OK
}

/// Emit an error. In `--json` mode emits the stable error envelope to stdout;
/// otherwise a concise `[CODE] message` line to stderr. Returns the exit code.
pub fn error(command: &str, json: bool, code: &str, message: &str) -> u8 {
    let exit_code = exit_for(code);
    if json {
        let env = json!({
            "ok": false,
            "command": command,
            "version": CLI_VERSION,
            "error": { "code": code, "message": message, "exit_code": exit_code },
            "warnings": [],
        });
        println!("{}", serde_json::to_string_pretty(&env).unwrap());
    } else {
        eprintln!(
            "[{code}] {message} — Docs: https://docs.pdfluent.dev/cli/errors/{}",
            code.to_lowercase()
        );
    }
    exit_code
}
