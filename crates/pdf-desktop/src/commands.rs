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
            raw_bytes: data,
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
