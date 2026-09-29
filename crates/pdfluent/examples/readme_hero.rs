// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Regenerates `docs/readme-hero.png`, the image on the first screen of the
//! public README (#205).
//!
//! WHY A PROGRAM AND NOT A SCREENSHOT
//!
//! The first screen of `pdfluent/pdfluent-sdk` has to show real output, and a
//! screenshot cannot be checked: nobody can tell later whether the picture came
//! from this engine, from an older one, or from a graphics editor. This example
//! makes the picture from the repository, so the claim under it is auditable --
//! re-run it and see whether the same image comes back out.
//!
//! It also settles where the document came from. The fixtures in this tree are
//! bare test pages, and the measurement corpus cannot be published; so the page
//! is authored here, in `docs/hero/hero_document.html`, and is ours outright.
//!
//! THE PIPELINE, WHICH IS ALSO THE DEMONSTRATION
//!
//!   1. headless Chrome prints `docs/hero/hero_document.html` to PDF -- the one
//!      job PDFluent deliberately does not do (see `html_to_pdf_via_chrome.rs`);
//!   2. PDFluent renders page 1 of it: the left panel;
//!   3. PDFluent locates the bank line, redacts that region, watermarks the page
//!      and converts the result to PDF/A-1B, then validates it and keeps what the
//!      validator said;
//!   4. PDFluent renders page 1 of that: the right panel;
//!   5. Chrome prints `docs/hero/hero_composite.html` -- the two panels, the
//!      captions, and the verdict from step 3 -- to a PDF of the exact page
//!      size the README wants;
//!   6. PDFluent renders that to `docs/readme-hero.png`.
//!
//! Steps 2, 3, 4 and 6 are the product. The picture is not a picture *of* the
//! renderer's output; it *is* the renderer's output.
//!
//! RUN IT
//!
//! ```text
//! cargo run --example readme_hero
//! cargo run --example readme_hero -- --out /tmp/hero.png --keep /tmp/hero-work
//! ```
//!
//! Chrome or Chromium has to be on PATH (or installed where macOS puts it).
//! Fonts are the machine's, so two machines do not produce byte-identical PNGs;
//! what is reproducible is the picture, not the bytes. The image that ships is
//! the one committed next to this file.
//!
//! This file is compiled by CI (`cargo build --examples`), so it cannot rot into
//! something that no longer builds while the README still shows what it made.

use pdfluent::compliance::PdfAProfile;
use pdfluent::prelude::PdfDocument;
use pdfluent::watermark::{Layer, WatermarkOptions};
use pdfluent::ImageFormat;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The README's budget for its own first screen. A landing page that makes a
/// reader wait for the picture has spent the attention it was meant to win;
/// #205 fixed the ceiling at 400 kB and the guard enforces it, so the generator
/// refuses to write a file the guard would reject.
const MAX_BYTES: usize = 400 * 1024;

/// The composite's canvas, in CSS pixels at 96 dpi. It sets the `@page` size,
/// the body box and, through `HERO_DPI`, the pixel dimensions of the file that
/// ships -- so it lives here once and is substituted into the template.
const CANVAS: (f64, f64) = (1200.0, 600.0);

/// What the hero is rasterized at, in dpi against a 96-dpi design canvas.
///
/// GitHub lays a README image out at about 890 CSS pixels wide, so 144 dpi over
/// a 1200-pixel canvas is 1800 device pixels -- better than two device pixels
/// per CSS pixel where it is actually shown, which is what "2x" is for. Going to
/// 192 costs another 180 kB and buys nothing a reader can see; measured
/// 07-09-2026 at 564 kB, over the budget below.
const HERO_DPI: u32 = 144;

/// The two panels are rasterized at 192 dpi, above the hero's own resolution,
/// because they are scaled down again inside the composite. Downsampling a
/// sharp panel looks better than upsampling a soft one, and these are working
/// files that never ship.
const PANEL_DPI: u32 = 192;

/// The line the right panel proves is gone. It is invented, it is on one line of
/// `docs/hero/hero_document.html`, and the redaction is aimed at where the text
/// layout says it sits rather than at a string in the content stream.
///
/// WHY BY REGION AND NOT BY TEXT. `redact(text, ..)` searches the content
/// stream, and on this input -- a subset-font PDF printed by Chrome -- it
/// matches nothing while `text()` reads the line back perfectly. Measured
/// 07-09-2026; reported separately. `text_with_layout()` and `redact_region()`
/// answer the same question through the model that does see the text, and the
/// assertion below is what keeps this honest either way.
const SECRET_LINE: &str = "NL02ABNA0123456789";

/// PDF/A-1B, not 2B, and the reason is on the page: PDF/A-1 forbids
/// transparency, so the watermark here is fully opaque. That is the constraint a
/// reader hits the first time they watermark an archival copy, which makes it
/// worth demonstrating rather than avoiding.
const PROFILE: PdfAProfile = PdfAProfile::A1b;

/// Chrome's binary name differs per platform and per install; try the usual
/// ones rather than making the reader edit the example first. Same list as
/// `html_to_pdf_via_chrome.rs`, for the same reason.
const CHROME_CANDIDATES: &[&str] = &[
    "google-chrome",
    "google-chrome-stable",
    "chromium",
    "chromium-browser",
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
];

fn find_chrome() -> Option<String> {
    CHROME_CANDIDATES
        .iter()
        .find(|c| {
            Path::new(c).exists()
                || Command::new(c)
                    .arg("--version")
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false)
        })
        .map(|c| (*c).to_string())
}

/// Print `html` to `pdf` with headless Chrome.
///
/// `--no-pdf-header-footer` matters: without it Chrome stamps the file URL and a
/// page number into the margin, and the file URL is somebody's home directory.
fn print_to_pdf(chrome: &str, html: &Path, pdf: &Path) -> Result<(), String> {
    let out = Command::new(chrome)
        .args([
            "--headless",
            "--disable-gpu",
            "--no-sandbox",
            "--no-pdf-header-footer",
            "--run-all-compositor-stages-before-draw",
            "--virtual-time-budget=4000",
        ])
        .arg(format!("--print-to-pdf={}", pdf.display()))
        .arg(file_url(html))
        .output()
        .map_err(|e| format!("could not run {chrome}: {e}"))?;

    if !pdf.is_file() {
        return Err(format!(
            "Chrome exited {} and wrote no PDF:\n{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}

/// A `file://` URL for an absolute path. Chrome will not load a relative one,
/// and the composite's `<img src>` needs the same treatment.
fn file_url(path: &Path) -> String {
    format!("file://{}", path.display())
}

fn render_page_1(doc: &PdfDocument, dpi: u32, to: &Path) -> Result<(), String> {
    let png = doc
        .render_page(1, dpi, ImageFormat::Png)
        .map_err(|e| format!("render failed: {e}"))?;
    std::fs::write(to, &png).map_err(|e| format!("could not write {}: {e}", to.display()))?;
    Ok(())
}

fn run() -> Result<(), String> {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("cannot find the repository root from CARGO_MANIFEST_DIR")?
        .to_path_buf();

    let mut out = repo.join("docs/readme-hero.png");
    let mut keep: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out = PathBuf::from(args.next().ok_or("--out needs a path")?),
            "--keep" => keep = Some(PathBuf::from(args.next().ok_or("--keep needs a path")?)),
            other => return Err(format!("unknown argument {other}")),
        }
    }

    let chrome = find_chrome().ok_or(
        "no Chrome or Chromium found. This example needs one for the two print steps; \
         PDFluent does the rest.",
    )?;

    // A scratch directory the caller can keep. When something looks wrong in the
    // final image, the intermediate PDFs and panels are what tell you which of
    // the six steps went wrong -- and they are the first thing you do not have
    // if the program tidied them away.
    let work = match &keep {
        Some(p) => p.clone(),
        None => std::env::temp_dir().join("pdfluent-readme-hero"),
    };
    std::fs::create_dir_all(&work)
        .map_err(|e| format!("could not make {}: {e}", work.display()))?;

    let source_html = repo.join("docs/hero/hero_document.html");
    let source_pdf = work.join("statement.pdf");
    println!("1/6  Chrome prints {}", source_html.display());
    print_to_pdf(&chrome, &source_html, &source_pdf)?;

    println!("2/6  PDFluent renders the left panel");
    let before = PdfDocument::open(&source_pdf).map_err(|e| format!("open failed: {e}"))?;
    let before_png = work.join("panel-before.png");
    render_page_1(&before, PANEL_DPI, &before_png)?;

    println!("3/6  PDFluent redacts, watermarks and archives");
    let mut working = PdfDocument::open(&source_pdf).map_err(|e| format!("open failed: {e}"))?;

    let bank_line = working
        .text_with_layout()
        .map_err(|e| format!("could not read the text layout: {e}"))?
        .into_iter()
        .find(|b| b.text.contains(SECRET_LINE))
        .ok_or("the bank line is not in the document any more -- hero_document.html changed")?;
    // A couple of points of margin: a glyph's ink can sit a hair outside the box
    // its own metrics claim, and a redaction that leaves a sliver of ink is the
    // kind that gets read off a screenshot.
    let [x0, y0, x1, y1] = bank_line.bbox;
    working
        .redact_region(bank_line.page, [x0 - 2.0, y0 - 2.0, x1 + 2.0, y1 + 2.0])
        .map_err(|e| format!("redaction failed: {e}"))?;

    working
        .add_watermark(
            "SPECIMEN",
            WatermarkOptions::centered()
                .rotated(30.0)
                // Opaque, because PDF/A-1 forbids transparency -- and in
                // front, because Chrome paints an opaque white rectangle for
                // the page background and anything behind it is invisible.
                // Light enough that the text it crosses stays readable.
                .opacity(1.0)
                .layer(Layer::Foreground)
                .font_size(56.0)
                .color(0.86, 0.86, 0.86),
        )
        .map_err(|e| format!("watermark failed: {e}"))?;

    let archived = working
        .convert_to_pdfa(PROFILE)
        .map_err(|e| format!("PDF/A conversion failed: {e}"))?;
    let report = archived
        .validate_pdfa(PROFILE)
        .map_err(|e| format!("PDF/A validation failed: {e}"))?;

    // The caption says what the validator said. A conversion that came out
    // non-compliant is a finding, not a caption to paper over, so it stops here
    // rather than shipping a picture that claims otherwise.
    if !report.is_compliant() {
        let why = report
            .violations
            .iter()
            .map(|v| format!("[{}] {}", v.rule, v.message))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!(
            "the converted document is not PDF/A-1B compliant, so the hero image would \
             be claiming something untrue. Validator said: {why}"
        ));
    }
    let verdict = "PDF/A-1B, and validate_pdfa() agrees.";

    // The redaction is checked, not assumed. A redaction that leaves the text
    // extractable is the worst outcome of the three -- worse than no redaction,
    // because the black box says the job was done. If the string survived, the
    // picture would be showing exactly that, so nothing gets written.
    let text = archived
        .text()
        .map_err(|e| format!("could not read the archived text back: {e}"))?;
    if text.contains(SECRET_LINE) {
        return Err(format!(
            "{SECRET_LINE} is still extractable from the archived document. The right \
             panel would be showing a redaction that did not happen."
        ));
    }

    println!("4/6  PDFluent renders the right panel");
    let after_png = work.join("panel-after.png");
    render_page_1(&archived, PANEL_DPI, &after_png)?;

    println!("5/6  Chrome prints the composite");
    let template = repo.join("docs/hero/hero_composite.html");
    let composite_html = work.join("composite.html");
    let page_size = format!("{}in {}in", CANVAS.0 / 96.0, CANVAS.1 / 96.0);
    let filled = std::fs::read_to_string(&template)
        .map_err(|e| format!("could not read {}: {e}", template.display()))?
        .replace("{{BEFORE_PNG}}", &file_url(&before_png))
        .replace("{{AFTER_PNG}}", &file_url(&after_png))
        .replace("{{VERDICT}}", verdict)
        .replace("{{CANVAS_W}}", &format!("{}", CANVAS.0))
        .replace("{{CANVAS_H}}", &format!("{}", CANVAS.1))
        .replace("{{PAGE_SIZE}}", &page_size);
    std::fs::write(&composite_html, filled)
        .map_err(|e| format!("could not write {}: {e}", composite_html.display()))?;
    let composite_pdf = work.join("composite.pdf");
    print_to_pdf(&chrome, &composite_html, &composite_pdf)?;

    println!("6/6  PDFluent renders the hero");
    let composite = PdfDocument::open(&composite_pdf).map_err(|e| format!("open failed: {e}"))?;
    let hero = composite
        .render_page(1, HERO_DPI, ImageFormat::Png)
        .map_err(|e| format!("render failed: {e}"))?;

    if hero.len() > MAX_BYTES {
        return Err(format!(
            "the hero came out at {} kB, over the {} kB the README budgets for its first \
             screen. Nothing is written: a file the guard rejects is worse than none.",
            hero.len() / 1024,
            MAX_BYTES / 1024
        ));
    }
    std::fs::write(&out, &hero).map_err(|e| format!("could not write {}: {e}", out.display()))?;

    println!(
        "\nwrote {} ({} kB of the {} kB budget); working files in {}",
        out.display(),
        hero.len() / 1024,
        MAX_BYTES / 1024,
        work.display()
    );
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("readme_hero: {e}");
        std::process::exit(1);
    }
}
