// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The final-table switch ships on with its exported model (#638).
//!
//! The switch serves the exported `table_is_v2` model from
//! `table_is_v2_probability`; it may only be on with that model in the tree,
//! and the measurement (m134) says on, so the default is on.

use pdf_engine::learned::models::table_is_v2;
use pdf_engine::learned::SWITCHES;

#[test]
fn the_switch_ships_on_with_its_exported_model() {
    assert!(
        !table_is_v2::MODEL_SHA256.is_empty(),
        "table_is_v2 is on with no exported model in the tree"
    );
    const {
        assert!(
            SWITCHES.table_is_v2,
            "table_is_v2 ships off: the measurement (m134) says on"
        );
    }
}
