//! XFA PDF Collector — download, detect, and classify XFA PDFs for the test corpus.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

// Import the collector module from the xfa-cli library.
// Since xfa-cli is a binary-only crate, we include the module directly.
#[path = "../collector.rs"]
mod collector;

#[derive(Parser)]
#[command(
    name = "xfa-collector",
    about = "Collect and classify XFA PDFs for testing"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Download PDFs from a URL list and keep only XFA forms.
    Collect {
        /// Path to a file with one URL per line.
        #[arg(long)]
        urls: PathBuf,
        /// Output directory for collected XFA PDFs.
        #[arg(long, default_value = "corpus")]
        output: PathBuf,
        /// Maximum number of URLs to process.
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Scan a directory of PDFs and classify each one.
    Scan {
        /// Directory containing PDF files.
        #[arg(long)]
        dir: PathBuf,
        /// Output path for the JSON report.
        #[arg(long, default_value = "report.json")]
        report: PathBuf,
    },
    /// Print corpus statistics.
    Stats {
        /// Directory containing collected PDFs or a report.json.
        #[arg(long)]
        dir: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Command::Collect {
            urls,
            output,
            limit,
        } => {
            println!("XFA Collector — downloading from {}", urls.display());
            collector::collect(&urls, &output, limit)
        }
        Command::Scan { dir, report } => collector::scan(&dir, &report),
        Command::Stats { dir } => collector::stats(&dir),
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
