//! XFA-Native-Rust CLI — PDF and XFA form processing toolkit.

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod cmd_completions;
mod cmd_debug_xfa;
mod cmd_demo;
mod cmd_doctor;
mod cmd_extract;
mod cmd_fill;
mod cmd_flatten;
mod cmd_flatten_check;
mod cmd_info;
mod cmd_manpage;
mod cmd_render;
mod cmd_sign;
mod cmd_validate;
pub mod error;

#[derive(Parser)]
#[command(
    name = "pdfluent",
    version,
    about = "PDF and XFA form processing toolkit"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
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
        /// Write per-page layout metadata to JSON.
        #[arg(long, value_name = "PATH")]
        dump_layout: Option<PathBuf>,
        /// XFA rendering policy. `saved-state` (default) honors the document's
        /// saved form state; `fresh-merge` is experimental and not yet
        /// implemented (D12).
        #[arg(long, value_name = "POLICY", default_value = "saved-state")]
        xfa_rendering_policy: String,
    },
    /// Flatten a PDF and print quality metrics comparing before and after.
    FlattenCheck {
        /// Input PDF file.
        input: PathBuf,
        /// Optional output path for the flattened PDF.
        #[arg(short, long)]
        output: Option<PathBuf>,
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
    /// Check installation and system environment.
    Doctor {
        /// Try to fix found issues.
        #[arg(long)]
        fix: bool,
    },
    /// Generate shell completions.
    Completions {
        /// Shell to generate completions for.
        shell: clap_complete::Shell,
    },
    /// Generate man pages.
    Man,
    /// Run the XFA engine demo pipeline.
    Demo,
    /// Dump the XFA render tree for a PDF (developer tool).
    DebugXfa {
        /// Input PDF file.
        input: PathBuf,
        /// Output format: 'tree' (default) or 'json'.
        #[arg(long, default_value = "tree")]
        format: cmd_debug_xfa::DebugFormat,
    },
}

fn main() {
    match run() {
        Ok(()) => {}
        Err(e) => {
            eprintln!("Error: {e:#}");
            let code = if let Some(engine_err) = e.downcast_ref::<pdf_engine::EngineError>() {
                match engine_err {
                    pdf_engine::EngineError::Encrypted(_) => 2,
                    pdf_engine::EngineError::InvalidPageGeometry { .. } => 3,
                    pdf_engine::EngineError::XfaFlattenFailed(_) => 4,
                    _ => 1,
                }
            } else {
                // Fallback: inspect the error message when the concrete EngineError
                // type is not preserved through the anyhow chain (e.g. wrapped in
                // CliError or another anyhow layer).
                let msg = format!("{e:?}");
                if msg.contains("XFA flatten failed") {
                    4
                } else if msg.contains("PDF is encrypted") || msg.contains("PasswordProtected") {
                    2
                } else if msg.contains("invalid page geometry") || msg.contains("zero pages") {
                    3
                } else {
                    1
                }
            };
            std::process::exit(code);
        }
    }
}

fn run() -> Result<()> {
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
        Commands::Flatten {
            input,
            output,
            dump_layout,
            xfa_rendering_policy,
        } => cmd_flatten::run(
            &input,
            &output,
            dump_layout.as_deref(),
            &xfa_rendering_policy,
        ),
        Commands::FlattenCheck { input, output } => {
            cmd_flatten_check::run(&input, output.as_deref())
        }
        Commands::Info { input, json } => cmd_info::run(&input, json),
        Commands::Validate {
            input,
            profile,
            json,
        } => cmd_validate::run(&input, &profile, json),
        Commands::Sign { input, json } => cmd_sign::run(&input, json),
        Commands::Doctor { fix: _ } => cmd_doctor::run(),
        Commands::Completions { shell } => {
            cmd_completions::run(shell);
            Ok(())
        }
        Commands::Man => cmd_manpage::run(),
        Commands::Demo => cmd_demo::run(),
        Commands::DebugXfa { input, format } => cmd_debug_xfa::run(&input, format),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatten_parses_dump_layout_flag() {
        let cli = Cli::parse_from([
            "pdfluent",
            "flatten",
            "input.pdf",
            "--output",
            "output.pdf",
            "--dump-layout",
            "/tmp/layout.json",
        ]);

        match cli.command {
            Commands::Flatten {
                input,
                output,
                dump_layout,
                xfa_rendering_policy,
            } => {
                assert_eq!(input, PathBuf::from("input.pdf"));
                assert_eq!(output, PathBuf::from("output.pdf"));
                assert_eq!(dump_layout, Some(PathBuf::from("/tmp/layout.json")));
                assert_eq!(xfa_rendering_policy, "saved-state");
            }
            _ => panic!("unexpected command"),
        }
    }
}
