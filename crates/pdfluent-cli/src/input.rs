//! Input handling shared by every command: files, `-` (stdin), globs and
//! directories, plus opening through the facade with mapped errors.
//!
//! Shells usually expand globs themselves, but Windows shells and quoted
//! patterns do not — so `extract` expands `*?[]` inputs on its own, with a
//! small built-in matcher rather than a new dependency.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::output::{self, Ctx};

/// What an extract input looks like before the facade opens it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputKind {
    /// A PDF (by magic bytes or `.pdf` extension): open it.
    Pdf,
    /// A format whose extractor exists in the workspace but is not exposed
    /// by the `pdfluent` facade: report, do not re-implement.
    FacadeGap {
        /// Uppercase label (`DOCX`, `HTML`, …).
        label: &'static str,
    },
    /// Anything else: attempt a PDF open and let the facade judge.
    Unknown,
}

/// Classify an input by magic bytes first, extension second.
pub(crate) fn kind_of(path: Option<&Path>, bytes: &[u8]) -> InputKind {
    if bytes.starts_with(b"%PDF-") {
        return InputKind::Pdf;
    }
    let ext = path
        .and_then(|p| p.extension())
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "pdf" => InputKind::Pdf,
        "docx" => InputKind::FacadeGap { label: "DOCX" },
        "xlsx" => InputKind::FacadeGap { label: "XLSX" },
        "pptx" => InputKind::FacadeGap { label: "PPTX" },
        "epub" => InputKind::FacadeGap { label: "EPUB" },
        "html" | "htm" => InputKind::FacadeGap { label: "HTML" },
        "md" | "markdown" => InputKind::FacadeGap { label: "Markdown" },
        "txt" | "text" => InputKind::FacadeGap { label: "text" },
        _ => InputKind::Unknown,
    }
}

/// Raw bytes plus where they came from.
pub struct LoadedInput {
    /// The bytes.
    pub bytes: Vec<u8>,
    /// What to call it in messages and JSON (`-`/stdin or the path).
    pub display: String,
    /// The file path, if it came from one.
    pub path: Option<PathBuf>,
}

/// Load one input: `-` reads stdin, anything else reads the file.
pub(crate) fn load(command: &str, ctx: &Ctx, raw: &str) -> Result<LoadedInput, u8> {
    load_pair(raw).map_err(|(code, msg)| output::error(command, ctx, &code, &msg))
}

/// Load one input, reporting failures as a `(code, message)` pair for
/// callers that emit their own error (batch workers).
pub(crate) fn load_pair(raw: &str) -> Result<LoadedInput, (String, String)> {
    if raw == "-" {
        let mut bytes = Vec::new();
        return match std::io::stdin().lock().read_to_end(&mut bytes) {
            Ok(_) => Ok(LoadedInput {
                bytes,
                display: "<stdin>".to_string(),
                path: None,
            }),
            Err(e) => Err((
                "UNREADABLE_INPUT".to_string(),
                format!("could not read stdin: {e}"),
            )),
        };
    }
    let path = PathBuf::from(raw);
    match std::fs::read(&path) {
        Ok(bytes) => Ok(LoadedInput {
            bytes,
            display: raw.to_string(),
            path: Some(path),
        }),
        Err(e) => Err(match e.kind() {
            std::io::ErrorKind::NotFound => {
                ("FILE_NOT_FOUND".to_string(), format!("no such file: {raw}"))
            }
            _ => (
                "UNREADABLE_INPUT".to_string(),
                format!("could not read {raw}: {e}"),
            ),
        }),
    }
}

/// Open loaded bytes through the facade, mapping errors to the contract.
pub(crate) fn open(
    command: &str,
    ctx: &Ctx,
    input: &LoadedInput,
) -> Result<pdfluent::PdfDocument, u8> {
    pdfluent::PdfDocument::from_bytes(&input.bytes)
        .map_err(|e| output::facade_error(command, ctx, &e))
}

/// Where one save went, and how many bytes it wrote.
pub(crate) struct SaveOutcome {
    /// The file written, or `None` for stdout.
    pub path: Option<PathBuf>,
    /// Bytes written: the file's size on disk, or the stdout byte count.
    pub bytes_written: u64,
}

/// Save a document: `-` writes bytes to stdout, anything else to the file —
/// atomically (temp file plus rename), refusing the input path itself always
/// and an existing file unless `force`.
pub(crate) fn save(
    command: &str,
    ctx: &Ctx,
    doc: &pdfluent::PdfDocument,
    out: &str,
    inputs: &[PathBuf],
    force: bool,
) -> Result<SaveOutcome, u8> {
    if out == "-" {
        if ctx.json {
            return Err(output::error(
                command,
                ctx,
                "BAD_USAGE",
                "binary PDF output and --json share stdout; pick one",
            ));
        }
        let bytes = doc
            .to_bytes()
            .map_err(|e| output::facade_error(command, ctx, &e))?;
        write_stdout(command, ctx, &bytes)?;
        return Ok(SaveOutcome {
            path: None,
            bytes_written: bytes.len() as u64,
        });
    }
    let path = PathBuf::from(out);
    guard_output(command, ctx, inputs, &path, force)?;
    let bytes_written = save_doc(command, ctx, doc, &path)?;
    Ok(SaveOutcome {
        path: Some(path),
        bytes_written,
    })
}

/// Save a document to `path` via temp file plus rename. No guards: the
/// caller checked them (upfront, so a batch fails before writing anything).
/// Returns the bytes on disk.
pub(crate) fn save_doc(
    command: &str,
    ctx: &Ctx,
    doc: &pdfluent::PdfDocument,
    path: &Path,
) -> Result<u64, u8> {
    let (tmp, file) = temp_sibling(path).map_err(|e| {
        output::error(
            command,
            ctx,
            "IO",
            &format!("could not write {}: {e}", path.display()),
        )
    })?;
    drop(file);
    if let Err(e) = doc.save_with(&tmp, pdfluent::SaveOptions::new().with_overwrite(true)) {
        let _ = std::fs::remove_file(&tmp);
        return Err(output::facade_error(command, ctx, &e));
    }
    rename_temp(command, ctx, &tmp, path)?;
    Ok(std::fs::metadata(path).map(|m| m.len()).unwrap_or(0))
}

/// Write bytes to a file: guarded (never the input, never an existing file
/// unless `force`) and atomic (temp file plus rename).
pub(crate) fn write_bytes(
    command: &str,
    ctx: &Ctx,
    bytes: &[u8],
    out: &Path,
    inputs: &[PathBuf],
    force: bool,
) -> Result<(), u8> {
    guard_output(command, ctx, inputs, out, force)?;
    write_atomic(command, ctx, out, bytes)
}

/// Write bytes to `out` via temp file plus rename. No guards: the caller
/// checked them (upfront, so a batch fails before writing anything).
pub(crate) fn write_atomic(command: &str, ctx: &Ctx, out: &Path, bytes: &[u8]) -> Result<(), u8> {
    write_atomic_pair(out, bytes).map_err(|(code, msg)| output::error(command, ctx, &code, &msg))
}

/// Write bytes to `out` via temp file plus rename, reporting failures as a
/// `(code, message)` pair for callers that emit their own error.
pub(crate) fn write_atomic_pair(out: &Path, bytes: &[u8]) -> Result<(), (String, String)> {
    let fail = |e: std::io::Error| {
        (
            "IO".to_string(),
            format!("could not write {}: {e}", out.display()),
        )
    };
    let tmp = stage_file_pair(out, bytes)?;
    if let Err(e) = std::fs::rename(&tmp, out) {
        let _ = std::fs::remove_file(&tmp);
        return Err(fail(e));
    }
    Ok(())
}

/// Write bytes to a temp sibling of `out` and return it, WITHOUT renaming.
/// The caller renames (or removes) the temp file: extract stages its
/// Markdown first and only then commits its figures, so a failed Markdown
/// write leaves no orphan figures behind.
pub(crate) fn stage_file_pair(out: &Path, bytes: &[u8]) -> Result<PathBuf, (String, String)> {
    use std::io::Write;
    let fail = |e: std::io::Error| {
        (
            "IO".to_string(),
            format!("could not write {}: {e}", out.display()),
        )
    };
    let (tmp, mut file) = temp_sibling(out).map_err(&fail)?;
    if let Err(e) = file.write_all(bytes) {
        let _ = std::fs::remove_file(&tmp);
        return Err(fail(e));
    }
    drop(file);
    Ok(tmp)
}

/// Guard one file output: a directory is never a file target, the input
/// itself is refused always (in-place rewriting would eat the source), and
/// an existing file needs `--force`.
pub(crate) fn guard_output(
    command: &str,
    ctx: &Ctx,
    inputs: &[PathBuf],
    out: &Path,
    force: bool,
) -> Result<(), u8> {
    if let Some((code, msg)) = guard_failure(inputs, out, force) {
        return Err(output::error(command, ctx, &code, &msg));
    }
    Ok(())
}

/// The refusal for one file output, if any, for callers that emit their own
/// error.
pub(crate) fn guard_failure(
    inputs: &[PathBuf],
    out: &Path,
    force: bool,
) -> Option<(String, String)> {
    if out.is_dir() {
        return Some((
            "BAD_USAGE".to_string(),
            format!("output {} is a directory; pass a file path", out.display()),
        ));
    }
    for input in inputs {
        if same_file(input, out) {
            return Some((
                "BAD_USAGE".to_string(),
                format!(
                    "output {} is the input file; refusing to rewrite it in place — choose another -o",
                    out.display()
                ),
            ));
        }
    }
    if !force && out.exists() {
        return Some((
            "BAD_USAGE".to_string(),
            format!(
                "output {} exists; pass --force to overwrite it",
                out.display()
            ),
        ));
    }
    None
}

/// True when both paths name the same file on disk. Either one missing (the
/// usual case: the output does not exist yet) is not the same file.
fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

static TEMP_SERIAL: AtomicU64 = AtomicU64::new(0);

/// Reserve an exclusively-created temp file beside `target`, so the final
/// rename never crosses filesystems. Unique per process and per call, so
/// parallel workers never share one.
fn temp_sibling(target: &Path) -> std::io::Result<(PathBuf, std::fs::File)> {
    let dir = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "out".to_string());
    for _ in 0..100 {
        let serial = TEMP_SERIAL.fetch_add(1, Ordering::Relaxed);
        let candidate = dir.join(format!(".{name}.tmp.{}-{serial}", std::process::id()));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        format!("could not pick a temp file beside {}", target.display()),
    ))
}

/// Move a finished temp file onto its target. The target never holds a
/// half-written file: before the rename it is untouched, after it is whole.
fn rename_temp(command: &str, ctx: &Ctx, tmp: &Path, target: &Path) -> Result<(), u8> {
    if let Err(e) = std::fs::rename(tmp, target) {
        let _ = std::fs::remove_file(tmp);
        return Err(output::error(
            command,
            ctx,
            "IO",
            &format!("could not write {}: {e}", target.display()),
        ));
    }
    Ok(())
}

/// A fresh staging directory inside `parent`: commands whose files are
/// written by the facade (image conversion, Markdown figures) render here,
/// then rename each finished file onto its guarded target.
pub(crate) fn stage_dir(command: &str, ctx: &Ctx, parent: &Path) -> Result<PathBuf, u8> {
    stage_dir_pair(parent).map_err(|(code, msg)| output::error(command, ctx, &code, &msg))
}

/// A fresh staging directory inside `parent`, reporting failures as a
/// `(code, message)` pair for callers that emit their own error.
pub(crate) fn stage_dir_pair(parent: &Path) -> Result<PathBuf, (String, String)> {
    for _ in 0..100 {
        let serial = TEMP_SERIAL.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(".staging.{}-{serial}", std::process::id()));
        match std::fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err((
                    "IO".to_string(),
                    format!("could not stage output in {}: {e}", parent.display()),
                ));
            }
        }
    }
    Err((
        "IO".to_string(),
        format!("could not pick a staging dir in {}", parent.display()),
    ))
}

/// Write raw bytes to stdout (binary-safe: lock, write, flush).
pub(crate) fn write_stdout(command: &str, ctx: &Ctx, bytes: &[u8]) -> Result<(), u8> {
    use std::io::Write;
    let mut lock = std::io::stdout().lock();
    lock.write_all(bytes)
        .and_then(|()| lock.flush())
        .map_err(|e| output::error(command, ctx, "IO", &format!("could not write stdout: {e}")))
}

/// Expand extract inputs: globs, directories, and plain files, in order.
pub(crate) fn expand_inputs(
    command: &str,
    ctx: &Ctx,
    raw: &[String],
    recursive: bool,
) -> Result<Vec<PathBuf>, u8> {
    let mut out = Vec::new();
    for item in raw {
        if item == "-" {
            return Err(output::error(
                command,
                ctx,
                "BAD_USAGE",
                "mixing `-` (stdin) with batch inputs is not supported; extract stdin on its own",
            ));
        }
        if is_glob(item) {
            out.extend(expand_glob(command, ctx, item)?);
            continue;
        }
        let path = PathBuf::from(item);
        if path.is_dir() {
            out.extend(collect_dir(command, ctx, &path, recursive)?);
        } else {
            // Existence is checked when the file is read, not here: `extract`
            // streams its inputs, so a missing file fails the batch where it
            // stands — with the outputs written so far kept — rather than
            // before anything is written.
            out.push(path);
        }
    }
    Ok(out)
}

fn is_glob(item: &str) -> bool {
    item.contains('*') || item.contains('?') || item.contains('[')
}

/// Expand one glob (`docs/*.pdf`, `?.pdf`) against the filesystem.
fn expand_glob(command: &str, ctx: &Ctx, pattern: &str) -> Result<Vec<PathBuf>, u8> {
    let (dir, file_pat) = match pattern.rsplit_once(['/', '\\']) {
        Some((d, f)) => (if d.is_empty() { "." } else { d }, f),
        None => (".", pattern),
    };
    let entries = std::fs::read_dir(dir).map_err(|_| {
        output::error(
            command,
            ctx,
            "FILE_NOT_FOUND",
            &format!("no matches for `{pattern}`"),
        )
    })?;
    let mut hits = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        if fnmatch(file_pat, &name) && entry.path().is_file() {
            hits.push(entry.path());
        }
    }
    if hits.is_empty() {
        return Err(output::error(
            command,
            ctx,
            "FILE_NOT_FOUND",
            &format!("no matches for `{pattern}`"),
        ));
    }
    hits.sort();
    Ok(hits)
}

/// Collect input files from a directory: every non-hidden regular file.
/// Type detection happens per file later, so an unexpected member fails the
/// batch with its own name on it rather than vanishing silently.
fn collect_dir(command: &str, ctx: &Ctx, dir: &Path, recursive: bool) -> Result<Vec<PathBuf>, u8> {
    let mut out = Vec::new();
    collect_dir_into(command, ctx, dir, recursive, 0, &mut out)?;
    out.sort();
    Ok(out)
}

fn collect_dir_into(
    command: &str,
    ctx: &Ctx,
    dir: &Path,
    recursive: bool,
    depth: usize,
    out: &mut Vec<PathBuf>,
) -> Result<(), u8> {
    let entries = std::fs::read_dir(dir).map_err(|e| {
        output::error(
            command,
            ctx,
            "UNREADABLE_INPUT",
            &format!("could not list {}: {e}", dir.display()),
        )
    })?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        // Symlinks are not followed: a linked directory loop would walk
        // forever, and a linked file is still found by naming it directly.
        if entry.file_type().map(|t| t.is_symlink()).unwrap_or(false) {
            continue;
        }
        if path.is_dir() {
            if recursive && depth < 32 {
                collect_dir_into(command, ctx, &path, recursive, depth + 1, out)?;
            }
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

/// Basename glob matching: `*`, `?`, and `[...]` classes (with `!` negation
/// and `a-z` ranges). Backslash escapes the next character.
pub(crate) fn fnmatch(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    match_at(&p, 0, &n, 0)
}

/// Whether `p[pi..]` matches `n[ni..]` in full.
fn match_at(p: &[char], mut pi: usize, n: &[char], mut ni: usize) -> bool {
    while pi < p.len() {
        match p[pi] {
            '*' => {
                while pi < p.len() && p[pi] == '*' {
                    pi += 1;
                }
                if pi == p.len() {
                    return true;
                }
                for k in ni..=n.len() {
                    if match_at(p, pi, n, k) {
                        return true;
                    }
                }
                return false;
            }
            '?' => {
                if ni >= n.len() {
                    return false;
                }
                pi += 1;
                ni += 1;
            }
            '[' => {
                if ni >= n.len() {
                    return false;
                }
                let Some((hit, next)) = match_class_at(p, pi, n[ni]) else {
                    return false; // unterminated class never matches
                };
                if !hit {
                    return false;
                }
                pi = next;
                ni += 1;
            }
            '\\' => {
                pi += 1;
                if pi >= p.len() {
                    return ni == n.len();
                }
                if ni >= n.len() || n[ni] != p[pi] {
                    return false;
                }
                pi += 1;
                ni += 1;
            }
            lit => {
                if ni >= n.len() || n[ni] != lit {
                    return false;
                }
                pi += 1;
                ni += 1;
            }
        }
    }
    ni == n.len()
}

/// Match one character against the class opening at `p[pi] == '['`.
/// Returns whether it hit and the index just past the closing `]`.
fn match_class_at(p: &[char], pi: usize, c: char) -> Option<(bool, usize)> {
    let mut i = pi + 1;
    let mut negate = false;
    if p.get(i) == Some(&'!') || p.get(i) == Some(&'^') {
        negate = true;
        i += 1;
    }
    let mut hit = false;
    let mut first = true;
    loop {
        let lo = *p.get(i)?;
        i += 1;
        if lo == ']' && !first {
            break;
        }
        first = false;
        if p.get(i) == Some(&'-') && p.get(i + 1).is_some_and(|hi| *hi != ']') {
            let hi = p[i + 1];
            i += 2;
            if lo <= c && c <= hi {
                hit = true;
            }
        } else {
            if c == lo {
                hit = true;
            }
            if p.get(i) == Some(&'-') {
                // A `-` that opens no range is literal.
                i += 1;
                if c == '-' {
                    hit = true;
                }
            }
        }
    }
    Some((hit != negate, i))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_stars_and_questions() {
        assert!(fnmatch("*.pdf", "a.pdf"));
        assert!(fnmatch("*.pdf", "a.PDF".to_lowercase().as_str()));
        assert!(!fnmatch("*.pdf", "a.pdfa"));
        assert!(!fnmatch("*.pdf", "pdf"));
        assert!(fnmatch("?.pdf", "a.pdf"));
        assert!(!fnmatch("?.pdf", "ab.pdf"));
        assert!(fnmatch("*", "anything"));
        assert!(fnmatch("a*b*c", "axbyc"));
        assert!(!fnmatch("a*b*c", "axby"));
    }

    #[test]
    fn glob_classes() {
        assert!(fnmatch("[abc].pdf", "b.pdf"));
        assert!(!fnmatch("[abc].pdf", "d.pdf"));
        assert!(fnmatch("[a-c].pdf", "b.pdf"));
        assert!(fnmatch("[!a-c].pdf", "d.pdf"));
        assert!(!fnmatch("[!a-c].pdf", "b.pdf"));
        assert!(!fnmatch("[abc", "a"));
    }

    #[test]
    fn kind_prefers_magic_over_extension() {
        assert_eq!(kind_of(None, b"%PDF-1.7..."), InputKind::Pdf);
        assert_eq!(
            kind_of(Some(Path::new("a.docx")), b"%PDF-1.7..."),
            InputKind::Pdf
        );
        assert_eq!(
            kind_of(Some(Path::new("a.DOCX")), b"PK\x03\x04..."),
            InputKind::FacadeGap { label: "DOCX" }
        );
        assert_eq!(
            kind_of(Some(Path::new("a.md")), b"# hi"),
            InputKind::FacadeGap { label: "Markdown" }
        );
        assert_eq!(kind_of(Some(Path::new("a.pdf")), b"junk"), InputKind::Pdf);
        assert_eq!(
            kind_of(Some(Path::new("a.bin")), b"junk"),
            InputKind::Unknown
        );
    }
}
