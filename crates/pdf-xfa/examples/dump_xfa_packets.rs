//! Dump XFA template and datasets packets from a PDF to stdout/files.
//!
//! Used by the M3-C investigation to inspect Polish PIT and similar
//! schema-bound documents. Not part of the public CLI surface.
//!
//! # Usage
//!
//! ```text
//! cargo run --example dump_xfa_packets --features xfa-js-sandboxed -- <input.pdf> <out_prefix>
//! ```

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::error::Error;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: dump_xfa_packets <input.pdf> <out_prefix>");
        std::process::exit(1);
    }
    let input = PathBuf::from(&args[1]);
    let out_prefix = &args[2];
    let pdf_bytes = std::fs::read(&input)?;
    let packets = pdf_xfa::extract::extract_xfa_from_bytes(pdf_bytes)?;
    println!("packets:");
    for (n, p) in &packets.packets {
        println!("  {} ({} bytes)", n, p.len());
    }
    if let Some(t) = packets.template() {
        let path = format!("{out_prefix}.template.xml");
        std::fs::write(&path, t)?;
        println!("wrote {path} ({} bytes)", t.len());
    }
    if let Some(d) = packets.datasets() {
        let path = format!("{out_prefix}.datasets.xml");
        std::fs::write(&path, d)?;
        println!("wrote {path} ({} bytes)", d.len());
    }
    Ok(())
}
