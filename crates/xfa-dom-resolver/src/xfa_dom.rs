//! XFA DOM — root container for all sub-DOMs.
//!
//! Per XFA 3.3 §3: The XFA DOM encapsulates all other DOMs.
//! Its root node has children for template, datasets, config, form, layout.

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
