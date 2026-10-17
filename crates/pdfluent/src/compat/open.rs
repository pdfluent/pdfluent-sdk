//! Non-PDF input for the compatible open path (lane L2).
//!
//! The handle only ever parses PDF; this module converts the other accepted
//! inputs to PDF bytes first: EPUB via `epub-extract`, images via the
//! image-insert pipeline, HTML via an external headless Chrome the user
//! installed. A named format we do not convert fails as unsupported; bytes
//! no rule recognises pass through untouched, so the parser reports them
//! as invalid PDF exactly as before.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use super::ErrorKind;
use crate::error::Result;

/// How the HTML recipe finds its renderer, per the cookbook page.
const CHROME_MISSING: &str =
    "HTML conversion requires Chrome or Chromium; see https://pdfluent.com/how-to/files/#generate-pdf-from-html-rust";
/// Chrome gets a minute: a cold first start can take tens of seconds.
const CHROME_TIMEOUT_SECS: u64 = 60;

/// Convert open input to PDF bytes, passing PDF through untouched.
///
/// `filetype` is the explicit hint (stream opens), `filename` the path the
/// bytes were read from (file opens; its extension is the fallback hint).
/// Magic wins over names: bytes that positively identify as PDF, EPUB or an
/// image convert by what they are, not what they are called.
pub fn input_to_pdf_bytes(
    data: &[u8],
    filetype: Option<&str>,
    filename: Option<&str>,
) -> Result<Vec<u8>> {
    if is_pdf_magic(data) {
        return Ok(data.to_vec());
    }
    if epub_extract::is_epub(data) {
        return epub_to_pdf_bytes(data);
    }
    if let Some(format) = pdf_manip::image_insert::detect_format(data) {
        return image_to_pdf_bytes(data, format);
    }
    let hint = filetype.map(normalize_hint).filter(|hint| !hint.is_empty());
    let named = hint.or_else(|| filename.and_then(extension_of));
    match named.as_deref() {
        None | Some("pdf") => Ok(data.to_vec()),
        Some("epub") => epub_to_pdf_bytes(data),
        Some("png") => image_to_pdf_bytes(data, pdf_manip::image_insert::ImageFormat::Png),
        Some("jpg" | "jpeg" | "jpe" | "jfif") => {
            image_to_pdf_bytes(data, pdf_manip::image_insert::ImageFormat::Jpeg)
        }
        Some("html" | "htm" | "xhtml" | "xht") => chrome_pdf_bytes(data),
        Some(other) => Err(crate::Error::Unsupported(format!(
            "not supported in this version: {other}"
        ))),
    }
}

/// Lowercase a filetype hint, tolerating a leading dot.
fn normalize_hint(filetype: &str) -> String {
    filetype.trim().trim_start_matches('.').to_ascii_lowercase()
}

/// Lowercase extension of a path, if it has one.
fn extension_of(filename: &str) -> Option<String> {
    let base = filename.rsplit(['/', '\\']).next().unwrap_or(filename);
    let dot = base.rfind('.')?;
    let ext = &base[dot + 1..];
    if ext.is_empty() || dot == 0 {
        return None;
    }
    Some(ext.to_ascii_lowercase())
}

/// Whether the bytes open as a PDF.
fn is_pdf_magic(data: &[u8]) -> bool {
    data.starts_with(b"%PDF-")
}

/// Render EPUB chapters as plain PDF pages.
fn epub_to_pdf_bytes(data: &[u8]) -> Result<Vec<u8>> {
    epub_extract::to_pdf_bytes(data).map_err(|e| crate::Error::InvalidPdf {
        byte_offset: None,
        reason: e.to_string(),
    })
}

/// Wrap one image as a one-page PDF sized to the image.
///
/// The page is the image's pixel size at its own DPI when the file carries
/// one, else at 72 dpi (one pixel per point).
fn image_to_pdf_bytes(
    data: &[u8],
    format: pdf_manip::image_insert::ImageFormat,
) -> Result<Vec<u8>> {
    use pdf_manip::image_insert::ImageFormat;
    let kind = match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpeg",
    };
    let dims = match format {
        ImageFormat::Png => png_dims(data),
        ImageFormat::Jpeg => jpeg_dims(data),
    };
    let Some((width, height)) = dims else {
        return Err(ErrorKind::UnreadableFile.to_error(format!("cannot decode {kind} image")));
    };
    let (dpi_x, dpi_y) = match format {
        ImageFormat::Png => png_dpi(data),
        ImageFormat::Jpeg => jpeg_dpi(data),
    };
    let page_w = f64::from(width) * 72.0 / dpi_x;
    let page_h = f64::from(height) * 72.0 / dpi_y;
    let mut doc = blank_single_page(page_w, page_h);
    let insert = pdf_manip::image_insert::ImageInsert {
        image_data: data.to_vec(),
        format,
        x: 0.0,
        y: 0.0,
        width: page_w,
        height: page_h,
        page_index: 1,
        opacity: None,
    };
    pdf_manip::image_insert::insert_image(&mut doc, &insert).map_err(|e| {
        ErrorKind::UnreadableFile.to_error(format!("cannot place {kind} image: {e}"))
    })?;
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes)
        .map_err(|source| crate::Error::Io { source, path: None })?;
    Ok(bytes)
}

/// One blank page of the given size in points.
fn blank_single_page(width: f64, height: f64) -> lopdf::Document {
    use lopdf::{dictionary, Document, Object, Stream};
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let content_id = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![
            Object::Real(0.0),
            Object::Real(0.0),
            Object::Real(width as f32),
            Object::Real(height as f32),
        ],
        "Contents" => content_id,
        "Resources" => dictionary! {},
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    doc
}

/// Pixel size from a PNG IHDR chunk.
fn png_dims(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() < 24 || &data[0..8] != b"\x89PNG\r\n\x1a\n" || &data[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(data[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(data[20..24].try_into().ok()?);
    if width == 0 || height == 0 {
        return None;
    }
    Some((width, height))
}

/// DPI from a PNG pHYs chunk, else 72. Pixels per metre times 0.0254.
fn png_dpi(data: &[u8]) -> (f64, f64) {
    const DEFAULT: (f64, f64) = (72.0, 72.0);
    if data.len() < 8 || &data[0..8] != b"\x89PNG\r\n\x1a\n" {
        return DEFAULT;
    }
    let mut at = 8usize;
    while at + 8 <= data.len() {
        let len = u32::from_be_bytes(data[at..at + 4].try_into().unwrap_or([0; 4])) as usize;
        let chunk = &data[at + 4..at + 8];
        let body = at + 8;
        if len > data.len().saturating_sub(body + 4) {
            return DEFAULT;
        }
        if chunk == b"pHYs" && len == 9 {
            let ppm_x = u32::from_be_bytes(data[body..body + 4].try_into().unwrap_or([0; 4]));
            let ppm_y = u32::from_be_bytes(data[body + 4..body + 8].try_into().unwrap_or([0; 4]));
            if data[body + 8] == 1 && ppm_x > 0 && ppm_y > 0 {
                return (f64::from(ppm_x) * 0.0254, f64::from(ppm_y) * 0.0254);
            }
            return DEFAULT;
        }
        if chunk == b"IDAT" {
            return DEFAULT;
        }
        at = body + len + 4;
    }
    DEFAULT
}

/// Pixel size from the first JPEG start-of-frame marker.
fn jpeg_dims(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return None;
    }
    let mut at = 2usize;
    while at + 4 <= data.len() {
        if data[at] != 0xFF {
            return None;
        }
        let marker = data[at + 1];
        if marker == 0xDA || marker == 0xD9 {
            return None;
        }
        if marker == 0xD8 || (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            at += 2;
            continue;
        }
        let len = u16::from_be_bytes([data[at + 2], data[at + 3]]) as usize;
        if len < 2 || at + 2 + len > data.len() {
            return None;
        }
        // SOF0-SOF15 except DHT (C4), JPG (C8) and DAC (CC).
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) && len >= 7 {
            let height = u16::from_be_bytes([data[at + 5], data[at + 6]]) as u32;
            let width = u16::from_be_bytes([data[at + 7], data[at + 8]]) as u32;
            if width > 0 && height > 0 {
                return Some((width, height));
            }
            return None;
        }
        at += 2 + len;
    }
    None
}

/// DPI from the JPEG APP0 JFIF segment, else 72.
fn jpeg_dpi(data: &[u8]) -> (f64, f64) {
    const DEFAULT: (f64, f64) = (72.0, 72.0);
    if data.len() < 20 || data[0] != 0xFF || data[1] != 0xD8 {
        return DEFAULT;
    }
    let mut at = 2usize;
    while at + 4 <= data.len() {
        if data[at] != 0xFF {
            return DEFAULT;
        }
        let marker = data[at + 1];
        if marker == 0xDA || marker == 0xD9 {
            return DEFAULT;
        }
        if marker == 0xD8 || (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            at += 2;
            continue;
        }
        let len = u16::from_be_bytes([data[at + 2], data[at + 3]]) as usize;
        if len < 2 || at + 2 + len > data.len() {
            return DEFAULT;
        }
        if marker == 0xE0 && len >= 14 && &data[at + 4..at + 9] == b"JFIF\0" {
            let units = data[at + 11];
            let dx = u16::from_be_bytes([data[at + 12], data[at + 13]]);
            let dy = u16::from_be_bytes([data[at + 14], data[at + 15]]);
            if dx == 0 || dy == 0 {
                return DEFAULT;
            }
            match units {
                1 => return (f64::from(dx), f64::from(dy)),
                2 => return (f64::from(dx) * 2.54, f64::from(dy) * 2.54),
                _ => return DEFAULT,
            }
        }
        at += 2 + len;
    }
    DEFAULT
}

/// Render HTML to PDF bytes with an external headless Chrome.
#[cfg(not(target_arch = "wasm32"))]
fn chrome_pdf_bytes(html: &[u8]) -> Result<Vec<u8>> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let chrome = resolve_chrome()?;
    let tag = format!(
        "pdfluent-html-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let dir = std::env::temp_dir();
    let html_path = dir.join(format!("{tag}.html"));
    let pdf_path = dir.join(format!("{tag}.pdf"));
    std::fs::write(&html_path, html).map_err(|source| crate::Error::Io {
        source,
        path: Some(html_path.clone()),
    })?;
    let outcome = run_chrome(&chrome, &html_path, &pdf_path);
    let _ = std::fs::remove_file(&html_path);
    outcome?;
    let bytes = std::fs::read(&pdf_path).map_err(|source| crate::Error::Io {
        source,
        path: Some(pdf_path.clone()),
    })?;
    let _ = std::fs::remove_file(&pdf_path);
    Ok(bytes)
}

/// No child processes on this target.
#[cfg(target_arch = "wasm32")]
fn chrome_pdf_bytes(_html: &[u8]) -> Result<Vec<u8>> {
    Err(crate::Error::UnsupportedOnWasm {
        operation: "HTML conversion",
    })
}

/// Locate a Chrome or Chromium: `PDFLUENT_CHROME` first, then the standard
/// install names on PATH and the standard install paths per OS.
///
/// An explicit `PDFLUENT_CHROME` that resolves to nothing fails loudly
/// instead of silently falling through: a stale override must not mask
/// itself as auto-discovery.
#[cfg(not(target_arch = "wasm32"))]
fn resolve_chrome() -> Result<std::path::PathBuf> {
    if let Ok(env) = std::env::var("PDFLUENT_CHROME") {
        let trimmed = env.trim();
        if !trimmed.is_empty() {
            return resolve_chrome_candidate(trimmed).ok_or_else(|| {
                crate::error::internal_error(format!(
                    "PDFLUENT_CHROME does not point at a usable Chrome: {trimmed}"
                ))
            });
        }
    }
    for name in [
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
        "chrome",
    ] {
        if let Some(found) = search_path(name) {
            return Ok(found);
        }
    }
    for path in chrome_install_paths() {
        if path.is_file() {
            return Ok(path);
        }
    }
    Err(crate::Error::Unsupported(CHROME_MISSING.to_string()))
}

/// A configured candidate: an existing path, or a bare command on PATH.
#[cfg(not(target_arch = "wasm32"))]
fn resolve_chrome_candidate(candidate: &str) -> Option<std::path::PathBuf> {
    if candidate.contains('/') || candidate.contains('\\') {
        let path = std::path::PathBuf::from(candidate);
        return path.is_file().then_some(path);
    }
    search_path(candidate)
}

/// First `name` on PATH that is a file.
#[cfg(not(target_arch = "wasm32"))]
fn search_path(name: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let full = dir.join(name);
        if full.is_file() {
            return Some(full);
        }
        #[cfg(target_os = "windows")]
        {
            let exe = dir.join(format!("{name}.exe"));
            if exe.is_file() {
                return Some(exe);
            }
        }
    }
    None
}

/// Standard install paths per OS.
#[cfg(not(target_arch = "wasm32"))]
fn chrome_install_paths() -> Vec<std::path::PathBuf> {
    #[allow(unused_mut)]
    let mut out = Vec::new();
    #[cfg(target_os = "macos")]
    {
        out.push("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome".into());
        out.push("/Applications/Chromium.app/Contents/MacOS/Chromium".into());
    }
    #[cfg(target_os = "linux")]
    {
        for path in [
            "/usr/bin/google-chrome",
            "/usr/bin/google-chrome-stable",
            "/usr/bin/chromium",
            "/usr/bin/chromium-browser",
            "/snap/bin/chromium",
            "/opt/google/chrome/chrome",
        ] {
            out.push(std::path::PathBuf::from(path));
        }
    }
    #[cfg(target_os = "windows")]
    {
        for var in ["ProgramFiles", "ProgramFiles(x86)", "LocalAppData"] {
            if let Ok(root) = std::env::var(var) {
                for leaf in [
                    "Google\\Chrome\\Application\\chrome.exe",
                    "Chromium\\Application\\chrome.exe",
                ] {
                    out.push(std::path::PathBuf::from(&root).join(leaf));
                }
            }
        }
    }
    out
}

/// Run Chrome's headless print-to-PDF over one file.
///
/// The flags are the cookbook recipe: `--headless --print-to-pdf`, with
/// `--disable-gpu` and `--no-pdf-header-footer` as documented there, and
/// deliberately no `--no-sandbox`.
#[cfg(not(target_arch = "wasm32"))]
fn run_chrome(
    chrome: &std::path::Path,
    html: &std::path::Path,
    out: &std::path::Path,
) -> Result<()> {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    let mut child = Command::new(chrome)
        .arg("--headless")
        .arg("--disable-gpu")
        .arg("--no-pdf-header-footer")
        .arg(format!("--print-to-pdf={}", out.display()))
        .arg(html)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            crate::error::internal_error(format!("cannot start {}: {e}", chrome.display()))
        })?;
    let deadline = Instant::now() + Duration::from_secs(CHROME_TIMEOUT_SECS);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let output = child.wait_with_output();
                    let stderr = output
                        .map(|o| String::from_utf8_lossy(&o.stderr).into_owned())
                        .unwrap_or_default();
                    return Err(crate::error::internal_error(format!(
                        "Chrome timed out after {CHROME_TIMEOUT_SECS}s converting HTML: {}",
                        stderr.trim()
                    )));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => {
                return Err(crate::error::internal_error(format!(
                    "cannot wait for {}: {e}",
                    chrome.display()
                )));
            }
        }
    }
    let output = child.wait_with_output().map_err(|e| {
        crate::error::internal_error(format!("cannot read {} output: {e}", chrome.display()))
    })?;
    if !output.status.success() {
        return Err(crate::error::internal_error(format!(
            "{} exited with {}: {}",
            chrome.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    if !out.is_file() {
        return Err(crate::error::internal_error(format!(
            "{} reported success but wrote no PDF: {}",
            chrome.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_normalize_and_extensions_fall_back() {
        assert_eq!(normalize_hint("  .PDF "), "pdf");
        assert_eq!(extension_of("/tmp/Page.HTML"), Some("html".to_string()));
        assert_eq!(extension_of("no-extension"), None);
        assert_eq!(extension_of(".hidden"), None);
    }

    #[test]
    fn pdf_magic_passes_through_with_any_name() {
        let pdf = b"%PDF-1.7 garbage";
        assert_eq!(input_to_pdf_bytes(pdf, None, None).unwrap(), pdf.to_vec());
        assert_eq!(
            input_to_pdf_bytes(pdf, Some("txt"), Some("x.txt")).unwrap(),
            pdf.to_vec()
        );
    }

    #[test]
    fn named_but_unconvertible_formats_fail_by_name() {
        for name in ["cbz", "fb2", "mobi", "xps", "svg", "txt"] {
            let err = input_to_pdf_bytes(b"junk", Some(name), None).unwrap_err();
            assert_eq!(
                err.to_string(),
                format!("Unsupported operation: not supported in this version: {name}")
            );
        }
        let err = input_to_pdf_bytes(b"junk", None, Some("/tmp/book.cbz")).unwrap_err();
        assert!(err
            .to_string()
            .contains("not supported in this version: cbz"));
    }

    #[test]
    fn nameless_junk_passes_through_for_the_parser() {
        assert_eq!(input_to_pdf_bytes(b"junk", None, None).unwrap(), b"junk");
    }

    #[test]
    fn png_dpi_reads_phys_and_defaults() {
        // 1x1 PNG: signature + IHDR + pHYs(2835 ppm ~= 72dpi) + IEND.
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend_from_slice(&[
            0, 0, 0, 13, b'I', b'H', b'D', b'R', 0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0,
        ]);
        png.extend_from_slice(&[0, 0, 0, 0]); // CRC skipped by the parser
        png.extend_from_slice(&[
            0, 0, 0, 9, b'p', b'H', b'Y', b's', 0, 0, 0x0B, 0x13, 0, 0, 0x0B, 0x13, 1,
        ]);
        png.extend_from_slice(&[0, 0, 0, 0]);
        png.extend_from_slice(&[0, 0, 0, 0, b'I', b'E', b'N', b'D', 0, 0, 0, 0]);
        assert_eq!(png_dims(&png), Some((1, 1)));
        let (dx, dy) = png_dpi(&png);
        assert!((dx - 72.009).abs() < 0.01, "dx={dx}");
        assert!((dy - 72.009).abs() < 0.01, "dy={dy}");
        assert_eq!(png_dpi(b"\x89PNG\r\n\x1a\n"), (72.0, 72.0));
    }

    #[test]
    fn jpeg_dpi_reads_jfif_and_defaults() {
        // SOI + APP0(JFIF, units=1, 150x150) + SOS terminator.
        let jpeg = [
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0x00, 0x01, 0x01, 0x01,
            0x00, 0x96, 0x00, 0x96, 0x00, 0x00, 0xFF, 0xDA, 0x00, 0x00,
        ];
        assert_eq!(jpeg_dpi(&jpeg), (150.0, 150.0));
        assert_eq!(jpeg_dpi(&[0xFF, 0xD8, 0xFF, 0xDA]), (72.0, 72.0));
        // SOF0 with 8x4.
        let sof = [
            0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x04, 0x00, 0x08, 0x01, 0x01, 0x11,
            0x00, 0xFF, 0xD9,
        ];
        assert_eq!(jpeg_dims(&sof), Some((8, 4)));
    }
}

// The Chrome recipe is native-only (`chrome_pdf_bytes` has a wasm32 fallback),
// so on wasm32 its two constants would be dead code under `-D warnings`.
#[cfg(target_arch = "wasm32")]
const _: (&str, u64) = (CHROME_MISSING, CHROME_TIMEOUT_SECS);
