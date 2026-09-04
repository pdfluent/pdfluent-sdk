//! XFA-Native-Rust CLI — PDF and XFA form processing toolkit.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

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
mod cmd_measure;
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
        /// XFA rendering policy for `flatten`: `saved-state` only (default).
        /// `flatten` always applies the production SavedStateFaithful policy.
        /// `fresh-merge` (FreshMergeExperimental) is experimental, opt-in, and
        /// pending corpus-scale (D13) validation — it is rejected by `flatten`;
        /// use `measure --policy fresh-merge` for experimental measurement.
        /// No Adobe-parity claim.
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
    /// Measure one PDF under one XFA rendering policy and emit stable JSON
    /// metrics for the D13 FreshMerge corpus measurement harness. Does not
    /// change flatten behavior. Default policy is `saved-state`.
    Measure {
        /// Input PDF file.
        #[arg(long)]
        input: PathBuf,
        /// Rendering policy: `saved-state` (default) or `fresh-merge`
        /// (experimental, opt-in; pending corpus-scale D13 validation).
        #[arg(long, value_name = "POLICY", default_value = "saved-state")]
        policy: String,
        /// Write measurement JSON to this path (else printed to stdout).
        #[arg(long, value_name = "PATH")]
        output_json: Option<PathBuf>,
        /// Document identifier (defaults to the input filename stem).
        #[arg(long, value_name = "ID")]
        doc_id: Option<String>,
        /// Oracle scope label: `full_document` | `first_n_pages` | `unknown`.
        #[arg(long, value_name = "SCOPE", default_value = "unknown")]
        oracle_scope: String,
        /// Oracle provider label: `pdfrest` | `speedtest` | `none`.
        #[arg(long, value_name = "PROVIDER", default_value = "none")]
        provider: String,
        /// Write the flattened PDF to this path (only if given).
        #[arg(long, value_name = "PATH")]
        output_pdf: Option<PathBuf>,
        /// Never write a PDF, even if --output-pdf is set.
        #[arg(long)]
        no_write_output_pdf: bool,
        /// Enable per-node XFA presence provenance trace on stderr.
        #[arg(long)]
        trace: bool,
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
        Commands::Measure {
            input,
            policy,
            output_json,
            doc_id,
            oracle_scope,
            provider,
            output_pdf,
            no_write_output_pdf,
            trace,
        } => cmd_measure::run(
            &input,
            &policy,
            output_json.as_deref(),
            doc_id.as_deref(),
            &oracle_scope,
            &provider,
            output_pdf.as_deref(),
            no_write_output_pdf,
            trace,
        ),
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

    #[test]
    fn measure_parses_with_defaults() {
        let cli = Cli::parse_from(["pdfluent", "measure", "--input", "in.pdf"]);
        match cli.command {
            Commands::Measure {
                input,
                policy,
                output_json,
                output_pdf,
                no_write_output_pdf,
                provider,
                oracle_scope,
                ..
            } => {
                assert_eq!(input, PathBuf::from("in.pdf"));
                // Default policy is saved-state; never fresh-merge by default.
                assert_eq!(policy, "saved-state");
                assert_eq!(output_json, None);
                assert_eq!(output_pdf, None);
                assert!(!no_write_output_pdf);
                assert_eq!(provider, "none");
                assert_eq!(oracle_scope, "unknown");
            }
            _ => panic!("unexpected command"),
        }
    }

    #[test]
    fn measure_parses_fresh_merge_and_flags() {
        let cli = Cli::parse_from([
            "pdfluent",
            "measure",
            "--input",
            "in.pdf",
            "--policy",
            "fresh-merge",
            "--output-json",
            "/tmp/out.json",
            "--doc-id",
            "abc123",
            "--no-write-output-pdf",
        ]);
        match cli.command {
            Commands::Measure {
                policy,
                output_json,
                doc_id,
                no_write_output_pdf,
                ..
            } => {
                assert_eq!(policy, "fresh-merge");
                assert_eq!(output_json, Some(PathBuf::from("/tmp/out.json")));
                assert_eq!(doc_id, Some("abc123".to_string()));
                assert!(no_write_output_pdf);
            }
            _ => panic!("unexpected command"),
        }
    }
}

#[cfg(test)]
mod page_list_tests {
    use super::parse_page_list;

    /// Paginabereiken komen rechtstreeks van de gebruiker en gaan van
    /// 1-gebaseerd naar 0-gebaseerd. Eén stap ernaast betekent dat iemand een
    /// andere pagina krijgt dan hij vroeg, en dat merkt hij pas bij het lezen.
    #[test]
    fn single_pages_shift_from_one_based_to_zero_based() {
        assert_eq!(parse_page_list("1", 10).unwrap(), vec![0]);
        assert_eq!(parse_page_list("10", 10).unwrap(), vec![9]);
        assert_eq!(parse_page_list("1,3,5", 10).unwrap(), vec![0, 2, 4]);
    }

    #[test]
    fn ranges_are_inclusive_at_both_ends() {
        assert_eq!(parse_page_list("2-4", 10).unwrap(), vec![1, 2, 3]);
        assert_eq!(parse_page_list("1-1", 10).unwrap(), vec![0]);
    }

    #[test]
    fn spaces_around_the_parts_are_allowed() {
        assert_eq!(parse_page_list(" 1 , 3 - 4 ", 10).unwrap(), vec![0, 2, 3]);
    }

    /// Pagina 0 bestaat niet voor een gebruiker, en boven het totaal ook niet.
    /// Allebei horen te weigeren in plaats van stil iets anders te doen.
    #[test]
    fn out_of_bounds_is_refused() {
        assert!(parse_page_list("0", 10).is_err(), "pagina 0 bestaat niet");
        assert!(parse_page_list("11", 10).is_err(), "boven het totaal");
        assert!(parse_page_list("0-3", 10).is_err());
        assert!(parse_page_list("8-11", 10).is_err());
        assert!(parse_page_list("abc", 10).is_err());
        assert!(parse_page_list("", 10).is_err());
    }

    /// Vastgelegd omdat het verrast: een omgekeerd bereik levert géén fout maar
    /// een lege lijst. `for i in 5..=3` loopt nul keer, dus wie `5-3` vraagt
    /// krijgt stil niets terug in plaats van de drie pagina's die hij bedoelde
    /// of een melding dat het andersom moet.
    ///
    /// Deze test legt het huidige gedrag vast, geen goedkeuring ervan. Wordt
    /// besloten dit te weigeren, dan hoort deze test rood te worden en dat is
    /// precies het signaal dat het gedrag bewust verandert.
    #[test]
    fn a_reversed_range_yields_nothing_and_does_not_complain() {
        assert_eq!(parse_page_list("5-3", 10).unwrap(), Vec::<usize>::new());
    }

    #[test]
    fn overlapping_parts_are_kept_as_given() {
        // Geen ontdubbeling: wie 1-3,2 vraagt krijgt pagina 2 twee keer.
        assert_eq!(parse_page_list("1-3,2", 10).unwrap(), vec![0, 1, 2, 1]);
    }
}
