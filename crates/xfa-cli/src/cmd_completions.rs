//! Shell completions generator.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use clap::CommandFactory;
use clap_complete::{generate, Shell};
use std::io;

pub fn run(shell: Shell) {
    let mut cmd = crate::Cli::command();
    let bin_name = "pdfluent";
    generate(shell, &mut cmd, bin_name, &mut io::stdout());
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap_complete::Shell;

    #[test]
    fn test_generate_completions() {
        // Just check it doesn't panic and produces some output
        run(Shell::Bash);
    }
}
