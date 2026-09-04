//! Man page generator.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use anyhow::Result;
use clap::CommandFactory;
use clap_mangen::Man;
use std::path::Path;

pub fn run() -> Result<()> {
    let cmd = crate::Cli::command();
    let out_dir = Path::new("man");
    std::fs::create_dir_all(out_dir)?;

    let man = Man::new(cmd);
    let mut buffer: Vec<u8> = Default::default();
    man.render(&mut buffer)?;
    std::fs::write(out_dir.join("pdfluent.1"), buffer)?;

    println!("✓ Man page generated in man/pdfluent.1");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_man_generation() {
        // Just check it doesn't panic
        let cmd = crate::Cli::command();
        let man = Man::new(cmd);
        let mut buffer: Vec<u8> = Default::default();
        man.render(&mut buffer).unwrap();
    }
}
