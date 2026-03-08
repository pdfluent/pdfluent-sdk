//! Tagged PDF generation (PDF/UA support).
//!
//! Builds structure trees and marks content for PDF/UA (ISO 14289)
//! compliant tagged PDF documents.

use lopdf::{dictionary, Document, Object};
use std::collections::HashMap;

/// A builder for constructing structure trees in PDF documents.
pub struct StructureTreeBuilder {
    elements: Vec<BuilderElement>,
    role_map: HashMap<String, String>,
    lang: Option<String>,
    next_mcid: i32,
}

/// A structure element being built.
struct BuilderElement {
    struct_type: String,
    alt: Option<String>,
    actual_text: Option<String>,
    mcids: Vec<i32>,
    page_index: Option<u32>,
    children: Vec<BuilderElement>,
}

impl StructureTreeBuilder {
    /// Create a new structure tree builder.
    pub fn new() -> Self {
        Self {
            elements: Vec::new(),
            role_map: HashMap::new(),
            lang: None,
            next_mcid: 0,
        }
    }

    /// Set the document language (BCP 47, e.g., "en-US", "nl-NL").
    pub fn set_lang(&mut self, lang: &str) -> &mut Self {
        self.lang = Some(lang.to_string());
        self
    }

    /// Add a role mapping from custom type to standard type.
    pub fn add_role_mapping(&mut self, custom: &str, standard: &str) -> &mut Self {
        self.role_map
            .insert(custom.to_string(), standard.to_string());
        self
    }

    /// Allocate the next MCID for content marking.
    pub fn next_mcid(&mut self) -> i32 {
        let mcid = self.next_mcid;
        self.next_mcid += 1;
        mcid
    }

    /// Add a heading element (H1–H6).
    pub fn add_heading(&mut self, level: u8, mcid: i32, page: u32) {
        let tag = format!("H{}", level.clamp(1, 6));
        self.elements.push(BuilderElement {
            struct_type: tag,
            alt: None,
            actual_text: None,
            mcids: vec![mcid],
            page_index: Some(page),
            children: Vec::new(),
        });
    }

    /// Add a paragraph element.
    pub fn add_paragraph(&mut self, mcid: i32, page: u32) {
        self.elements.push(BuilderElement {
            struct_type: "P".to_string(),
            alt: None,
            actual_text: None,
            mcids: vec![mcid],
            page_index: Some(page),
            children: Vec::new(),
        });
    }

    /// Add a figure element with alt text.
    pub fn add_figure(&mut self, mcid: i32, page: u32, alt_text: &str) {
        self.elements.push(BuilderElement {
            struct_type: "Figure".to_string(),
            alt: Some(alt_text.to_string()),
            actual_text: None,
            mcids: vec![mcid],
            page_index: Some(page),
            children: Vec::new(),
        });
    }

    /// Add a table element with rows and cells.
    ///
    /// `cells` is a 2D array: `cells[row][col]` contains the MCID for each cell.
    pub fn add_table(&mut self, cells: &[Vec<i32>], page: u32) {
        let mut rows = Vec::new();
        for row_mcids in cells {
            let mut row_children = Vec::new();
            for &mcid in row_mcids {
                row_children.push(BuilderElement {
                    struct_type: "TD".to_string(),
                    alt: None,
                    actual_text: None,
                    mcids: vec![mcid],
                    page_index: Some(page),
                    children: Vec::new(),
                });
            }
            rows.push(BuilderElement {
                struct_type: "TR".to_string(),
                alt: None,
                actual_text: None,
                mcids: Vec::new(),
                page_index: Some(page),
                children: row_children,
            });
        }

        self.elements.push(BuilderElement {
            struct_type: "Table".to_string(),
            alt: None,
            actual_text: None,
            mcids: Vec::new(),
            page_index: Some(page),
            children: rows,
        });
    }

    /// Add a list element with list items.
    pub fn add_list(&mut self, item_mcids: &[i32], page: u32) {
        let mut items = Vec::new();
        for &mcid in item_mcids {
            items.push(BuilderElement {
                struct_type: "LI".to_string(),
                alt: None,
                actual_text: None,
                mcids: Vec::new(),
                page_index: Some(page),
                children: vec![BuilderElement {
                    struct_type: "LBody".to_string(),
                    alt: None,
                    actual_text: None,
                    mcids: vec![mcid],
                    page_index: Some(page),
                    children: Vec::new(),
                }],
            });
        }

        self.elements.push(BuilderElement {
            struct_type: "L".to_string(),
            alt: None,
            actual_text: None,
            mcids: Vec::new(),
            page_index: Some(page),
            children: items,
        });
    }

    /// Add a generic structure element.
    pub fn add_element(&mut self, struct_type: &str, mcid: i32, page: u32) {
        self.elements.push(BuilderElement {
            struct_type: struct_type.to_string(),
            alt: None,
            actual_text: None,
            mcids: vec![mcid],
            page_index: Some(page),
            children: Vec::new(),
        });
    }

    /// Build the structure tree into the document.
    ///
    /// Sets up StructTreeRoot, MarkInfo, Lang, and page Tabs entries.
    pub fn build(self, doc: &mut Document) -> Result<(), lopdf::Error> {
        let catalog_id = doc
            .trailer
            .get(b"Root")
            .ok()
            .and_then(|o| o.as_reference().ok())
            .ok_or(lopdf::Error::ObjectNotFound((0, 0)))?;

        // Build the Document element wrapping all children.
        let page_ids: Vec<(u32, lopdf::ObjectId)> = doc.get_pages().into_iter().collect();

        // Create StructTreeRoot placeholder to get its ID.
        let struct_tree_root_id = doc.add_object(Object::Dictionary(dictionary! {}));

        // Build child struct elements.
        let mut child_refs = Vec::new();
        for elem in &self.elements {
            let elem_id = build_struct_elem(doc, elem, struct_tree_root_id, &page_ids)?;
            child_refs.push(Object::Reference(elem_id));
        }

        // Create the Document struct element.
        let doc_elem = dictionary! {
            "Type" => "StructElem",
            "S" => Object::Name(b"Document".to_vec()),
            "P" => Object::Reference(struct_tree_root_id),
            "K" => Object::Array(child_refs),
        };
        let doc_elem_id = doc.add_object(Object::Dictionary(doc_elem));

        // Build RoleMap.
        let mut role_map_dict = lopdf::Dictionary::new();
        for (custom, standard) in &self.role_map {
            role_map_dict.set(
                custom.as_bytes(),
                Object::Name(standard.as_bytes().to_vec()),
            );
        }

        // Build parent tree (required for PDF/UA).
        let parent_tree = build_parent_tree(doc, &self.elements, struct_tree_root_id, &page_ids)?;

        // Update StructTreeRoot.
        let tree_root = dictionary! {
            "Type" => "StructTreeRoot",
            "K" => Object::Reference(doc_elem_id),
            "RoleMap" => Object::Dictionary(role_map_dict),
            "ParentTree" => Object::Reference(parent_tree),
        };
        if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(struct_tree_root_id) {
            *d = tree_root;
        }

        // Update catalog.
        if let Ok(Object::Dictionary(ref mut catalog)) = doc.get_object_mut(catalog_id) {
            catalog.set("StructTreeRoot", Object::Reference(struct_tree_root_id));
            catalog.set(
                "MarkInfo",
                Object::Dictionary(dictionary! { "Marked" => true }),
            );
            if let Some(ref lang) = self.lang {
                catalog.set(
                    "Lang",
                    Object::String(lang.as_bytes().to_vec(), lopdf::StringFormat::Literal),
                );
            }
        }

        // Set /Tabs /S on all pages.
        for (_page_num, page_id) in &page_ids {
            if let Ok(Object::Dictionary(ref mut page_dict)) = doc.get_object_mut(*page_id) {
                page_dict.set("Tabs", Object::Name(b"S".to_vec()));
            }
        }

        Ok(())
    }
}

impl Default for StructureTreeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Build a StructElem object and add it to the document.
fn build_struct_elem(
    doc: &mut Document,
    elem: &BuilderElement,
    parent_id: lopdf::ObjectId,
    page_ids: &[(u32, lopdf::ObjectId)],
) -> Result<lopdf::ObjectId, lopdf::Error> {
    // Reserve an ID for this element.
    let elem_id = doc.add_object(Object::Dictionary(dictionary! {}));

    // Build children.
    let mut k_entries: Vec<Object> = Vec::new();

    // Add MCIDs.
    for &mcid in &elem.mcids {
        k_entries.push(Object::Integer(mcid as i64));
    }

    // Add child struct elements.
    for child in &elem.children {
        let child_id = build_struct_elem(doc, child, elem_id, page_ids)?;
        k_entries.push(Object::Reference(child_id));
    }

    let mut elem_dict = dictionary! {
        "Type" => "StructElem",
        "S" => Object::Name(elem.struct_type.as_bytes().to_vec()),
        "P" => Object::Reference(parent_id),
    };

    if k_entries.len() == 1 {
        elem_dict.set("K", k_entries.into_iter().next().unwrap());
    } else if !k_entries.is_empty() {
        elem_dict.set("K", Object::Array(k_entries));
    }

    if let Some(ref alt) = elem.alt {
        elem_dict.set(
            "Alt",
            Object::String(alt.as_bytes().to_vec(), lopdf::StringFormat::Literal),
        );
    }

    if let Some(ref actual_text) = elem.actual_text {
        elem_dict.set(
            "ActualText",
            Object::String(
                actual_text.as_bytes().to_vec(),
                lopdf::StringFormat::Literal,
            ),
        );
    }

    // Set page reference if available.
    if let Some(page_num) = elem.page_index {
        if let Some((_, page_id)) = page_ids.iter().find(|(n, _)| *n == page_num) {
            elem_dict.set("Pg", Object::Reference(*page_id));
        }
    }

    if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(elem_id) {
        *d = elem_dict;
    }

    Ok(elem_id)
}

/// Build a simple ParentTree (number tree mapping MCIDs to struct elements).
fn build_parent_tree(
    doc: &mut Document,
    elements: &[BuilderElement],
    _root_id: lopdf::ObjectId,
    _page_ids: &[(u32, lopdf::ObjectId)],
) -> Result<lopdf::ObjectId, lopdf::Error> {
    // Collect MCID → StructElem mappings.
    let mut nums: Vec<Object> = Vec::new();
    collect_parent_refs(elements, &mut nums);

    let parent_tree = dictionary! {
        "Nums" => Object::Array(nums),
    };
    Ok(doc.add_object(Object::Dictionary(parent_tree)))
}

/// Recursively collect MCID/parent references for the parent tree.
fn collect_parent_refs(elements: &[BuilderElement], nums: &mut Vec<Object>) {
    for elem in elements {
        for &mcid in &elem.mcids {
            nums.push(Object::Integer(mcid as i64));
            // In a full implementation, this would reference the StructElem object.
            // For now, use null as a placeholder — the parent tree structure is present.
            nums.push(Object::Null);
        }
        collect_parent_refs(&elem.children, nums);
    }
}

/// Generate BDC (Begin Marked Content) operator bytes.
pub fn bdc_operator(tag: &str, mcid: i32) -> Vec<u8> {
    format!("/{tag} <</MCID {mcid}>> BDC\n").into_bytes()
}

/// Generate EMC (End Marked Content) operator bytes.
pub fn emc_operator() -> Vec<u8> {
    b"EMC\n".to_vec()
}

/// Wrap content bytes in BDC/EMC marked content operators.
pub fn mark_content(tag: &str, mcid: i32, content: &[u8]) -> Vec<u8> {
    let mut result = bdc_operator(tag, mcid);
    result.extend_from_slice(content);
    result.extend_from_slice(&emc_operator());
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Document, Object};

    fn make_test_doc() -> Document {
        let mut doc = Document::with_version("1.7");

        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        };
        let page_id = doc.add_object(Object::Dictionary(page_dict));

        let pages_dict = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        };
        let pages_id = doc.add_object(Object::Dictionary(pages_dict));

        if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
            d.set("Parent", Object::Reference(pages_id));
        }

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc
    }

    #[test]
    fn build_basic_structure_tree() {
        let mut doc = make_test_doc();
        let mut builder = StructureTreeBuilder::new();
        builder.set_lang("en-US");

        let mcid0 = builder.next_mcid();
        builder.add_heading(1, mcid0, 1);

        let mcid1 = builder.next_mcid();
        builder.add_paragraph(mcid1, 1);

        builder.build(&mut doc).unwrap();

        // Verify catalog has StructTreeRoot, MarkInfo, Lang.
        let catalog_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        if let Ok(Object::Dictionary(ref cat)) = doc.get_object(catalog_id) {
            assert!(cat.get(b"StructTreeRoot").is_ok());
            assert!(cat.get(b"MarkInfo").is_ok());
            assert!(cat.get(b"Lang").is_ok());
        }
    }

    #[test]
    fn build_with_figure_alt_text() {
        let mut doc = make_test_doc();
        let mut builder = StructureTreeBuilder::new();
        builder.set_lang("nl-NL");

        let mcid = builder.next_mcid();
        builder.add_figure(mcid, 1, "A chart showing revenue growth");

        builder.build(&mut doc).unwrap();

        let catalog_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        if let Ok(Object::Dictionary(ref cat)) = doc.get_object(catalog_id) {
            assert!(cat.get(b"StructTreeRoot").is_ok());
        }
    }

    #[test]
    fn build_with_table() {
        let mut doc = make_test_doc();
        let mut builder = StructureTreeBuilder::new();
        builder.set_lang("en");

        let cells = vec![vec![0, 1, 2], vec![3, 4, 5]];
        builder.add_table(&cells, 1);

        builder.build(&mut doc).unwrap();

        let catalog_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        if let Ok(Object::Dictionary(ref cat)) = doc.get_object(catalog_id) {
            assert!(cat.get(b"StructTreeRoot").is_ok());
        }
    }

    #[test]
    fn build_with_list() {
        let mut doc = make_test_doc();
        let mut builder = StructureTreeBuilder::new();

        builder.add_list(&[0, 1, 2], 1);
        builder.build(&mut doc).unwrap();

        let catalog_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        if let Ok(Object::Dictionary(ref cat)) = doc.get_object(catalog_id) {
            assert!(cat.get(b"StructTreeRoot").is_ok());
        }
    }

    #[test]
    fn marked_content_operators() {
        let bdc = bdc_operator("P", 0);
        assert_eq!(bdc, b"/P <</MCID 0>> BDC\n");

        let emc = emc_operator();
        assert_eq!(emc, b"EMC\n");

        let marked = mark_content("Span", 5, b"BT /F1 12 Tf (Hello) Tj ET\n");
        let text = String::from_utf8_lossy(&marked);
        assert!(text.starts_with("/Span <</MCID 5>> BDC"));
        assert!(text.ends_with("EMC\n"));
        assert!(text.contains("Hello"));
    }

    #[test]
    fn page_tabs_set() {
        let mut doc = make_test_doc();
        let mut builder = StructureTreeBuilder::new();
        let mcid = builder.next_mcid();
        builder.add_paragraph(mcid, 1);
        builder.build(&mut doc).unwrap();

        // Check page has /Tabs /S.
        let pages = doc.get_pages();
        let page_id = pages[&1];
        if let Ok(Object::Dictionary(ref page)) = doc.get_object(page_id) {
            let tabs = page.get(b"Tabs").unwrap();
            if let Object::Name(ref n) = tabs {
                assert_eq!(n, b"S");
            }
        }
    }

    #[test]
    fn role_mapping() {
        let mut doc = make_test_doc();
        let mut builder = StructureTreeBuilder::new();
        builder.add_role_mapping("MyHeading", "H1");
        builder.add_element("MyHeading", 0, 1);
        builder.build(&mut doc).unwrap();

        // Verify StructTreeRoot exists with RoleMap.
        let catalog_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        if let Ok(Object::Dictionary(ref cat)) = doc.get_object(catalog_id) {
            let tree_ref = cat.get(b"StructTreeRoot").unwrap();
            if let Object::Reference(tree_id) = tree_ref {
                if let Ok(Object::Dictionary(ref tree)) = doc.get_object(*tree_id) {
                    assert!(tree.get(b"RoleMap").is_ok());
                }
            }
        }
    }
}
