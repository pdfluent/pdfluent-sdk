//! Render PDF pages to PNG images.

use anyhow::{Context, Result};
use std::path::Path;

use crate::error::CliError;
use pdf_engine::{EngineError, PdfDocument, RenderOptions};

fn try_xfa_flatten(data: &[u8]) -> Option<Vec<u8>> {
    // GL-QA36: wrap in catch_unwind so a panic inside the XFA engine
    // (e.g. LayoutFailed on stressful files) becomes None instead of SIGABRT.
    std::panic::catch_unwind(|| pdf_xfa::flatten_xfa_to_pdf(data))
        .ok()
        .and_then(|r| r.ok())
}

pub fn run(input: &Path, output: &Path, dpi: f64, pages: Option<&str>) -> Result<()> {
    let data = std::fs::read(input).map_err(|e| {
        anyhow::anyhow!(CliError {
            message: format!("Could not read input PDF: {}", input.display()),
            why: Some(e.to_string()),
            fix: Some("Check if the file exists and is readable.".to_string()),
            docs: Some("https://docs.pdfluent.com/errors/E001".to_string()),
        })
    })?;

    let doc = match PdfDocument::open(data.clone()) {
        Ok(doc) => doc,
        Err(e) => {
            // GL-QA35: Encrypted and InvalidPageGeometry must propagate as
            // EngineError so main.rs can map them to exit codes 2 and 3.
            // Wrapping them in CliError loses the concrete type and causes
            // downcast_ref to return None → wrong exit code.
            if matches!(e, EngineError::Encrypted(_) | EngineError::InvalidPageGeometry { .. }) {
                return Err(anyhow::anyhow!(e));
            }
            if let Some(flattened) = try_xfa_flatten(&data) {
                PdfDocument::open(flattened).map_err(|open_err| {
                    anyhow::anyhow!(EngineError::XfaFlattenFailed(format!(
                        "{}: {}",
                        input.display(),
                        open_err
                    )))
                })?
            } else {
                return Err(anyhow::anyhow!(CliError {
                    message: format!("Could not open PDF: {}", input.display()),
                    why: Some(e.to_string()),
                    fix: Some(
                        "Ensure the file is a valid PDF document and not corrupted.".to_string()
                    ),
                    docs: Some("https://docs.pdfluent.com/errors/E005".to_string()),
                }));
            }
        }
    };
    let total = doc.page_count();

    // GL-QA37: A document with zero pages cannot produce output; treat as
    // InvalidPageGeometry (exit 3) rather than silently succeeding.
    if total == 0 {
        return Err(anyhow::anyhow!(EngineError::InvalidPageGeometry {
            width: 0.0,
            height: 0.0,
            reason: "document has zero pages".to_string(),
        }));
    }

    let page_indices = match pages {
        Some(s) => crate::parse_page_list(s, total)?,
        None => (0..total).collect(),
    };

    let opts = RenderOptions {
        dpi,
        ..Default::default()
    };

    if output != Path::new(".") {
        std::fs::create_dir_all(output).context("failed to create output directory")?;
    }

    for &idx in &page_indices {
        let rendered = doc
            .render_page(idx, &opts)
            .context(format!("failed to render page {}", idx + 1))?;

        let img = image::RgbaImage::from_raw(rendered.width, rendered.height, rendered.pixels)
            .context("failed to create image from rendered pixels")?;

        let filename = if output == Path::new(".") {
            format!("page-{}.png", idx + 1)
        } else {
            output
                .join(format!("page-{}.png", idx + 1))
                .to_string_lossy()
                .to_string()
        };
        img.save(&filename)
            .context(format!("failed to save {filename}"))?;

        println!(
            "  page {} -> {} ({}x{})",
            idx + 1,
            filename,
            rendered.width,
            rendered.height
        );
    }

    println!("Rendered {} pages at {dpi} DPI", page_indices.len());
    Ok(())
}
