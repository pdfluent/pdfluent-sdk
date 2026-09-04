//! XFA DOM — root container for all sub-DOMs.
//!
//! Per XFA 3.3 §3: The XFA DOM encapsulates all other DOMs.
//! Its root node has children for template, datasets, config, form, layout.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::data_dom::DataDom;

/// The root XFA DOM containing all sub-DOMs.
pub struct XfaDom {
    /// The Data DOM (parsed from datasets/data packet).
    pub data: DataDom,
    // TODO: Add template, form, layout, config DOMs as they are implemented.
}

impl XfaDom {
    /// Create a new XFA DOM with a Data DOM parsed from XML.
    pub fn from_data_xml(xml: &str) -> crate::error::Result<Self> {
        let data = DataDom::from_xml(xml)?;
        Ok(Self { data })
    }
}
