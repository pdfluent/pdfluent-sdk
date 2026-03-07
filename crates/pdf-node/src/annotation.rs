//! Annotation access bindings for Node.js.

use napi_derive::napi;
use pdf_annot::Annotation;
use pdf_syntax::Pdf;

/// Information about a single annotation.
#[napi(object)]
pub struct AnnotationInfo {
    /// Annotation subtype (e.g., "Text", "Link", "Widget").
    pub annotation_type: String,
    /// Annotation rectangle [x0, y0, x1, y1] in page coordinates.
    pub rect: Option<Vec<f64>>,
    /// Text contents of the annotation, if any.
    pub contents: Option<String>,
    /// Author (T entry), if any.
    pub author: Option<String>,
    /// Whether the annotation is hidden.
    pub hidden: bool,
    /// Whether the annotation should print.
    pub printable: bool,
}

/// Extract annotations from a specific page of a PDF.
///
/// Returns an error if `page_index` is out of range.
pub(crate) fn page_annotations(pdf: &Pdf, page_index: usize) -> napi::Result<Vec<AnnotationInfo>> {
    let pages = pdf.pages();
    let page = pages.get(page_index).ok_or_else(|| {
        let count = pages.len();
        napi::Error::from_reason(format!(
            "page {page_index} out of range (document has {count} pages)"
        ))
    })?;

    Ok(Annotation::from_page(page)
        .into_iter()
        .map(|annot| {
            let rect = annot.rect().map(|r| vec![r.x0, r.y0, r.x1, r.y1]);
            AnnotationInfo {
                annotation_type: format!("{:?}", annot.annotation_type()),
                rect,
                contents: annot.contents(),
                author: annot.author(),
                hidden: annot.is_hidden(),
                printable: annot.is_printable(),
            }
        })
        .collect())
}
