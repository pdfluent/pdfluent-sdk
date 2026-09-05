// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A PDF/A conversion may not mangle a name in the content stream.
//!
//! Measured on govdocs 127_127125 while comparing two conversion routes on
//! 24-08-2026: the published beta.17.5 turned `/P <</MCID 0 >> BDC` into
//! `/P <<CD 0 >> BDC`. Three characters gone from the middle of a name.
//!
//! What makes that worth its own test is the direction of the damage.
//! veraPDF called the mangled file **conformant** and the intact one not,
//! because a validator cannot fully inspect content it cannot parse. The
//! damage did not show up as a lower score; it showed up as a higher one. On
//! the same 120 documents the mangled route also lost text on 37 of them
//! against 4, and put 10 below half their words against 2.
//!
//! So conformance alone cannot catch this class, and neither can a text
//! comparison: `/MCID` carries no text. Only the operator names can.

use std::collections::BTreeSet;

/// Every name token in every content stream, as a set.
///
/// Deliberately a set of names rather than a byte comparison: a conversion is
/// allowed to reorder, recompress and renumber. It is not allowed to invent a
/// name that was not there, which is what mangling looks like from here.
fn content_stream_names(pdf: &[u8]) -> BTreeSet<String> {
    let doc = lopdf::Document::load_mem(pdf).expect("output is not loadable");
    let mut names = BTreeSet::new();
    for page_id in doc.page_iter().collect::<Vec<_>>() {
        // get_page_content returns the bytes directly, not a Result.
        let content = doc.get_page_content(page_id);
        let Ok(parsed) = lopdf::content::Content::decode(&content) else {
            continue;
        };
        for op in parsed.operations {
            for operand in &op.operands {
                collect_names(operand, &mut names);
            }
        }
    }
    names
}

fn collect_names(obj: &lopdf::Object, out: &mut BTreeSet<String>) {
    match obj {
        lopdf::Object::Name(n) => {
            out.insert(String::from_utf8_lossy(n).into_owned());
        }
        lopdf::Object::Array(items) => {
            for i in items {
                collect_names(i, out);
            }
        }
        lopdf::Object::Dictionary(d) => {
            for (k, v) in d.iter() {
                out.insert(String::from_utf8_lossy(k).into_owned());
                collect_names(v, out);
            }
        }
        _ => {}
    }
}

#[test]
fn conversion_invents_no_names_the_source_did_not_have() {
    const SOURCE: &[u8] = include_bytes!("../../../tests/corpus-mini/simple.pdf");

    let before = content_stream_names(SOURCE);
    assert!(
        !before.is_empty(),
        "the fixture has no names in its content stream, so this test proves nothing"
    );

    let opts = pdf_manip::pdfa::PdfAConvertOptions::default();
    let converted = pdf_manip::pdfa::convert_bytes(SOURCE, &opts).expect("conversion failed");
    let after = content_stream_names(&converted);

    // A conversion may drop a name (removing an unused resource is legitimate)
    // and may add one it constructed deliberately. What it may not do is
    // produce a name that looks like a damaged version of one that was there.
    let invented: Vec<_> = after.difference(&before).collect();
    for name in &invented {
        for original in &before {
            assert!(
                !looks_truncated(name, original),
                "the conversion produced {name:?}, which reads as a damaged {original:?}"
            );
        }
    }
}

/// Whether `candidate` looks like `original` with a run of characters removed.
///
/// `MCID` -> `CD` is the shape: same last character, same relative order, and
/// shorter. Not a general similarity measure -- it is aimed at exactly the
/// damage seen in the published package, because a looser rule would fire on
/// legitimate short names like `CS` versus `CS0`.
fn looks_truncated(candidate: &str, original: &str) -> bool {
    if candidate.len() >= original.len() || candidate.is_empty() {
        return false;
    }
    if original.len() - candidate.len() < 2 {
        return false;
    }
    let mut chars = original.chars();
    candidate.chars().all(|c| chars.any(|o| o == c))
}

#[test]
fn the_damage_detector_recognises_the_case_it_was_written_for() {
    // The detector is the whole test above; a detector that never fires would
    // make it a decoration. This is the exact pair measured on 127_127125.
    assert!(looks_truncated("CD", "MCID"));
    // And these must not fire, or the test above becomes noise.
    assert!(
        !looks_truncated("CS0", "CS"),
        "a longer name is not truncation"
    );
    assert!(
        !looks_truncated("CS", "CS0"),
        "one character shorter is not the pattern"
    );
    assert!(!looks_truncated("GS0", "MCID"));
}
