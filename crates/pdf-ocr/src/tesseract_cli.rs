//! Tesseract OCR through the system binary, as a subprocess.
//!
//! [`TesseractCliEngine`] implements [`OcrEngine`] without linking any OCR
//! library: it pipes the page raster to a `tesseract` found on `PATH` (or at
//! `PDFLUENT_TESSERACT`) as `tesseract stdin stdout --dpi <dpi> -l <lang> tsv`
//! and parses the TSV back into words. Standard library only, so the feature
//! adds no dependency and nothing to the package.
//!
//! Availability is detected once, in [`TesseractCliEngine::new`]: the binary
//! must answer `--version` and `--list-langs` must name the requested
//! language. A missing binary is [`TesseractCliError::NoBinary`], which the
//! caller treats as "no engine found"; anything else is a loud failure.
//!
//! [`OcrEngine`]: crate::engine::OcrEngine

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::engine::{rgb_pixels, OcrEngine, OcrPageResult, OcrWord};

/// Environment override for the Tesseract binary: when set to a non-empty
/// value it names the binary outright and `PATH` is not searched.
pub const TESSERACT_ENV_OVERRIDE: &str = "PDFLUENT_TESSERACT";

///
/// Non-exhaustive: future probes may fail in new ways, and callers match with
/// a wildcard or branch on [`is_no_binary`](Self::is_no_binary) instead.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TesseractCliError {
    /// No `tesseract` binary: neither the override nor `PATH` produced one
    NoBinary(String),
    /// The binary runs but cannot do the job: the language data is missing,
    /// a probe exited non-zero, and so on.
    Other(String),
}

impl TesseractCliError {
    /// Whether this error is a missing binary rather than a failed probe.
    pub fn is_no_binary(&self) -> bool {
        matches!(self, TesseractCliError::NoBinary(_))
    }
}

impl std::fmt::Display for TesseractCliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TesseractCliError::NoBinary(msg) | TesseractCliError::Other(msg) => {
                write!(f, "{msg}")
            }
        }
    }
}

impl std::error::Error for TesseractCliError {}

/// Tesseract OCR through the system `tesseract` binary.
///
/// Built by [`new`](Self::new), which resolves the binary and checks the
/// language up front so a page is never rendered for an engine that cannot
/// run. [`recognize`](OcrEngine::recognize) accepts RGB or RGBA row-major
/// pixels, like the linked backend.
#[derive(Debug, Clone)]
pub struct TesseractCliEngine {
    binary: PathBuf,
    language: String,
}

impl TesseractCliEngine {
    /// Resolve the `tesseract` binary and check it knows `language`.
    ///
    /// `language` is one Tesseract language tag, or several joined with `+`
    /// as `-l` takes them; every part must be listed by `--list-langs`.
    pub fn new(language: &str) -> Result<Self, TesseractCliError> {
        let binary = resolve_binary().ok_or_else(|| {
            TesseractCliError::NoBinary(
                "tesseract binary not found: install tesseract-ocr or point \
                 PDFLUENT_TESSERACT at the binary"
                    .to_string(),
            )
        })?;
        Self::with_binary(binary, language)
    }

    /// Check an explicitly named binary and language, for embedders and tests.
    pub fn with_binary(
        binary: impl Into<PathBuf>,
        language: &str,
    ) -> Result<Self, TesseractCliError> {
        let binary = binary.into();
        check_runs(&binary)?;
        check_language(&binary, language)?;
        Ok(Self {
            binary,
            language: language.to_string(),
        })
    }

    /// Whether a `tesseract` binary answers `--version` right now.
    ///
    /// The probe tests use this to skip with a reason where no engine is
    /// installed; it checks the binary only, never the language.
    pub fn is_available() -> bool {
        resolve_binary().is_some_and(|binary| check_runs(&binary).is_ok())
    }

    /// The resolved binary this engine shells out to.
    pub fn binary_path(&self) -> &Path {
        &self.binary
    }

    /// The `-l` argument recognition passes to Tesseract.
    pub fn language(&self) -> &str {
        &self.language
    }
}

impl OcrEngine for TesseractCliEngine {
    fn recognize(
        &self,
        image_data: &[u8],
        width: u32,
        height: u32,
        dpi: u32,
    ) -> Result<OcrPageResult, String> {
        let rgb = rgb_pixels(image_data, width, height)?;
        let png = encode_rgb_png(&rgb, width, height)
            .map_err(|e| format!("encode PNG for tesseract: {e}"))?;

        let mut child = Command::new(&self.binary)
            .args([
                "stdin",
                "stdout",
                "--dpi",
                &dpi.to_string(),
                "-l",
                &self.language,
                "tsv",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("run tesseract at {}: {e}", self.binary.display()))?;
        child
            .stdin
            .as_mut()
            .ok_or_else(|| "tesseract stdin was not piped".to_string())?
            .write_all(&png)
            .map_err(|e| format!("write page PNG to tesseract: {e}"))?;
        // Close stdin so Tesseract sees end of input; dropping the handle does.
        drop(child.stdin.take());
        let output = child
            .wait_with_output()
            .map_err(|e| format!("wait for tesseract: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "tesseract exited with {}: {}",
                output.status,
                stderr.trim()
            ));
        }

        let tsv = String::from_utf8_lossy(&output.stdout);
        let words = parse_tsv(&tsv);
        let confidence = if words.is_empty() {
            0.0
        } else {
            words.iter().map(|w| w.confidence).sum::<f32>() / words.len() as f32
        };
        Ok(OcrPageResult {
            words,
            confidence,
            image_width: width,
            image_height: height,
        })
    }

    fn supported_languages(&self) -> Vec<String> {
        self.language.split('+').map(str::to_string).collect()
    }
}

/// The binary to run: the override when set and non-empty, else `tesseract`
/// off `PATH`. An override naming nothing runnable still wins -- it is an
/// explicit choice, and falling back to `PATH` behind it would run a binary
/// the caller did not ask for.
fn resolve_binary() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(TESSERACT_ENV_OVERRIDE) {
        if !path.is_empty() {
            return Some(PathBuf::from(path));
        }
    }
    find_on_path("tesseract", std::env::var_os("PATH"))
}

/// `name` off a `PATH`-shaped value: the first entry that names a file.
/// Split into its own function so the search is testable without touching
/// the process environment.
fn find_on_path(name: &str, path_var: Option<OsString>) -> Option<PathBuf> {
    let path_var = path_var?;
    #[cfg(windows)]
    let separator = ';';
    #[cfg(not(windows))]
    let separator = ':';
    for dir in path_var.to_string_lossy().split(separator) {
        if dir.is_empty() {
            continue;
        }
        #[cfg(windows)]
        let candidate = PathBuf::from(dir).join(format!("{name}.exe"));
        #[cfg(not(windows))]
        let candidate = PathBuf::from(dir).join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// The binary answers `--version` with a zero exit.
fn check_runs(binary: &Path) -> Result<(), TesseractCliError> {
    let output = Command::new(binary)
        .arg("--version")
        .output()
        .map_err(|e| {
            TesseractCliError::NoBinary(format!(
                "tesseract binary not runnable at {}: {e}",
                binary.display()
            ))
        })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(TesseractCliError::Other(format!(
            "tesseract at {} failed --version with {}",
            binary.display(),
            output.status
        )))
    }
}

/// Every `+`-separated part of `language` is listed by `--list-langs`.
fn check_language(binary: &Path, language: &str) -> Result<(), TesseractCliError> {
    let output = Command::new(binary)
        .arg("--list-langs")
        .output()
        .map_err(|e| {
            TesseractCliError::Other(format!(
                "tesseract at {} failed --list-langs: {e}",
                binary.display()
            ))
        })?;
    if !output.status.success() {
        return Err(TesseractCliError::Other(format!(
            "tesseract at {} failed --list-langs with {}",
            binary.display(),
            output.status
        )));
    }
    // The list goes to stdout on Tesseract 5 and to stderr on 4; read both.
    let mut listed = String::from_utf8_lossy(&output.stdout).into_owned();
    listed.push_str(&String::from_utf8_lossy(&output.stderr));
    let listed: Vec<&str> = listed.lines().map(str::trim).collect();
    for part in language.split('+').map(str::trim) {
        if part.is_empty() || !listed.iter().any(|line| line == &part) {
            return Err(TesseractCliError::Other(format!(
                "tesseract language data for '{part}' is not installed \
                 (tesseract --list-langs at {})",
                binary.display()
            )));
        }
    }
    Ok(())
}

/// Parse Tesseract TSV output (`level ... conf text`) into words.
///
/// Only `level == 5` rows with non-empty text become words; page, block,
/// paragraph and line rows carry no text of their own. A malformed row is
/// skipped rather than failing the page: one garbled line must not lose the
/// words around it.
pub fn parse_tsv(tsv: &str) -> Vec<OcrWord> {
    let mut words = Vec::new();
    for (index, line) in tsv.lines().enumerate() {
        if index == 0 && line.starts_with("level\t") {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        // The text is the last field and may itself hold a tab, so the
        // first eleven fields are read positionally and the rest is text.
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 12 {
            continue;
        }
        if parts[0].trim() != "5" {
            continue;
        }
        let text: String = parts[11..].join("\t").trim().to_string();
        if text.is_empty() {
            continue;
        }
        let numbers: Option<Vec<i64>> = parts[6..10]
            .iter()
            .map(|n| n.trim().parse::<i64>().ok())
            .collect();
        let Some(numbers) = numbers else {
            continue;
        };
        let conf: f32 = parts[10].trim().parse().unwrap_or(-1.0);
        if conf < 0.0 {
            continue;
        }
        let [left, top, width, height] = [numbers[0], numbers[1], numbers[2], numbers[3]];
        if left < 0 || top < 0 || width <= 0 || height <= 0 {
            continue;
        }
        words.push(OcrWord {
            text,
            bbox_px: [
                left as u32,
                top as u32,
                (left + width) as u32,
                (top + height) as u32,
            ],
            confidence: (conf / 100.0).clamp(0.0, 1.0),
        });
    }
    words
}

/// Encode RGB row-major pixels as a PNG: 8-bit truecolour, one IDAT.
///
/// Standard library only, which is the point of this module: the deflate
/// stream is stored (uncompressed) blocks inside zlib, so no compressor is
/// needed. Returns an error rather than a corrupt image when the buffer is
/// short.
pub fn encode_rgb_png(rgb: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let stride = width as usize * 3;
    if rgb.len() != stride * height as usize {
        return Err(format!(
            "unexpected image size: {} bytes for {}x{} RGB",
            rgb.len(),
            width,
            height,
        ));
    }
    let mut out = Vec::with_capacity(rgb.len() + 128);
    out.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8); // Bit depth.
    ihdr.push(2); // Colour type: truecolour.
    ihdr.extend_from_slice(&[0, 0, 0]); // Compression, filter, interlace.
    push_chunk(&mut out, b"IHDR", &ihdr);

    // Each scanline is a filter byte (0: none) plus the row's bytes.
    let mut raw = Vec::with_capacity((stride + 1) * height as usize);
    for row in rgb.chunks_exact(stride) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    push_chunk(&mut out, b"IDAT", &zlib_store(&raw));
    push_chunk(&mut out, b"IEND", &[]);
    Ok(out)
}

/// Append a PNG chunk: length, type, data, CRC-32 over type and data.
fn push_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = Crc32::new();
    crc.update(kind);
    crc.update(data);
    out.extend_from_slice(&crc.finish().to_be_bytes());
}

/// `data` as a zlib stream of stored (uncompressed) deflate blocks.
fn zlib_store(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 16);
    out.extend_from_slice(&[0x78, 0x01]); // zlib header: 32K window, no compression.
    let mut chunks = data.chunks(65535).peekable();
    if data.is_empty() {
        // Even an empty image needs one (empty, final) block.
        out.extend_from_slice(&[0x01, 0x00, 0x00, 0xFF, 0xFF]);
    }
    while let Some(chunk) = chunks.next() {
        let final_block = chunks.peek().is_none();
        out.push(u8::from(final_block)); // BFINAL + BTYPE 00 (stored).
        let len = chunk.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(chunk);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

/// Adler-32 over `data`, the zlib trailer.
fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65521;
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + u32::from(byte)) % MOD;
        b = (b + a) % MOD;
    }
    (b << 16) | a
}

/// CRC-32 (ISO 3309) over PNG chunk bytes, table built once.
struct Crc32 {
    crc: u32,
}

impl Crc32 {
    fn new() -> Self {
        Self { crc: 0xFFFF_FFFF }
    }

    fn update(&mut self, data: &[u8]) {
        for &byte in data {
            let index = ((self.crc ^ u32::from(byte)) & 0xFF) as usize;
            self.crc = CRC_TABLE[index] ^ (self.crc >> 8);
        }
    }

    fn finish(self) -> u32 {
        self.crc ^ 0xFFFF_FFFF
    }
}

/// The CRC-32 table, generated rather than pasted: bit by bit, polynomial
/// 0xEDB88320.
const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 == 1 {
                0xEDB8_8320 ^ (crc >> 1)
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Captured from the real binary, not handwritten: `fixtures/scanned/
    /// scan_01_sample.pdf` rendered at 150 dpi, piped through `tesseract
    /// stdin stdout --dpi 150 -l eng tsv` with Tesseract 5.5.3.
    const SAMPLE_TSV: &str = include_str!("../tests/fixtures/tesseract_scan01.tsv");

    #[test]
    fn the_sample_reads_two_words() {
        let words = parse_tsv(SAMPLE_TSV);
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "Test");
        assert_eq!(words[0].bbox_px, [434, 362, 534, 400]);
        assert!((words[0].confidence - 0.958_953_8).abs() < 1e-6);
        assert_eq!(words[1].text, "page");
        assert_eq!(words[1].bbox_px, [552, 371, 664, 411]);
    }

    #[test]
    fn non_word_rows_and_empty_text_are_not_words() {
        let tsv = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
             1\t1\t0\t0\t0\t0\t0\t0\t100\t100\t-1\t\n\
             5\t1\t1\t1\t1\t1\t1\t2\t10\t8\t-1\t\n\
             5\t1\t1\t1\t1\t2\t1\t2\t10\t8\t90\tkept\n";
        let words = parse_tsv(tsv);
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, "kept");
        assert_eq!(words[0].bbox_px, [1, 2, 11, 10]);
        assert!((words[0].confidence - 0.9).abs() < 1e-6);
    }

    #[test]
    fn a_garbled_row_does_not_lose_its_neighbours() {
        let tsv = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
             5\t1\t1\t1\t1\t1\t0\t0\t10\t10\t80\tbefore\n\
             5\tnot\ta\trow\n\
             5\t1\t1\t1\t1\t2\tx\ty\t10\t10\t80\tnumbers\n\
             5\t1\t1\t1\t1\t3\t5\t5\t10\t10\t70\tafter\n";
        let words = parse_tsv(tsv);
        let texts: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(texts, vec!["before", "after"]);
    }

    #[test]
    fn an_empty_tsv_is_no_words() {
        assert!(parse_tsv("").is_empty());
        assert!(
            parse_tsv("level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n")
                .is_empty()
        );
    }

    #[test]
    fn the_png_carries_signature_dimensions_and_end() {
        let png = encode_rgb_png(&[255, 0, 0, 0, 255, 0], 2, 1).unwrap();
        assert_eq!(&png[0..8], b"\x89PNG\r\n\x1a\n");
        // IHDR chunk: length 13, then width/height big-endian, 8-bit RGB.
        assert_eq!(&png[8..16], &[0, 0, 0, 13, b'I', b'H', b'D', b'R']);
        assert_eq!(&png[16..24], &[0, 0, 0, 2, 0, 0, 0, 1]);
        assert_eq!(png[24], 8);
        assert_eq!(png[25], 2);
        // IEND chunk closes the file.
        assert_eq!(
            &png[png.len() - 12..],
            &[0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82]
        );
    }

    #[test]
    fn the_png_rejects_a_short_buffer() {
        assert!(encode_rgb_png(&[0u8; 5], 2, 1).is_err());
    }

    /// Whether the suite requires the binary: where
    /// `PDFLUENT_REQUIRE_TESSERACT` is set a missing binary fails the test,
    /// so a broken install cannot hide behind skips.
    fn tesseract_required() -> bool {
        std::env::var_os("PDFLUENT_REQUIRE_TESSERACT").is_some()
    }

    #[test]
    fn the_binary_reads_a_white_page_as_blank() {
        let engine = match TesseractCliEngine::new("eng") {
            Ok(engine) => engine,
            Err(e) if e.is_no_binary() && tesseract_required() => {
                panic!(
                    "PDFLUENT_REQUIRE_TESSERACT is set but no tesseract binary \
                     answers --version"
                );
            }
            Err(_) => {
                eprintln!("SKIPPED (not a pass): no tesseract binary installed");
                return;
            }
        };
        // A blank white image: the point is the plumbing (spawn, PNG on
        // stdin, TSV back), not the recognition.
        let white = vec![255u8; 200 * 100 * 3];
        let page = engine.recognize(&white, 200, 100, 150).unwrap();
        assert!(page.words.len() < 5);
        assert_eq!(page.image_width, 200);
        assert_eq!(page.image_height, 100);
    }

    #[test]
    fn an_unknown_language_is_loud_not_missing() {
        if !TesseractCliEngine::is_available() {
            if tesseract_required() {
                panic!(
                    "PDFLUENT_REQUIRE_TESSERACT is set but no tesseract binary \
                     answers --version"
                );
            }
            eprintln!("SKIPPED (not a pass): no tesseract binary installed");
            return;
        }
        let err = TesseractCliEngine::new("xx-not-a-language-xx").unwrap_err();
        assert!(
            !err.is_no_binary(),
            "a bad language is a caller error, not a missing binary: {err}"
        );
    }

    #[test]
    fn path_search_finds_a_file_and_nothing_else() {
        let dir = std::env::temp_dir().join("pdf_ocr_path_search");
        let _ = std::fs::create_dir_all(&dir);
        #[cfg(windows)]
        let name = "tesseract.exe";
        #[cfg(not(windows))]
        let name = "tesseract";
        let candidate = dir.join(name);
        std::fs::write(&candidate, b"fake").unwrap();
        let path_var = OsString::from(format!("/nonexistent{}", {
            #[cfg(windows)]
            {
                format!(";{}", dir.display())
            }
            #[cfg(not(windows))]
            {
                format!(":{}", dir.display())
            }
        }));
        assert_eq!(
            find_on_path("tesseract", Some(path_var)),
            Some(candidate.clone())
        );
        assert_eq!(
            find_on_path("tesseract", Some(OsString::from("/nonexistent"))),
            None
        );
        assert_eq!(find_on_path("tesseract", None), None);
        let _ = std::fs::remove_file(&candidate);
    }
}
