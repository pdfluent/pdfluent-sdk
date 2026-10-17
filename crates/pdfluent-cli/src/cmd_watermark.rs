//! `watermark`: text or image watermarks, thin over
//! [`add_watermark`](pdfluent::PdfDocument::add_watermark) and
//! [`insert_image`](pdfluent::PdfDocument::insert_image).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use clap::Args;
use serde_json::json;

use crate::imagedims::{self, ImageKind};
use crate::input;
use crate::output::{self, Ctx};

/// Watermark placement on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Place {
    /// Centred (default).
    Center,
    /// Top-left corner.
    TopLeft,
    /// Top-right corner.
    TopRight,
    /// Bottom-left corner.
    BottomLeft,
    /// Bottom-right corner.
    BottomRight,
    /// Exact coordinates (needs `--at X,Y`).
    Exact,
}

/// Watermark layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Layer {
    /// In front of the content.
    Foreground,
    /// Behind the content.
    Background,
}

#[derive(Debug, Args)]
pub struct WatermarkArgs {
    /// Input PDF, or `-` for stdin.
    #[arg(required = true)]
    pub input: String,

    /// Output PDF, or `-` for stdout.
    #[arg(long, short = 'o', required = true)]
    pub out: String,

    /// Watermark text (exactly one of --text/--image).
    #[arg(long, conflicts_with = "image")]
    pub text: Option<String>,

    /// Watermark image (PNG or JPEG; exactly one of --text/--image).
    #[arg(long, value_name = "FILE", conflicts_with = "text")]
    pub image: Option<String>,

    /// Placement on the page.
    #[arg(long, value_enum, default_value = "center")]
    pub position: Place,

    /// Coordinates in PDF points: inset from the corner for corner
    /// placements (default `36,36`), absolute for `exact` (required).
    #[arg(long, value_name = "X,Y")]
    pub at: Option<String>,

    /// Rotation in degrees (text only).
    #[arg(long, default_value = "0")]
    pub rotate: f32,

    /// Opacity from 0 (invisible) to 1 (opaque).
    #[arg(long, default_value = "0.5")]
    pub opacity: f64,

    /// Foreground or background placement (text only).
    #[arg(long, value_enum, default_value = "foreground")]
    pub layer: Layer,

    /// Font size in points (text only).
    #[arg(long, default_value = "48")]
    pub font_size: f32,

    /// Text colour as RRGGBB hex (text only).
    #[arg(long, default_value = "999999")]
    pub color: String,

    /// Image display width in points (image only; default scales the image
    /// to half the page width, keeping its aspect).
    #[arg(long)]
    pub width: Option<f64>,

    /// Image display height in points (image only; default keeps aspect).
    #[arg(long)]
    pub height: Option<f64>,

    /// Overwrite the existing output file.
    #[arg(long)]
    pub force: bool,
}

pub(crate) fn run(ctx: &Ctx, args: &WatermarkArgs) -> u8 {
    if args.opacity < 0.0 || args.opacity > 1.0 || !args.opacity.is_finite() {
        return output::error("watermark", ctx, "BAD_USAGE", "--opacity takes 0 through 1");
    }
    match (&args.text, &args.image) {
        (Some(_), Some(_)) => {
            // Unreachable: clap conflicts. Kept for the error shape.
            return output::error(
                "watermark",
                ctx,
                "BAD_USAGE",
                "pass exactly one of --text or --image",
            );
        }
        (None, None) => {
            return output::error(
                "watermark",
                ctx,
                "BAD_USAGE",
                "pass exactly one of --text or --image",
            );
        }
        _ => {}
    }
    let loaded = match input::load("watermark", ctx, &args.input) {
        Ok(loaded) => loaded,
        Err(code) => return code,
    };
    let mut doc = match input::open("watermark", ctx, &loaded) {
        Ok(doc) => doc,
        Err(code) => return code,
    };
    let mut warnings = Vec::new();
    // Not fatal: centre plus coordinates is contradictory input, and
    // refusing costs a re-run for something with an obvious meaning.
    if args.position == Place::Center && args.at.is_some() {
        warnings.push("--at is ignored with --position center".to_string());
    }
    if let Some(text) = args.text.as_deref() {
        if text.is_empty() {
            return output::error("watermark", ctx, "BAD_USAGE", "--text needs words");
        }
        if args.width.is_some() || args.height.is_some() {
            return output::error(
                "watermark",
                ctx,
                "BAD_USAGE",
                "--width/--height only apply to --image",
            );
        }
        let color = match parse_color(&args.color) {
            Ok(color) => color,
            Err(msg) => return output::error("watermark", ctx, "BAD_USAGE", &msg),
        };
        let position = match resolve_position(args) {
            Ok(position) => position,
            Err(msg) => return output::error("watermark", ctx, "BAD_USAGE", &msg),
        };
        let opts = pdfluent::WatermarkOptions::centered()
            .at(position)
            .rotated(args.rotate)
            .opacity(args.opacity as f32)
            .layer(match args.layer {
                Layer::Foreground => pdfluent::Layer::Foreground,
                Layer::Background => pdfluent::Layer::Background,
            })
            .font_size(args.font_size)
            .color(color.0, color.1, color.2);
        if let Err(e) = doc.add_watermark(text, opts) {
            return output::facade_error("watermark", ctx, &e);
        }
    } else if let Some(image) = args.image.as_deref() {
        let bytes = match std::fs::read(image) {
            Ok(bytes) => bytes,
            Err(e) => {
                return output::error(
                    "watermark",
                    ctx,
                    match e.kind() {
                        std::io::ErrorKind::NotFound => "FILE_NOT_FOUND",
                        _ => "UNREADABLE_INPUT",
                    },
                    &format!("could not read image {image}: {e}"),
                );
            }
        };
        let (pixel_w, pixel_h, kind) = match imagedims::dimensions(&bytes) {
            Ok(dims) => dims,
            Err(msg) => return output::error("watermark", ctx, "UNREADABLE_INPUT", &msg),
        };
        let format = match kind {
            ImageKind::Png => pdfluent::InsertImageFormat::Png,
            ImageKind::Jpeg => pdfluent::InsertImageFormat::Jpeg,
        };
        let pages = doc.page_count();
        for page in 1..=pages {
            let (page_w, page_h) = match doc.page(page) {
                Ok(handle) => handle.dimensions(),
                Err(e) => return output::facade_error("watermark", ctx, &e),
            };
            let (w, h) = display_size(args, pixel_w, pixel_h, page_w, page_h);
            let (x, y) = match image_xy(args, page_w, page_h, w, h) {
                Ok(xy) => xy,
                Err(msg) => return output::error("watermark", ctx, "BAD_USAGE", &msg),
            };
            let insert = pdfluent::ImageInsert::new(bytes.clone(), format, page, x, y, w, h)
                .with_opacity(args.opacity.clamp(0.0, 1.0));
            if let Err(e) = doc.insert_image(insert) {
                return output::facade_error("watermark", ctx, &e);
            }
            output::verbose(
                ctx,
                &format!("watermark: image on page {page} at {x:.0},{y:.0}"),
            );
        }
    }
    let page_count = doc.page_count();
    let saved = match input::save(
        "watermark",
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
    output::success(
        "watermark",
        ctx,
        json!({
            "input": loaded.display,
            "page_count": page_count,
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

fn parse_xy(spec: &str) -> Result<(f32, f32), String> {
    let (x, y) = spec
        .split_once(',')
        .ok_or_else(|| format!("`{spec}` is not X,Y (want e.g. `36,36`)"))?;
    let x: f32 = x
        .trim()
        .parse()
        .map_err(|_| format!("`{spec}` is not X,Y (want e.g. `36,36`)"))?;
    let y: f32 = y
        .trim()
        .parse()
        .map_err(|_| format!("`{spec}` is not X,Y (want e.g. `36,36`)"))?;
    Ok((x, y))
}

fn offsets(args: &WatermarkArgs) -> Result<(f32, f32), String> {
    match args.at.as_deref() {
        Some(spec) => parse_xy(spec),
        None => Ok((36.0, 36.0)),
    }
}

fn resolve_position(args: &WatermarkArgs) -> Result<pdfluent::Position, String> {
    match args.position {
        Place::Center => Ok(pdfluent::Position::Center),
        Place::TopLeft => offsets(args).map(|(x, y)| pdfluent::Position::TopLeft(x, y)),
        Place::TopRight => offsets(args).map(|(x, y)| pdfluent::Position::TopRight(x, y)),
        Place::BottomLeft => offsets(args).map(|(x, y)| pdfluent::Position::BottomLeft(x, y)),
        Place::BottomRight => offsets(args).map(|(x, y)| pdfluent::Position::BottomRight(x, y)),
        Place::Exact => match args.at.as_deref() {
            Some(spec) => parse_xy(spec).map(|(x, y)| pdfluent::Position::Exact(x, y)),
            None => Err("--position exact needs --at X,Y".to_string()),
        },
    }
}

/// Image bottom-left in points for the placement. PDF coordinates start at
/// the bottom-left; corner insets mirror the text positions.
fn image_xy(
    args: &WatermarkArgs,
    page_w: f64,
    page_h: f64,
    w: f64,
    h: f64,
) -> Result<(f64, f64), String> {
    let (ox, oy) = offsets(args)?;
    let (ox, oy) = (ox as f64, oy as f64);
    match args.position {
        Place::Center => Ok(((page_w - w) / 2.0, (page_h - h) / 2.0)),
        Place::TopLeft => Ok((ox, page_h - oy - h)),
        Place::TopRight => Ok((page_w - ox - w, page_h - oy - h)),
        Place::BottomLeft => Ok((ox, oy)),
        Place::BottomRight => Ok((page_w - ox - w, oy)),
        Place::Exact => match args.at.as_deref() {
            Some(_) => Ok((ox, oy)),
            None => Err("--position exact needs --at X,Y".to_string()),
        },
    }
}

/// Display size in points: explicit flags win, one flag keeps aspect,
/// neither scales the image to half the page width.
fn display_size(
    args: &WatermarkArgs,
    pixel_w: u32,
    pixel_h: u32,
    page_w: f64,
    page_h: f64,
) -> (f64, f64) {
    let aspect = pixel_h as f64 / pixel_w.max(1) as f64;
    match (args.width, args.height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, w * aspect),
        (None, Some(h)) => (h / aspect.max(f64::EPSILON), h),
        (None, None) => {
            let w = page_w.min(page_h) * 0.5;
            (w, w * aspect)
        }
    }
}

fn parse_color(spec: &str) -> Result<(f32, f32, f32), String> {
    let hex = spec.strip_prefix('#').unwrap_or(spec);
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("`{spec}` is not RRGGBB hex (want e.g. `999999`)"));
    }
    let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0) as f32 / 255.0;
    Ok((channel(0), channel(2), channel(4)))
}
