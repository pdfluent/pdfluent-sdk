//! The generated models (#599). Every file below this one is written by
//! `scripts/extraction/learned/export_rust.py` from the dump in
//! `scripts/extraction/learned/models/<name>/`, and a test re-exports each dump
//! and requires the same bytes: nothing here is edited by hand.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

// Skipped by rustfmt: the files are the exporter's bytes, and a reformatted
// file is one the reproducibility test can no longer match to its dump.
#[rustfmt::skip]
pub mod caption_lines;
#[rustfmt::skip]
pub mod heading_lines;
#[rustfmt::skip]
pub mod line_classes;
#[rustfmt::skip]
pub mod line_gnn;
#[rustfmt::skip]
pub mod line_gnn_v2;
#[rustfmt::skip]
pub mod list_item;
#[rustfmt::skip]
pub mod list_opens;
#[rustfmt::skip]
pub mod table_region;
#[rustfmt::skip]
pub mod table_row_boundary;
#[rustfmt::skip]
pub mod table_column_boundary;
#[rustfmt::skip]
pub mod table_column_bridge;
#[rustfmt::skip]
pub mod table_is;
#[rustfmt::skip]
pub mod table_is_v2;
#[rustfmt::skip]
pub mod table_judge;
#[rustfmt::skip]
pub mod table_verifier;
