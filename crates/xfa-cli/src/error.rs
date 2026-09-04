// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::fmt;

#[derive(Debug)]
pub struct CliError {
    pub message: String,      // "Could not convert PDF to PDF/A"
    pub why: Option<String>,  // "Font 'Helvetica' is not embedded"
    pub fix: Option<String>,  // "Embed the font before converting: pdfluent fonts embed input.pdf"
    pub docs: Option<String>, // "https://docs.pdfluent.com/errors/E001"
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "-- ERROR -----------------------------------------------------------------------"
        )?;
        writeln!(f, "{}", self.message)?;

        if let Some(ref why) = self.why {
            writeln!(f, "\nWhy:")?;
            writeln!(f, "{}", why)?;
        }

        if let Some(ref fix) = self.fix {
            writeln!(f, "\nHow to fix:")?;
            writeln!(f, "{}", fix)?;
        }

        if let Some(ref docs) = self.docs {
            writeln!(f, "\nDocs: {}", docs)?;
        }

        write!(
            f,
            "--------------------------------------------------------------------------------"
        )
    }
}

impl std::error::Error for CliError {}
