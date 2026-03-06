//! PDF bookmarks (outlines) — read, create, modify, import/export.
//!
//! Bookmarks in PDF are stored as an outline tree using linked-list
//! dictionaries with /First, /Last, /Next, /Prev, /Parent pointers.

use crate::error::{ManipError, Result};
use lopdf::{dictionary, Document, Object, ObjectId};

/// A bookmark action (where clicking the bookmark navigates to).
#[derive(Debug, Clone)]
pub enum BookmarkAction {
    /// Navigate to a page in this document.
    /// Page number is 1-based.
    GoTo {
        page: u32,
        /// Fit type: "Fit", "FitH", "FitV", "XYZ", etc.
        fit: FitType,
    },
    /// Navigate to a page in an external document.
    GoToR { file: String, page: u32 },
    /// Open a URI.
    Uri(String),
    /// Named action (e.g., "NextPage", "PrevPage").
    Named(String),
}

/// Destination fit types.
#[derive(Debug, Clone, Default)]
pub enum FitType {
    /// Fit the entire page in the window.
    #[default]
    Fit,
    /// Fit the width of the page.
    FitH(f32),
    /// Fit the height of the page.
    FitV(f32),
    /// Display at specific position and zoom.
    Xyz {
        x: Option<f32>,
        y: Option<f32>,
        zoom: Option<f32>,
    },
}

/// Text style flags for bookmarks.
#[derive(Debug, Clone, Copy, Default)]
pub struct BookmarkStyle {
    /// Italic text.
    pub italic: bool,
    /// Bold text.
    pub bold: bool,
    /// Text color (RGB, 0.0–1.0). None = default (black).
    pub color: Option<(f32, f32, f32)>,
}

/// A bookmark node in the outline tree.
#[derive(Debug, Clone)]
pub struct Bookmark {
    /// Display title.
    pub title: String,
    /// Action when clicked.
    pub action: BookmarkAction,
    /// Text style.
    pub style: BookmarkStyle,
    /// Whether the bookmark is initially open (showing children).
    pub open: bool,
    /// Child bookmarks.
    pub children: Vec<Bookmark>,
}

impl Bookmark {
    /// Create a simple bookmark to a page.
    pub fn new(title: impl Into<String>, page: u32) -> Self {
        Self {
            title: title.into(),
            action: BookmarkAction::GoTo {
                page,
                fit: FitType::Fit,
            },
            style: BookmarkStyle::default(),
            open: false,
            children: Vec::new(),
        }
    }

    /// Create a bookmark with children.
    pub fn with_children(title: impl Into<String>, page: u32, children: Vec<Bookmark>) -> Self {
        Self {
            title: title.into(),
            action: BookmarkAction::GoTo {
                page,
                fit: FitType::Fit,
            },
            style: BookmarkStyle::default(),
            open: true,
            children,
        }
    }
}

/// Read all bookmarks from a document.
pub fn read_bookmarks(doc: &Document) -> Result<Vec<Bookmark>> {
    let catalog_ref = doc
        .trailer
        .get(b"Root")
        .map_err(|_| ManipError::Other("no Root in trailer".into()))?
        .as_reference()
        .map_err(|_| ManipError::Other("Root is not a reference".into()))?;

    let catalog = doc
        .get_dictionary(catalog_ref)
        .map_err(|_| ManipError::Other("cannot read catalog".into()))?;

    let outlines_ref = match catalog.get(b"Outlines") {
        Ok(obj) => match obj.as_reference() {
            Ok(r) => r,
            Err(_) => return Ok(Vec::new()),
        },
        Err(_) => return Ok(Vec::new()),
    };

    let outlines = doc
        .get_dictionary(outlines_ref)
        .map_err(|_| ManipError::Other("cannot read Outlines".into()))?;

    let first_ref = match outlines.get(b"First") {
        Ok(obj) => obj
            .as_reference()
            .map_err(|_| ManipError::Other("First is not a reference".into()))?,
        Err(_) => return Ok(Vec::new()),
    };

    let pages = doc.get_pages();
    let page_id_to_num: std::collections::HashMap<ObjectId, u32> =
        pages.iter().map(|(&num, &id)| (id, num)).collect();

    read_siblings(doc, first_ref, &page_id_to_num)
}

/// Read a linked list of sibling bookmark nodes.
fn read_siblings(
    doc: &Document,
    first_ref: ObjectId,
    page_map: &std::collections::HashMap<ObjectId, u32>,
) -> Result<Vec<Bookmark>> {
    let mut bookmarks = Vec::new();
    let mut current_ref = Some(first_ref);
    let mut visited = std::collections::HashSet::new();

    while let Some(ref_id) = current_ref {
        // Guard against circular references.
        if !visited.insert(ref_id) {
            break;
        }

        let dict = match doc.get_dictionary(ref_id) {
            Ok(d) => d,
            Err(_) => break,
        };

        let title = dict
            .get(b"Title")
            .ok()
            .and_then(|t| match t {
                Object::String(bytes, _) => String::from_utf8(bytes.clone()).ok(),
                _ => None,
            })
            .unwrap_or_default();

        let action = parse_bookmark_action(doc, dict, page_map);
        let style = parse_bookmark_style(dict);

        // Read children.
        let children = if let Ok(child_ref) = dict.get(b"First").and_then(|o| {
            o.as_reference()
                .map_err(|_| lopdf::Error::DictKey("First".into()))
        }) {
            read_siblings(doc, child_ref, page_map)?
        } else {
            Vec::new()
        };

        // Check if open (Count > 0 means open).
        let open = dict
            .get(b"Count")
            .ok()
            .and_then(|o| o.as_i64().ok())
            .map(|c| c > 0)
            .unwrap_or(false);

        bookmarks.push(Bookmark {
            title,
            action,
            style,
            open,
            children,
        });

        // Move to next sibling.
        current_ref = dict.get(b"Next").ok().and_then(|o| o.as_reference().ok());
    }

    Ok(bookmarks)
}

/// Parse a bookmark's destination/action.
fn parse_bookmark_action(
    doc: &Document,
    dict: &lopdf::Dictionary,
    page_map: &std::collections::HashMap<ObjectId, u32>,
) -> BookmarkAction {
    // Check for /Dest (direct destination).
    if let Ok(dest) = dict.get(b"Dest") {
        if let Some(action) = parse_dest(dest, page_map) {
            return action;
        }
    }

    // Check for /A (action dictionary).
    if let Ok(action_obj) = dict.get(b"A") {
        if let Ok(action_ref) = action_obj.as_reference() {
            if let Ok(action_dict) = doc.get_dictionary(action_ref) {
                return parse_action_dict(action_dict, page_map);
            }
        }
        if let Object::Dictionary(ref action_dict) = action_obj {
            return parse_action_dict(action_dict, page_map);
        }
    }

    // Default fallback.
    BookmarkAction::GoTo {
        page: 1,
        fit: FitType::Fit,
    }
}

fn parse_dest(
    dest: &Object,
    page_map: &std::collections::HashMap<ObjectId, u32>,
) -> Option<BookmarkAction> {
    if let Object::Array(arr) = dest {
        if let Some(page_ref) = arr.first().and_then(|o| o.as_reference().ok()) {
            let page = page_map.get(&page_ref).copied().unwrap_or(1);
            let fit = parse_fit_from_array(arr);
            return Some(BookmarkAction::GoTo { page, fit });
        }
    }
    None
}

fn parse_action_dict(
    dict: &lopdf::Dictionary,
    page_map: &std::collections::HashMap<ObjectId, u32>,
) -> BookmarkAction {
    let s = dict
        .get(b"S")
        .ok()
        .and_then(|o| {
            if let Object::Name(n) = o {
                Some(n.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();

    match s.as_slice() {
        b"URI" => {
            let uri = dict
                .get(b"URI")
                .ok()
                .and_then(|o| match o {
                    Object::String(bytes, _) => String::from_utf8(bytes.clone()).ok(),
                    _ => None,
                })
                .unwrap_or_default();
            BookmarkAction::Uri(uri)
        }
        b"GoToR" => {
            let file = dict
                .get(b"F")
                .ok()
                .and_then(|o| match o {
                    Object::String(bytes, _) => String::from_utf8(bytes.clone()).ok(),
                    _ => None,
                })
                .unwrap_or_default();
            BookmarkAction::GoToR { file, page: 1 }
        }
        b"Named" => {
            let name = dict
                .get(b"N")
                .ok()
                .and_then(|o| {
                    if let Object::Name(n) = o {
                        String::from_utf8(n.clone()).ok()
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            BookmarkAction::Named(name)
        }
        _ => {
            // Try GoTo with /D destination.
            if let Ok(dest) = dict.get(b"D") {
                if let Some(action) = parse_dest(dest, page_map) {
                    return action;
                }
            }
            BookmarkAction::GoTo {
                page: 1,
                fit: FitType::Fit,
            }
        }
    }
}

fn parse_fit_from_array(arr: &[Object]) -> FitType {
    let fit_name = arr.get(1).and_then(|o| {
        if let Object::Name(n) = o {
            Some(n.as_slice())
        } else {
            None
        }
    });

    match fit_name {
        Some(b"Fit") | Some(b"FitB") => FitType::Fit,
        Some(b"FitH") | Some(b"FitBH") => {
            let top = arr.get(2).and_then(obj_to_f32).unwrap_or(0.0);
            FitType::FitH(top)
        }
        Some(b"FitV") | Some(b"FitBV") => {
            let left = arr.get(2).and_then(obj_to_f32).unwrap_or(0.0);
            FitType::FitV(left)
        }
        Some(b"XYZ") => FitType::Xyz {
            x: arr.get(2).and_then(obj_to_f32),
            y: arr.get(3).and_then(obj_to_f32),
            zoom: arr.get(4).and_then(obj_to_f32),
        },
        _ => FitType::Fit,
    }
}

fn parse_bookmark_style(dict: &lopdf::Dictionary) -> BookmarkStyle {
    let flags = dict
        .get(b"F")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0);

    let color = dict.get(b"C").ok().and_then(|o| {
        if let Object::Array(arr) = o {
            if arr.len() >= 3 {
                let r = obj_to_f32(&arr[0])?;
                let g = obj_to_f32(&arr[1])?;
                let b = obj_to_f32(&arr[2])?;
                return Some((r, g, b));
            }
        }
        None
    });

    BookmarkStyle {
        italic: flags & 1 != 0,
        bold: flags & 2 != 0,
        color,
    }
}

fn obj_to_f32(obj: &Object) -> Option<f32> {
    match obj {
        Object::Integer(n) => Some(*n as f32),
        Object::Real(n) => Some(*n),
        _ => None,
    }
}

/// Write bookmarks into a document, replacing any existing outlines.
pub fn write_bookmarks(doc: &mut Document, bookmarks: &[Bookmark]) -> Result<()> {
    if bookmarks.is_empty() {
        // Remove outlines.
        let catalog_ref = doc
            .trailer
            .get(b"Root")
            .map_err(|_| ManipError::Other("no Root".into()))?
            .as_reference()
            .map_err(|_| ManipError::Other("Root not a ref".into()))?;
        if let Some(Object::Dictionary(ref mut cat)) = doc.objects.get_mut(&catalog_ref) {
            cat.remove(b"Outlines");
        }
        return Ok(());
    }

    let pages = doc.get_pages();
    let outlines_id = doc.new_object_id();
    let total_count = count_bookmarks(bookmarks);

    // Build the outline tree.
    let (first_id, last_id) = write_bookmark_siblings(doc, bookmarks, outlines_id, &pages)?;

    let outlines = dictionary! {
        "Type" => "Outlines",
        "First" => Object::Reference(first_id),
        "Last" => Object::Reference(last_id),
        "Count" => Object::Integer(total_count as i64),
    };
    doc.objects
        .insert(outlines_id, Object::Dictionary(outlines));

    // Set in catalog.
    let catalog_ref = doc
        .trailer
        .get(b"Root")
        .map_err(|_| ManipError::Other("no Root".into()))?
        .as_reference()
        .map_err(|_| ManipError::Other("Root not a ref".into()))?;
    if let Some(Object::Dictionary(ref mut cat)) = doc.objects.get_mut(&catalog_ref) {
        cat.set("Outlines", Object::Reference(outlines_id));
    }

    Ok(())
}

/// Write a list of sibling bookmarks and return (first_id, last_id).
fn write_bookmark_siblings(
    doc: &mut Document,
    bookmarks: &[Bookmark],
    parent_id: ObjectId,
    pages: &std::collections::BTreeMap<u32, ObjectId>,
) -> Result<(ObjectId, ObjectId)> {
    let mut ids: Vec<ObjectId> = Vec::new();

    for bm in bookmarks {
        let bm_id = doc.new_object_id();
        ids.push(bm_id);

        let mut dict = lopdf::Dictionary::new();
        dict.set(
            "Title",
            Object::String(bm.title.as_bytes().to_vec(), lopdf::StringFormat::Literal),
        );
        dict.set("Parent", Object::Reference(parent_id));

        // Set destination.
        match &bm.action {
            BookmarkAction::GoTo { page, fit } => {
                if let Some(&page_id) = pages.get(page) {
                    let dest = build_dest_array(page_id, fit);
                    dict.set("Dest", Object::Array(dest));
                }
            }
            BookmarkAction::Uri(uri) => {
                let action = dictionary! {
                    "S" => "URI",
                    "URI" => Object::String(uri.as_bytes().to_vec(), lopdf::StringFormat::Literal),
                };
                dict.set("A", Object::Dictionary(action));
            }
            BookmarkAction::GoToR { file, page } => {
                let action = dictionary! {
                    "S" => "GoToR",
                    "F" => Object::String(file.as_bytes().to_vec(), lopdf::StringFormat::Literal),
                    "D" => Object::Array(vec![Object::Integer((*page as i64) - 1), Object::Name(b"Fit".to_vec())]),
                };
                dict.set("A", Object::Dictionary(action));
            }
            BookmarkAction::Named(name) => {
                let action = dictionary! {
                    "S" => "Named",
                    "N" => Object::Name(name.as_bytes().to_vec()),
                };
                dict.set("A", Object::Dictionary(action));
            }
        }

        // Style.
        let flags = (if bm.style.italic { 1 } else { 0 }) | (if bm.style.bold { 2 } else { 0 });
        if flags != 0 {
            dict.set("F", Object::Integer(flags));
        }
        if let Some((r, g, b)) = bm.style.color {
            dict.set(
                "C",
                Object::Array(vec![Object::Real(r), Object::Real(g), Object::Real(b)]),
            );
        }

        // Children.
        if !bm.children.is_empty() {
            let (first_child, last_child) =
                write_bookmark_siblings(doc, &bm.children, bm_id, pages)?;
            dict.set("First", Object::Reference(first_child));
            dict.set("Last", Object::Reference(last_child));
            let child_count = count_bookmarks(&bm.children) as i64;
            dict.set(
                "Count",
                Object::Integer(if bm.open { child_count } else { -child_count }),
            );
        }

        doc.objects.insert(bm_id, Object::Dictionary(dict));
    }

    // Set Next/Prev pointers.
    for i in 0..ids.len() {
        if i > 0 {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&ids[i]) {
                dict.set("Prev", Object::Reference(ids[i - 1]));
            }
        }
        if i + 1 < ids.len() {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&ids[i]) {
                dict.set("Next", Object::Reference(ids[i + 1]));
            }
        }
    }

    Ok((ids[0], *ids.last().unwrap()))
}

fn build_dest_array(page_id: ObjectId, fit: &FitType) -> Vec<Object> {
    let mut arr = vec![Object::Reference(page_id)];
    match fit {
        FitType::Fit => {
            arr.push(Object::Name(b"Fit".to_vec()));
        }
        FitType::FitH(top) => {
            arr.push(Object::Name(b"FitH".to_vec()));
            arr.push(Object::Real(*top));
        }
        FitType::FitV(left) => {
            arr.push(Object::Name(b"FitV".to_vec()));
            arr.push(Object::Real(*left));
        }
        FitType::Xyz { x, y, zoom } => {
            arr.push(Object::Name(b"XYZ".to_vec()));
            arr.push(x.map(Object::Real).unwrap_or(Object::Null));
            arr.push(y.map(Object::Real).unwrap_or(Object::Null));
            arr.push(zoom.map(Object::Real).unwrap_or(Object::Null));
        }
    }
    arr
}

fn count_bookmarks(bookmarks: &[Bookmark]) -> usize {
    let mut count = bookmarks.len();
    for bm in bookmarks {
        count += count_bookmarks(&bm.children);
    }
    count
}

/// Export bookmarks to JSON.
#[cfg(feature = "serde")]
pub fn export_bookmarks_json(bookmarks: &[Bookmark]) -> Result<String> {
    let json_bookmarks: Vec<JsonBookmark> = bookmarks.iter().map(to_json_bookmark).collect();
    serde_json::to_string_pretty(&json_bookmarks)
        .map_err(|e| ManipError::Other(format!("JSON serialization error: {e}")))
}

/// Import bookmarks from JSON.
#[cfg(feature = "serde")]
pub fn import_bookmarks_json(json: &str) -> Result<Vec<Bookmark>> {
    let json_bookmarks: Vec<JsonBookmark> = serde_json::from_str(json)
        .map_err(|e| ManipError::Other(format!("JSON parse error: {e}")))?;
    Ok(json_bookmarks.iter().map(from_json_bookmark).collect())
}

#[cfg(feature = "serde")]
#[derive(serde::Serialize, serde::Deserialize)]
struct JsonBookmark {
    title: String,
    page: Option<u32>,
    uri: Option<String>,
    open: bool,
    bold: bool,
    italic: bool,
    children: Vec<JsonBookmark>,
}

#[cfg(feature = "serde")]
fn to_json_bookmark(bm: &Bookmark) -> JsonBookmark {
    let (page, uri) = match &bm.action {
        BookmarkAction::GoTo { page, .. } => (Some(*page), None),
        BookmarkAction::Uri(uri) => (None, Some(uri.clone())),
        _ => (None, None),
    };
    JsonBookmark {
        title: bm.title.clone(),
        page,
        uri,
        open: bm.open,
        bold: bm.style.bold,
        italic: bm.style.italic,
        children: bm.children.iter().map(to_json_bookmark).collect(),
    }
}

#[cfg(feature = "serde")]
fn from_json_bookmark(jb: &JsonBookmark) -> Bookmark {
    let action = if let Some(uri) = &jb.uri {
        BookmarkAction::Uri(uri.clone())
    } else {
        BookmarkAction::GoTo {
            page: jb.page.unwrap_or(1),
            fit: FitType::Fit,
        }
    };
    Bookmark {
        title: jb.title.clone(),
        action,
        style: BookmarkStyle {
            bold: jb.bold,
            italic: jb.italic,
            color: None,
        },
        open: jb.open,
        children: jb.children.iter().map(from_json_bookmark).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_doc(num_pages: usize) -> Document {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let mut kids = Vec::new();

        for _ in 0..num_pages {
            let page = dictionary! {
                "Type" => "Page",
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => Object::Array(vec![
                    Object::Integer(0), Object::Integer(0),
                    Object::Integer(612), Object::Integer(792),
                ]),
            };
            let page_id = doc.add_object(Object::Dictionary(page));
            kids.push(Object::Reference(page_id));
        }

        let pages = dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(num_pages as i64),
            "Kids" => Object::Array(kids),
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc
    }

    #[test]
    fn test_read_empty_bookmarks() {
        let doc = make_test_doc(3);
        let bookmarks = read_bookmarks(&doc).unwrap();
        assert!(bookmarks.is_empty());
    }

    #[test]
    fn test_count_bookmarks() {
        let bookmarks = vec![
            Bookmark::new("A", 1),
            Bookmark::with_children(
                "B",
                2,
                vec![Bookmark::new("B.1", 3), Bookmark::new("B.2", 4)],
            ),
        ];
        assert_eq!(count_bookmarks(&bookmarks), 4);
    }

    #[test]
    fn test_build_dest_array_fit() {
        let page_id = (1, 0);
        let arr = build_dest_array(page_id, &FitType::Fit);
        assert_eq!(arr.len(), 2);
    }

    #[test]
    fn test_build_dest_array_xyz() {
        let page_id = (1, 0);
        let arr = build_dest_array(
            page_id,
            &FitType::Xyz {
                x: Some(100.0),
                y: Some(200.0),
                zoom: Some(1.5),
            },
        );
        assert_eq!(arr.len(), 5);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn test_json_roundtrip() {
        let bookmarks = vec![
            Bookmark::new("Chapter 1", 1),
            Bookmark::with_children("Chapter 2", 5, vec![Bookmark::new("Section 2.1", 6)]),
        ];
        let json = export_bookmarks_json(&bookmarks).unwrap();
        let restored = import_bookmarks_json(&json).unwrap();
        assert_eq!(restored.len(), 2);
        assert_eq!(restored[0].title, "Chapter 1");
        assert_eq!(restored[1].children.len(), 1);
    }
}
