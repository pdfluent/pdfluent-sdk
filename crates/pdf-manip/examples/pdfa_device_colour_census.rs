// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! What a corpus reaches, before anything converts it.
//!
//! `census_device_colors` decides which Default\* ICC profiles a conversion
//! embeds (#379 round 2, `benchmarks/pdfa/size/ROUND2.md`). Two figures in that
//! round file come from here and are not otherwise reproducible: how many
//! documents reach DeviceCMYK, and how many streams the strict content parser
//! refuses -- the second being the difference between a census that reads a
//! parser complaint as "unreadable" and one that reads the bytes.
//!
//!     pdfa_device_colour_census CORPUS_DIR LIST
//!
//! The census here runs on the input document. A conversion runs it after the
//! cleanup and font passes, so the two need not agree exactly; what this
//! answers is what the corpus contains, which is the question the round file
//! asks.

use lopdf::Document;
use pdf_manip::pdfa_colorspace::census_device_colors;
use std::path::Path;

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(dir), Some(list)) = (args.next(), args.next()) else {
        eprintln!("usage: pdfa_device_colour_census CORPUS_DIR LIST");
        std::process::exit(2);
    };
    let names = std::fs::read_to_string(&list).expect("the document list must be readable");

    let (mut documents, mut refused_to_load) = (0usize, 0usize);
    let (mut gray, mut rgb, mut cmyk, mut undecodable) = (0usize, 0usize, 0usize, 0usize);
    let (mut refused_streams, mut documents_with_a_refused_stream) = (0usize, 0usize);

    for name in names.lines().map(str::trim).filter(|n| !n.is_empty()) {
        let Ok(doc) = Document::load(Path::new(&dir).join(name)) else {
            refused_to_load += 1;
            continue;
        };
        documents += 1;
        let census = census_device_colors(&doc);
        gray += usize::from(census.gray);
        rgb += usize::from(census.rgb);
        cmyk += usize::from(census.cmyk);
        undecodable += usize::from(census.undecodable_stream);

        let refused = pdf_manip::pdfa_colorspace::content_streams_the_parser_refuses(&doc);
        refused_streams += refused;
        documents_with_a_refused_stream += usize::from(refused > 0);
    }

    println!("documents               {documents}");
    println!("would not load          {refused_to_load}");
    println!("reach DeviceGray        {gray}");
    println!("reach DeviceRGB         {rgb}");
    println!("reach DeviceCMYK        {cmyk}");
    println!("an unreadable stream    {undecodable}");
    println!("streams the strict parser refuses  {refused_streams}");
    println!("  in documents                     {documents_with_a_refused_stream}");
}
