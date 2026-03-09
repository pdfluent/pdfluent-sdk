use crate::state::{render_page_png, render_thumbnail_png, AppState, OpenDocument};
use base64::Engine;
use pdf_engine::PdfDocument;
use serde::Serialize;
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

// ── Commands ────────────────────────────────────────────────────────

#[tauri::command]
pub fn open_document(
    state: State<'_, AppState>,
    path: String,
    password: Option<String>,
) -> Result<DocumentHandle, String> {
    let data = std::fs::read(&path).map_err(|e| format!("failed to read file: {e}"))?;

    let doc = if let Some(pw) = password {
        PdfDocument::open_with_password(data, &pw)
    } else {
        PdfDocument::open(data)
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
    state
        .documents
        .lock()
        .unwrap()
        .insert(handle, OpenDocument { path, doc });

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

// ── Page manipulation (context menu actions) ────────────────────────
// NOTE: PdfDocument is currently read-only (pdf-syntax based).
// Full mutation support (rotate, delete, reorder) will be added when
// the pdf-engine gains a mutation layer on top of lopdf (issue #333).

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

fn file_name_from_path(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Untitled")
        .to_string()
}
