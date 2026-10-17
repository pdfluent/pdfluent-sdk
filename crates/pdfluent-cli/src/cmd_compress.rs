//! `compress`: file-size reduction, thin over
//! [`compress`](pdfluent::PdfDocument::compress).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use clap::Args;
use serde_json::json;

use crate::input;
use crate::output::{self, Ctx};

/// Compression preset, straight from the facade's named presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Level {
    /// Full stack, no lossy passes.
    Strict,
    /// Everything strict enables, plus future lossy passes.
    Lossy,
    /// Maximally safe: keeps unused objects incremental updates may need.
    Archival,
}

impl From<Level> for pdfluent::CompressOptions {
    fn from(value: Level) -> Self {
        match value {
            Level::Strict => pdfluent::CompressOptions::strict(),
            Level::Lossy => pdfluent::CompressOptions::lossy(),
            Level::Archival => pdfluent::CompressOptions::archival(),
        }
    }
}

#[derive(Debug, Args)]
pub struct CompressArgs {
    /// Input PDF, or `-` for stdin.
    #[arg(required = true)]
    pub input: String,

    /// Output PDF, or `-` for stdout.
    #[arg(long, short = 'o', required = true)]
    pub out: String,

    /// Compression preset.
    #[arg(long, value_enum, default_value = "strict")]
    pub level: Level,

    /// Overwrite the existing output file.
    #[arg(long)]
    pub force: bool,
}

pub(crate) fn run(ctx: &Ctx, args: &CompressArgs) -> u8 {
    let loaded = match input::load("compress", ctx, &args.input) {
        Ok(loaded) => loaded,
        Err(code) => return code,
    };
    let bytes_before = loaded.bytes.len() as u64;
    let mut doc = match input::open("compress", ctx, &loaded) {
        Ok(doc) => doc,
        Err(code) => return code,
    };
    let report = match doc.compress(args.level.into()) {
        Ok(report) => report,
        Err(e) => return output::facade_error("compress", ctx, &e),
    };
    let saved = match input::save(
        "compress",
        ctx,
        &doc,
        &args.out,
        loaded.path.as_slice(),
        args.force,
    ) {
        Ok(saved) => saved,
        Err(code) => return code,
    };
    let path = saved.path;
    let bytes_after = saved.bytes_written;
    output::verbose(
        ctx,
        &format!(
            "compress: {bytes_before} -> {bytes_after} bytes ({} streams, {} deduped, {} unused removed)",
            report.streams_compressed, report.streams_deduplicated, report.unused_removed,
        ),
    );
    output::success(
        "compress",
        ctx,
        json!({
            "input": loaded.display,
            "bytes_before": bytes_before,
            "bytes_after": bytes_after,
            "streams_compressed": report.streams_compressed,
            "streams_deduplicated": report.streams_deduplicated,
            "unused_removed": report.unused_removed,
            "fonts_subsetted": report.font_subset.as_ref().map(|f| f.fonts_subsetted).unwrap_or(0),
            "output_path": path.as_ref().map(|p| p.display().to_string()),
        }),
        vec![],
        |ctx| {
            if let Some(path) = path {
                output::note(
                    ctx,
                    &format!(
                        "Wrote {} ({bytes_before} -> {bytes_after} bytes)",
                        path.display()
                    ),
                );
            } else if !ctx.quiet {
                // Stdout holds the PDF bytes, so the size goes to stderr:
                // without it `-o -` would report nothing at all.
                eprintln!("compressed {bytes_before} -> {bytes_after} bytes");
            }
        },
    )
}
