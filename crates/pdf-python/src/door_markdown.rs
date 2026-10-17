//! Door lane L1: markdown converters (SPEC sections 2, 3, 10, 12, 18, 23, 24).
//!
//! Native helpers behind `pdfluent_extract._markdown`: style flags on spans,
//! per-page spans and image boxes for the legacy converter and the line
//! helpers, and the module-level markdown renderer.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{Dictionary, Document as LopdfDocument, Object};
use pdf_engine::TextSpan;
use pyo3::prelude::*;

use crate::{engine_err_to_py, PyBlock, PyDocument, PyTextSpan};

/// Style and layout bits for one engine span (lane L1).
///
/// 16 bold, 2 italic and 8 monospace mirror the front door's own span
/// flags; 32 marks text the page hides (render mode 3 or zero alpha) and
/// 64 marks text that does not run horizontally. The door partitions on
/// the last two and renders on the first three.
pub(crate) fn span_flags(span: &TextSpan) -> i32 {
    let mut flags = 0;
    if span.is_bold {
        flags |= 16;
    }
    if span.is_italic {
        flags |= 2;
    }
    if span.is_monospace == Some(true) {
        flags |= 8;
    }
    if span.is_hidden() {
        flags |= 32;
    }
    if !span_is_horizontal(span) {
        flags |= 64;
    }
    flags
}

/// Whether a span runs horizontally, upright or upside down.
///
/// Read off the x-axis angle of the span's transform; a span without one
/// counts as horizontal.
fn span_is_horizontal(span: &TextSpan) -> bool {
    let Some(t) = span.transform else {
        return true;
    };
    let angle = t[1].atan2(t[0]).to_degrees().abs();
    !(1.0..=179.0).contains(&angle)
}

/// Every span of one page, hidden ones included, in draw order.
///
/// The door's legacy converter and line helpers partition these into
/// visible and hidden themselves (a page whose only text is hidden keeps
/// it); default block extraction would already have dropped the hidden.
#[pyfunction]
#[pyo3(signature = (doc, page))]
fn page_spans(doc: &PyDocument, page: usize) -> PyResult<Vec<PyTextSpan>> {
    let blocks = doc
        .engine()?
        .extract_text_blocks(page)
        .map_err(engine_err_to_py)?;
    Ok(blocks
        .into_iter()
        .flat_map(|block| block.spans)
        .map(PyTextSpan)
        .collect())
}

/// The image boxes of one page in PDF user space, y up.
///
/// Each entry is `[x0, y0, x1, y1]` for one drawn image XObject, read off
/// the page's own content (placement transform with the unit square an
/// image draws into). The legacy converter drops text standing over one
/// when force_text asks. A page that cannot be read reports no images.
#[pyfunction]
#[pyo3(signature = (doc, page))]
fn page_images(doc: &PyDocument, page: usize) -> PyResult<Vec<[f64; 4]>> {
    let bytes = doc.current_bytes()?;
    Ok(image_boxes(&bytes, page).unwrap_or_default())
}

/// Render blocks as markdown, the module-level renderer.
///
/// Takes the blocks [`Document::extract_blocks`] hands back and returns the
/// same markdown [`Document::extract_markdown`] would write for them.
///
/// [`Document::extract_blocks`]: crate::PyDocument
/// [`Document::extract_markdown`]: crate::PyDocument
#[pyfunction]
#[pyo3(signature = (blocks))]
fn render_markdown(blocks: Vec<PyBlock>) -> String {
    let inner: Vec<pdf_engine::Block> = blocks.into_iter().map(|block| block.0).collect();
    pdf_engine::to_markdown(&inner)
}

/// Register this lane's native items on the module.
pub(crate) fn register(m: &Bound<PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(page_spans, m)?)?;
    m.add_function(wrap_pyfunction!(page_images, m)?)?;
    m.add_function(wrap_pyfunction!(render_markdown, m)?)?;
    Ok(())
}

/// A PDF graphics matrix `[a, b, c, d, e, f]`, as `cm` takes it.
type Matrix = [f64; 6];

const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// Multiply two graphics matrices: `apply` runs first, `then` after it.
fn concat(apply: &Matrix, then: &Matrix) -> Matrix {
    [
        apply[0] * then[0] + apply[1] * then[2],
        apply[0] * then[1] + apply[1] * then[3],
        apply[2] * then[0] + apply[3] * then[2],
        apply[2] * then[1] + apply[3] * then[3],
        apply[4] * then[0] + apply[5] * then[2] + then[4],
        apply[4] * then[1] + apply[5] * then[3] + then[5],
    ]
}

/// Run a point through a graphics matrix.
fn transform_point(matrix: &Matrix, x: f64, y: f64) -> (f64, f64) {
    (
        matrix[0] * x + matrix[2] * y + matrix[4],
        matrix[1] * x + matrix[3] * y + matrix[5],
    )
}

/// The box a matrix draws the unit square into, as `[x0, y0, x1, y1]`.
fn unit_box(matrix: &Matrix) -> [f64; 4] {
    let corners = [
        transform_point(matrix, 0.0, 0.0),
        transform_point(matrix, 1.0, 0.0),
        transform_point(matrix, 0.0, 1.0),
        transform_point(matrix, 1.0, 1.0),
    ];
    [
        corners.iter().map(|c| c.0).fold(f64::INFINITY, f64::min),
        corners.iter().map(|c| c.1).fold(f64::INFINITY, f64::min),
        corners
            .iter()
            .map(|c| c.0)
            .fold(f64::NEG_INFINITY, f64::max),
        corners
            .iter()
            .map(|c| c.1)
            .fold(f64::NEG_INFINITY, f64::max),
    ]
}

fn number(object: &Object) -> Option<f64> {
    match object {
        Object::Integer(n) => Some(*n as f64),
        Object::Real(n) => Some(f64::from(*n)),
        _ => None,
    }
}

fn resolve<'a>(doc: &'a LopdfDocument, object: &'a Object) -> &'a Object {
    match object {
        Object::Reference(id) => doc.get_object(*id).unwrap_or(object),
        _ => object,
    }
}

fn as_dict<'a>(doc: &'a LopdfDocument, object: &'a Object) -> Option<&'a Dictionary> {
    match resolve(doc, object) {
        Object::Dictionary(dict) => Some(dict),
        Object::Stream(stream) => Some(&stream.dict),
        _ => None,
    }
}

fn subtype_is(doc: &LopdfDocument, object: &Object, want: &[u8]) -> bool {
    as_dict(doc, object)
        .and_then(|dict| dict.get(b"Subtype").ok())
        .and_then(|subtype| resolve(doc, subtype).as_name().ok())
        .is_some_and(|name| name == want)
}

/// Collect the image boxes drawn by one content stream under `ctm`.
///
/// Form XObjects recurse with their own matrix and resources, bounded so a
/// self-referencing form cannot loop forever. Anything unreadable is
/// skipped: a partial box list still drops the text the door can see.
fn walk_content(
    doc: &LopdfDocument,
    content: &[u8],
    resources: Option<&Dictionary>,
    ctm: &Matrix,
    depth: u8,
    out: &mut Vec<[f64; 4]>,
) {
    if depth > 8 {
        return;
    }
    let Ok(parsed) = lopdf::content::Content::decode(content) else {
        return;
    };
    let mut stack: Vec<Matrix> = Vec::new();
    let mut current = *ctm;
    for op in &parsed.operations {
        match op.operator.as_str() {
            "q" => stack.push(current),
            "Q" => {
                if let Some(matrix) = stack.pop() {
                    current = matrix;
                }
            }
            "cm" => {
                let numbers: Vec<f64> = op.operands.iter().filter_map(number).collect();
                if numbers.len() == 6 {
                    let matrix: Matrix = [
                        numbers[0], numbers[1], numbers[2], numbers[3], numbers[4], numbers[5],
                    ];
                    current = concat(&matrix, &current);
                }
            }
            "Do" => {
                let Some(name) = op.operands.first().and_then(|o| o.as_name().ok()) else {
                    continue;
                };
                let xobjects = resources
                    .and_then(|dict| dict.get(b"XObject").ok())
                    .and_then(|xobjects| as_dict(doc, xobjects));
                let Some(entry) = xobjects.and_then(|dict| dict.get(name).ok()) else {
                    continue;
                };
                if subtype_is(doc, entry, b"Image") {
                    out.push(unit_box(&current));
                } else if subtype_is(doc, entry, b"Form") {
                    let Some(dict) = as_dict(doc, entry) else {
                        continue;
                    };
                    let matrix: Matrix = dict
                        .get(b"Matrix")
                        .ok()
                        .and_then(|m| resolve(doc, m).as_array().ok())
                        .and_then(|array| {
                            let numbers: Vec<f64> = array
                                .iter()
                                .filter_map(|o| number(resolve(doc, o)))
                                .collect();
                            (numbers.len() == 6).then(|| {
                                [
                                    numbers[0], numbers[1], numbers[2], numbers[3], numbers[4],
                                    numbers[5],
                                ]
                            })
                        })
                        .unwrap_or(IDENTITY);
                    let nested = concat(&matrix, &current);
                    let nested_resources =
                        dict.get(b"Resources").ok().and_then(|r| as_dict(doc, r));
                    if let Object::Stream(stream) = resolve(doc, entry) {
                        if let Ok(bytes) = stream.decompressed_content() {
                            walk_content(doc, &bytes, nested_resources, &nested, depth + 1, out);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Image boxes for the 0-based `page`, or an error when the bytes do not
/// open (the caller treats that as no images). Coordinates ignore page
/// rotation and crop offsets: the fixtures the door is measured on have
/// neither.
fn image_boxes(bytes: &[u8], page: usize) -> Result<Vec<[f64; 4]>, String> {
    let doc = LopdfDocument::load_mem(bytes).map_err(|e| e.to_string())?;
    let pages = doc.get_pages();
    let Some((_, page_id)) = pages.iter().nth(page) else {
        return Ok(Vec::new());
    };
    let content = doc.get_page_content(*page_id);
    let resources = doc.get_page_resources(*page_id).ok().and_then(|r| r.0);
    let mut out = Vec::new();
    walk_content(&doc, &content, resources, &IDENTITY, 0, &mut out);
    Ok(out)
}
