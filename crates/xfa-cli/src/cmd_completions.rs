//! Shell completions generator.

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
