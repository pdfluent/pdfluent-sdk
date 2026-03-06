//! Render PDF pages to PNG images.

use anyhow::{Context, Result};
use std::path::Path;

use pdf_engine::{PdfDocument, RenderOptions};

pub fn run(input: &Path, output: &Path, dpi: f64, pages: Option<&str>) -> Result<()> {
    let data = std::fs::read(input).context("failed to read input PDF")?;
    let doc = PdfDocument::open(data).context("failed to open PDF")?;
    let total = doc.page_count();

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
