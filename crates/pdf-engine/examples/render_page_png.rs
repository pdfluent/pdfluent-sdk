//! One page as a PNG, for the best tier's detector (#641).
//!
//! An example rather than a subcommand, for the reason `structured_markdown`
//! next door gives: the benchmark needs our rendering in the form the detector
//! reads, and what the command line should offer is a separate question.
//!
//! Run: `render_page_png <pdf> <out.png> [--page N] [--dpi DPI]`. Page N counts
//! from zero (default 0, the first page); DPI defaults to 96. The page is
//! rendered with `PdfDocument::render_page` -- RGB on white -- and written as
//! an 8-bit RGB PNG with the `png` crate, in-process. No shell-out: unlike
//! `render_ours`, which converts through macOS `sips`, this is the binary the
//! best tier measures, so its bytes must not depend on a tool outside this
//! tree.
//!
//! Exit 1 on a read, render or encode failure, with the message on stderr;
//! exit 2 on a wrong invocation. `main` is only the exit: [`run_with_args`]
//! holds the parsing so the integration test drives the real thing in-process.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_engine::{PdfDocument, PixelFormat, RenderOptions};
use std::io::Write;

/// The program, minus the exit: `args` without the argv[0], the receipt to
/// `out`, diagnostics to `err`. Returns the exit code.
pub fn run_with_args(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let mut page = 0usize;
    let mut dpi = 96.0f64;
    let mut positionals: Vec<&str> = Vec::new();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--page" => match rest.next().and_then(|n| n.parse().ok()) {
                Some(n) => page = n,
                None => return usage(err),
            },
            "--dpi" => match rest
                .next()
                .and_then(|n| n.parse().ok())
                .filter(|n: &f64| *n > 0.0)
            {
                Some(n) => dpi = n,
                None => return usage(err),
            },
            other if other.starts_with("--page=") => match other["--page=".len()..].parse() {
                Ok(n) => page = n,
                Err(_) => return usage(err),
            },
            other if other.starts_with("--dpi=") => {
                match other["--dpi=".len()..]
                    .parse()
                    .ok()
                    .filter(|n: &f64| *n > 0.0)
                {
                    Some(n) => dpi = n,
                    None => return usage(err),
                }
            }
            other if other.starts_with("--") => return usage(err),
            other => positionals.push(other),
        }
    }
    let [pdf, out_png] = positionals.as_slice() else {
        return usage(err);
    };
    let bytes = match std::fs::read(pdf) {
        Ok(bytes) => bytes,
        Err(read_err) => {
            let _ = writeln!(err, "read {pdf}: {read_err}");
            return 1;
        }
    };
    let doc = match PdfDocument::open(bytes) {
        Ok(doc) => doc,
        Err(open_err) => {
            let _ = writeln!(err, "open {pdf}: {open_err}");
            return 1;
        }
    };
    let opts = RenderOptions {
        dpi,
        background: [1.0, 1.0, 1.0, 1.0],
        ..Default::default()
    };
    let rendered = match doc.render_page(page, &opts) {
        Ok(rendered) => rendered,
        Err(render_err) => {
            let _ = writeln!(err, "render {pdf} page {}: {render_err}", page + 1);
            return 1;
        }
    };
    if rendered.pixel_format != PixelFormat::Rgba8 {
        let _ = writeln!(err, "render {pdf}: expected RGBA pixels");
        return 1;
    }
    // Premultiplied RGBA composited over the white background: with an opaque
    // page this is the channels as they stand; anywhere the alpha is short
    // the white shows through by exactly the missing share.
    let mut rgb = Vec::with_capacity(rendered.pixels.len() / 4 * 3);
    for px in rendered.pixels.chunks_exact(4) {
        let missing = 255 - px[3];
        rgb.push(px[0].saturating_add(missing));
        rgb.push(px[1].saturating_add(missing));
        rgb.push(px[2].saturating_add(missing));
    }
    let file = match std::fs::File::create(out_png) {
        Ok(file) => std::io::BufWriter::new(file),
        Err(write_err) => {
            let _ = writeln!(err, "write {out_png}: {write_err}");
            return 1;
        }
    };
    let mut encoder = png::Encoder::new(file, rendered.width, rendered.height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = match encoder.write_header() {
        Ok(writer) => writer,
        Err(encode_err) => {
            let _ = writeln!(err, "encode {out_png}: {encode_err}");
            return 1;
        }
    };
    if let Err(encode_err) = writer.write_image_data(&rgb) {
        let _ = writeln!(err, "encode {out_png}: {encode_err}");
        return 1;
    }
    let _ = writeln!(out, "{out_png}: {}x{}", rendered.width, rendered.height);
    0
}

fn usage(err: &mut dyn Write) -> i32 {
    let _ = writeln!(
        err,
        "usage: render_page_png <pdf> <out.png> [--page N] [--dpi DPI]"
    );
    2
}

/// Absent under `cfg(test)`, when the integration test includes this file as
/// a module to drive [`run_with_args`] in-process.
#[cfg(not(test))]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(run_with_args(
        &args,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    ));
}
