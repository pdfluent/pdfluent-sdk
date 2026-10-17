//! Internal man-page generator: `pdfluent generate-man-pages <DIR>`.
//!
//! Renders the real clap tree with `clap_mangen`: the top-level page plus one
//! page per subcommand, including the nested `xfa flatten`. Intercepted in
//! `run_cli` before clap parsing rather than declared as a subcommand, so the
//! generator itself never appears in `--help`, completions, or the pages it
//! writes. Output is deterministic (no timestamps): the currency test
//! `tests/cli_man.rs` regenerates into a temp dir and diffs bytes.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::ffi::OsString;
use std::path::Path;

use clap_mangen::Man;

use crate::output::{self, Ctx, CLI_VERSION};

/// Run the internal `generate-man-pages` command (argv after the word itself).
pub(crate) fn run(args: &[OsString]) -> u8 {
    let ctx = Ctx {
        json: false,
        quiet: false,
        verbose: 0,
    };
    let [dir] = args else {
        return output::error(
            "generate-man-pages",
            &ctx,
            "BAD_USAGE",
            "usage: pdfluent generate-man-pages <DIR>",
        );
    };
    let dir = Path::new(dir);
    match write_man_pages(dir) {
        Ok(count) => {
            eprintln!("Wrote {count} man pages to {}", dir.display());
            output::exit::OK
        }
        Err(message) => output::error("generate-man-pages", &ctx, "MANPAGE_WRITE_FAILED", &message),
    }
}

/// Render every page into `out_dir` (created when missing). Returns the page
/// count. Deterministic: same tree, same bytes, so the currency test diffs.
pub(crate) fn write_man_pages(out_dir: &Path) -> Result<usize, String> {
    std::fs::create_dir_all(out_dir)
        .map_err(|e| format!("cannot create {}: {e}", out_dir.display()))?;
    // Build the full tree first: global flags and the version propagate down,
    // so each subcommand page documents `--json`/`-q`/`-v` like its `--help`.
    // The auto `help` subcommand (`pdfluent help <cmd>`) is not a page: it
    // would render as `pdfluent-help.1` plus one topic page per command.
    // Display-only on this rendering clone; parsing is unaffected.
    let mut cmd = crate::cli_command().disable_help_subcommand(true);
    cmd.build();
    let mut count = 0;
    render(&cmd, "pdfluent", "pdfluent", out_dir)?;
    count += 1;
    for sub in cmd.get_subcommands() {
        let page = format!("pdfluent-{}", sub.get_name());
        let invocation = format!("pdfluent {}", sub.get_name());
        render(sub, &page, &invocation, out_dir)?;
        count += 1;
        for nested in sub.get_subcommands() {
            let nested_page = format!("{page}-{}", nested.get_name());
            let nested_invocation = format!("{invocation} {}", nested.get_name());
            render(nested, &nested_page, &nested_invocation, out_dir)?;
            count += 1;
        }
    }
    Ok(count)
}

/// Render one command as `page.1`: the display name drives the `.TH` title,
/// the file name and the NAME section; the invocation the SYNOPSIS line.
fn render(cmd: &clap::Command, page: &str, invocation: &str, out_dir: &Path) -> Result<(), String> {
    let owned = cmd.clone().display_name(page).bin_name(invocation);
    let man = Man::new(owned)
        .title(page.to_uppercase())
        .source(format!("pdfluent {CLI_VERSION}"))
        .manual("PDFluent");
    let mut buffer = Vec::new();
    man.render(&mut buffer)
        .map_err(|e| format!("cannot render {page}.1: {e}"))?;
    std::fs::write(out_dir.join(format!("{page}.1")), buffer)
        .map_err(|e| format!("cannot write {page}.1: {e}"))?;
    Ok(())
}
