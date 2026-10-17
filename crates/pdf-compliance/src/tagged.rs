//! Tagged PDF: structure tree parsing, reading order, alt text extraction.
//!
//! Parses the structure tree from a PDF document (StructTreeRoot),
//! resolves role mappings, extracts reading order, alt text, and
//! table/list structure.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_syntax::object::dict::keys;
use pdf_syntax::object::{Dict, Name, Object, ObjectIdentifier};
use pdf_syntax::Pdf;
use std::collections::{BTreeSet, HashMap};

/// Maximum structure-tree depth. Bounds recursion on deeply nested or
/// adversarial `/K` trees so a malformed tagged PDF cannot overflow the stack.
/// Real structure trees are far shallower (tables/lists rarely exceed ~30).
const MAX_STRUCTURE_DEPTH: usize = 100;

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
    ///
    /// PDF/UA-1 §7.3 requires figures to have /Alt OR /ActualText.
    /// Both satisfy the text-equivalent requirement. (#FP-7.3-actualtext)
    pub fn figures_without_alt(&self) -> Vec<&StructElement> {
        self.reading_order()
            .into_iter()
            .filter(|e| e.is_figure() && e.alt.is_none() && e.actual_text.is_none())
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

/// Per-element values parsed beside a [`StructureTree`], keyed by element path.
///
/// [`StructElement`] is constructible by downstream crates -- every field is
/// `pub` -- so it cannot grow a field without breaking their struct
/// literals. The values the `use_tags` evidence reads per element travel
/// here instead: each marked-content reference's own page, and the table
/// spans. The key is the element's path, the child indices from a root, so
/// the second child of the first root is `[0, 1]`.
///
/// [`parse_with_details`] records an entry for every element it parses.
/// Elements without an entry -- hand-built trees -- read as defaults:
/// spans `(1, 1)`, and every MCID falling back to the element's
/// [`StructElement::page_index`].
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct StructureTreeDetails {
    by_path: HashMap<Vec<usize>, ElementDetails>,
}

/// The per-element values beside one [`StructElement`].
///
/// See [`StructureTreeDetails`] for how these are keyed and what a missing
/// entry reads as.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ElementDetails {
    /// Per-MCID page index, parallel to [`StructElement::mcids`], from each
    /// marked content reference's own `/Pg`. `None` entries fall back to
    /// [`StructElement::page_index`].
    pub mcid_pages: Vec<Option<usize>>,
    /// Table row span from the `/A` attributes (`/RowSpan`), 1 unless the
    /// document says otherwise. Only meaningful on `TD`/`TH`.
    pub row_span: u16,
    /// Table column span from the `/A` attributes (`/ColSpan`), 1 unless the
    /// document says otherwise. Only meaningful on `TD`/`TH`.
    pub col_span: u16,
}

impl ElementDetails {
    /// The details for one element: its references' own pages and its spans.
    pub fn new(mcid_pages: Vec<Option<usize>>, row_span: u16, col_span: u16) -> Self {
        Self {
            mcid_pages,
            row_span,
            col_span,
        }
    }
}

impl StructureTreeDetails {
    /// The details recorded for the element at `path`, if any.
    pub fn get(&self, path: &[usize]) -> Option<&ElementDetails> {
        self.by_path.get(path)
    }

    /// Record the details for the element at `path`.
    pub fn insert(&mut self, path: &[usize], details: ElementDetails) {
        self.by_path.insert(path.to_vec(), details);
    }

    /// The page the `index`-th MCID of the element at `path` resolves to
    /// from its own `/Pg`, or `None` where the caller falls back to the
    /// element's [`StructElement::page_index`].
    pub fn mcid_page(&self, path: &[usize], index: usize) -> Option<usize> {
        self.get(path)?.mcid_pages.get(index).copied().flatten()
    }

    /// The `(row_span, col_span)` of the element at `path`, `(1, 1)` where
    /// no details were recorded.
    pub fn spans(&self, path: &[usize]) -> (u16, u16) {
        self.get(path)
            .map(|d| (d.row_span, d.col_span))
            .unwrap_or((1, 1))
    }
}

/// Parse the structure tree from a PDF document.
pub fn parse(pdf: &Pdf) -> Option<StructureTree> {
    parse_with_details(pdf).map(|(tree, _)| tree)
}

/// Parse the structure tree and the per-element details beside it.
///
/// [`parse`] drops the details; the `use_tags` evidence keeps them (see
/// [`StructureTreeDetails`]).
pub fn parse_with_details(pdf: &Pdf) -> Option<(StructureTree, StructureTreeDetails)> {
    let xref = pdf.xref();
    let catalog: Dict<'_> = xref.get(xref.root_id())?;
    let tree_root = catalog.get::<Dict<'_>>(keys::STRUCT_TREE_ROOT)?;

    // Parse role mapping
    let role_map = parse_role_map(&tree_root);

    // Page order, so `/Pg` references resolve to 0-based page indices.
    let pages = page_order(&catalog);

    // Parse children (K entry). `visited` guards against cyclic /K references
    // and shared-node "tree bombs"; `depth` bounds deep nesting. `path` is
    // the details key under construction: the child indices from a root.
    let mut visited = BTreeSet::new();
    let ctx = ParseContext {
        role_map: &role_map,
        pages: &pages,
    };
    let mut details = StructureTreeDetails::default();
    let mut path = Vec::new();
    let root_elements = parse_children(
        &tree_root,
        &ctx,
        0,
        &mut visited,
        None,
        &mut path,
        &mut details,
    );

    // Get document language
    let lang = catalog
        .get::<pdf_syntax::object::String>(keys::LANG)
        .and_then(|s| std::string::String::from_utf8(s.as_bytes().to_vec()).ok());

    Some((
        StructureTree {
            root_elements,
            role_map,
            lang,
        },
        details,
    ))
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

/// Shared state for one structure-tree parse.
struct ParseContext<'a> {
    role_map: &'a HashMap<String, String>,
    /// Page object identifiers in document order, for `/Pg` resolution.
    pages: &'a [Option<ObjectIdentifier>],
}

/// Page object identifiers in document order, by walking the `/Pages` tree.
///
/// Entries that are not indirect objects (inline page dicts, which no `/Pg`
/// can reference) come back as `None`.
fn page_order(catalog: &Dict<'_>) -> Vec<Option<ObjectIdentifier>> {
    let mut out = Vec::new();
    if let Some(pages) = catalog.get::<Dict<'_>>(keys::PAGES) {
        collect_pages(&pages, &mut out, 0);
    }
    out
}

/// Depth of `/Pages` intermediate nodes. Real trees are a handful deep; this
/// bounds adversarial ones the way [`MAX_STRUCTURE_DEPTH`] bounds the tree.
const MAX_PAGE_TREE_DEPTH: usize = 64;

fn collect_pages(node: &Dict<'_>, out: &mut Vec<Option<ObjectIdentifier>>, depth: usize) {
    if depth >= MAX_PAGE_TREE_DEPTH {
        return;
    }
    let Some(kids) = node.get::<Object<'_>>(keys::KIDS) else {
        // No `/Kids`: a page (or a malformed node holding no pages).
        out.push(node.obj_id());
        return;
    };
    let Object::Array(arr) = kids else {
        out.push(node.obj_id());
        return;
    };
    for kid in arr.iter::<Object<'_>>() {
        if let Object::Dict(kid_dict) = kid {
            // An intermediate node carries `/Kids` of its own; anything else
            // is a page.
            if kid_dict.contains_key(keys::KIDS) {
                collect_pages(&kid_dict, out, depth + 1);
            } else {
                out.push(kid_dict.obj_id());
            }
        }
    }
}

/// Resolve a `/Pg` entry on `dict` to a 0-based page index.
fn resolve_page(dict: &Dict<'_>, pages: &[Option<ObjectIdentifier>]) -> Option<usize> {
    let page = dict.get::<Dict<'_>>(keys::PG)?;
    let id = page.obj_id()?;
    pages.iter().position(|p| *p == Some(id))
}

/// Parse children from a structure element's /K entry.
fn parse_children(
    dict: &Dict<'_>,
    ctx: &ParseContext<'_>,
    depth: usize,
    visited: &mut BTreeSet<ObjectIdentifier>,
    parent_page: Option<usize>,
    path: &mut Vec<usize>,
    details: &mut StructureTreeDetails,
) -> Vec<StructElement> {
    let mut elements = Vec::new();

    let Some(k) = dict.get::<Object<'_>>(keys::K) else {
        return elements;
    };

    // A `/Pg` on this element is what its bare-integer `/K` entries (and,
    // where they carry none of their own, its children's) refer to.
    let page = resolve_page(dict, ctx.pages).or(parent_page);

    match k {
        Object::Dict(child_dict) => {
            // A lone content reference as `/K`: content, not a child element.
            if !child_dict.contains_key(keys::MCID) {
                path.push(elements.len());
                if let Some(elem) =
                    parse_struct_elem(&child_dict, ctx, depth, visited, page, path, details)
                {
                    elements.push(elem);
                }
                path.pop();
            }
        }
        Object::Array(arr) => {
            for item in arr.iter::<Object<'_>>() {
                match item {
                    Object::Dict(child_dict) => {
                        // A content reference (`/Type /MCR` or `/OBJR`) is
                        // content, not a child element; only structure
                        // elements recurse. (`/MCID` alone says MCR: no
                        // structure element carries one.)
                        let is_reference = child_dict.contains_key(keys::MCID)
                            || child_dict
                                .get::<Name>(keys::TYPE)
                                .is_some_and(|t| t.as_ref() == b"MCR" || t.as_ref() == b"OBJR");
                        if !is_reference {
                            path.push(elements.len());
                            if let Some(elem) = parse_struct_elem(
                                &child_dict,
                                ctx,
                                depth,
                                visited,
                                page,
                                path,
                                details,
                            ) {
                                elements.push(elem);
                            }
                            path.pop();
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
fn parse_struct_elem(
    dict: &Dict<'_>,
    ctx: &ParseContext<'_>,
    depth: usize,
    visited: &mut BTreeSet<ObjectIdentifier>,
    parent_page: Option<usize>,
    path: &mut Vec<usize>,
    details: &mut StructureTreeDetails,
) -> Option<StructElement> {
    // Bound deep nesting (defends against adversarial chains of distinct nodes).
    if depth >= MAX_STRUCTURE_DEPTH {
        return None;
    }
    // Break cycles and shared-node "tree bombs": a struct element reachable from
    // itself (directly or via a /K back-reference) is parsed once.
    if let Some(id) = dict.obj_id() {
        if !visited.insert(id) {
            return None;
        }
    }

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
    let standard_type = ctx
        .role_map
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

    // The page this element's bare-integer `/K` entries refer to: its own
    // `/Pg`, else its parent's. On a one-page document there is nowhere else
    // they could be.
    let page_index = resolve_page(dict, ctx.pages)
        .or(parent_page)
        .or_else(|| (ctx.pages.len() == 1).then_some(0));

    // Collect MCIDs from /K, with each reference's own page where it has one.
    let refs = collect_mcids(dict, ctx.pages);
    let (mcids, mcid_pages): (Vec<i32>, Vec<Option<usize>>) = refs.into_iter().unzip();

    // Table spans from the /A attributes.
    let (row_span, col_span) = parse_spans(dict);

    // Parse child structure elements
    let children = parse_children(dict, ctx, depth + 1, visited, page_index, path, details);

    // The values beside the element, under this element's path. Recording
    // them here is safe: every early return is above (depth, cycle, type),
    // so a failed element never leaves orphan entries behind.
    details.insert(path, ElementDetails::new(mcid_pages, row_span, col_span));

    Some(StructElement {
        struct_type,
        standard_type,
        alt,
        actual_text,
        lang,
        mcids,
        page_index,
        children,
    })
}

/// Collect `(MCID, page)` pairs from the /K entry of a struct element.
///
/// Bare integers take the element's own page (`None` here, resolved by the
/// caller to the element's `page_index`); marked content reference dicts
/// resolve their own `/Pg`.
fn collect_mcids(dict: &Dict<'_>, pages: &[Option<ObjectIdentifier>]) -> Vec<(i32, Option<usize>)> {
    fn mcid_of(d: &Dict<'_>, pages: &[Option<ObjectIdentifier>]) -> Option<(i32, Option<usize>)> {
        let Object::Number(n) = d.get::<Object<'_>>(keys::MCID)? else {
            return None;
        };
        Some((n.as_i64() as i32, resolve_page(d, pages)))
    }

    let mut mcids = Vec::new();

    let Some(k) = dict.get::<Object<'_>>(keys::K) else {
        return mcids;
    };

    match k {
        Object::Number(n) => {
            mcids.push((n.as_i64() as i32, None));
        }
        Object::Dict(d) => {
            // MCR (marked content reference): dict with /MCID key
            if let Some(pair) = mcid_of(&d, pages) {
                mcids.push(pair);
            }
        }
        Object::Array(arr) => {
            for item in arr.iter::<Object<'_>>() {
                match item {
                    Object::Number(n) => {
                        mcids.push((n.as_i64() as i32, None));
                    }
                    Object::Dict(d) => {
                        if let Some(pair) = mcid_of(&d, pages) {
                            mcids.push(pair);
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

/// Read `/RowSpan` and `/ColSpan` from a structure element's `/A` attributes.
///
/// Either may sit in a lone attribute dict or in an array of them; a missing
/// or unparsable span is 1, and a span below 1 is clamped to it.
fn parse_spans(dict: &Dict<'_>) -> (u16, u16) {
    fn span_of(d: &Dict<'_>, key: &[u8]) -> Option<u16> {
        let n = d.get::<Object<'_>>(key)?;
        let Object::Number(n) = n else {
            return None;
        };
        let v = n.as_i64().clamp(1, u16::MAX as i64);
        Some(v as u16)
    }

    fn from_dict(d: &Dict<'_>) -> (Option<u16>, Option<u16>) {
        (span_of(d, b"RowSpan"), span_of(d, b"ColSpan"))
    }

    let Some(a) = dict.get::<Object<'_>>(keys::A) else {
        return (1, 1);
    };
    let mut row_span = None;
    let mut col_span = None;
    match a {
        Object::Dict(d) => {
            (row_span, col_span) = from_dict(&d);
        }
        Object::Array(arr) => {
            // Attribute objects may carry a revision number beside each dict;
            // only the dicts are read.
            for item in arr.iter::<Object<'_>>() {
                if let Object::Dict(d) = item {
                    let (r, c) = from_dict(&d);
                    row_span = row_span.or(r);
                    col_span = col_span.or(c);
                }
            }
        }
        _ => {}
    }
    (row_span.unwrap_or(1), col_span.unwrap_or(1))
}

fn name_to_string(name: &Name) -> String {
    std::str::from_utf8(name.as_ref())
        .unwrap_or("<invalid>")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Assemble a minimal one-page PDF from indirect object bodies.
    fn pdf_of(objs: &[&[u8]]) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut offsets = Vec::with_capacity(objs.len() + 1);
        offsets.push(0);
        buf.extend_from_slice(b"%PDF-1.7\n");
        for (i, body) in objs.iter().enumerate() {
            offsets.push(buf.len());
            buf.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
            buf.extend_from_slice(body);
            buf.extend_from_slice(b"\nendobj\n");
        }
        let xref_off = buf.len();
        buf.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes(),
        );
        for o in &offsets[1..] {
            buf.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
        }
        buf.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_off}\n%%EOF",
                objs.len() + 1
            )
            .as_bytes(),
        );
        buf
    }

    #[test]
    fn page_index_resolves_through_pg() {
        // A bare-integer /K under a /Pg, and an MCR carrying its own /Pg.
        let bytes = pdf_of(&[
            b"<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 4 0 R >>",
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
            b"<< /Type /StructTreeRoot /K [5 0 R 6 0 R] >>",
            b"<< /Type /StructElem /S /H1 /Pg 3 0 R /K 0 >>",
            b"<< /Type /StructElem /S /P /K <</Type /MCR /Pg 3 0 R /MCID 7>> >>",
        ]);
        let pdf = Pdf::new(bytes).expect("load tagged PDF");
        let (tree, details) = parse_with_details(&pdf).expect("structure tree parses");
        assert_eq!(tree.root_elements.len(), 2);
        let h1 = &tree.root_elements[0];
        assert_eq!(h1.mcids, vec![0]);
        assert_eq!(h1.page_index, Some(0));
        // A bare-integer /K carries no page of its own: the details say
        // `None` and the element's page is the fallback.
        assert_eq!(details.mcid_page(&[0], 0), None);
        assert_eq!(details.mcid_page(&[0], 0).or(h1.page_index), Some(0));
        let p = &tree.root_elements[1];
        assert_eq!(p.mcids, vec![7]);
        assert_eq!(details.mcid_page(&[1], 0), Some(0));
    }

    #[test]
    fn row_and_column_spans_come_from_attributes() {
        let bytes = pdf_of(&[
            b"<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 4 0 R >>",
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
            b"<< /Type /StructTreeRoot /K [5 0 R 6 0 R] >>",
            b"<< /Type /StructElem /S /TD /Pg 3 0 R /K 0 /A <</O /Table /RowSpan 2 /ColSpan 3>> >>",
            b"<< /Type /StructElem /S /TD /Pg 3 0 R /K 1 >>",
        ]);
        let pdf = Pdf::new(bytes).expect("load tagged PDF");
        let (tree, details) = parse_with_details(&pdf).expect("structure tree parses");
        assert_eq!(tree.root_elements.len(), 2);
        assert_eq!(details.spans(&[0]), (2, 3));
        assert_eq!(details.spans(&[1]), (1, 1));
        // A path the parse never recorded still reads as defaults.
        assert_eq!(details.spans(&[7]), (1, 1));
    }

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
    fn cyclic_structure_tree_terminates_and_is_bounded() {
        // Two struct elements whose /K entries reference each other form a cycle;
        // without the visited/depth guard parsing recurses until stack overflow.
        fn cyclic_pdf() -> Vec<u8> {
            let objs: [&[u8]; 6] = [
                b"<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R >>",
                b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
                b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
                b"<< /Type /StructElem /S /Sect /K [6 0 R] >>",
                b"<< /Type /StructTreeRoot /K [4 0 R] >>",
                b"<< /Type /StructElem /S /Sect /K [4 0 R] >>",
            ];
            let mut buf = Vec::new();
            let mut offsets = [0usize; 7];
            buf.extend_from_slice(b"%PDF-1.7\n");
            for (i, body) in objs.iter().enumerate() {
                offsets[i + 1] = buf.len();
                buf.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
                buf.extend_from_slice(body);
                buf.extend_from_slice(b"\nendobj\n");
            }
            let xref_off = buf.len();
            buf.extend_from_slice(b"xref\n0 7\n0000000000 65535 f \n");
            for o in &offsets[1..7] {
                buf.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
            }
            buf.extend_from_slice(
                format!("trailer\n<< /Size 7 /Root 1 0 R >>\nstartxref\n{xref_off}\n%%EOF")
                    .as_bytes(),
            );
            buf
        }

        fn count(e: &StructElement) -> usize {
            1 + e.children.iter().map(count).sum::<usize>()
        }

        let pdf = Pdf::new(cyclic_pdf()).expect("load cyclic-structure PDF");
        let tree = parse(&pdf).expect("structure tree parses");
        // Terminates (no stack overflow) and the cycle is broken into a finite,
        // small tree (the two distinct elements, each parsed once).
        let total: usize = tree.root_elements.iter().map(count).sum();
        assert!(
            total <= 3,
            "cycle/shared nodes must not inflate the tree; got {total} nodes"
        );
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
