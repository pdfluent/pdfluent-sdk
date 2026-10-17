//! Shared CLI test helpers: binary runners, temp dirs, in-memory fixtures.
//!
//! Fixtures are built in memory (never committed as binaries) or reuse the
//! repository's existing ones by relative path. Every helper keeps tests
//! hermetic under parallel execution.
//!
//! Each integration test compiles this module separately and uses a subset;
//! the allow keeps the shared helpers warning-free in every consumer.
#![allow(dead_code)]

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

/// The primary binary under test.
pub fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_pdfluent"))
}

/// The compatibility alias binary.
pub fn alias_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_pdfluent-cli"))
}

/// Run the CLI with argv, no stdin, no extra env.
pub fn run(args: &[&str]) -> Output {
    Command::new(bin())
        .args(args)
        .output()
        .expect("spawn pdfluent")
}

/// Run the CLI with piped stdin bytes.
pub fn run_with_stdin(args: &[&str], stdin_bytes: &[u8]) -> Output {
    let mut child = Command::new(bin())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pdfluent");
    use std::io::Write;
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(stdin_bytes)
        .expect("write stdin");
    child.wait_with_output().expect("wait")
}

/// Run the CLI with extra environment variables.
pub fn run_with_env(args: &[&str], env: &[(&str, &str)]) -> Output {
    Command::new(bin())
        .args(args)
        .envs(env.iter().copied())
        .output()
        .expect("spawn pdfluent")
}

/// Run the CLI with argv and a working directory.
pub fn run_in(cwd: &Path, args: &[&str]) -> Output {
    Command::new(bin())
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("spawn pdfluent")
}

/// Run the CLI with extra environment variables, failing after `timeout`:
/// a hang is a failure, never a wait. The child is killed first; the `Err`
/// describes the timeout.
pub fn run_with_env_timeout(
    args: &[&str],
    env: &[(&str, &str)],
    timeout: std::time::Duration,
) -> Result<Output, String> {
    let mut child = Command::new(bin())
        .args(args)
        .envs(env.iter().copied())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pdfluent");
    let started = std::time::Instant::now();
    loop {
        match child.try_wait().expect("poll pdfluent") {
            Some(_) => return Ok(child.wait_with_output().expect("reap pdfluent")),
            None => {
                if started.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "{} timed out after {:.0}s; a hang is a failure, never retried",
                        args.join(" "),
                        timeout.as_secs_f64()
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }
}

pub fn stdout_text(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

pub fn stderr_text(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

pub fn stdout_json(out: &Output) -> serde_json::Value {
    serde_json::from_slice(&out.stdout).expect("stdout is a single JSON document")
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A fresh temp directory, unique per call (parallel-safe).
pub fn tempdir(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "pdfluent-cli-tests-{}-{}-{tag}-{n}",
        std::process::id(),
        // Truncated nanos keep names short while staying unique with pid+n.
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0),
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// The facade's shared sample. Tracked in the repo, so a missing file is a
/// broken checkout and the test FAILS — never a skip that reports green.
pub fn sample_pdf_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../pdfluent/tests/fixtures/sample.pdf")
}

/// The facade's shared sample, when the fixture tree is checked out.
pub fn sample_pdf() -> Option<PathBuf> {
    let path = sample_pdf_path();
    path.exists().then_some(path)
}

/// Require the sample, failing when the fixture is absent.
#[macro_export]
macro_rules! require_sample {
    () => {
        match $crate::common::sample_pdf() {
            Some(path) => path,
            None => panic!(
                "fixture missing: {}",
                $crate::common::sample_pdf_path().display()
            ),
        }
    };
}

/// The pdf-sign RSA fixture identity + its password. Tracked in the repo,
/// like the sample: a missing file is a broken checkout and the test FAILS.
/// Deliberately not the `skip_test!` path — nothing about this identity is
/// legitimately optional (it is a committed file, usable on every platform).
pub fn p12_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../pdf-sign/tests/fixtures/test-rsa.p12")
}

/// The pdf-sign RSA fixture identity + its password.
pub fn p12_fixture() -> Option<(PathBuf, &'static str)> {
    let path = p12_fixture_path();
    path.exists().then_some((path, "test123"))
}

#[macro_export]
macro_rules! require_p12 {
    () => {
        match $crate::common::p12_fixture() {
            Some(fixture) => fixture,
            None => panic!(
                "fixture missing: {}",
                $crate::common::p12_fixture_path().display()
            ),
        }
    };
}

/// Build a one-page-per-text PDF in memory (Helvetica `Tj` runs like the
/// facade's own fixtures). Texts are extractable, searchable, redactable.
pub fn pdf_with_texts(texts: &[&str]) -> Vec<u8> {
    use lopdf::{dictionary, Document, Object, Stream};

    fn escape(text: &str) -> String {
        text.replace('\\', "\\\\")
            .replace('(', "\\(")
            .replace(')', "\\)")
    }

    let mut doc = Document::with_version("1.7");
    let font_id = doc.new_object_id();
    let font = dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    };
    doc.objects.insert(font_id, Object::Dictionary(font));

    let pages_id = doc.new_object_id();
    let mut page_ids = Vec::new();
    for text in texts {
        let content = format!("BT /F1 12 Tf 72 720 Td ({}) Tj ET", escape(text));
        let content_id = doc.new_object_id();
        doc.objects.insert(
            content_id,
            Object::Stream(Stream::new(dictionary! {}, content.into_bytes())),
        );
        let page_id = doc.new_object_id();
        let page = dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! {
                "Font" => dictionary! { "F1" => font_id },
            },
        };
        doc.objects.insert(page_id, Object::Dictionary(page));
        page_ids.push(page_id);
    }
    let kids: Vec<Object> = page_ids.into_iter().map(Object::Reference).collect();
    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => kids,
        "Count" => texts.len() as u32,
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));
    let catalog_id = doc.new_object_id();
    let catalog = dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    };
    doc.objects.insert(catalog_id, Object::Dictionary(catalog));
    doc.trailer.set("Root", catalog_id);

    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("serialise in-memory PDF");
    bytes
}

/// Bytes that are not a PDF at all.
pub fn corrupt_bytes() -> Vec<u8> {
    b"this is not a pdf file, merely some bytes wearing a .pdf extension\n".to_vec()
}

/// Write `lines` (each + `\n`) as a password file.
pub fn password_file(dir: &Path, name: &str, lines: &[&str]) -> PathBuf {
    let path = dir.join(name);
    let mut text = String::new();
    for line in lines {
        text.push_str(line);
        text.push('\n');
    }
    std::fs::write(&path, text).expect("write password file");
    path
}

/// Write a small text file.
pub fn text_file(dir: &Path, name: &str, text: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, text).expect("write text file");
    path
}

/// A minimal JPEG: SOI + SOF0 (w x h) + EOI. Enough header for dimension
/// readers; the facade inserts JPEG bytes raw, so no pixel payload is
/// needed for watermark tests.
pub fn tiny_jpeg(w: u16, h: u16) -> Vec<u8> {
    let mut v = vec![0xFF, 0xD8]; // SOI
    v.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x0B, 8]); // SOF0, len 11, precision
    v.extend_from_slice(&h.to_be_bytes());
    v.extend_from_slice(&w.to_be_bytes());
    v.extend_from_slice(&[1, 1, 0x11, 0x00]); // 1 component + padding
    v.extend_from_slice(&[0xFF, 0xD9]); // EOI
    v
}

/// Page count of a PDF on disk, via the CLI itself (black-box).
pub fn page_count_of(pdf: &Path) -> usize {
    let out = run(&["info", &pdf.to_string_lossy(), "--json"]);
    assert!(out.status.success(), "info on {}", pdf.display());
    stdout_json(&out)["data"]["page_count"]
        .as_u64()
        .expect("page_count") as usize
}

/// Extracted text of a PDF on disk, via the CLI itself (black-box).
pub fn text_of(pdf: &Path) -> String {
    let out = run(&["extract-text", &pdf.to_string_lossy()]);
    assert!(out.status.success(), "extract-text on {}", pdf.display());
    stdout_text(&out)
}
