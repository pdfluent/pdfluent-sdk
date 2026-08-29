//! Subsetting must not write back a font it broke (#184).
//!
//! `subset_fonts` is reachable from the public SDK as `doc.subset_fonts()`. On
//! 17 of the 120 holdout documents it wrote fonts back missing `cmap`, `OS/2`
//! or `GDEF` — still a file, still smaller, and no longer valid. veraPDF then
//! could not validate any of the 17: seventeen conformant became zero.
//!
//! That is the shape worth guarding. Subsetting halves the size, which reads as
//! progress on every axis anybody was watching, while the axis nobody measured
//! went to zero.
//!
//! The guard compares the SFNT table directory before and after and skips the
//! write when tables went missing. This tests the comparison directly, because
//! the corpus that produced the original finding is not on this machine.

// The upstream version of this test used a Liberation face from
// pdf-substitute-fonts, a crate that does not exist here. Building the table
// directory by hand tests the same predicate and needs no fixture.
fn sfnt(tables: &[&str]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&[0x00, 0x01, 0x00, 0x00]);
    out.extend_from_slice(&(tables.len() as u16).to_be_bytes());
    out.extend_from_slice(&[0, 0, 0, 0, 0, 0]); // searchRange, entrySelector, rangeShift
    for tag in tables {
        let mut naam = tag.as_bytes().to_vec();
        naam.resize(4, b' ');
        out.extend_from_slice(&naam);
        out.extend_from_slice(&[0; 12]); // checksum, offset, length
    }
    out
}

#[test]
fn a_subset_that_dropped_tables_is_detected() {
    let voor = sfnt(&["cmap", "glyf", "head", "OS/2", "GDEF"]);
    let na = sfnt(&["glyf", "head"]);

    let verloren = pdf_manip::font_subset::tables_lost_for_test(&voor, &na)
        .expect("dropping cmap, OS/2 and GDEF must be reported");

    for tabel in ["cmap", "OS/2", "GDEF"] {
        assert!(
            verloren.iter().any(|t| t == tabel),
            "{tabel} went missing and was not reported; got {verloren:?}",
        );
    }
}

#[test]
fn a_subset_that_kept_every_table_is_not_flagged() {
    // Without this the check could pass by reporting everything as broken,
    // which would skip every font and make subsetting a no-op.
    let voor = sfnt(&["cmap", "glyf", "head"]);
    let na = sfnt(&["cmap", "glyf", "head"]);
    assert!(
        pdf_manip::font_subset::tables_lost_for_test(&voor, &na).is_none(),
        "an intact subset must not be reported as broken",
    );
}

#[test]
fn bare_cff_has_no_table_directory_and_is_left_alone() {
    // FontFile3 without an SFNT wrapper: nothing to compare, so nothing to
    // report. Flagging these would skip every CFF font in the corpus.
    let cff = vec![
        0x01, 0x00, 0x04, 0x01, 0x00, 0x00, 0x00, 0x00, 0, 0, 0, 0, 0,
    ];
    assert!(
        pdf_manip::font_subset::tables_lost_for_test(&cff, &cff).is_none(),
        "bare CFF has no table directory and must not be treated as broken",
    );
}
