//! `convert`: office, image, and PDF/A output, thin over
//! [`to_docx`](pdfluent::PdfDocument::to_docx),
//! [`to_xlsx`](pdfluent::PdfDocument::to_xlsx),
//! [`to_pptx`](pdfluent::PdfDocument::to_pptx),
//! [`to_images`](pdfluent::PdfDocument::to_images), and
//! [`convert_to_pdfa`](pdfluent::PdfDocument::convert_to_pdfa).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::path::PathBuf;

use clap::Args;
use serde_json::json;

use crate::input;
use crate::output::{self, Ctx};

/// Conversion target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Target {
    /// Word document (`.docx`).
    Docx,
    /// Excel workbook (`.xlsx`).
    Xlsx,
    /// PowerPoint deck (`.pptx`).
    Pptx,
    /// PNG images, one per page (needs `-o <dir>`).
    Png,
    /// JPEG images, one per page (needs `-o <dir>`).
    Jpeg,
    /// Archival PDF (PDF/A).
    Pdfa,
}

/// PDF/A conformance profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Profile {
    /// PDF/A-1b.
    A1b,
    /// PDF/A-2b (default).
    A2b,
    /// PDF/A-3b.
    A3b,
}

impl From<Profile> for pdfluent::PdfAProfile {
    fn from(value: Profile) -> Self {
        match value {
            Profile::A1b => pdfluent::PdfAProfile::A1b,
            Profile::A2b => pdfluent::PdfAProfile::A2b,
            Profile::A3b => pdfluent::PdfAProfile::A3b,
        }
    }
}

/// Rasterization precision/speed trade-off for image output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Raster {
    /// Higher-precision f32 pipeline (default, historical output).
    Quality,
    /// Faster u8 pipeline (~1.4–1.6× on content-heavy pages).
    Speed,
}

impl From<Raster> for pdfluent::RasterQuality {
    fn from(value: Raster) -> Self {
        match value {
            Raster::Quality => pdfluent::RasterQuality::Quality,
            Raster::Speed => pdfluent::RasterQuality::Speed,
        }
    }
}

#[derive(Debug, Args)]
pub struct ConvertArgs {
    /// Input PDF, or `-` for stdin.
    #[arg(required = true)]
    pub input: String,

    /// Conversion target.
    #[arg(long, value_enum, required = true)]
    pub to: Target,

    /// Output file, or a directory for images (created when missing).
    /// Single-file targets also accept `-` for stdout.
    #[arg(long, short = 'o', required = true)]
    pub out: String,

    /// Render resolution for images (default 150).
    #[arg(long)]
    pub dpi: Option<u32>,

    /// PDF/A profile for `--to pdfa` (default a2b).
    #[arg(long, value_enum)]
    pub profile: Option<Profile>,

    /// Page range for images, e.g. `1-3` (single range only).
    #[arg(long)]
    pub pages: Option<String>,

    /// Raster pipeline for images: `quality` (default, f32) or `speed` (u8, faster).
    #[arg(long, value_enum)]
    pub raster: Option<Raster>,

    /// Overwrite existing output files.
    #[arg(long)]
    pub force: bool,
}

pub(crate) fn run(ctx: &Ctx, args: &ConvertArgs) -> u8 {
    let mut warnings = Vec::new();
    let is_images = matches!(args.to, Target::Png | Target::Jpeg);
    if args.dpi.is_some() && !is_images {
        warnings.push("--dpi only applies to image output; ignored".to_string());
    }
    if args.profile.is_some() && args.to != Target::Pdfa {
        warnings.push("--profile only applies to --to pdfa; ignored".to_string());
    }
    if args.pages.is_some() && !is_images {
        warnings.push("--pages only applies to image output; ignored".to_string());
    }
    if args.raster.is_some() && !is_images {
        warnings.push("--raster only applies to image output; ignored".to_string());
    }
    if let Some(0) = args.dpi {
        return output::error("convert", ctx, "BAD_USAGE", "--dpi must be at least 1");
    }

    let loaded = match input::load("convert", ctx, &args.input) {
        Ok(loaded) => loaded,
        Err(code) => return code,
    };
    let doc = match input::open("convert", ctx, &loaded) {
        Ok(doc) => doc,
        Err(code) => return code,
    };

    match args.to {
        Target::Docx | Target::Xlsx | Target::Pptx => {
            let bytes = match args.to {
                Target::Docx => doc.to_docx_bytes(),
                Target::Xlsx => doc.to_xlsx_bytes(),
                _ => doc.to_pptx_bytes(),
            };
            let bytes = match bytes {
                Ok(bytes) => bytes,
                Err(e) => return output::facade_error("convert", ctx, &e),
            };
            let path = if args.out == "-" {
                if ctx.json {
                    return output::error(
                        "convert",
                        ctx,
                        "BAD_USAGE",
                        "binary conversion output and --json share stdout; pick one",
                    );
                }
                if let Err(code) = input::write_stdout("convert", ctx, &bytes) {
                    return code;
                }
                None
            } else {
                let path = PathBuf::from(&args.out);
                if let Err(code) = input::write_bytes(
                    "convert",
                    ctx,
                    &bytes,
                    &path,
                    loaded.path.as_slice(),
                    args.force,
                ) {
                    return code;
                }
                Some(path.display().to_string())
            };
            output::success(
                "convert",
                ctx,
                json!({
                    "input": loaded.display,
                    "to": target_name(args.to),
                    "bytes": bytes.len(),
                    "output_path": path,
                }),
                warnings,
                |ctx| {
                    if let Some(path) = path {
                        output::note(ctx, &format!("Wrote {path}"));
                    }
                },
            )
        }
        Target::Png | Target::Jpeg => {
            if args.out == "-" {
                return output::error(
                    "convert",
                    ctx,
                    "BAD_USAGE",
                    "image output is one file per page; pass -o <dir>",
                );
            }
            let dir = PathBuf::from(&args.out);
            if dir.is_file() {
                return output::error(
                    "convert",
                    ctx,
                    "BAD_USAGE",
                    "image output needs -o <dir>, not a file",
                );
            }
            if let Err(e) = std::fs::create_dir_all(&dir) {
                return output::error(
                    "convert",
                    ctx,
                    "IO",
                    &format!("could not create directory {}: {e}", dir.display()),
                );
            }
            let mut opts = pdfluent::ToImagesOptions::new().with_format(match args.to {
                Target::Png => pdfluent::ImageFormat::Png,
                _ => pdfluent::ImageFormat::Jpeg,
            });
            if let Some(dpi) = args.dpi {
                opts = opts.with_dpi(dpi);
            }
            if let Some(raster) = args.raster {
                opts = opts.with_quality(raster.into());
            }
            if let Some(spec) = args.pages.as_deref() {
                match crate::pagespec::parse_ranges(spec) {
                    Ok(ranges) if ranges.len() == 1 => {
                        opts = opts.with_pages(ranges[0].0, ranges[0].1);
                    }
                    Ok(_) => {
                        return output::error(
                            "convert",
                            ctx,
                            "BAD_USAGE",
                            "--pages for images takes a single range (e.g. `1-3`)",
                        );
                    }
                    Err(e) => {
                        let (code, msg) = match e {
                            crate::pagespec::SpecError::Syntax(msg) => ("BAD_USAGE", msg),
                            crate::pagespec::SpecError::OutOfRange(msg) => ("PAGE_RANGE", msg),
                        };
                        return output::error("convert", ctx, code, &msg);
                    }
                }
            }
            let stem = loaded
                .path
                .as_ref()
                .and_then(|p| p.file_stem())
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "page".to_string());
            let ext = match args.to {
                Target::Png => "png",
                _ => "jpg",
            };
            // The facade writes the image files itself, so it renders into a
            // staging directory and each finished file is guarded and moved
            // onto its target: no half-written images at their final names.
            let stage = match input::stage_dir("convert", ctx, &dir) {
                Ok(stage) => stage,
                Err(code) => return code,
            };
            let pattern = stage.join(format!("{stem}-p{{page}}.{ext}"));
            let report = match doc.to_images(&pattern, opts) {
                Ok(report) => report,
                Err(e) => {
                    let _ = std::fs::remove_dir_all(&stage);
                    return output::facade_error("convert", ctx, &e);
                }
            };
            let mut finals = Vec::with_capacity(report.paths.len());
            for staged in &report.paths {
                let Some(name) = staged.file_name() else {
                    let _ = std::fs::remove_dir_all(&stage);
                    return output::error(
                        "convert",
                        ctx,
                        "IO",
                        &format!(
                            "facade reported an unusable image path: {}",
                            staged.display()
                        ),
                    );
                };
                let target = dir.join(name);
                if let Err(code) =
                    input::guard_output("convert", ctx, loaded.path.as_slice(), &target, args.force)
                {
                    let _ = std::fs::remove_dir_all(&stage);
                    return code;
                }
                finals.push((staged.clone(), target));
            }
            let mut images = Vec::with_capacity(finals.len());
            for (staged, target) in &finals {
                if let Err(e) = std::fs::rename(staged, target) {
                    let _ = std::fs::remove_dir_all(&stage);
                    return output::error(
                        "convert",
                        ctx,
                        "IO",
                        &format!("could not write {}: {e}", target.display()),
                    );
                }
                images.push(target.display().to_string());
            }
            let _ = std::fs::remove_dir_all(&stage);
            output::success(
                "convert",
                ctx,
                json!({
                    "input": loaded.display,
                    "to": target_name(args.to),
                    "images": images,
                }),
                warnings,
                |ctx| {
                    for image in &images {
                        output::note(ctx, &format!("Wrote {image}"));
                    }
                },
            )
        }
        Target::Pdfa => {
            let profile = args.profile.unwrap_or(Profile::A2b);
            let archived = match doc.convert_to_pdfa(profile.into()) {
                Ok(doc) => doc,
                Err(e) => return output::facade_error("convert", ctx, &e),
            };
            let saved = match input::save(
                "convert",
                ctx,
                &archived,
                &args.out,
                loaded.path.as_slice(),
                args.force,
            ) {
                Ok(saved) => saved,
                Err(code) => return code,
            };
            let path = saved.path;
            output::success(
                "convert",
                ctx,
                json!({
                    "input": loaded.display,
                    "to": "pdfa",
                    "profile": profile_name(profile),
                    "page_count": archived.page_count(),
                    "output_path": path.as_ref().map(|p| p.display().to_string()),
                }),
                warnings,
                |ctx| {
                    if let Some(path) = path {
                        output::note(ctx, &format!("Wrote {}", path.display()));
                    }
                },
            )
        }
    }
}

fn target_name(target: Target) -> &'static str {
    match target {
        Target::Docx => "docx",
        Target::Xlsx => "xlsx",
        Target::Pptx => "pptx",
        Target::Png => "png",
        Target::Jpeg => "jpeg",
        Target::Pdfa => "pdfa",
    }
}

fn profile_name(profile: Profile) -> &'static str {
    match profile {
        Profile::A1b => "a1b",
        Profile::A2b => "a2b",
        Profile::A3b => "a3b",
    }
}
