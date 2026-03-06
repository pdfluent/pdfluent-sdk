//! Render all XFA PDFs in the corpus to PNG images.
//!
//! This is used by the AVRT (Automated Visual Regression Testing) pipeline
//! to generate engine renders that are compared against Adobe gold masters.
//!
//! Usage:
//!   cargo run --release --bin corpus-render -- --corpus corpus/ --output renders/

use anyhow::Result;
use clap::Parser;
use pdfium_ffi_bridge::native_renderer::RenderConfig;
use pdfium_ffi_bridge::pipeline;
use pdfium_ffi_bridge::template_parser;
use pdfium_ffi_bridge::xfa_extract;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "corpus-render", about = "Render XFA PDFs to PNG for AVRT")]
struct Args {
    /// Path to corpus directory
    #[arg(short, long, default_value = "corpus")]
    corpus: PathBuf,

    /// Output directory for rendered PNGs
    #[arg(short, long, default_value = "renders")]
    output: PathBuf,

    /// Scale factor (1.0 = 72 DPI, 2.0 = 144 DPI)
    #[arg(long, default_value = "2.0")]
    scale: f64,

    /// Only render files matching this prefix
    #[arg(long)]
    filter: Option<String>,

    /// Output JSON summary
    #[arg(long)]
    json: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    std::fs::create_dir_all(&args.output)?;

    let mut entries: Vec<PathBuf> = std::fs::read_dir(&args.corpus)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "pdf"))
        .collect();
    entries.sort();

    if let Some(ref filter) = args.filter {
        entries.retain(|p| {
            p.file_stem()
                .is_some_and(|s| s.to_string_lossy().starts_with(filter.as_str()))
        });
    }

    let total = entries.len();
    let mut rendered = 0u32;
    let mut failed = 0u32;
    let mut skipped = 0u32;

    let config = RenderConfig {
        scale: args.scale,
        ..RenderConfig::default()
    };

    for (i, path) in entries.iter().enumerate() {
        let stem = path.file_stem().unwrap().to_string_lossy();
        eprint!("[{}/{}] {}... ", i + 1, total, stem);

        let data = std::fs::read(path)?;

        // Try to extract XFA
        let packets = match xfa_extract::scan_pdf_for_xfa(&data) {
            Ok(Some(p)) => p,
            Ok(None) => {
                eprintln!("SKIP (no XFA)");
                skipped += 1;
                continue;
            }
            Err(e) => {
                eprintln!("SKIP (extract error: {})", e);
                skipped += 1;
                continue;
            }
        };

        let template = match packets.template() {
            Some(t) => t,
            None => {
                eprintln!("SKIP (no template)");
                skipped += 1;
                continue;
            }
        };

        // Parse and render
        match template_parser::parse_template(template, packets.datasets()) {
            Ok((mut tree, root)) => match pipeline::render_form_tree(&mut tree, root, &config) {
                Ok(images) => {
                    let prefix = stem.to_string();
                    match pipeline::save_pages_as_png(&images, &args.output, &prefix) {
                        Ok(paths) => {
                            eprintln!("OK ({} pages)", paths.len());
                            rendered += 1;
                        }
                        Err(e) => {
                            eprintln!("FAIL (save: {})", e);
                            failed += 1;
                        }
                    }
                }
                Err(e) => {
                    eprintln!("FAIL (render: {})", e);
                    failed += 1;
                }
            },
            Err(e) => {
                eprintln!("FAIL (parse: {})", e);
                failed += 1;
            }
        }
    }

    eprintln!();
    eprintln!(
        "Results: {rendered} rendered, {failed} failed, {skipped} skipped (of {total} total)"
    );

    if args.json {
        let summary = serde_json::json!({
            "total": total,
            "rendered": rendered,
            "failed": failed,
            "skipped": skipped,
        });
        println!("{}", serde_json::to_string_pretty(&summary)?);
    }

    Ok(())
}
