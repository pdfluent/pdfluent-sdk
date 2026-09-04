//! flatten-check — flatten a PDF and print quality metrics (XFA-F6-04 #1112).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use anyhow::{Context, Result};
use std::path::Path;

/// Flatten `input`, optionally write the result to `output`, and print
/// [`pdf_xfa::FlattenQualityMetrics`] to stdout.
pub fn run(input: &Path, output: Option<&Path>) -> Result<()> {
    let original_bytes = std::fs::read(input).context("failed to read input PDF")?;

    let flattened_bytes =
        pdf_xfa::flatten_xfa_to_pdf(&original_bytes).context("XFA flatten failed")?;

    if let Some(out_path) = output {
        std::fs::write(out_path, &flattened_bytes).context("failed to write flattened PDF")?;
        println!("Flattened PDF written to: {}", out_path.display());
    }

    let metrics = pdf_xfa::compare_flatten_quality(&original_bytes, &flattened_bytes)
        .context("quality comparison failed")?;

    println!("Flatten quality metrics for: {}", input.display());
    println!("  Pages before : {}", metrics.page_count_before);
    println!("  Pages after  : {}", metrics.page_count_after);
    println!("  Page match   : {}", metrics.page_count_match);
    println!(
        "  Content bytes before : {}",
        metrics.content_stream_bytes_before
    );
    println!(
        "  Content bytes after  : {}",
        metrics.content_stream_bytes_after
    );
    println!("  Content ratio        : {:.4}", metrics.content_ratio);

    Ok(())
}
