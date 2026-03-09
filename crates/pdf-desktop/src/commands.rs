use crate::state::{render_page_png, render_thumbnail_png, AppState, OpenDocument};
use base64::Engine;
use pdf_annot::builder::{
    add_annotation_to_page, AnnotRect, AnnotationBuilder, LineEnding, StampName, TextIcon,
};
use pdf_engine::PdfDocument;
use serde::{Deserialize, Serialize};
use tauri::State;

// ── Response types ──────────────────────────────────────────────────

#[derive(Serialize)]
pub struct DocumentHandle {
    pub handle: u32,
    pub page_count: usize,
    pub title: String,
    pub file_name: String,
}

#[derive(Serialize)]
pub struct DocumentInfoResponse {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    pub page_count: usize,
}

#[derive(Serialize)]
pub struct PageGeometryResponse {
    pub width: f64,
    pub height: f64,
    pub rotation: u32,
}

#[derive(Serialize)]
pub struct BookmarkResponse {
    pub title: String,
    pub page: Option<usize>,
    pub children: Vec<BookmarkResponse>,
}

// ── Annotation request types ────────────────────────────────────────

#[derive(Deserialize)]
pub struct AddAnnotationRequest {
    pub handle: u32,
    pub page: u32,
    #[serde(rename = "type")]
    pub annot_type: String,
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub color: Option<[f64; 3]>,
    pub opacity: Option<f64>,
    pub border_width: Option<f64>,
    pub contents: Option<String>,
    pub font_size: Option<f64>,
    pub stamp_name: Option<String>,
    pub icon: Option<String>,
    pub line_ending_start: Option<String>,
    pub line_ending_end: Option<String>,
    pub ink_paths: Option<Vec<Vec<f64>>>,
}

#[derive(Serialize)]
pub struct AnnotationInfo {
    pub index: usize,
    pub subtype: String,
    pub page: u32,
    pub rect: [f64; 4],
}

// ── Commands ────────────────────────────────────────────────────────

#[tauri::command]
pub fn open_document(
    state: State<'_, AppState>,
    path: String,
    password: Option<String>,
) -> Result<DocumentHandle, String> {
    let data = std::fs::read(&path).map_err(|e| format!("failed to read file: {e}"))?;

    let doc = if let Some(pw) = &password {
        PdfDocument::open_with_password(data.clone(), pw)
    } else {
        PdfDocument::open(data.clone())
    }
    .map_err(|e| format!("failed to open PDF: {e}"))?;

    let page_count = doc.page_count();
    let info = doc.info();
    let title = info
        .title
        .clone()
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| file_name_from_path(&path));
    let file_name = file_name_from_path(&path);

    let handle = state.alloc_handle();
    state.documents.lock().unwrap().insert(
        handle,
        OpenDocument {
            path,
            doc,
            saved_bytes: data.clone(),
            raw_bytes: data,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        },
    );

    Ok(DocumentHandle {
        handle,
        page_count,
        title,
        file_name,
    })
}

#[tauri::command]
pub fn close_document(state: State<'_, AppState>, handle: u32) -> Result<(), String> {
    state
        .documents
        .lock()
        .unwrap()
        .remove(&handle)
        .ok_or_else(|| "document not found".to_string())?;
    Ok(())
}

#[tauri::command]
pub fn page_count(state: State<'_, AppState>, handle: u32) -> Result<usize, String> {
    let docs = state.documents.lock().unwrap();
    let open = docs
        .get(&handle)
        .ok_or_else(|| "document not found".to_string())?;
    Ok(open.doc.page_count())
}

#[tauri::command]
pub fn render_page(
    state: State<'_, AppState>,
    handle: u32,
    page_index: usize,
    dpi: f64,
) -> Result<String, String> {
    let docs = state.documents.lock().unwrap();
    let open = docs
        .get(&handle)
        .ok_or_else(|| "document not found".to_string())?;

    let png_bytes = render_page_png(&open.doc, page_index, dpi)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&png_bytes))
}

#[tauri::command]
pub fn render_thumbnail(
    state: State<'_, AppState>,
    handle: u32,
    page_index: usize,
) -> Result<String, String> {
    let docs = state.documents.lock().unwrap();
    let open = docs
        .get(&handle)
        .ok_or_else(|| "document not found".to_string())?;

    let png_bytes = render_thumbnail_png(&open.doc, page_index, 200)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&png_bytes))
}

#[tauri::command]
pub fn document_info(
    state: State<'_, AppState>,
    handle: u32,
) -> Result<DocumentInfoResponse, String> {
    let docs = state.documents.lock().unwrap();
    let open = docs
        .get(&handle)
        .ok_or_else(|| "document not found".to_string())?;
    let info = open.doc.info();
    Ok(DocumentInfoResponse {
        title: info.title.clone(),
        author: info.author.clone(),
        subject: info.subject.clone(),
        creator: info.creator.clone(),
        producer: info.producer.clone(),
        page_count: open.doc.page_count(),
    })
}

#[tauri::command]
pub fn get_page_geometry(
    state: State<'_, AppState>,
    handle: u32,
    page_index: usize,
) -> Result<PageGeometryResponse, String> {
    let docs = state.documents.lock().unwrap();
    let open = docs
        .get(&handle)
        .ok_or_else(|| "document not found".to_string())?;
    let geom = open
        .doc
        .page_geometry(page_index)
        .map_err(|e| format!("geometry error: {e}"))?;
    Ok(PageGeometryResponse {
        width: geom.media_box.width(),
        height: geom.media_box.height(),
        rotation: geom.rotation.degrees(),
    })
}

#[tauri::command]
pub fn get_bookmarks(
    state: State<'_, AppState>,
    handle: u32,
) -> Result<Vec<BookmarkResponse>, String> {
    let docs = state.documents.lock().unwrap();
    let open = docs
        .get(&handle)
        .ok_or_else(|| "document not found".to_string())?;

    fn convert(items: &[pdf_engine::BookmarkItem]) -> Vec<BookmarkResponse> {
        items
            .iter()
            .map(|b| BookmarkResponse {
                title: b.title.clone(),
                page: b.page,
                children: convert(&b.children),
            })
            .collect()
    }
    Ok(convert(&open.doc.bookmarks()))
}

// ── Page manipulation stubs ─────────────────────────────────────────

#[tauri::command]
pub fn rotate_page(
    _state: State<'_, AppState>,
    _handle: u32,
    _page_index: usize,
    _degrees: u32,
) -> Result<(), String> {
    Err("Page rotation not yet implemented — requires mutation layer".to_string())
}

#[tauri::command]
pub fn delete_page(
    _state: State<'_, AppState>,
    _handle: u32,
    _page_index: usize,
) -> Result<(), String> {
    Err("Page deletion not yet implemented — requires mutation layer".to_string())
}

// ── Text extraction and search ───────────────────────────────────────

#[derive(Serialize)]
pub struct TextBlockResponse {
    pub text: String,
    pub x: f64,
    pub y: f64,
}

#[tauri::command]
pub fn extract_page_text(
    state: State<'_, AppState>,
    handle: u32,
    page_index: usize,
) -> Result<String, String> {
    let docs = state.documents.lock().unwrap();
    let open = docs
        .get(&handle)
        .ok_or_else(|| "document not found".to_string())?;
    open.doc
        .extract_text(page_index)
        .map_err(|e| format!("text extraction error: {e}"))
}

#[tauri::command]
pub fn extract_text_blocks(
    state: State<'_, AppState>,
    handle: u32,
    page_index: usize,
) -> Result<Vec<TextBlockResponse>, String> {
    let docs = state.documents.lock().unwrap();
    let open = docs
        .get(&handle)
        .ok_or_else(|| "document not found".to_string())?;
    let blocks = open
        .doc
        .extract_text_blocks(page_index)
        .map_err(|e| format!("text block extraction error: {e}"))?;
    Ok(blocks
        .iter()
        .map(|b| {
            let first_span = b.spans.first();
            TextBlockResponse {
                text: b.text(),
                x: first_span.map(|s| s.x).unwrap_or(0.0),
                y: first_span.map(|s| s.y).unwrap_or(0.0),
            }
        })
        .collect())
}

#[tauri::command]
pub fn search_document(
    state: State<'_, AppState>,
    handle: u32,
    query: String,
) -> Result<Vec<usize>, String> {
    let docs = state.documents.lock().unwrap();
    let open = docs
        .get(&handle)
        .ok_or_else(|| "document not found".to_string())?;
    Ok(open.doc.search_text(&query))
}

// ── Print and Save ───────────────────────────────────────────────────

#[tauri::command]
pub fn print_document(state: State<'_, AppState>, handle: u32) -> Result<String, String> {
    let docs = state.documents.lock().unwrap();
    let open = docs
        .get(&handle)
        .ok_or_else(|| "document not found".to_string())?;

    // Write the current PDF to a temp file for printing.
    let tmp_dir = std::env::temp_dir();
    let tmp_path = tmp_dir.join(format!("xfa-print-{handle}.pdf"));
    std::fs::write(&tmp_path, &open.raw_bytes)
        .map_err(|e| format!("failed to write temp PDF: {e}"))?;

    let path_str = tmp_path
        .to_str()
        .ok_or_else(|| "invalid temp path".to_string())?
        .to_string();

    // Launch the platform print command.
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("lpr")
            .arg(&path_str)
            .spawn()
            .map_err(|e| format!("failed to launch lpr: {e}"))?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/c", "start", "/min", "AcroRd32", "/p", &path_str])
            .spawn()
            .or_else(|_| {
                // Fallback: use ShellExecute via powershell
                std::process::Command::new("powershell")
                    .args([
                        "-Command",
                        &format!("Start-Process -FilePath '{}' -Verb Print", path_str),
                    ])
                    .spawn()
            })
            .map_err(|e| format!("failed to launch print: {e}"))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("lp")
            .arg(&path_str)
            .spawn()
            .map_err(|e| format!("failed to launch lp: {e}"))?;
    }

    Ok(path_str)
}

#[tauri::command]
pub fn save_document_as(
    state: State<'_, AppState>,
    handle: u32,
    path: String,
) -> Result<(), String> {
    let docs = state.documents.lock().unwrap();
    let open = docs
        .get(&handle)
        .ok_or_else(|| "document not found".to_string())?;

    std::fs::write(&path, &open.raw_bytes).map_err(|e| format!("failed to save PDF: {e}"))?;
    Ok(())
}

// ── Annotation commands ──────────────────────────────────────────────

#[tauri::command]
pub fn add_annotation(
    state: State<'_, AppState>,
    request: AddAnnotationRequest,
) -> Result<(), String> {
    let mut docs = state.documents.lock().unwrap();
    let open = docs
        .get_mut(&request.handle)
        .ok_or_else(|| "document not found".to_string())?;

    open.push_undo();

    let mut lopdf_doc = lopdf::Document::load_mem(&open.raw_bytes)
        .map_err(|e| format!("failed to parse PDF for annotation: {e}"))?;

    let rect = AnnotRect::new(request.x0, request.y0, request.x1, request.y1);

    let mut builder = match request.annot_type.as_str() {
        "highlight" => AnnotationBuilder::highlight(rect),
        "underline" => AnnotationBuilder::underline(rect),
        "strikeout" => AnnotationBuilder::strikeout(rect),
        "squiggly" => AnnotationBuilder::squiggly(rect),
        "freetext" => {
            let text = request.contents.as_deref().unwrap_or("");
            let font_size = request.font_size.unwrap_or(12.0);
            AnnotationBuilder::free_text(rect, text, font_size)
        }
        "text" | "stickynote" => {
            let icon = match request.icon.as_deref() {
                Some("Comment") => TextIcon::Comment,
                Some("Key") => TextIcon::Key,
                Some("Help") => TextIcon::Help,
                Some("NewParagraph") => TextIcon::NewParagraph,
                Some("Paragraph") => TextIcon::Paragraph,
                Some("Insert") => TextIcon::Insert,
                _ => TextIcon::Note,
            };
            AnnotationBuilder::sticky_note(rect, icon)
        }
        "stamp" => {
            let name = match request.stamp_name.as_deref() {
                Some("Approved") => StampName::Approved,
                Some("Draft") => StampName::Draft,
                Some("Confidential") => StampName::Confidential,
                Some("Final") => StampName::Final,
                Some("Expired") => StampName::Expired,
                Some("NotApproved") => StampName::NotApproved,
                Some("ForComment") => StampName::ForComment,
                Some("TopSecret") => StampName::TopSecret,
                Some("Departmental") => StampName::Departmental,
                Some("ForPublicRelease") => StampName::ForPublicRelease,
                Some("NotForPublicRelease") => StampName::NotForPublicRelease,
                Some("Sold") => StampName::Sold,
                Some("Experimental") => StampName::Experimental,
                Some("AsIs") => StampName::AsIs,
                _ => StampName::Draft,
            };
            AnnotationBuilder::stamp(rect, name)
        }
        "ink" => {
            let paths = request.ink_paths.unwrap_or_default();
            AnnotationBuilder::ink(rect, paths)
        }
        "square" | "rectangle" => AnnotationBuilder::square(rect),
        "circle" => AnnotationBuilder::circle(rect),
        "line" => {
            let mut b = AnnotationBuilder::line(request.x0, request.y0, request.x1, request.y1);
            if let (Some(start), Some(end)) = (&request.line_ending_start, &request.line_ending_end)
            {
                b = b.line_endings(parse_line_ending(start), parse_line_ending(end));
            }
            b
        }
        "arrow" => AnnotationBuilder::line(request.x0, request.y0, request.x1, request.y1)
            .line_endings(LineEnding::None, LineEnding::OpenArrow),
        other => return Err(format!("unknown annotation type: {other}")),
    };

    if let Some([r, g, b]) = request.color {
        builder = builder.color(r, g, b);
    }
    if let Some(opacity) = request.opacity {
        builder = builder.opacity(opacity);
    }
    if let Some(bw) = request.border_width {
        builder = builder.border_width(bw);
    }
    if let Some(contents) = &request.contents {
        if request.annot_type != "freetext" {
            builder = builder.contents(contents);
        }
    }

    let annot_id = builder
        .build(&mut lopdf_doc)
        .map_err(|e| format!("annotation build error: {e}"))?;

    add_annotation_to_page(&mut lopdf_doc, request.page, annot_id)
        .map_err(|e| format!("failed to add annotation to page: {e}"))?;

    let mut buf = Vec::new();
    lopdf_doc
        .save_to(&mut buf)
        .map_err(|e| format!("failed to serialize PDF: {e}"))?;
    open.raw_bytes = buf;
    open.reload()?;

    Ok(())
}

#[tauri::command]
pub fn delete_annotation(
    state: State<'_, AppState>,
    handle: u32,
    page: u32,
    annot_index: usize,
) -> Result<(), String> {
    let mut docs = state.documents.lock().unwrap();
    let open = docs
        .get_mut(&handle)
        .ok_or_else(|| "document not found".to_string())?;

    open.push_undo();

    let mut lopdf_doc = lopdf::Document::load_mem(&open.raw_bytes)
        .map_err(|e| format!("failed to parse PDF: {e}"))?;

    let pages = lopdf_doc.get_pages();
    let page_id = *pages
        .get(&page)
        .ok_or_else(|| format!("page {page} not found"))?;

    let page_obj = lopdf_doc
        .get_object_mut(page_id)
        .map_err(|e| format!("failed to get page object: {e}"))?;
    let annots = page_obj
        .as_dict_mut()
        .map_err(|e| format!("page is not a dict: {e}"))?
        .get_mut(b"Annots")
        .map_err(|_| "page has no annotations".to_string())?;
    let arr = annots
        .as_array_mut()
        .map_err(|e| format!("Annots is not an array: {e}"))?;

    if annot_index >= arr.len() {
        return Err(format!(
            "annotation index {annot_index} out of range ({})",
            arr.len()
        ));
    }
    arr.remove(annot_index);

    let mut buf = Vec::new();
    lopdf_doc
        .save_to(&mut buf)
        .map_err(|e| format!("failed to serialize PDF: {e}"))?;
    open.raw_bytes = buf;
    open.reload()?;

    Ok(())
}

#[tauri::command]
pub fn list_annotations(
    state: State<'_, AppState>,
    handle: u32,
    page: u32,
) -> Result<Vec<AnnotationInfo>, String> {
    let docs = state.documents.lock().unwrap();
    let open = docs
        .get(&handle)
        .ok_or_else(|| "document not found".to_string())?;

    let lopdf_doc = lopdf::Document::load_mem(&open.raw_bytes)
        .map_err(|e| format!("failed to parse PDF: {e}"))?;

    let pages = lopdf_doc.get_pages();
    let page_id = match pages.get(&page) {
        Some(id) => *id,
        None => return Ok(Vec::new()),
    };

    let page_dict = lopdf_doc
        .get_object(page_id)
        .map_err(|e| format!("failed to get page: {e}"))?;
    let annots = match page_dict.as_dict().ok().and_then(|d| d.get(b"Annots").ok()) {
        Some(obj) => obj,
        None => return Ok(Vec::new()),
    };

    let arr = match annots.as_array() {
        Ok(a) => a,
        Err(_) => return Ok(Vec::new()),
    };

    let mut result = Vec::new();
    for (i, obj) in arr.iter().enumerate() {
        let annot_id = match obj.as_reference() {
            Ok(id) => id,
            Err(_) => continue,
        };
        let annot_dict = match lopdf_doc
            .get_object(annot_id)
            .ok()
            .and_then(|o| o.as_dict().ok())
        {
            Some(d) => d,
            None => continue,
        };

        let subtype = annot_dict
            .get(b"Subtype")
            .ok()
            .and_then(|o| o.as_name().ok())
            .map(|n| std::str::from_utf8(n).unwrap_or("Unknown"))
            .unwrap_or("Unknown")
            .to_string();

        if subtype == "Widget" {
            continue;
        }

        let rect = annot_dict
            .get(b"Rect")
            .ok()
            .and_then(|o| o.as_array().ok())
            .map(|a| {
                let get_f = |idx: usize| -> f64 {
                    a.get(idx)
                        .and_then(|v| match v {
                            lopdf::Object::Real(f) => Some(*f as f64),
                            lopdf::Object::Integer(i) => Some(*i as f64),
                            _ => None,
                        })
                        .unwrap_or(0.0)
                };
                [get_f(0), get_f(1), get_f(2), get_f(3)]
            })
            .unwrap_or([0.0; 4]);

        result.push(AnnotationInfo {
            index: i,
            subtype,
            page,
            rect,
        });
    }

    Ok(result)
}

// ── Undo / Redo / Save ───────────────────────────────────────────────

#[tauri::command]
pub fn undo_document(state: State<'_, AppState>, handle: u32) -> Result<bool, String> {
    let mut docs = state.documents.lock().unwrap();
    let open = docs
        .get_mut(&handle)
        .ok_or_else(|| "document not found".to_string())?;

    if let Some(prev) = open.undo_stack.pop() {
        open.redo_stack.push(open.raw_bytes.clone());
        open.raw_bytes = prev;
        open.reload()?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[tauri::command]
pub fn redo_document(state: State<'_, AppState>, handle: u32) -> Result<bool, String> {
    let mut docs = state.documents.lock().unwrap();
    let open = docs
        .get_mut(&handle)
        .ok_or_else(|| "document not found".to_string())?;

    if let Some(next) = open.redo_stack.pop() {
        open.undo_stack.push(open.raw_bytes.clone());
        open.raw_bytes = next;
        open.reload()?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[tauri::command]
pub fn save_document(state: State<'_, AppState>, handle: u32) -> Result<(), String> {
    let mut docs = state.documents.lock().unwrap();
    let open = docs
        .get_mut(&handle)
        .ok_or_else(|| "document not found".to_string())?;

    std::fs::write(&open.path, &open.raw_bytes).map_err(|e| format!("failed to save: {e}"))?;
    open.saved_bytes = open.raw_bytes.clone();
    Ok(())
}

#[tauri::command]
pub fn is_document_dirty(state: State<'_, AppState>, handle: u32) -> Result<bool, String> {
    let docs = state.documents.lock().unwrap();
    let open = docs
        .get(&handle)
        .ok_or_else(|| "document not found".to_string())?;
    Ok(open.is_dirty())
}

// ── Helpers ──────────────────────────────────────────────────────────

fn file_name_from_path(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Untitled")
        .to_string()
}

fn parse_line_ending(s: &str) -> LineEnding {
    match s {
        "Square" => LineEnding::Square,
        "Circle" => LineEnding::Circle,
        "Diamond" => LineEnding::Diamond,
        "OpenArrow" => LineEnding::OpenArrow,
        "ClosedArrow" => LineEnding::ClosedArrow,
        "Butt" => LineEnding::Butt,
        "ROpenArrow" => LineEnding::ROpenArrow,
        "RClosedArrow" => LineEnding::RClosedArrow,
        "Slash" => LineEnding::Slash,
        _ => LineEnding::None,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────
//
// These tests exercise Tauri command logic directly through AppState,
// bypassing the State<'_, AppState> wrapper which cannot be constructed
// outside the Tauri runtime.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{render_page_png, render_thumbnail_png, AppState, OpenDocument};

    fn fixture_path(name: &str) -> String {
        let manifest = env!("CARGO_MANIFEST_DIR");
        format!("{manifest}/../../tests/corpus-mini/{name}")
    }

    /// Load a test PDF into a fresh AppState, returning the state and handle.
    fn setup(pdf_name: &str) -> (AppState, u32) {
        let path = fixture_path(pdf_name);
        let data = std::fs::read(&path).expect("failed to read test fixture");
        let doc = PdfDocument::open(data.clone()).expect("failed to open test PDF");
        let state = AppState::default();
        let handle = state.alloc_handle();
        state.documents.lock().unwrap().insert(
            handle,
            OpenDocument {
                path,
                doc,
                raw_bytes: data.clone(),
                saved_bytes: data,
                undo_stack: Vec::new(),
                redo_stack: Vec::new(),
            },
        );
        (state, handle)
    }

    // ── Open / Close ──────────────────────────────────────────────

    #[test]
    fn open_and_close_document() {
        let (state, h) = setup("simple.pdf");
        assert!(state.documents.lock().unwrap().contains_key(&h));
        state.documents.lock().unwrap().remove(&h);
        assert!(!state.documents.lock().unwrap().contains_key(&h));
    }

    #[test]
    fn handle_allocation_is_unique() {
        let state = AppState::default();
        let h1 = state.alloc_handle();
        let h2 = state.alloc_handle();
        let h3 = state.alloc_handle();
        assert_ne!(h1, h2);
        assert_ne!(h2, h3);
    }

    #[test]
    fn missing_handle_returns_error() {
        let state = AppState::default();
        let docs = state.documents.lock().unwrap();
        assert!(docs.get(&9999).is_none());
    }

    // ── Page count ────────────────────────────────────────────────

    #[test]
    fn page_count_simple() {
        let (state, h) = setup("simple.pdf");
        let docs = state.documents.lock().unwrap();
        let open = docs.get(&h).unwrap();
        assert!(open.doc.page_count() >= 1);
    }

    #[test]
    fn page_count_multipage() {
        let (state, h) = setup("multi-page.pdf");
        let docs = state.documents.lock().unwrap();
        let open = docs.get(&h).unwrap();
        assert!(open.doc.page_count() >= 2);
    }

    // ── Render ────────────────────────────────────────────────────

    #[test]
    fn render_page_returns_png() {
        let (state, h) = setup("simple.pdf");
        let docs = state.documents.lock().unwrap();
        let open = docs.get(&h).unwrap();
        let png = render_page_png(&open.doc, 0, 72.0).unwrap();
        assert!(!png.is_empty());
        // Verify PNG magic bytes.
        assert_eq!(&png[..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn render_thumbnail_returns_png() {
        let (state, h) = setup("simple.pdf");
        let docs = state.documents.lock().unwrap();
        let open = docs.get(&h).unwrap();
        let png = render_thumbnail_png(&open.doc, 0, 200).unwrap();
        assert!(!png.is_empty());
        assert_eq!(&png[..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn render_base64_roundtrip() {
        let (state, h) = setup("simple.pdf");
        let docs = state.documents.lock().unwrap();
        let open = docs.get(&h).unwrap();
        let png = render_page_png(&open.doc, 0, 72.0).unwrap();
        let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
        assert!(!b64.is_empty());
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&b64)
            .unwrap();
        assert_eq!(decoded, png);
    }

    // ── Document info ─────────────────────────────────────────────

    #[test]
    fn document_info_returns_metadata() {
        let (state, h) = setup("simple.pdf");
        let docs = state.documents.lock().unwrap();
        let open = docs.get(&h).unwrap();
        let info = open.doc.info();
        // Just verify it doesn't panic and returns a valid struct.
        let _ = info.title;
        let _ = info.author;
        let _ = info.producer;
    }

    // ── Page geometry ─────────────────────────────────────────────

    #[test]
    fn page_geometry_has_positive_dimensions() {
        let (state, h) = setup("simple.pdf");
        let docs = state.documents.lock().unwrap();
        let open = docs.get(&h).unwrap();
        let geom = open.doc.page_geometry(0).unwrap();
        assert!(geom.media_box.width() > 0.0);
        assert!(geom.media_box.height() > 0.0);
    }

    // ── Bookmarks ─────────────────────────────────────────────────

    #[test]
    fn bookmarks_returns_without_panic() {
        let (state, h) = setup("simple.pdf");
        let docs = state.documents.lock().unwrap();
        let open = docs.get(&h).unwrap();
        let _ = open.doc.bookmarks(); // May be empty, just must not panic.
    }

    // ── Text extraction ───────────────────────────────────────────

    #[test]
    fn extract_text_returns_string() {
        let (state, h) = setup("simple.pdf");
        let docs = state.documents.lock().unwrap();
        let open = docs.get(&h).unwrap();
        // Should not error on a valid page.
        let result = open.doc.extract_text(0);
        assert!(result.is_ok());
    }

    #[test]
    fn extract_text_blocks_returns_vec() {
        let (state, h) = setup("simple.pdf");
        let docs = state.documents.lock().unwrap();
        let open = docs.get(&h).unwrap();
        let result = open.doc.extract_text_blocks(0);
        assert!(result.is_ok());
    }

    #[test]
    fn extract_text_invalid_page_returns_error() {
        let (state, h) = setup("simple.pdf");
        let docs = state.documents.lock().unwrap();
        let open = docs.get(&h).unwrap();
        let result = open.doc.extract_text(9999);
        assert!(result.is_err());
    }

    // ── Search ────────────────────────────────────────────────────

    #[test]
    fn search_document_returns_pages() {
        let (state, h) = setup("simple.pdf");
        let docs = state.documents.lock().unwrap();
        let open = docs.get(&h).unwrap();
        // Search for a string unlikely to be found.
        let results = open.doc.search_text("zzzznonexistent");
        assert!(results.is_empty());
    }

    // ── Annotations ───────────────────────────────────────────────

    #[test]
    fn add_and_list_annotations() {
        let (state, h) = setup("simple.pdf");

        // Add a highlight annotation via lopdf (same path as add_annotation command).
        {
            let mut docs = state.documents.lock().unwrap();
            let open = docs.get_mut(&h).unwrap();
            open.push_undo();

            let mut lopdf_doc = lopdf::Document::load_mem(&open.raw_bytes).unwrap();
            let rect = AnnotRect::new(72.0, 700.0, 200.0, 720.0);
            let annot_id = AnnotationBuilder::highlight(rect)
                .color(1.0, 1.0, 0.0)
                .quad_points_from_rect(&rect)
                .build(&mut lopdf_doc)
                .unwrap();
            add_annotation_to_page(&mut lopdf_doc, 1, annot_id).unwrap();

            let mut buf = Vec::new();
            lopdf_doc.save_to(&mut buf).unwrap();
            open.raw_bytes = buf;
            open.reload().unwrap();
        }

        // List annotations (same logic as list_annotations command).
        {
            let docs = state.documents.lock().unwrap();
            let open = docs.get(&h).unwrap();
            let lopdf_doc = lopdf::Document::load_mem(&open.raw_bytes).unwrap();
            let pages = lopdf_doc.get_pages();
            let page_id = *pages.get(&1).unwrap();
            let page_dict = lopdf_doc.get_object(page_id).unwrap();
            let annots = page_dict
                .as_dict()
                .unwrap()
                .get(b"Annots")
                .unwrap()
                .as_array()
                .unwrap();
            assert!(!annots.is_empty());
        }
    }

    #[test]
    fn delete_annotation_removes_entry() {
        let (state, h) = setup("simple.pdf");

        // Add an annotation first.
        {
            let mut docs = state.documents.lock().unwrap();
            let open = docs.get_mut(&h).unwrap();
            let mut lopdf_doc = lopdf::Document::load_mem(&open.raw_bytes).unwrap();
            let rect = AnnotRect::new(10.0, 10.0, 50.0, 50.0);
            let annot_id = AnnotationBuilder::square(rect)
                .build(&mut lopdf_doc)
                .unwrap();
            add_annotation_to_page(&mut lopdf_doc, 1, annot_id).unwrap();
            let mut buf = Vec::new();
            lopdf_doc.save_to(&mut buf).unwrap();
            open.raw_bytes = buf;
            open.reload().unwrap();
        }

        // Count annotations before delete.
        let count_before = {
            let docs = state.documents.lock().unwrap();
            let open = docs.get(&h).unwrap();
            let lopdf_doc = lopdf::Document::load_mem(&open.raw_bytes).unwrap();
            let pages = lopdf_doc.get_pages();
            let page_id = *pages.get(&1).unwrap();
            let page_dict = lopdf_doc.get_object(page_id).unwrap();
            page_dict
                .as_dict()
                .ok()
                .and_then(|d| d.get(b"Annots").ok())
                .and_then(|a| a.as_array().ok())
                .map(|a| a.len())
                .unwrap_or(0)
        };

        // Delete last annotation (same logic as delete_annotation command).
        {
            let mut docs = state.documents.lock().unwrap();
            let open = docs.get_mut(&h).unwrap();
            open.push_undo();
            let mut lopdf_doc = lopdf::Document::load_mem(&open.raw_bytes).unwrap();
            let pages = lopdf_doc.get_pages();
            let page_id = *pages.get(&1).unwrap();
            let page_obj = lopdf_doc.get_object_mut(page_id).unwrap();
            let arr = page_obj
                .as_dict_mut()
                .unwrap()
                .get_mut(b"Annots")
                .unwrap()
                .as_array_mut()
                .unwrap();
            if !arr.is_empty() {
                arr.remove(arr.len() - 1);
            }
            let mut buf = Vec::new();
            lopdf_doc.save_to(&mut buf).unwrap();
            open.raw_bytes = buf;
            open.reload().unwrap();
        }

        // Count after delete.
        let count_after = {
            let docs = state.documents.lock().unwrap();
            let open = docs.get(&h).unwrap();
            let lopdf_doc = lopdf::Document::load_mem(&open.raw_bytes).unwrap();
            let pages = lopdf_doc.get_pages();
            let page_id = *pages.get(&1).unwrap();
            let page_dict = lopdf_doc.get_object(page_id).unwrap();
            page_dict
                .as_dict()
                .ok()
                .and_then(|d| d.get(b"Annots").ok())
                .and_then(|a| a.as_array().ok())
                .map(|a| a.len())
                .unwrap_or(0)
        };

        assert_eq!(count_after, count_before - 1);
    }

    // ── Undo / Redo ───────────────────────────────────────────────

    #[test]
    fn undo_restores_previous_state() {
        let (state, h) = setup("simple.pdf");

        let original_len = {
            let docs = state.documents.lock().unwrap();
            docs.get(&h).unwrap().raw_bytes.len()
        };

        // Mutate: add annotation.
        {
            let mut docs = state.documents.lock().unwrap();
            let open = docs.get_mut(&h).unwrap();
            open.push_undo();
            let mut lopdf_doc = lopdf::Document::load_mem(&open.raw_bytes).unwrap();
            let rect = AnnotRect::new(10.0, 10.0, 50.0, 50.0);
            let annot_id = AnnotationBuilder::circle(rect)
                .build(&mut lopdf_doc)
                .unwrap();
            add_annotation_to_page(&mut lopdf_doc, 1, annot_id).unwrap();
            let mut buf = Vec::new();
            lopdf_doc.save_to(&mut buf).unwrap();
            open.raw_bytes = buf;
            open.reload().unwrap();
        }

        let mutated_len = {
            let docs = state.documents.lock().unwrap();
            docs.get(&h).unwrap().raw_bytes.len()
        };
        assert_ne!(original_len, mutated_len);

        // Undo.
        {
            let mut docs = state.documents.lock().unwrap();
            let open = docs.get_mut(&h).unwrap();
            let prev = open.undo_stack.pop().unwrap();
            open.redo_stack.push(open.raw_bytes.clone());
            open.raw_bytes = prev;
            open.reload().unwrap();
        }

        let restored_len = {
            let docs = state.documents.lock().unwrap();
            docs.get(&h).unwrap().raw_bytes.len()
        };
        assert_eq!(original_len, restored_len);
    }

    #[test]
    fn redo_after_undo() {
        let (state, h) = setup("simple.pdf");

        // Push undo + mutate.
        {
            let mut docs = state.documents.lock().unwrap();
            let open = docs.get_mut(&h).unwrap();
            open.push_undo();
            // Simulate mutation by appending a comment.
            let mut new_bytes = open.raw_bytes.clone();
            new_bytes.extend_from_slice(b"\n% test comment\n");
            open.raw_bytes = new_bytes;
        }

        // Undo.
        {
            let mut docs = state.documents.lock().unwrap();
            let open = docs.get_mut(&h).unwrap();
            let prev = open.undo_stack.pop().unwrap();
            open.redo_stack.push(open.raw_bytes.clone());
            open.raw_bytes = prev;
        }

        // Redo.
        {
            let mut docs = state.documents.lock().unwrap();
            let open = docs.get_mut(&h).unwrap();
            assert!(!open.redo_stack.is_empty());
            let next = open.redo_stack.pop().unwrap();
            open.undo_stack.push(open.raw_bytes.clone());
            open.raw_bytes = next;
        }

        // After redo, the comment should be back.
        let docs = state.documents.lock().unwrap();
        let open = docs.get(&h).unwrap();
        let content = String::from_utf8_lossy(&open.raw_bytes);
        assert!(content.contains("% test comment"));
    }

    #[test]
    fn push_undo_clears_redo_stack() {
        let (state, h) = setup("simple.pdf");
        {
            let mut docs = state.documents.lock().unwrap();
            let open = docs.get_mut(&h).unwrap();
            open.redo_stack.push(vec![1, 2, 3]);
            assert!(!open.redo_stack.is_empty());
            open.push_undo();
            assert!(open.redo_stack.is_empty());
        }
    }

    // ── Save / Dirty ──────────────────────────────────────────────

    #[test]
    fn new_document_is_not_dirty() {
        let (state, h) = setup("simple.pdf");
        let docs = state.documents.lock().unwrap();
        let open = docs.get(&h).unwrap();
        assert!(!open.is_dirty());
    }

    #[test]
    fn mutated_document_is_dirty() {
        let (state, h) = setup("simple.pdf");
        let mut docs = state.documents.lock().unwrap();
        let open = docs.get_mut(&h).unwrap();
        open.raw_bytes.push(0);
        assert!(open.is_dirty());
    }

    #[test]
    fn save_document_as_writes_file() {
        let (state, h) = setup("simple.pdf");
        let tmp_path = std::env::temp_dir().join("xfa-test-save.pdf");
        {
            let docs = state.documents.lock().unwrap();
            let open = docs.get(&h).unwrap();
            std::fs::write(&tmp_path, &open.raw_bytes).unwrap();
        }
        let saved = std::fs::read(&tmp_path).unwrap();
        assert!(!saved.is_empty());
        assert_eq!(&saved[..5], b"%PDF-");
        let _ = std::fs::remove_file(&tmp_path);
    }

    #[test]
    fn save_resets_dirty_flag() {
        let (state, h) = setup("simple.pdf");
        let tmp_path = std::env::temp_dir().join("xfa-test-save-dirty.pdf");
        {
            let mut docs = state.documents.lock().unwrap();
            let open = docs.get_mut(&h).unwrap();
            // Mutate to make dirty.
            open.raw_bytes.push(0);
            assert!(open.is_dirty());
            // Save.
            open.saved_bytes = open.raw_bytes.clone();
            assert!(!open.is_dirty());
        }
        let _ = std::fs::remove_file(&tmp_path);
    }

    // ── File name helper ──────────────────────────────────────────

    #[test]
    fn file_name_from_path_extracts_name() {
        assert_eq!(file_name_from_path("/foo/bar/test.pdf"), "test.pdf");
        assert_eq!(file_name_from_path("simple.pdf"), "simple.pdf");
        assert_eq!(file_name_from_path("/"), "Untitled");
    }

    // ── Line ending parser ────────────────────────────────────────

    #[test]
    fn parse_line_ending_known_values() {
        assert!(matches!(parse_line_ending("Square"), LineEnding::Square));
        assert!(matches!(
            parse_line_ending("OpenArrow"),
            LineEnding::OpenArrow
        ));
        assert!(matches!(
            parse_line_ending("ClosedArrow"),
            LineEnding::ClosedArrow
        ));
        assert!(matches!(parse_line_ending("unknown"), LineEnding::None));
        assert!(matches!(parse_line_ending(""), LineEnding::None));
    }

    // ── Encrypted PDF ─────────────────────────────────────────────

    #[test]
    fn encrypted_pdf_fails_without_password() {
        let path = fixture_path("encrypted.pdf");
        let data = std::fs::read(&path).unwrap();
        // Should fail to open without password.
        let result = PdfDocument::open(data);
        assert!(result.is_err());
    }

    // ── Screenshot regression ────────────────────────────────────────

    fn golden_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("golden")
    }

    /// Render a test PDF and compare with golden screenshot.
    ///
    /// If the golden file doesn't exist, it is created (first run).
    /// On subsequent runs, the rendered PNG dimensions and size are
    /// compared to detect regressions.
    fn check_screenshot(pdf_name: &str, page: usize, dpi: f64) {
        let (state, h) = setup(pdf_name);
        let docs = state.documents.lock().unwrap();
        let open = docs.get(&h).unwrap();
        let png = render_page_png(&open.doc, page, dpi).unwrap();
        assert!(!png.is_empty());

        let golden_name = format!(
            "{}_{}_{}dpi.png",
            pdf_name.replace(".pdf", ""),
            page,
            dpi as u32
        );
        let golden_path = golden_dir().join(&golden_name);

        if !golden_path.exists() {
            std::fs::write(&golden_path, &png).unwrap();
            eprintln!("Golden created: {golden_name} ({} bytes)", png.len());
            return;
        }

        let golden = std::fs::read(&golden_path).unwrap();

        // Verify PNG header.
        assert_eq!(&png[..4], &[0x89, b'P', b'N', b'G']);

        // Extract dimensions from PNG IHDR chunk (bytes 16-23).
        let png_w = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
        let png_h = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
        let golden_w = u32::from_be_bytes([golden[16], golden[17], golden[18], golden[19]]);
        let golden_h = u32::from_be_bytes([golden[20], golden[21], golden[22], golden[23]]);

        assert_eq!(
            (png_w, png_h),
            (golden_w, golden_h),
            "Dimension mismatch for {golden_name}: rendered {png_w}x{png_h} vs golden {golden_w}x{golden_h}"
        );

        // File size should be within 20% (accounts for minor rendering differences).
        let size_ratio = png.len() as f64 / golden.len() as f64;
        assert!(
            (0.80..=1.20).contains(&size_ratio),
            "Size regression for {golden_name}: rendered {} bytes vs golden {} bytes (ratio {size_ratio:.2})",
            png.len(),
            golden.len(),
        );
    }

    #[test]
    fn screenshot_simple_pdf_page0() {
        check_screenshot("simple.pdf", 0, 72.0);
    }

    #[test]
    fn screenshot_simple_pdf_page0_150dpi() {
        check_screenshot("simple.pdf", 0, 150.0);
    }

    #[test]
    fn screenshot_multipage_pdf_page0() {
        check_screenshot("multi-page.pdf", 0, 72.0);
    }

    #[test]
    fn screenshot_acroform_page0() {
        check_screenshot("acroform.pdf", 0, 72.0);
    }
}
