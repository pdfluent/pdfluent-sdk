use pdf_engine::{PdfDocument, RenderOptions, ThumbnailOptions};
use std::collections::HashMap;
use std::sync::Mutex;

/// A document opened in the viewer.
pub struct OpenDocument {
    #[allow(dead_code)] // used by later issues (save, print)
    pub path: String,
    pub doc: PdfDocument,
    /// Raw PDF bytes — kept for lopdf-based mutations (annotations, save).
    pub raw_bytes: Vec<u8>,
}

impl OpenDocument {
    /// Re-parse the raw bytes into a fresh PdfDocument after mutation.
    pub fn reload(&mut self) -> Result<(), String> {
        let doc = PdfDocument::open(self.raw_bytes.clone())
            .map_err(|e| format!("failed to reload document: {e}"))?;
        self.doc = doc;
        Ok(())
    }
}

/// Shared application state managed by Tauri.
pub struct AppState {
    pub documents: Mutex<HashMap<u32, OpenDocument>>,
    next_handle: Mutex<u32>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            documents: Mutex::new(HashMap::new()),
            next_handle: Mutex::new(1),
        }
    }
}

impl AppState {
    /// Allocate a new unique handle for a document.
    pub fn alloc_handle(&self) -> u32 {
        let mut h = self.next_handle.lock().unwrap();
        let id = *h;
        *h += 1;
        id
    }
}

/// Render a page to PNG bytes.
pub fn render_page_png(doc: &PdfDocument, page_index: usize, dpi: f64) -> Result<Vec<u8>, String> {
    let opts = RenderOptions {
        dpi,
        background: [1.0, 1.0, 1.0, 1.0],
        render_annotations: true,
        width: None,
        height: None,
    };
    let rendered = doc
        .render_page(page_index, &opts)
        .map_err(|e| format!("render error: {e}"))?;

    let img = image::RgbaImage::from_raw(rendered.width, rendered.height, rendered.pixels)
        .ok_or_else(|| "failed to create image from rendered pixels".to_string())?;

    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| format!("PNG encode error: {e}"))?;

    Ok(buf.into_inner())
}

/// Render a thumbnail to PNG bytes.
pub fn render_thumbnail_png(
    doc: &PdfDocument,
    page_index: usize,
    max_dim: u32,
) -> Result<Vec<u8>, String> {
    let opts = ThumbnailOptions {
        max_dimension: max_dim,
    };
    let rendered = doc
        .thumbnail(page_index, &opts)
        .map_err(|e| format!("thumbnail error: {e}"))?;

    let img = image::RgbaImage::from_raw(rendered.width, rendered.height, rendered.pixels)
        .ok_or_else(|| "failed to create thumbnail image".to_string())?;

    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| format!("PNG encode error: {e}"))?;

    Ok(buf.into_inner())
}
