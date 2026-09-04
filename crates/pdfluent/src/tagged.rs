//! Reading the logical structure tree of a tagged PDF (ISO 32000-1 §14.7).
//!
//! Tagged PDFs carry a `/StructTreeRoot` describing the document's logical
//! structure — headings, paragraphs, tables, lists, figures — in reading order,
//! with accessibility attributes (alternate text, replacement text, language).
//! [`crate::PdfDocument::structure_tree`] surfaces that tree; this module owns
//! the public data model and the conversion from the internal parser output.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_compliance::tagged::{StructElement, StructureTree};

/// The logical structure of a tagged PDF.
///
/// Returned by [`crate::PdfDocument::structure_tree`]. The element list is in
/// document reading order. Useful for accessibility auditing (heading outline,
/// alt text on figures) and structure-aware processing.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct DocumentStructure {
    /// Document language tag from the catalog `/Lang` (e.g. `"en-US"`), if set.
    pub language: Option<String>,
    /// Root structure elements, in reading order.
    pub elements: Vec<StructureNode>,
}

/// A single node in the logical structure tree.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct StructureNode {
    /// Standard structure type after role mapping (e.g. `"H1"`, `"P"`,
    /// `"Table"`, `"Figure"`).
    pub tag: String,
    /// Structure type exactly as authored, before role mapping. Differs from
    /// [`tag`](Self::tag) only when the document remaps a custom role.
    pub raw_tag: String,
    /// Alternate description (`/Alt`) — the accessible text for a figure or
    /// other non-textual element.
    pub alt_text: Option<String>,
    /// Replacement text (`/ActualText`) — the exact text this element stands in
    /// for (e.g. a ligature or an image of a word).
    pub actual_text: Option<String>,
    /// Element language (`/Lang`) when it overrides the document language.
    pub language: Option<String>,
    /// 0-based index of the page this element's content lives on, if known.
    pub page: Option<usize>,
    /// Heading level 1–6 when this is a heading element (`H1`–`H6`), else
    /// `None`.
    pub heading_level: Option<u8>,
    /// Child elements, in reading order.
    pub children: Vec<StructureNode>,
}

impl From<&StructElement> for StructureNode {
    fn from(element: &StructElement) -> Self {
        StructureNode {
            tag: element.standard_type.clone(),
            raw_tag: element.struct_type.clone(),
            alt_text: element.alt.clone(),
            actual_text: element.actual_text.clone(),
            language: element.lang.clone(),
            page: element.page_index,
            heading_level: element.heading_level(),
            children: element.children.iter().map(StructureNode::from).collect(),
        }
    }
}

/// Convert the internal parser output into the public structure model.
pub(crate) fn from_structure_tree(tree: StructureTree) -> DocumentStructure {
    DocumentStructure {
        language: tree.lang,
        elements: tree.root_elements.iter().map(StructureNode::from).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_compliance::tagged::{StructElement, StructureTree};
    use std::collections::HashMap;

    fn elem(
        struct_type: &str,
        standard: &str,
        alt: Option<&str>,
        children: Vec<StructElement>,
    ) -> StructElement {
        StructElement {
            struct_type: struct_type.to_string(),
            standard_type: standard.to_string(),
            alt: alt.map(str::to_string),
            actual_text: None,
            lang: None,
            mcids: Vec::new(),
            page_index: Some(0),
            children,
        }
    }

    #[test]
    fn converts_nested_tree_with_headings_and_alt() {
        let tree = StructureTree {
            role_map: HashMap::new(),
            lang: Some("en-US".to_string()),
            root_elements: vec![elem(
                "Document",
                "Document",
                None,
                vec![
                    elem("H1", "H1", None, Vec::new()),
                    elem("Figure", "Figure", Some("A diagram"), Vec::new()),
                ],
            )],
        };

        let structure = from_structure_tree(tree);
        assert_eq!(structure.language.as_deref(), Some("en-US"));
        assert_eq!(structure.elements.len(), 1);

        let doc = &structure.elements[0];
        assert_eq!(doc.tag, "Document");
        assert_eq!(doc.heading_level, None);
        assert_eq!(doc.children.len(), 2);

        let heading = &doc.children[0];
        assert_eq!(heading.tag, "H1");
        assert_eq!(heading.heading_level, Some(1));

        let figure = &doc.children[1];
        assert_eq!(figure.tag, "Figure");
        assert_eq!(figure.alt_text.as_deref(), Some("A diagram"));
        assert_eq!(figure.heading_level, None);
        assert_eq!(figure.page, Some(0));
    }

    #[test]
    fn role_mapped_tag_differs_from_raw_tag() {
        // A custom role "Sect" mapped to the standard "H2".
        let node = StructureNode::from(&elem("Sect", "H2", None, Vec::new()));
        assert_eq!(node.raw_tag, "Sect");
        assert_eq!(node.tag, "H2");
        assert_eq!(node.heading_level, Some(2));
    }
}
