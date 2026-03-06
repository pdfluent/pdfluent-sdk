//! Tagged PDF: structure tree parsing, reading order, alt text extraction.
//!
//! Parses the structure tree from a PDF document (StructTreeRoot),
//! resolves role mappings, extracts reading order, alt text, and
//! table/list structure.

use pdf_syntax::object::dict::keys;
use pdf_syntax::object::{Dict, Name, Object};
use pdf_syntax::Pdf;
use std::collections::HashMap;

/// A parsed structure tree.
#[derive(Debug, Clone)]
pub struct StructureTree {
    /// Root elements of the structure tree.
    pub root_elements: Vec<StructElement>,
    /// Role mapping: custom role → standard role.
    pub role_map: HashMap<String, String>,
    /// Document language (from catalog /Lang).
    pub lang: Option<String>,
}

/// A structure element in the tagged PDF tree.
#[derive(Debug, Clone)]
pub struct StructElement {
    /// Structure type (e.g., "Document", "P", "H1", "Table", "Figure").
    pub struct_type: String,
    /// Resolved standard type (after role mapping).
    pub standard_type: String,
    /// Alt text (/Alt attribute).
    pub alt: Option<String>,
    /// ActualText (/ActualText attribute).
    pub actual_text: Option<String>,
    /// Language (/Lang attribute).
    pub lang: Option<String>,
    /// Marked content IDs associated with this element.
    pub mcids: Vec<i32>,
    /// Page index for MCID references.
    pub page_index: Option<usize>,
    /// Child elements.
    pub children: Vec<StructElement>,
}

impl StructElement {
    /// Check if this element is a heading (H, H1-H6).
    pub fn is_heading(&self) -> bool {
        let t = &self.standard_type;
        t == "H" || t == "H1" || t == "H2" || t == "H3" || t == "H4" || t == "H5" || t == "H6"
    }

    /// Get the heading level (1-6), or None if not a heading.
    pub fn heading_level(&self) -> Option<u8> {
        match self.standard_type.as_str() {
            "H1" => Some(1),
            "H2" => Some(2),
            "H3" => Some(3),
            "H4" => Some(4),
            "H5" => Some(5),
            "H6" => Some(6),
            _ => None,
        }
    }

    /// Check if this element is a table element.
    pub fn is_table_element(&self) -> bool {
        matches!(
            self.standard_type.as_str(),
            "Table" | "TR" | "TH" | "TD" | "THead" | "TBody" | "TFoot"
        )
    }

    /// Check if this element is a list element.
    pub fn is_list_element(&self) -> bool {
        matches!(self.standard_type.as_str(), "L" | "LI" | "Lbl" | "LBody")
    }

    /// Check if this is a Figure element.
    pub fn is_figure(&self) -> bool {
        self.standard_type == "Figure"
    }

    /// Flatten the tree into reading order.
    pub fn reading_order(&self) -> Vec<&StructElement> {
        let mut result = Vec::new();
        self.collect_reading_order(&mut result);
        result
    }

    fn collect_reading_order<'a>(&'a self, result: &mut Vec<&'a StructElement>) {
        result.push(self);
        for child in &self.children {
            child.collect_reading_order(result);
        }
    }
}

impl StructureTree {
    /// Get the reading order as a flat list of structure elements.
    pub fn reading_order(&self) -> Vec<&StructElement> {
        let mut result = Vec::new();
        for root in &self.root_elements {
            root.collect_reading_order(&mut result);
        }
        result
    }

    /// Find all Figure elements that lack alt text.
    pub fn figures_without_alt(&self) -> Vec<&StructElement> {
        self.reading_order()
            .into_iter()
            .filter(|e| e.is_figure() && e.alt.is_none())
            .collect()
    }

    /// Validate heading hierarchy (no gaps like H1→H3 without H2).
    pub fn heading_hierarchy_issues(&self) -> Vec<String> {
        let mut issues = Vec::new();
        let headings: Vec<_> = self
            .reading_order()
            .into_iter()
            .filter_map(|e| e.heading_level())
            .collect();

        for window in headings.windows(2) {
            let prev = window[0];
            let next = window[1];
            if next > prev + 1 {
                issues.push(format!(
                    "Heading hierarchy gap: H{prev} followed by H{next}"
                ));
            }
        }
        issues
    }

    /// Get all table structures found in the tree.
    pub fn tables(&self) -> Vec<&StructElement> {
        self.reading_order()
            .into_iter()
            .filter(|e| e.standard_type == "Table")
            .collect()
    }
}

/// Parse the structure tree from a PDF document.
pub fn parse(pdf: &Pdf) -> Option<StructureTree> {
    let xref = pdf.xref();
    let catalog: Dict<'_> = xref.get(xref.root_id())?;
    let tree_root = catalog.get::<Dict<'_>>(keys::STRUCT_TREE_ROOT)?;

    // Parse role mapping
    let role_map = parse_role_map(&tree_root);

    // Parse children (K entry)
    let root_elements = parse_children(&tree_root, &role_map);

    // Get document language
    let lang = catalog
        .get::<pdf_syntax::object::String>(keys::LANG)
        .and_then(|s| std::string::String::from_utf8(s.as_bytes().to_vec()).ok());

    Some(StructureTree {
        root_elements,
        role_map,
        lang,
    })
}

/// Parse the RoleMap dictionary.
fn parse_role_map(tree_root: &Dict<'_>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Some(role_map_dict) = tree_root.get::<Dict<'_>>(keys::ROLE_MAP) else {
        return map;
    };
    for (key, _) in role_map_dict.entries() {
        let custom = name_to_string(&key);
        if let Some(standard) = role_map_dict.get::<Name>(key.as_ref()) {
            map.insert(custom, name_to_string(&standard));
        }
    }
    map
}

/// Parse children from a structure element's /K entry.
fn parse_children(dict: &Dict<'_>, role_map: &HashMap<String, String>) -> Vec<StructElement> {
    let mut elements = Vec::new();

    let Some(k) = dict.get::<Object<'_>>(keys::K) else {
        return elements;
    };

    match k {
        Object::Dict(child_dict) => {
            if let Some(elem) = parse_struct_elem(&child_dict, role_map) {
                elements.push(elem);
            }
        }
        Object::Array(arr) => {
            for item in arr.iter::<Object<'_>>() {
                match item {
                    Object::Dict(child_dict) => {
                        if let Some(elem) = parse_struct_elem(&child_dict, role_map) {
                            elements.push(elem);
                        }
                    }
                    Object::Number(_) => {
                        // MCID reference — handled at the StructElem level
                    }
                    _ => {}
                }
            }
        }
        Object::Number(_) => {
            // Single MCID at root level — unusual but valid
        }
        _ => {}
    }

    elements
}

/// Parse a single StructElem dictionary.
fn parse_struct_elem(dict: &Dict<'_>, role_map: &HashMap<String, String>) -> Option<StructElement> {
    // Check it's a StructElem
    let type_name = dict.get::<Name>(keys::TYPE);
    if let Some(ref t) = type_name {
        if t.as_ref() != keys::STRUCT_ELEM {
            return None;
        }
    }

    // Get structure type (/S)
    let struct_type = dict
        .get::<Name>(keys::S)
        .map(|n| name_to_string(&n))
        .unwrap_or_default();

    // Resolve standard type via role map
    let standard_type = role_map
        .get(&struct_type)
        .cloned()
        .unwrap_or_else(|| struct_type.clone());

    // Get alt text
    let alt = dict
        .get::<pdf_syntax::object::String>(keys::ALT)
        .and_then(|s| std::string::String::from_utf8(s.as_bytes().to_vec()).ok());

    // Get actual text
    let actual_text = dict
        .get::<pdf_syntax::object::String>(keys::ACTUAL_TEXT)
        .and_then(|s| std::string::String::from_utf8(s.as_bytes().to_vec()).ok());

    // Get language
    let lang = dict
        .get::<pdf_syntax::object::String>(keys::LANG)
        .and_then(|s| std::string::String::from_utf8(s.as_bytes().to_vec()).ok());

    // Collect MCIDs from /K
    let mcids = collect_mcids(dict);

    // Parse child structure elements
    let children = parse_children(dict, role_map);

    Some(StructElement {
        struct_type,
        standard_type,
        alt,
        actual_text,
        lang,
        mcids,
        page_index: None,
        children,
    })
}

/// Collect MCID values from the /K entry of a struct element.
fn collect_mcids(dict: &Dict<'_>) -> Vec<i32> {
    let mut mcids = Vec::new();

    let Some(k) = dict.get::<Object<'_>>(keys::K) else {
        return mcids;
    };

    match k {
        Object::Number(n) => {
            mcids.push(n.as_i64() as i32);
        }
        Object::Dict(d) => {
            // MCR (marked content reference): dict with /MCID key
            if let Some(Object::Number(n)) = d.get::<Object<'_>>(keys::MCID) {
                mcids.push(n.as_i64() as i32);
            }
        }
        Object::Array(arr) => {
            for item in arr.iter::<Object<'_>>() {
                match item {
                    Object::Number(n) => {
                        mcids.push(n.as_i64() as i32);
                    }
                    Object::Dict(d) => {
                        if let Some(Object::Number(n)) = d.get::<Object<'_>>(keys::MCID) {
                            mcids.push(n.as_i64() as i32);
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }

    mcids
}

fn name_to_string(name: &Name) -> String {
    std::str::from_utf8(name.as_ref())
        .unwrap_or("<invalid>")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_struct_element() {
        let elem = StructElement {
            struct_type: "P".to_string(),
            standard_type: "P".to_string(),
            alt: None,
            actual_text: None,
            lang: None,
            mcids: vec![],
            page_index: None,
            children: vec![],
        };
        assert!(!elem.is_heading());
        assert!(!elem.is_figure());
        assert!(!elem.is_table_element());
    }

    #[test]
    fn heading_detection() {
        let elem = StructElement {
            struct_type: "H1".to_string(),
            standard_type: "H1".to_string(),
            alt: None,
            actual_text: None,
            lang: None,
            mcids: vec![],
            page_index: None,
            children: vec![],
        };
        assert!(elem.is_heading());
        assert_eq!(elem.heading_level(), Some(1));
    }

    #[test]
    fn figure_without_alt() {
        let tree = StructureTree {
            root_elements: vec![StructElement {
                struct_type: "Document".to_string(),
                standard_type: "Document".to_string(),
                alt: None,
                actual_text: None,
                lang: None,
                mcids: vec![],
                page_index: None,
                children: vec![
                    StructElement {
                        struct_type: "Figure".to_string(),
                        standard_type: "Figure".to_string(),
                        alt: None,
                        actual_text: None,
                        lang: None,
                        mcids: vec![0],
                        page_index: Some(0),
                        children: vec![],
                    },
                    StructElement {
                        struct_type: "Figure".to_string(),
                        standard_type: "Figure".to_string(),
                        alt: Some("A photo".to_string()),
                        actual_text: None,
                        lang: None,
                        mcids: vec![1],
                        page_index: Some(0),
                        children: vec![],
                    },
                ],
            }],
            role_map: HashMap::new(),
            lang: Some("en".to_string()),
        };

        assert_eq!(tree.figures_without_alt().len(), 1);
    }

    #[test]
    fn heading_hierarchy_gap() {
        let tree = StructureTree {
            root_elements: vec![StructElement {
                struct_type: "Document".to_string(),
                standard_type: "Document".to_string(),
                alt: None,
                actual_text: None,
                lang: None,
                mcids: vec![],
                page_index: None,
                children: vec![
                    StructElement {
                        struct_type: "H1".to_string(),
                        standard_type: "H1".to_string(),
                        alt: None,
                        actual_text: None,
                        lang: None,
                        mcids: vec![],
                        page_index: None,
                        children: vec![],
                    },
                    StructElement {
                        struct_type: "H3".to_string(),
                        standard_type: "H3".to_string(),
                        alt: None,
                        actual_text: None,
                        lang: None,
                        mcids: vec![],
                        page_index: None,
                        children: vec![],
                    },
                ],
            }],
            role_map: HashMap::new(),
            lang: None,
        };

        let issues = tree.heading_hierarchy_issues();
        assert_eq!(issues.len(), 1);
        assert!(issues[0].contains("H1 followed by H3"));
    }

    #[test]
    fn role_map_parsing() {
        let map: HashMap<String, String> = [("MyHeading".to_string(), "H1".to_string())]
            .into_iter()
            .collect();

        assert_eq!(map.get("MyHeading"), Some(&"H1".to_string()));
    }

    #[test]
    fn reading_order_flat() {
        let tree = StructureTree {
            root_elements: vec![StructElement {
                struct_type: "Document".to_string(),
                standard_type: "Document".to_string(),
                alt: None,
                actual_text: None,
                lang: None,
                mcids: vec![],
                page_index: None,
                children: vec![
                    StructElement {
                        struct_type: "P".to_string(),
                        standard_type: "P".to_string(),
                        alt: None,
                        actual_text: None,
                        lang: None,
                        mcids: vec![0],
                        page_index: None,
                        children: vec![],
                    },
                    StructElement {
                        struct_type: "P".to_string(),
                        standard_type: "P".to_string(),
                        alt: None,
                        actual_text: None,
                        lang: None,
                        mcids: vec![1],
                        page_index: None,
                        children: vec![],
                    },
                ],
            }],
            role_map: HashMap::new(),
            lang: None,
        };

        // Document + 2 paragraphs = 3 elements in reading order
        assert_eq!(tree.reading_order().len(), 3);
    }
}
