//! XFA-Native-Rust CLI — PDF and XFA form processing toolkit.

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod cmd_demo;
mod cmd_extract;
mod cmd_fill;
mod cmd_flatten;
mod cmd_info;
mod cmd_render;
mod cmd_sign;
mod cmd_validate;

#[derive(Parser)]
#[command(
    name = "xfa-cli",
    version,
    about = "PDF and XFA form processing toolkit"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Render PDF pages to PNG images.
    Render {
        /// Input PDF file.
        input: PathBuf,
        /// Output directory for PNG files.
        #[arg(short, long, default_value = ".")]
        output: PathBuf,
        /// Resolution in DPI.
        #[arg(short, long, default_value_t = 150.0)]
        dpi: f64,
        /// Page selection (e.g. "1,3-5").
        #[arg(short, long)]
        pages: Option<String>,
    },
    /// Extract text from PDF pages.
    Extract {
        /// Input PDF file.
        input: PathBuf,
        /// Page selection (e.g. "1,3-5").
        #[arg(short, long)]
        pages: Option<String>,
        /// Output as JSON with text blocks.
        #[arg(long)]
        json: bool,
    },
    /// Fill AcroForm fields from a JSON file.
    Fill {
        /// Input PDF file.
        input: PathBuf,
        /// Output PDF file.
        #[arg(short, long)]
        output: PathBuf,
        /// JSON file with field name→value pairs.
        #[arg(short, long)]
        data: PathBuf,
    },
    /// Flatten form fields (remove interactive elements).
    Flatten {
        /// Input PDF file.
        input: PathBuf,
        /// Output PDF file.
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Display PDF document information.
    Info {
        /// Input PDF file.
        input: PathBuf,
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Validate PDF against compliance profiles (PDF/A, PDF/UA).
    Validate {
        /// Input PDF file.
        input: PathBuf,
        /// Compliance profile (e.g. pdf-a2b, pdf-ua).
        #[arg(short = 'P', long, default_value = "pdf-a2b")]
        profile: String,
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Validate digital signatures in a PDF.
    Sign {
        /// Input PDF file.
        input: PathBuf,
        /// Output as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Run the XFA engine demo pipeline.
    Demo,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Render {
            input,
            output,
            dpi,
            pages,
        } => cmd_render::run(&input, &output, dpi, pages.as_deref()),
        Commands::Extract { input, pages, json } => {
            cmd_extract::run(&input, pages.as_deref(), json)
        }
        Commands::Fill {
            input,
            output,
            data,
        } => cmd_fill::run(&input, &output, &data),
        Commands::Flatten { input, output } => cmd_flatten::run(&input, &output),
        Commands::Info { input, json } => cmd_info::run(&input, json),
        Commands::Validate {
            input,
            profile,
            json,
        } => cmd_validate::run(&input, &profile, json),
        Commands::Sign { input, json } => cmd_sign::run(&input, json),
        Commands::Demo => cmd_demo::run(),
    }
}

/// Parse a comma-separated page list (1-based) into 0-based indices.
/// Supports ranges like "1,3-5,8".
pub fn parse_page_list(s: &str, total: usize) -> Result<Vec<usize>> {
    let mut result = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        if let Some((start, end)) = part.split_once('-') {
            let start: usize = start.trim().parse()?;
            let end: usize = end.trim().parse()?;
            if start == 0 || end == 0 || start > total || end > total {
                bail!("page range {start}-{end} out of bounds (1-{total})");
            }
            for i in start..=end {
                result.push(i - 1);
            }
        } else {
            let page: usize = part.parse()?;
            if page == 0 || page > total {
                bail!("page {page} out of bounds (1-{total})");
            }
            result.push(page - 1);
        }
    }
    Ok(result)
}
