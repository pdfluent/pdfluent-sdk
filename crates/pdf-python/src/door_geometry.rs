//! Door lane L2: page geometry (SPEC sections 4, 5, 19).
//!
//! Native bridge behind `pdfluent_extract._geometry`: per-span glyph
//! boxes for column detection, and per-page links for JSON output.
//! Both take the door's native page object and hand back plain
//! Python data; all door policy lives in the Python layer.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::collections::BTreeSet;

use pdf_syntax::object::dict::keys::{CROP_BOX, MEDIA_BOX, PARENT, ROTATE};
use pdf_syntax::object::{Array, Dict};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::door_chunks::array_rect;
use crate::engine_err_to_py;
use crate::PyPage;
use pdf_engine::document::LinkTarget;
use pdf_engine::{BoundsSource, PdfDocument};

/// Register this lane's native items on the module.
pub(crate) fn register(m: &Bound<PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(door_page_spans, m)?)?;
    m.add_function(wrap_pyfunction!(door_page_links, m)?)?;
    m.add_function(wrap_pyfunction!(door_page_size, m)?)?;
    m.add_function(wrap_pyfunction!(door_page_span_geometry, m)?)?;
    Ok(())
}

/// Whether a span runs upright (straight along +x) from its transform.
fn span_is_upright(transform: Option<[f64; 6]>) -> bool {
    match transform {
        None => true,
        Some([a, b, c, d, _, _]) => b == 0.0 && c == 0.0 && a > 0.0 && d > 0.0,
    }
}

/// One page's exact size in points, read from its boxes in full precision.
///
/// The syntax layer parses every box through `f32`, which moves A4 pages by
/// ~3e-5pt; the door's y-down flips need the exact numbers, so this reads the
/// MediaBox and CropBox numbers as `f64` straight from the page tree,
/// following `/Parent` for inherited boxes. Rotation swaps the axes like the
/// engine's effective dimensions do. Falls back to the engine geometry when
/// the tree carries no usable box.
pub(crate) fn exact_page_size(doc: &PdfDocument, page: usize) -> (f64, f64) {
    let fallback = || {
        doc.page_geometry(page)
            .map(|geometry| geometry.effective_dimensions())
            .unwrap_or((0.0, 0.0))
    };
    let pdf = doc.pdf();
    let Some(handle) = pdf.pages().get(page) else {
        return fallback();
    };
    let mut media = None;
    let mut crop = None;
    let mut rotate = None;
    let mut current = Some(handle.raw().clone());
    let mut seen = BTreeSet::new();
    let mut depth = 0usize;
    while let Some(dict) = current {
        if depth > 32 {
            break;
        }
        if let Some(id) = dict.obj_id() {
            if !seen.insert(id) {
                break;
            }
        }
        if media.is_none() {
            media = dict.get::<Array<'_>>(MEDIA_BOX).and_then(array_rect);
        }
        if crop.is_none() {
            crop = dict.get::<Array<'_>>(CROP_BOX).and_then(array_rect);
        }
        if rotate.is_none() {
            rotate = dict.get::<i32>(ROTATE);
        }
        current = dict.get::<Dict<'_>>(PARENT);
        depth += 1;
    }
    let Some(media) = media else {
        return fallback();
    };
    let effective = crop.unwrap_or(media);
    let width = (effective[2] - effective[0]).abs();
    let height = (effective[3] - effective[1]).abs();
    match rotate.unwrap_or(0).rem_euclid(360) {
        90 | 270 => (height, width),
        _ => (width, height),
    }
}

/// One page's exact size in points as `(width, height)`.
///
/// Same contract as [`exact_page_size`]: the door's y-down flips read this,
/// never the `f32`-rounded page dimensions.
#[pyfunction]
#[pyo3(signature = (page))]
fn door_page_size(page: &Bound<'_, PyPage>) -> PyResult<(f64, f64)> {
    let borrowed = page.borrow();
    Ok(exact_page_size(&borrowed.doc, borrowed.index))
}

/// One text span for the door: text, origin, font size, one advance
/// box per drawn glyph in draw order, and whether the span runs
/// upright (straight along +x).
type DoorSpan = (String, f64, f64, f64, Vec<[f64; 4]>, bool);

/// One page's text spans with per-glyph advance boxes.
///
/// Turned spans are reported, not filtered; the caller decides what
/// to do with them.
#[pyfunction]
#[pyo3(signature = (page))]
fn door_page_spans(page: &Bound<'_, PyPage>) -> PyResult<Vec<DoorSpan>> {
    let borrowed = page.borrow();
    let blocks = borrowed
        .doc
        .extract_text_blocks(borrowed.index)
        .map_err(engine_err_to_py)?;
    let mut spans = Vec::new();
    for block in &blocks {
        for span in &block.spans {
            spans.push((
                span.text.clone(),
                span.x,
                span.y,
                span.font_size,
                span.char_bounds.clone(),
                span_is_upright(span.transform),
            ));
        }
    }
    Ok(spans)
}

/// One span's rich geometry for the door span dicts: text, origin, the
/// advance-sum end x, the tight ink band in page space (flat when no
/// glyph carries ink), and whether the span wires boxes (an embedded
/// font runs it upright).
///
/// The entries run in extraction order, one per span of
/// `extract_text_blocks`, so the caller zips them with those spans and
/// checks text and origin before trusting the alignment.
type DoorSpanGeometry = (String, f64, f64, f64, Option<f64>, Option<f64>, bool);

/// One page's spans with tight-geometry boxes for the door span dicts.
#[pyfunction]
#[pyo3(signature = (page))]
fn door_page_span_geometry(page: &Bound<'_, PyPage>) -> PyResult<Vec<DoorSpanGeometry>> {
    let borrowed = page.borrow();
    let blocks = borrowed
        .doc
        .extract_text_blocks_rich(borrowed.index)
        .map_err(engine_err_to_py)?;
    let mut out = Vec::new();
    for block in &blocks {
        for span in &block.spans {
            // A weight survives only from an embedded OS/2 table; the
            // standard-14 fallback leaves None (base-14 stays unwired).
            let wired = span.font_weight.is_some() && span_is_upright(span.transform);
            let end_x = span.x + span.glyph_advances.iter().sum::<f64>();
            let mut ink: Option<[f64; 2]> = None;
            if wired {
                for (bound, source) in span
                    .tight_char_bounds
                    .iter()
                    .zip(span.glyph_bounds_sources.iter())
                {
                    if *source != BoundsSource::Tight {
                        continue;
                    }
                    ink = Some(match ink {
                        None => [bound[1], bound[3]],
                        Some([low, high]) => [low.min(bound[1]), high.max(bound[3])],
                    });
                }
            }
            out.push((
                span.text.clone(),
                span.x,
                span.y,
                end_x,
                ink.map(|band| band[0]),
                ink.map(|band| band[1]),
                wired,
            ));
        }
    }
    Ok(out)
}

/// One page's links as plain dicts.
///
/// Each dict carries `kind` (`"uri"`, `"page"`, `"named"` or
/// `"unknown"`), `xref` (the annotation object number), `rect` (the
/// region in PDF user space, bottom-left origin), `uri`/`page`/`name`
/// (whichever the kind carries, else `None`) and `id` (the `/NM`
/// entry or `""`).
#[pyfunction]
#[pyo3(signature = (page))]
fn door_page_links<'py>(
    py: Python<'py>,
    page: &Bound<'_, PyPage>,
) -> PyResult<Vec<Bound<'py, PyDict>>> {
    let borrowed = page.borrow();
    let links = borrowed
        .doc
        .page_links(borrowed.index)
        .map_err(engine_err_to_py)?;
    let mut out = Vec::with_capacity(links.len());
    for link in &links {
        let dict = PyDict::new(py);
        let (kind, uri, target_page, name) = match &link.target {
            LinkTarget::Uri(uri) => ("uri", Some(uri.as_str()), None, None),
            LinkTarget::Page(index) => ("page", None, Some(*index), None),
            LinkTarget::Named(name) => ("named", None, None, Some(name.as_str())),
            LinkTarget::Unknown => ("unknown", None, None, None),
            _ => return Err(crate::compat::unknown_variant("link target")),
        };
        dict.set_item("kind", kind)?;
        dict.set_item("xref", link.xref)?;
        dict.set_item("rect", link.rect.to_vec())?;
        dict.set_item("uri", uri)?;
        dict.set_item("target_page", target_page)?;
        dict.set_item("name", name)?;
        dict.set_item("id", link.id.as_str())?;
        out.push(dict);
    }
    Ok(out)
}
