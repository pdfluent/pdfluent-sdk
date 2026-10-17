//! PDFluent Rust SDK — golden-path example.
//!
//! Pinned to `pdfluent 1.0.0-beta.8` (workspace crate during development).
//!
//! Demonstrates the canonical SDK lifecycle:
//!
//! 1. Open a PDF from disk.
//! 2. Read page count + metadata.
//! 3. Extract text from the first page.
//! 4. Typed error handling using the `Error` enum (no string matching).
//!
//! There is no licence step. The crate has no key, no activation call and no
//! tier: every feature works in every build (#226). The licence you accept is
//! AGPL-3.0 or the commercial one, and neither is checked at runtime.
//!
//! Build:
//!     cargo build
//!
//! Run:
//!     cargo run -- path/to/file.pdf
//!     cargo run -- ../../tests/corpus-mini/multi-page.pdf
//!
//! Without an argument the example falls back to the in-repo fixture
//! `../../tests/corpus-mini/multi-page.pdf` so that `cargo run` works
//! out of the box during development.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use pdfluent::{Error, PdfDocument, ResourceLimitKind};

/// Default fixture used when the binary is run without arguments.
const DEFAULT_FIXTURE: &str = "../../tests/corpus-mini/multi-page.pdf";

fn main() -> ExitCode {
    // ── 1. Resolve the input path. ───────────────────────────────────────
    let arg = std::env::args().nth(1);
    let path: PathBuf = arg
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_FIXTURE));

    match run(&path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{}", format_error(&err));
            // Exit code reflects the error class so shell scripts can
            // distinguish "file missing" from "corrupt PDF".
            ExitCode::from(exit_code_for(&err))
        }
    }
}

fn run(path: &Path) -> pdfluent::Result<()> {
    println!("Opening : {}", path.display());

    // ── 2. Open the PDF. ─────────────────────────────────────────────────
    let doc = PdfDocument::open(path)?;

    println!("Pages   : {}", doc.page_count());
    println!("Version : {:?}", doc.version());

    // ── 3. Metadata. ─────────────────────────────────────────────────────
    let meta = doc.metadata();
    println!(
        "Title   : {}",
        meta.title.as_deref().unwrap_or("(none)")
    );
    println!(
        "Author  : {}",
        meta.author.as_deref().unwrap_or("(none)")
    );
    println!(
        "Producer: {}",
        meta.producer.as_deref().unwrap_or("(none)")
    );

    // ── 4. Text extraction (first 200 chars of the document). ────────────
    let text = doc.extract_text()?;
    let preview: String = text.chars().take(200).collect();
    let suffix = if text.chars().count() > 200 { "…" } else { "" };
    println!("Text    : {preview}{suffix}");

    Ok(())
}

/// Format an error using typed variants — no `to_string()` pattern matching.
fn format_error(err: &Error) -> String {
    match err {
        Error::FileNotFound { path } => {
            format!("file not found: {}", path.display())
        }
        Error::Io { source, path } => match path {
            Some(p) => format!("I/O error on {}: {source}", p.display()),
            None => format!("I/O error: {source}"),
        },
        Error::InvalidPdf {
            byte_offset,
            reason,
        } => match byte_offset {
            Some(off) => format!("invalid PDF at byte {off}: {reason}"),
            None => format!("invalid PDF: {reason}"),
        },
        Error::UnsupportedPdfVersion {
            found,
            supported_up_to,
        } => {
            format!("unsupported PDF version {found} (max supported {supported_up_to})")
        }
        Error::DecryptionFailed { reason } => {
            format!("decryption failed: {reason:?}")
        }
        Error::ResourceLimitExceeded {
            kind,
            observed,
            limit,
        } => match kind {
            ResourceLimitKind::FileTooLarge => {
                format!("file too large: {observed} bytes (limit {limit})")
            }
            _ => format!("resource limit exceeded ({kind}): {observed} > {limit}"),
        },
        other => format!("{other}"),
    }
}

/// Map an error to a stable process exit code.
fn exit_code_for(err: &Error) -> u8 {
    match err {
        Error::FileNotFound { .. } => 10,
        Error::Io { .. } => 11,
        Error::InvalidPdf { .. } => 12,
        Error::UnsupportedPdfVersion { .. } => 13,
        Error::DecryptionFailed { .. } => 14,
        Error::ResourceLimitExceeded { .. } => 17,
        _ => 1,
    }
}
