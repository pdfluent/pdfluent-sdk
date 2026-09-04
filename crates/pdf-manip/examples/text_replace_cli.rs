//! Minimal runner for the corpus gate: replace text in one PDF.
//!
//! Test tooling only — this is what `scripts/ci/text_replace_corpus_gate.py`
//! shells out to for each document. It is deliberately not a product CLI:
//! programmatic PDF processing is SDK territory, and the free editor ships no
//! command-line mode (LICENSE.md §3).
//!
//! Exit codes matter to the gate:
//!   0  replacement applied and saved
//!   1  the document opened but the replacement did not apply
//!   2  the document could not be opened or the arguments were wrong
//!
//! The distinction between 1 and 2 is what lets the gate separate "we broke it"
//! from "it was already broken", which otherwise turns both numbers to mush.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_manip::text_edit::{
    begin_text_edit, DocumentRevision, FontFallback, ReplaceOptions, TextQuery,
};
use std::path::PathBuf;
use std::process::ExitCode;

struct Args {
    input: PathBuf,
    output: PathBuf,
    find: String,
    replace: String,
    /// Welke fallback het vervangen mag gebruiken als het lettertype een teken
    /// niet kan schrijven. `deny` is de bibliotheekstandaard en weigert; met
    /// `standard` wordt er een Helvetica/WinAnsi-hulpbron toegevoegd.
    ///
    /// Bestond niet, terwijl `corpus:text-replace-capability` er wel
    /// `--fallback standard` aan meegaf. Die job faalde daardoor binnen een
    /// seconde op het doorgeven, en de meting die hij moest opleveren is nooit
    /// gedraaid.
    fallback: FontFallback,
}

fn parse() -> Option<Args> {
    let mut input = None;
    let mut output = None;
    let mut find = None;
    let mut replace = None;
    let mut fallback = FontFallback::Deny;
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let value = it.next()?;
        match flag.as_str() {
            "--input" => input = Some(PathBuf::from(value)),
            "--output" => output = Some(PathBuf::from(value)),
            "--find" => find = Some(value),
            "--replace" => replace = Some(value),
            // Een onbekende waarde wordt geweigerd in plaats van stil op
            // `deny` te vallen: anders zou een tikfout een meting opleveren
            // die iets anders meet dan hij zegt.
            "--fallback" => {
                fallback = match value.as_str() {
                    "deny" => FontFallback::Deny,
                    "standard" => FontFallback::InjectStandard,
                    other => {
                        eprintln!("unknown --fallback {other:?}; expected deny or standard");
                        return None;
                    }
                }
            }
            _ => return None,
        }
    }
    Some(Args {
        input: input?,
        output: output?,
        find: find?,
        replace: replace?,
        fallback,
    })
}

fn main() -> ExitCode {
    let Some(args) = parse() else {
        eprintln!("usage: --input <pdf> --output <pdf> --find <text> --replace <text> [--fallback deny|standard]");
        return ExitCode::from(2);
    };

    let bytes = match std::fs::read(&args.input) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("cannot read input: {e}");
            return ExitCode::from(2);
        }
    };
    let mut doc = match lopdf::Document::load_mem(&bytes) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("cannot open PDF: {e}");
            return ExitCode::from(2);
        }
    };

    let revision = DocumentRevision::from_source_bytes(&bytes);
    let mut session = match begin_text_edit(&mut doc, revision) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot start an edit session: {e}");
            return ExitCode::from(2);
        }
    };

    let matches = match session.find_text(TextQuery::exact(&args.find)) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("search failed: {e}");
            return ExitCode::from(1);
        }
    };
    let Some(first) = matches.first() else {
        eprintln!("no match for {:?}", args.find);
        return ExitCode::from(1);
    };

    let options = ReplaceOptions::default().font_fallback(args.fallback.clone());
    if let Err(e) = session.stage_replace(&first.id, &args.replace, options) {
        eprintln!("stage failed: {e}");
        return ExitCode::from(1);
    }
    let report = match session.commit() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("commit failed: {e}");
            return ExitCode::from(1);
        }
    };
    if report.replacements_applied == 0 {
        eprintln!("commit reported zero replacements");
        return ExitCode::from(1);
    }

    match std::fs::File::create(&args.output).and_then(|mut f| doc.save_to(&mut f).map(|_| ())) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("cannot write output: {e}");
            ExitCode::from(2)
        }
    }
}
