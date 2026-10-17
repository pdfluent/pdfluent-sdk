//! `render_page_png` writes our own pixels as an RGB PNG.
//!
//! The example's real entry, in-process, over the committed `structured.pdf`:
//! the file must be a PNG (magic, IHDR dimensions, 8-bit RGB), byte-identical
//! across runs, honouring `--dpi`, and refusing a missing file, a page past
//! the end, and a wrong invocation with the documented exit codes. Every
//! assertion reads the bytes the example wrote, so a regression to `sips` or
//! PPM fails here rather than weakening the pin.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

#[path = "../examples/render_page_png.rs"]
mod render_page_png;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

/// A scratch dir under the system temp dir, removed on drop.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let unique = NEXT.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "render_page_png_{}_{}_{}",
            std::process::id(),
            unique,
            name
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn join(&self, name: &str) -> String {
        self.path.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn fixture() -> String {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/structured.pdf").to_string()
}

/// The example's real entry, in-process: exit code, stdout, stderr.
fn run(args: &[&str]) -> (i32, String, String) {
    let owned: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = render_page_png::run_with_args(&owned, &mut out, &mut err);
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

/// `(width, height, bit depth, color type)` from the IHDR, after the magic.
fn ihdr(png: &[u8]) -> (u32, u32, u8, u8) {
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "not a PNG file");
    assert_eq!(&png[12..16], b"IHDR", "first chunk is not IHDR");
    let width = u32::from_be_bytes(png[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(png[20..24].try_into().unwrap());
    (width, height, png[24], png[25])
}

#[test]
fn render_page_png_writes_an_rgb_png() {
    let tmp = TempDir::new("rgb");
    let out = tmp.join("p.png");
    let (code, _, err) = run(&[&fixture(), &out]);
    assert_eq!((code, err.as_str()), (0, ""));
    let png = std::fs::read(&out).expect("the PNG the example wrote");
    assert_eq!(ihdr(&png), (816, 1056, 8, 2), "96 dpi RGB, first page");
}

#[test]
fn render_page_png_is_byte_deterministic() {
    let tmp = TempDir::new("deterministic");
    let (first, second) = (tmp.join("a.png"), tmp.join("b.png"));
    let pdf = fixture();
    assert_eq!(run(&[&pdf, &first]).0, 0);
    assert_eq!(run(&[&pdf, &second]).0, 0);
    assert_eq!(
        std::fs::read(&first).unwrap(),
        std::fs::read(&second).unwrap(),
        "two renders of one page must be byte-identical"
    );
}

#[test]
fn render_page_png_honours_dpi() {
    let tmp = TempDir::new("dpi");
    let out = tmp.join("p.png");
    let (code, _, _) = run(&[&fixture(), &out, "--dpi", "72"]);
    assert_eq!(code, 0);
    let png = std::fs::read(&out).unwrap();
    assert_eq!(ihdr(&png), (612, 792, 8, 2));
}

#[test]
fn render_page_png_refuses_a_page_past_the_end() {
    let tmp = TempDir::new("page");
    let out = tmp.join("p.png");
    let (code, _, err) = run(&[&fixture(), &out, "--page", "9"]);
    assert_eq!(code, 1, "stderr: {err}");
}

#[test]
fn render_page_png_refuses_a_missing_file() {
    let tmp = TempDir::new("missing");
    let out = tmp.join("p.png");
    let missing = tmp.join("no-such.pdf");
    let (code, _, err) = run(&[&missing, &out]);
    assert_eq!(code, 1, "stderr: {err}");
}

#[test]
fn render_page_png_refuses_a_wrong_invocation() {
    assert_eq!(run(&[]).0, 2);
    let tmp = TempDir::new("usage");
    let out = tmp.join("p.png");
    assert_eq!(run(&[&fixture()]).0, 2, "the output path is required");
    assert_eq!(run(&[&fixture(), &out, "--dpi", "0"]).0, 2);
    assert_eq!(run(&[&fixture(), &out, "--zoom", "2"]).0, 2);
}
