//! Compatible document operations (lane L2): `Document` open/save/info methods.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! `#[pymethods]` blocks for the `Document`/`Page` classes covering the
//! read-side document surface: open state, quad/rect-free info reads, the
//! outline, page labels, encryption status, save/tobytes, and the low-level
//! cross-reference readers. Mutation-heavy siblings (`set_toc`,
//! `set_metadata`, link editing, page insertion) belong to the editing lane,
//! which adds its own blocks in its own file.
//!
//! Geometry-typed returns (`Rect`, `Point`, `Matrix`) and `Colorspace` are,
//! until the value-type and image lanes land, served by the small private
//! stand-ins below. They expose exactly the numeric attributes the recorded
//! shapes read (`x0`/`y0`/`x1`/`y1`, `x`/`y`, `a`..`f`, `n`/`name`) and are
//! deliberately NOT registered in the module namespace, so the real classes
//! take those names without a fight when they arrive.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use lopdf::{dictionary, Document as LopdfDocument, Object as LObject};
use pdf_engine::PdfDocument as EngineDocument;
use pdfluent::compat::document::Link as FacadeLink;
use pdfluent::compat::{Page as FacadePage, Value as FacadeValue};
use pdfluent::document::PdfDocument as FacadePdfDocument;
use pyo3::exceptions::{PyIndexError, PyKeyError, PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyList};

use super::super::{engine_err_to_py, PdfluentError, PdfluentIoError, PyDocument, PyPage};
use super::errors::{
    facade_error as compat_err_to_py, file_data_error, member_error, validation_error,
    FzErrorArgument, Member, ValidationKind,
};

/// The `layers` argument as a facade value: a list stays a list (the
/// generic converter would read `[]` as empty bytes), anything else converts
/// generically.
fn bboxlog_layers(value: &Bound<'_, PyAny>) -> PyResult<FacadeValue> {
    if let Ok(list) = value.downcast::<PyList>() {
        let mut out = Vec::new();
        for item in list.iter() {
            out.push(super::edit::value_from_py(&item)?);
        }
        return Ok(FacadeValue::List(out));
    }
    super::edit::value_from_py(value)
}

/// Minimal zero-page PDF bytes for an argument-less open.
///
/// Called by the compatible `Document` constructor once that lands
/// (P1 follow-up, #643); unused until then.
#[allow(dead_code)]
pub(crate) fn empty_pdf_bytes() -> Vec<u8> {
    let mut doc = LopdfDocument::with_version("1.7");
    let pages_id = doc.new_object_id();
    doc.objects.insert(
        pages_id,
        LObject::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => Vec::<LObject>::new(),
            "Count" => 0,
        }),
    );
    let catalog_id = doc.new_object_id();
    doc.objects.insert(
        catalog_id,
        LObject::Dictionary(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        }),
    );
    doc.trailer.set("Root", catalog_id);
    let mut buf = Vec::new();
    doc.save_to(&mut buf)
        .expect("an in-memory empty document serialises");
    buf
}

// ---------------------------------------------------------------------------
// Geometry stand-ins (unregistered; see the module docs)
// ---------------------------------------------------------------------------

/// Y-down rectangle stand-in: `x0`/`y0`/`x1`/`y1` attributes only.
#[pyclass(name = "Rect")]
#[derive(Clone, Copy)]
pub(crate) struct PyCompatRect {
    #[pyo3(get)]
    x0: f64,
    #[pyo3(get)]
    y0: f64,
    #[pyo3(get)]
    x1: f64,
    #[pyo3(get)]
    y1: f64,
}

#[pymethods]
impl PyCompatRect {
    fn __repr__(&self) -> String {
        format!("Rect({}, {}, {}, {})", self.x0, self.y0, self.x1, self.y1)
    }
}

/// Point stand-in: `x`/`y` attributes only.
#[pyclass(name = "Point")]
#[derive(Clone, Copy)]
pub(crate) struct PyCompatPoint {
    #[pyo3(get)]
    x: f64,
    #[pyo3(get)]
    y: f64,
}

#[pymethods]
impl PyCompatPoint {
    fn __repr__(&self) -> String {
        format!("Point({}, {})", self.x, self.y)
    }
}

/// Affine matrix stand-in: `a`/`b`/`c`/`d`/`e`/`f` attributes only.
#[pyclass(name = "Matrix")]
#[derive(Clone, Copy)]
pub(crate) struct PyCompatMatrix {
    #[pyo3(get)]
    pub(crate) a: f64,
    #[pyo3(get)]
    pub(crate) b: f64,
    #[pyo3(get)]
    pub(crate) c: f64,
    #[pyo3(get)]
    pub(crate) d: f64,
    #[pyo3(get)]
    pub(crate) e: f64,
    #[pyo3(get)]
    pub(crate) f: f64,
}

#[pymethods]
impl PyCompatMatrix {
    fn __repr__(&self) -> String {
        format!(
            "Matrix({}, {}, {}, {}, {}, {})",
            self.a, self.b, self.c, self.d, self.e, self.f
        )
    }
}

// ---------------------------------------------------------------------------
// Link
// ---------------------------------------------------------------------------

/// A document link: rectangle, kind, destination, and styling.
#[pyclass(name = "Link", module = "pdfluent")]
#[derive(Clone)]
pub(crate) struct PyLink {
    rect: PyCompatRect,
    kind: i32,
    uri: String,
    xref: i32,
    id: String,
    border_width: f64,
    border_style: Option<String>,
    border_dashes: Vec<i32>,
    border_clouds: i32,
    stroke: Vec<f64>,
    fill: Vec<f64>,
    flags: i32,
    dest_page: Option<i32>,
    dest_to: Option<(f64, f64)>,
    dest_zoom: f64,
    next: Option<Box<PyLink>>,
}

impl PyLink {
    /// A page link snapshot as a live `Link` object.
    pub(crate) fn from_compat(link: &pdfluent::compat::Link) -> Self {
        Self {
            rect: PyCompatRect {
                x0: link.rect.x0,
                y0: link.rect.y0,
                x1: link.rect.x1,
                y1: link.rect.y1,
            },
            kind: link.kind,
            uri: link.uri.clone(),
            xref: link.xref,
            id: link.id.clone(),
            border_width: link.border.width,
            border_style: link.border.style.clone(),
            border_dashes: link.border.dashes.clone(),
            border_clouds: link.border.clouds,
            stroke: link.colors.stroke.clone(),
            fill: link.colors.fill.clone(),
            flags: link.flags,
            dest_page: link.dest.as_ref().map(|d| d.page),
            dest_to: link.dest.as_ref().map(|d| (d.target.x, d.target.y)),
            dest_zoom: link.dest.as_ref().map(|d| d.zoom).unwrap_or(0.0),
            next: None,
        }
    }
}

#[pymethods]
impl PyLink {
    /// A link only comes from a page: direct construction raises.
    #[new]
    #[pyo3(signature = (this))]
    fn new(this: &Bound<'_, PyAny>) -> PyResult<Self> {
        if this.is_none() {
            return Err(validation_error(
                ValidationKind::Precondition,
                "link needs a handle",
            ));
        }
        if this.extract::<i64>().is_ok_and(|handle| handle == 0) {
            return Err(validation_error(
                ValidationKind::Precondition,
                "link needs a handle",
            ));
        }
        Ok(Self {
            rect: PyCompatRect {
                x0: 0.0,
                y0: 0.0,
                x1: 0.0,
                y1: 0.0,
            },
            kind: 0,
            uri: String::new(),
            xref: 0,
            id: String::new(),
            border_width: 0.0,
            border_style: None,
            border_dashes: Vec::new(),
            border_clouds: 0,
            stroke: Vec::new(),
            fill: Vec::new(),
            flags: 0,
            dest_page: None,
            dest_to: None,
            dest_zoom: 0.0,
            next: None,
        })
    }

    /// Link rectangle.
    #[getter]
    fn rect(&self) -> PyCompatRect {
        self.rect
    }

    /// Link kind code.
    #[getter]
    fn kind(&self) -> i32 {
        self.kind
    }

    /// Target URI for remote links.
    #[getter]
    fn uri(&self) -> String {
        if self.kind == 1 {
            if let Some(page) = self.dest_page {
                return format!("#page={}&zoom=nan,0,0", page + 1);
            }
        }
        self.uri.clone()
    }

    /// Zero-based destination page, -1 for remote links.
    #[getter]
    fn page(&self) -> i32 {
        self.dest_page.unwrap_or(-1)
    }

    /// Link object number.
    #[getter]
    fn xref(&self) -> i32 {
        self.xref
    }

    /// Link identifier.
    #[getter]
    fn id(&self) -> &str {
        &self.id
    }

    /// Link flags bitfield.
    #[getter]
    fn flags(&self) -> i32 {
        self.flags
    }

    /// Border styling.
    #[getter]
    fn border(&self, py: Python<'_>) -> PyResult<PyObject> {
        let dict = PyDict::new(py);
        dict.set_item("width", self.border_width)?;
        dict.set_item("style", self.border_style.clone())?;
        dict.set_item("dashes", self.border_dashes.clone())?;
        dict.set_item("clouds", self.border_clouds)?;
        Ok(dict.into_any().unbind())
    }

    /// Stroke and fill colours.
    #[getter]
    fn colors(&self, py: Python<'_>) -> PyResult<PyObject> {
        let dict = PyDict::new(py);
        dict.set_item("stroke", self.stroke.clone())?;
        dict.set_item("fill", self.fill.clone())?;
        Ok(dict.into_any().unbind())
    }

    /// Follow-on destination for local links.
    #[getter]
    fn dest(&self) -> Option<PyLinkDest> {
        if let Some(page) = self.dest_page {
            let to = self.dest_to.unwrap_or((0.0, 0.0));
            return Some(PyLinkDest::for_goto(page, to, self.dest_zoom));
        }
        if self.kind == 0 {
            return None;
        }
        Some(PyLinkDest::for_uri(self.kind, self.uri.clone()))
    }

    /// Whether the link leaves the document.
    #[getter]
    fn is_external(&self) -> bool {
        self.kind != 0 && self.kind != 1
    }

    /// Next link on the page.
    #[getter]
    fn next(&self) -> Option<PyLink> {
        self.next.as_deref().cloned()
    }

    /// Set the border styling.
    #[pyo3(signature = (border=None, width=0.0, dashes=None, style=None))]
    fn set_border(
        &mut self,
        border: Option<&Bound<'_, PyAny>>,
        width: f64,
        dashes: Option<Vec<i32>>,
        style: Option<String>,
    ) -> PyResult<()> {
        if let Some(border) = border {
            if let Ok(dict) = border.downcast::<PyDict>() {
                if let Ok(Some(w)) = dict.get_item("width") {
                    self.border_width = w.extract().unwrap_or(self.border_width);
                }
                if let Ok(Some(s)) = dict.get_item("style") {
                    self.border_style = s.extract().unwrap_or(None);
                }
                if let Ok(Some(d)) = dict.get_item("dashes") {
                    self.border_dashes = d.extract().unwrap_or_default();
                }
                if let Ok(Some(c)) = dict.get_item("clouds") {
                    self.border_clouds = c.extract().unwrap_or(0);
                }
                return Ok(());
            }
        }
        self.border_width = width;
        if let Some(dashes) = dashes {
            self.border_dashes = dashes;
        }
        self.border_style = Some(style.unwrap_or_else(|| "S".to_string()));
        Ok(())
    }

    /// Set stroke and fill colours.
    #[pyo3(signature = (colors=None, stroke=None, fill=None))]
    fn set_colors(
        &mut self,
        colors: Option<&Bound<'_, PyAny>>,
        stroke: Option<&Bound<'_, PyAny>>,
        fill: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        if colors.is_none() && stroke.is_none() && fill.is_none() {
            return Err(PyTypeError::new_err(
                "set_colors() needs colors, stroke, or fill",
            ));
        }
        if let Some(colors) = colors {
            if let Ok(dict) = colors.downcast::<PyDict>() {
                if let Ok(Some(s)) = dict.get_item("stroke") {
                    self.stroke = s.extract().unwrap_or_default();
                }
                if let Ok(Some(f)) = dict.get_item("fill") {
                    self.fill = f.extract().unwrap_or_default();
                }
                return Ok(());
            }
            return Err(validation_error(
                ValidationKind::DrawArg,
                "colors must be a dict",
            ));
        }
        if let Some(stroke) = stroke {
            self.stroke = color_components(stroke)?;
        }
        if let Some(fill) = fill {
            self.fill = color_components(fill)?;
        }
        Ok(())
    }

    /// Set the flags bitfield.
    fn set_flags(&mut self, flags: &Bound<'_, PyAny>) -> PyResult<()> {
        self.flags = flags
            .extract::<i32>()
            .map_err(|_| validation_error(ValidationKind::DrawArg, "flags must be an int"))?;
        Ok(())
    }

    fn __repr__(&self) -> String {
        format!("Link(kind={}, uri={:?})", self.kind, self.uri)
    }
}

impl PyLink {
    /// Wrap one facade link: unset border widths read as zero.
    pub(crate) fn from_facade(link: &FacadeLink) -> Self {
        Self {
            rect: PyCompatRect {
                x0: link.rect.x0,
                y0: link.rect.y0,
                x1: link.rect.x1,
                y1: link.rect.y1,
            },
            kind: link.kind,
            uri: link.uri.clone(),
            xref: link.xref,
            id: link.id.clone(),
            border_width: if link.border.width < 0.0 {
                0.0
            } else {
                link.border.width
            },
            border_style: link.border.style.clone(),
            border_dashes: link.border.dashes.clone(),
            border_clouds: link.border.clouds,
            stroke: link.colors.stroke.clone(),
            fill: link.colors.fill.clone(),
            flags: link.flags,
            dest_page: link.dest.as_ref().map(|dest| dest.page),
            dest_to: link
                .dest
                .as_ref()
                .map(|dest| (dest.target.x, dest.target.y)),
            dest_zoom: link.dest.as_ref().map(|dest| dest.zoom).unwrap_or(0.0),
            next: None,
        }
    }

    /// Chain the next link on the page.
    pub(crate) fn set_next(&mut self, next: Option<PyLink>) {
        self.next = next.map(Box::new);
    }
}

/// Colour components: a number sequence, nothing else.
fn color_components(value: &Bound<'_, PyAny>) -> PyResult<Vec<f64>> {
    value
        .extract::<Vec<f64>>()
        .map_err(|_| validation_error(ValidationKind::DrawArg, "color must be a number sequence"))
}

// ---------------------------------------------------------------------------
// Link destinations, outlines, display lists, native handles
// ---------------------------------------------------------------------------

/// Opaque handle to the native document behind `Document.this`.
#[pyclass(name = "FzDocument", module = "pdfluent")]
#[derive(Clone)]
pub(crate) struct PyFzDocument {
    token: usize,
}

#[pymethods]
impl PyFzDocument {
    fn __repr__(&self) -> String {
        format!("FzDocument(token={})", self.token)
    }
}

/// A link destination: kind, page, target, and viewer hints.
#[pyclass(name = "LinkDest", module = "pdfluent")]
#[derive(Clone)]
pub(crate) struct PyLinkDest {
    kind: i32,
    page: i32,
    uri: String,
    is_uri: bool,
    is_map: bool,
    flags: i32,
    new_window: String,
    file_spec: String,
    target: (f64, f64),
}

impl PyLinkDest {
    /// Destination for a remote link.
    pub(crate) fn for_uri(kind: i32, uri: String) -> Self {
        Self {
            kind,
            page: -1,
            uri,
            is_uri: kind == 2,
            is_map: false,
            flags: 0,
            new_window: String::new(),
            file_spec: String::new(),
            target: (0.0, 0.0),
        }
    }

    /// Destination for a local link.
    pub(crate) fn for_goto(page: i32, target: (f64, f64), zoom: f64) -> Self {
        Self {
            kind: 1,
            page,
            uri: format!("#page={}&zoom={},{},{}", page + 1, target.0, target.1, zoom),
            is_uri: false,
            is_map: false,
            flags: 3,
            new_window: String::new(),
            file_spec: String::new(),
            target,
        }
    }
}

#[pymethods]
impl PyLinkDest {
    /// Destination kind code.
    #[getter]
    fn kind(&self) -> i32 {
        self.kind
    }

    /// Zero-based destination page, -1 for remote links.
    #[getter]
    fn page(&self) -> i32 {
        self.page
    }

    /// Destination address.
    #[getter]
    fn uri(&self) -> &str {
        &self.uri
    }

    /// Whether the destination is a remote address.
    #[getter]
    fn is_uri(&self) -> bool {
        self.is_uri
    }

    /// Whether the destination is a map click.
    #[getter]
    fn is_map(&self) -> bool {
        self.is_map
    }

    /// Destination validity flags.
    #[getter]
    fn flags(&self) -> i32 {
        self.flags
    }

    /// Named destination entries.
    #[getter]
    fn named(&self, py: Python<'_>) -> PyResult<PyObject> {
        Ok(PyDict::new(py).into_any().unbind())
    }

    /// New-window hint.
    #[getter]
    fn new_window(&self) -> &str {
        &self.new_window
    }

    /// File specification for remote-go links.
    #[getter]
    fn file_spec(&self) -> &str {
        &self.file_spec
    }

    /// Target point, top-left view.
    #[getter]
    fn lt(&self) -> PyCompatPoint {
        PyCompatPoint {
            x: self.target.0,
            y: self.target.1,
        }
    }

    /// Target point, bottom-right view.
    #[getter]
    fn rb(&self) -> PyCompatPoint {
        PyCompatPoint {
            x: self.target.0,
            y: self.target.1,
        }
    }

    fn __repr__(&self) -> String {
        format!("LinkDest(kind={}, page={})", self.kind, self.page)
    }
}

/// One outline node: title, page, openness, destination, and neighbours.
#[pyclass(name = "Outline", module = "pdfluent")]
#[derive(Clone)]
pub(crate) struct PyOutline {
    title: Option<String>,
    page: i32,
    is_open: i32,
    dest: Option<PyLinkDest>,
    next: Option<Box<PyOutline>>,
    down: Option<Box<PyOutline>>,
}

impl PyOutline {
    /// An outline with no entries: only `title` raises.
    pub(crate) fn empty() -> Self {
        Self {
            title: None,
            page: -1,
            is_open: 0,
            dest: None,
            next: None,
            down: None,
        }
    }
}

#[pymethods]
impl PyOutline {
    /// Entry title.
    #[getter]
    fn title(&self) -> PyResult<&str> {
        self.title
            .as_deref()
            .ok_or_else(|| validation_error(ValidationKind::NoAttr, "outline has no entries"))
    }

    /// Zero-based destination page.
    #[getter]
    fn page(&self) -> i32 {
        self.page
    }

    /// Whether the entry starts open.
    #[getter]
    fn is_open(&self) -> i32 {
        self.is_open
    }

    /// Entry destination, if any.
    #[getter]
    fn dest(&self) -> Option<PyLinkDest> {
        self.dest.clone()
    }

    /// Next entry on this level, if any.
    #[getter]
    fn next(&self) -> Option<PyOutline> {
        self.next.as_deref().cloned()
    }

    /// First entry below this one, if any.
    #[getter]
    fn down(&self) -> Option<PyOutline> {
        self.down.as_deref().cloned()
    }

    fn __repr__(&self) -> String {
        format!("Outline(title={:?})", self.title)
    }
}

/// Build one outline node from a bookmark.
fn outline_node(bookmark: &pdf_manip::bookmarks::OutlineEntry) -> PyOutline {
    use pdf_manip::bookmarks::BookmarkAction as Action;
    let (page, dest) = match &bookmark.action {
        Some(Action::GoTo { page, .. }) => {
            let page = *page as i32 - 1;
            (page, Some(PyLinkDest::for_goto(page, (0.0, 0.0), 0.0)))
        }
        Some(Action::Uri(uri)) => (-1, Some(PyLinkDest::for_uri(2, uri.clone()))),
        Some(Action::GoToR { page, file }) => {
            let mut dest = PyLinkDest::for_goto(*page as i32 - 1, (0.0, 0.0), 0.0);
            dest.kind = 5;
            dest.file_spec.clone_from(file);
            (*page as i32 - 1, Some(dest))
        }
        Some(Action::Named(name)) => (-1, Some(PyLinkDest::for_uri(4, name.clone()))),
        None => (-1, None),
    };
    let mut down: Option<Box<PyOutline>> = None;
    for child in bookmark.children.iter().rev() {
        let mut node = outline_node(child);
        node.next = down.take();
        down = Some(Box::new(node));
    }
    PyOutline {
        title: Some(bookmark.title.clone()),
        page,
        is_open: i32::from(bookmark.open),
        dest,
        next: None,
        down,
    }
}

/// Build the outline head from top-level bookmarks.
fn outline_head(bookmarks: &[pdf_manip::bookmarks::OutlineEntry]) -> PyOutline {
    let mut head: Option<Box<PyOutline>> = None;
    for bookmark in bookmarks.iter().rev() {
        let mut node = outline_node(bookmark);
        node.next = head.take();
        head = Some(Box::new(node));
    }
    head.map(|node| *node).unwrap_or_else(PyOutline::empty)
}

/// A retained rendering of a page, replayed through `run`.
#[pyclass(name = "DisplayList", module = "pdfluent")]
#[derive(Clone)]
pub(crate) struct PyDisplayList {
    rect: PyCompatRect,
}

#[pymethods]
impl PyDisplayList {
    /// Page area the rendering covers.
    #[getter]
    fn rect(&self) -> PyCompatRect {
        self.rect
    }

    fn __repr__(&self) -> String {
        format!(
            "DisplayList(rect=[{}, {}, {}, {}])",
            self.rect.x0, self.rect.y0, self.rect.x1, self.rect.y1
        )
    }

    /// Replay a retained rendering: unimplemented, fails loudly.
    fn run(&self, dw: &Bound<'_, PyAny>, m: &Bound<'_, PyAny>) -> PyResult<()> {
        let _ = (dw, m);
        Err(PdfluentError::new_err(
            "not supported in this version: DisplayList.run; see KNOWN_DIFFERENCES",
        ))
    }
}

// ---------------------------------------------------------------------------
// Structure readers over lopdf
// ---------------------------------------------------------------------------

/// A decrypted structural snapshot: the mutation handle when one exists,
/// else the raw bytes parsed (and unlocked with the stored password).
fn snapshot(doc: &PyDocument) -> PyResult<LopdfDocument> {
    if let Some(handle) = doc.lopdf.lock().unwrap().as_ref() {
        return Ok(handle.clone());
    }
    let mut parsed = LopdfDocument::load_mem(doc.raw_bytes.as_ref())
        .map_err(|e| file_data_error(format!("cannot parse PDF: {e}")))?;
    if parsed.is_encrypted() {
        let password = doc.password.lock().unwrap().clone().unwrap_or_default();
        parsed
            .decrypt(&password)
            .map_err(|_| file_data_error("document needs a password"))?;
    }
    Ok(parsed)
}

/// Page object ids in page order.
fn page_ids(parsed: &LopdfDocument) -> Vec<lopdf::ObjectId> {
    let pages = parsed.get_pages();
    let mut numbers: Vec<u32> = pages.keys().copied().collect();
    numbers.sort_unstable();
    numbers
        .into_iter()
        .filter_map(|n| pages.get(&n).copied())
        .collect()
}

/// Entries of one name tree: names with their destination values.
fn name_tree_entries(parsed: &LopdfDocument, tree: &LObject) -> Vec<(String, LObject)> {
    let mut out = Vec::new();
    name_tree_walk(parsed, tree, &mut out, 0);
    out
}

/// Walk one name tree node, collecting `/Names` pairs and descending `/Kids`.
fn name_tree_walk(
    parsed: &LopdfDocument,
    node: &LObject,
    out: &mut Vec<(String, LObject)>,
    depth: usize,
) {
    if depth > 32 {
        return;
    }
    let node = deref(parsed, node);
    let Some(LObject::Dictionary(dict)) = node else {
        return;
    };
    if let Ok(names) = dict.get(b"Names").and_then(|o| o.as_array()) {
        for pair in names.chunks(2) {
            if pair.len() < 2 {
                break;
            }
            let name = match &pair[0] {
                LObject::String(bytes, _) => String::from_utf8_lossy(bytes).into_owned(),
                _ => continue,
            };
            out.push((name, pair[1].clone()));
        }
    }
    if let Ok(kids) = dict.get(b"Kids").and_then(|o| o.as_array()) {
        for kid in kids {
            name_tree_walk(parsed, kid, out, depth + 1);
        }
    }
}

/// Zero-based page behind a destination value, if it names one.
fn dest_page(parsed: &LopdfDocument, ids: &[lopdf::ObjectId], dest: &LObject) -> Option<usize> {
    let dest = deref(parsed, dest)?;
    match dest {
        LObject::Reference(id) => ids.iter().position(|known| *known == id),
        LObject::Array(items) => {
            let first = items.first()?;
            match first {
                LObject::Reference(id) => ids.iter().position(|known| known == id),
                LObject::Integer(n) => usize::try_from(*n).ok().filter(|n| *n < ids.len()),
                _ => None,
            }
        }
        LObject::Dictionary(dict) => {
            let page = dict.get(b"D").ok().cloned()?;
            dest_page(parsed, ids, &page)
        }
        _ => None,
    }
}

/// Resolve a layer-config index: 0-based, negatives from the end.
fn resolve_layer(index: i32, len: usize) -> PyResult<usize> {
    let target = if index < 0 { len as i32 + index } else { index };
    if target < 0 || target as usize >= len {
        return Err(validation_error(
            ValidationKind::BadXref,
            format!("config index {index} out of range ({len} entries)"),
        ));
    }
    Ok(target as usize)
}

/// Resolve a possibly negative page number: out-of-range is a value error.
fn resolve_page(page: i32, count: usize) -> PyResult<usize> {
    let target = if page < 0 { count as i32 + page } else { page };
    if target < 0 || target as usize >= count {
        return Err(validation_error(
            ValidationKind::BadXref,
            format!("page number {page} out of range ({count} pages)"),
        ));
    }
    Ok(target as usize)
}

/// Resolve a possibly negative index; `what` names it in the error.
fn resolve_index(index: i32, count: usize, what: &str) -> PyResult<usize> {
    let count_i64 = count as i64;
    let resolved = if index < 0 {
        count_i64 + i64::from(index)
    } else {
        i64::from(index)
    };
    if resolved < 0 || resolved >= count_i64 {
        return Err(PyIndexError::new_err(format!(
            "{what} index {index} out of range ({count} entries)"
        )));
    }
    Ok(resolved as usize)
}

/// A PDF number as `f64`.
fn number_as_f64(obj: &LObject) -> Option<f64> {
    match obj {
        LObject::Integer(i) => Some(*i as f64),
        LObject::Real(r) => Some(f64::from(*r)),
        _ => None,
    }
}

/// Read a rectangle array as y-up `[x0, y0, x1, y1]`.
fn array_as_rect(obj: &LObject) -> Option<[f64; 4]> {
    let items = obj.as_array().ok()?;
    if items.len() != 4 {
        return None;
    }
    Some([
        number_as_f64(&items[0])?,
        number_as_f64(&items[1])?,
        number_as_f64(&items[2])?,
        number_as_f64(&items[3])?,
    ])
}

/// Inherited page-tree lookup: walk from the page dict up through `/Parent`.
fn inherited(parsed: &LopdfDocument, mut id: lopdf::ObjectId, key: &[u8]) -> Option<LObject> {
    for _ in 0..64 {
        let dict = parsed.get_dictionary(id).ok()?;
        if let Ok(value) = dict.get(key) {
            return Some(value.clone());
        }
        id = dict.get(b"Parent").ok()?.as_reference().ok()?;
    }
    None
}

/// Resolved y-up boxes plus the inherited rotation of a page.
struct PageFrames {
    media: [f64; 4],
    crop: [f64; 4],
    rotation: i32,
}

/// Read the media/crop boxes of a page with inheritance and fallbacks.
fn page_frames(parsed: &LopdfDocument, index: usize) -> PyResult<PageFrames> {
    let ids = page_ids(parsed);
    let id = *ids
        .get(index)
        .ok_or_else(|| PyIndexError::new_err(format!("page index {index} out of range")))?;
    let media = inherited(parsed, id, b"MediaBox")
        .as_ref()
        .and_then(array_as_rect)
        .unwrap_or([0.0, 0.0, 595.0, 842.0]);
    let crop = inherited(parsed, id, b"CropBox")
        .as_ref()
        .and_then(array_as_rect)
        .unwrap_or(media);
    let rotation = inherited(parsed, id, b"Rotate")
        .as_ref()
        .and_then(number_as_f64)
        .unwrap_or(0.0) as i32;
    Ok(PageFrames {
        media,
        crop,
        rotation: rotation.rem_euclid(360),
    })
}

/// Flip a y-up box into a y-down stand-in rectangle.
fn flip_rect(box_y_up: [f64; 4], page_top: f64) -> PyCompatRect {
    PyCompatRect {
        x0: box_y_up[0].min(box_y_up[2]),
        y0: page_top - box_y_up[3].max(box_y_up[1]),
        x1: box_y_up[0].max(box_y_up[2]),
        y1: page_top - box_y_up[1].min(box_y_up[3]),
    }
}

/// Read one named page box, defaulting art/bleed/trim to the crop box.
///
/// Every box lives in page coordinates, so the y-down flip mirrors against
/// the media top — including the crop box itself, which reads back what
/// `set_cropbox` wrote only then.
fn named_box(parsed: &LopdfDocument, index: usize, key: &[u8]) -> PyResult<PyCompatRect> {
    let frames = page_frames(parsed, index)?;
    let top = frames.media[3].max(frames.media[1]);
    if key == b"CropBox" {
        return Ok(flip_rect(frames.crop, top));
    }
    if key == b"MediaBox" {
        return Ok(flip_rect(frames.media, top));
    }
    let ids = page_ids(parsed);
    let found = inherited(parsed, ids[index], key)
        .as_ref()
        .and_then(array_as_rect);
    Ok(flip_rect(found.unwrap_or(frames.crop), top))
}

/// Catalog id, when the trailer names one.
fn catalog_id(parsed: &LopdfDocument) -> Option<lopdf::ObjectId> {
    parsed.trailer.get(b"Root").ok()?.as_reference().ok()
}

/// Catalog name or string entry as text.
fn catalog_text(parsed: &LopdfDocument, key: &[u8]) -> String {
    let Some(id) = catalog_id(parsed) else {
        return String::new();
    };
    let Ok(dict) = parsed.get_dictionary(id) else {
        return String::new();
    };
    match dict.get(key) {
        Ok(LObject::Name(name)) => String::from_utf8_lossy(name).into_owned(),
        Ok(LObject::String(bytes, _)) => String::from_utf8_lossy(bytes).into_owned(),
        _ => String::new(),
    }
}

/// Resolve an object that may sit behind a reference.
fn deref(parsed: &LopdfDocument, obj: &LObject) -> Option<LObject> {
    match obj {
        LObject::Reference(id) => parsed.objects.get(id).cloned(),
        other => Some(other.clone()),
    }
}

/// The `/AcroForm` dictionary, resolved.
fn acroform_dict(parsed: &LopdfDocument) -> Option<lopdf::Dictionary> {
    let id = catalog_id(parsed)?;
    let form = parsed
        .get_dictionary(id)
        .ok()?
        .get(b"AcroForm")
        .ok()?
        .clone();
    match deref(parsed, &form)? {
        LObject::Dictionary(dict) => Some(dict),
        LObject::Stream(stream) => Some(stream.dict),
        _ => None,
    }
}

/// Serialise one object for the catalogue/trailer/xref readers.
fn write_object(out: &mut String, obj: &LObject, ascii: bool) {
    match obj {
        LObject::Null => out.push_str("null"),
        LObject::Boolean(b) => out.push_str(if *b { "true" } else { "false" }),
        LObject::Integer(i) => out.push_str(&i.to_string()),
        LObject::Real(r) => out.push_str(&format!("{r}")),
        LObject::Name(name) => {
            out.push('/');
            out.push_str(&String::from_utf8_lossy(name));
        }
        LObject::String(bytes, _) => {
            out.push('(');
            for byte in bytes {
                if ascii && !byte.is_ascii() {
                    out.push_str(&format!("\\{byte:03o}"));
                } else {
                    match byte {
                        b'(' | b')' | b'\\' => {
                            out.push('\\');
                            out.push(*byte as char);
                        }
                        _ if byte.is_ascii_graphic() || *byte == b' ' => {
                            out.push(*byte as char);
                        }
                        _ => out.push_str(&format!("\\{byte:03o}")),
                    }
                }
            }
            out.push(')');
        }
        LObject::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                write_object(out, item, ascii);
            }
            out.push(']');
        }
        LObject::Dictionary(dict) => {
            out.push_str("<<");
            for (key, value) in dict.iter() {
                out.push(' ');
                out.push('/');
                out.push_str(&String::from_utf8_lossy(key));
                out.push(' ');
                write_object(out, value, ascii);
            }
            out.push_str(" >>");
        }
        LObject::Stream(stream) => {
            write_object(out, &LObject::Dictionary(stream.dict.clone()), ascii);
            out.push_str(" stream ... endstream");
        }
        LObject::Reference(id) => out.push_str(&format!("{} {} R", id.0, id.1)),
    }
}

/// One object's source text, packed: no padding around entries.
fn packed_source(obj: &LObject) -> String {
    match obj {
        LObject::Dictionary(dict) => {
            let mut out = String::from("<<");
            for (key, value) in dict.iter() {
                out.push('/');
                out.push_str(&String::from_utf8_lossy(key));
                let rendered = packed_source(value);
                if !rendered.starts_with(['/', '[', '<', '(']) {
                    out.push(' ');
                }
                out.push_str(&rendered);
            }
            out.push_str(">>");
            out
        }
        LObject::Array(items) => {
            let mut out = String::from("[");
            let mut first = true;
            for item in items {
                if !first {
                    out.push(' ');
                }
                first = false;
                out.push_str(&packed_source(item));
            }
            out.push(']');
            out
        }
        other => {
            let mut out = String::new();
            write_object(&mut out, other, false);
            out
        }
    }
}

/// Serialise one object to text.
fn object_source(obj: &LObject, ascii: bool) -> String {
    let mut out = String::new();
    write_object(&mut out, obj, ascii);
    out
}

/// Short type name of an object.
fn object_type_name(obj: &LObject) -> &'static str {
    match obj {
        LObject::Null => "null",
        LObject::Boolean(_) => "bool",
        LObject::Integer(_) => "int",
        LObject::Real(_) => "real",
        LObject::Name(_) => "name",
        LObject::String(_, _) => "str",
        LObject::Array(_) => "array",
        LObject::Dictionary(_) => "dict",
        LObject::Stream(_) => "stream",
        LObject::Reference(_) => "ref",
    }
}

/// Look up a live object by spec xref.
fn object_by_xref(parsed: &LopdfDocument, xref: i32) -> PyResult<LObject> {
    if xref <= 0 {
        return Err(PyValueError::new_err(format!("bad xref {xref}")));
    }
    parsed
        .objects
        .iter()
        .find(|(id, _)| id.0 == xref as u32)
        .map(|(_, obj)| obj.clone())
        .ok_or_else(|| PyValueError::new_err(format!("unknown xref {xref}")))
}

/// Permission bits: `/Encrypt /P`, else all-granted.
fn permission_bits(parsed: &LopdfDocument) -> i32 {
    parsed
        .trailer
        .get(b"Encrypt")
        .ok()
        .cloned()
        .and_then(|obj| deref(parsed, &obj))
        .and_then(|obj| match obj {
            LObject::Dictionary(dict) => dict.get(b"P").ok()?.as_i64().ok(),
            _ => None,
        })
        .map(|p| p as i32)
        .unwrap_or(-4)
}

/// External-object numbers named by a page's resources.
fn page_xobject_ids(parsed: &LopdfDocument, index: usize) -> PyResult<Vec<i32>> {
    let ids = page_ids(parsed);
    let id = *ids
        .get(index)
        .ok_or_else(|| PyIndexError::new_err(format!("page index {index} out of range")))?;
    let dict = inherited(parsed, id, b"Resources").and_then(|obj| match deref(parsed, &obj)? {
        LObject::Dictionary(dict) => Some(dict),
        LObject::Stream(stream) => Some(stream.dict),
        _ => None,
    });
    let mut out = Vec::new();
    if let Some(dict) = dict {
        if let Some(xobjects) =
            dict.get(b"XObject")
                .ok()
                .cloned()
                .and_then(|obj| match deref(parsed, &obj)? {
                    LObject::Dictionary(dict) => Some(dict),
                    _ => None,
                })
        {
            for (_, value) in xobjects.iter() {
                if let Ok(ref_id) = value.as_reference() {
                    out.push(ref_id.0 as i32);
                }
            }
        }
    }
    out.sort_unstable();
    Ok(out)
}

/// Content stream object numbers of a page, in order.
fn content_ids(parsed: &LopdfDocument, index: usize) -> PyResult<Vec<lopdf::ObjectId>> {
    let ids = page_ids(parsed);
    let id = *ids
        .get(index)
        .ok_or_else(|| PyIndexError::new_err(format!("page index {index} out of range")))?;
    let mut out = Vec::new();
    match inherited(parsed, id, b"Contents") {
        Some(LObject::Reference(ref_id)) => out.push(ref_id),
        Some(LObject::Array(items)) => {
            for item in &items {
                if let Ok(ref_id) = item.as_reference() {
                    out.push(ref_id);
                }
            }
        }
        _ => {}
    }
    Ok(out)
}

/// Walk a name tree, collecting leaf `(name, value)` pairs.
fn walk_name_tree(
    parsed: &LopdfDocument,
    node: &lopdf::Dictionary,
    out: &mut Vec<(String, LObject)>,
) {
    if let Ok(LObject::Array(names)) = node.get(b"Names") {
        for pair in names.chunks(2) {
            if pair.len() != 2 {
                continue;
            }
            let name = match &pair[0] {
                LObject::String(bytes, _) => String::from_utf8_lossy(bytes).into_owned(),
                _ => continue,
            };
            out.push((name, pair[1].clone()));
        }
    }
    if let Ok(LObject::Array(kids)) = node.get(b"Kids") {
        for kid in kids {
            let dict = match kid {
                LObject::Reference(id) => parsed.get_dictionary(*id).ok().cloned(),
                LObject::Dictionary(dict) => Some(dict.clone()),
                _ => None,
            };
            if let Some(dict) = dict {
                walk_name_tree(parsed, &dict, out);
            }
        }
    }
}

/// Embedded files: `(name, file-spec dict id)` pairs from `/EmbeddedFiles`.
fn embedded_files(parsed: &LopdfDocument) -> Vec<(String, lopdf::ObjectId)> {
    let Some(root) = catalog_id(parsed) else {
        return Vec::new();
    };
    let names = parsed
        .get_dictionary(root)
        .ok()
        .and_then(|catalog| catalog.get(b"Names").ok().cloned())
        .and_then(|obj| deref(parsed, &obj));
    let Some(LObject::Dictionary(names)) = names else {
        return Vec::new();
    };
    let tree = names
        .get(b"EmbeddedFiles")
        .ok()
        .cloned()
        .and_then(|obj| deref(parsed, &obj));
    let Some(LObject::Dictionary(tree)) = tree else {
        return Vec::new();
    };
    let mut pairs = Vec::new();
    walk_name_tree(parsed, &tree, &mut pairs);
    pairs
        .into_iter()
        .filter_map(|(name, value)| match value {
            LObject::Reference(id) => Some((name, id)),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Save / write rendering
// ---------------------------------------------------------------------------

/// Encryption flavours accepted by `save`/`tobytes`.
fn encrypt_algorithm(selector: i32) -> PyResult<Option<pdf_manip::encrypt::EncryptionAlgorithm>> {
    use pdf_manip::encrypt::EncryptionAlgorithm as Algo;
    match selector {
        1 | 0 => Ok(None),
        5 => Ok(Some(Algo::Aes256)),
        4 => Ok(Some(Algo::Aes128)),
        3 => Ok(Some(Algo::Rc4_128)),
        2 => Ok(Some(Algo::Rc4_40)),
        other => Err(PyValueError::new_err(format!("bad encryption {other}"))),
    }
}

/// The bytes as they stand: the mutation handle when one exists, else raw.
fn current_bytes(doc: &PyDocument) -> PyResult<Vec<u8>> {
    let mut guard = doc.lopdf.lock().unwrap();
    match *guard {
        Some(ref mut handle) => {
            let mut buf = Vec::new();
            handle
                .save_to(&mut buf)
                .map_err(|e| PdfluentIoError::new_err(format!("serialise failed: {e}")))?;
            Ok(buf)
        }
        None => Ok(doc.raw_bytes.as_ref().clone()),
    }
}

/// Render the write bytes for `save`/`tobytes`/`write`, honouring the
/// encryption selector on a scratch copy.
fn rendered_bytes(
    doc: &PyDocument,
    encryption: i32,
    permissions: i32,
    owner_pw: Option<String>,
    user_pw: Option<String>,
) -> PyResult<Vec<u8>> {
    let Some(algorithm) = encrypt_algorithm(encryption)? else {
        return current_bytes(doc);
    };
    // Encrypt from plaintext: the raw bytes when clean, else the handle.
    let mut scratch = snapshot(doc)?;
    if scratch.is_encrypted() {
        let password = doc.password.lock().unwrap().clone().unwrap_or_default();
        pdf_manip::encrypt::decrypt(&mut scratch, &password)
            .map_err(|_| PyValueError::new_err("document needs a password"))?;
    }
    let user_password = user_pw.unwrap_or_default().into_bytes();
    let owner_password = owner_pw
        .map(String::into_bytes)
        .unwrap_or_else(|| user_password.clone());
    let config = pdf_manip::encrypt::EncryptConfig {
        user_password,
        owner_password,
        algorithm,
        permissions: pdf_manip::encrypt::Permissions::from_p_value(permissions),
    };
    let mut buf = Vec::new();
    pdf_manip::encrypt::encrypt_and_save(&mut scratch, &config, &mut buf)
        .map_err(|e| PdfluentIoError::new_err(format!("encrypt failed: {e}")))?;
    Ok(buf)
}

/// Shared `save` body behind the `Document.save` wrapper in `lib.rs`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn compat_save(
    doc: &PyDocument,
    filename: &str,
    incremental: bool,
    linear: bool,
    encryption: i32,
    permissions: i32,
    owner_pw: Option<String>,
    user_pw: Option<String>,
) -> PyResult<()> {
    doc.ensure_open()?;
    if linear {
        return Err(PyValueError::new_err("linearization is not supported"));
    }
    if incremental {
        if filename != doc.name {
            return Err(PyValueError::new_err(
                "incremental save needs the original path",
            ));
        }
        if doc.lopdf.lock().unwrap().is_none() {
            return Err(FzErrorArgument::new_err("nothing to append"));
        }
    }
    let bytes = rendered_bytes(doc, encryption, permissions, owner_pw, user_pw)?;
    std::fs::write(filename, bytes)
        .map_err(|e| validation_error(ValidationKind::SaveFailed, e.to_string()))?;
    doc.dirty.store(false, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

/// Rebuild the engine document from the given bytes (post-authentication and
/// structural edits share this).
fn reparse_engine(doc: &PyDocument, bytes: &[u8]) -> PyResult<Arc<EngineDocument>> {
    let password = doc.password.lock().unwrap().clone();
    let opened = match password.as_deref() {
        Some(pw) => EngineDocument::open_with_password(bytes.to_vec(), pw),
        None => EngineDocument::open(bytes.to_vec()),
    };
    let parsed = opened.map_err(engine_err_to_py)?;
    let shared = Arc::new(parsed);
    *doc.inner.lock().unwrap() = Some(Arc::clone(&shared));
    Ok(shared)
}

/// Parse the raw bytes without unlocking: for the lock-state readers that
/// must work before authentication.
fn raw_snapshot(doc: &PyDocument) -> PyResult<LopdfDocument> {
    LopdfDocument::load_mem(doc.raw_bytes.as_ref())
        .map_err(|e| file_data_error(format!("cannot parse PDF: {e}")))
}

/// Read one outline level/title/page row list from bookmarks.
fn simple_toc_rows(
    py: Python<'_>,
    items: &[pdf_manip::bookmarks::OutlineEntry],
    level: i32,
    out: &mut Vec<PyObject>,
) -> PyResult<()> {
    use pdf_manip::bookmarks::BookmarkAction;
    for item in items {
        let page = match &item.action {
            Some(BookmarkAction::GoTo { page, .. } | BookmarkAction::GoToR { page, .. }) => {
                *page as i32
            }
            Some(BookmarkAction::Uri(_) | BookmarkAction::Named(_)) => 0,
            None => -1,
        };
        let row = vec![
            level.into_pyobject(py)?.into_any().unbind(),
            item.title.clone().into_pyobject(py)?.into_any().unbind(),
            page.into_pyobject(py)?.into_any().unbind(),
        ];
        out.push(row.into_pyobject(py)?.into_any().unbind());
        simple_toc_rows(py, &item.children, level + 1, out)?;
    }
    Ok(())
}

/// Read one full outline row (with destination dict) from bookmarks.
fn full_toc_rows(
    py: Python<'_>,
    parsed: &LopdfDocument,
    items: &[pdf_manip::bookmarks::OutlineEntry],
    level: i32,
    out: &mut Vec<PyObject>,
) -> PyResult<()> {
    use pdf_manip::bookmarks::{BookmarkAction, FitType};
    for item in items {
        let (kind, page_0, to, zoom) = match &item.action {
            Some(BookmarkAction::GoTo { page, fit }) => {
                let page_0 = (*page as i32).saturating_sub(1);
                let (to, zoom) = match fit {
                    FitType::Xyz { x, y, zoom } => {
                        let height = page_frames(parsed, page_0.max(0) as usize)
                            .map(|f| f.crop[3].max(f.crop[1]) - f.crop[1].min(f.crop[3]))
                            .unwrap_or(0.0);
                        (
                            (x.unwrap_or(0.0) as f64, height - y.unwrap_or(0.0) as f64),
                            zoom.unwrap_or(0.0) as f64,
                        )
                    }
                    FitType::FitH(top) => ((0.0, f64::from(*top)), 0.0),
                    FitType::FitV(left) => ((f64::from(*left), 0.0), 0.0),
                    FitType::Fit => ((0.0, 0.0), 0.0),
                };
                (1, page_0, to, zoom)
            }
            Some(BookmarkAction::GoToR { page, .. }) => {
                (5, (*page as i32).saturating_sub(1), (0.0, 0.0), 0.0)
            }
            Some(BookmarkAction::Uri(_)) => (2, -1, (0.0, 0.0), 0.0),
            Some(BookmarkAction::Named(_)) => (4, -1, (0.0, 0.0), 0.0),
            None => (0, -1, (0.0, 0.0), 0.0),
        };
        let dest = PyDict::new(py);
        dest.set_item("kind", kind)?;
        dest.set_item("page", page_0)?;
        dest.set_item("to", (to.0, to.1))?;
        dest.set_item("zoom", zoom)?;
        if !item.children.is_empty() {
            dest.set_item("collapse", !item.open)?;
        }
        // The full row mirrors the simple row's page: -1 without a target.
        let sheet = if item.action.is_none() {
            -1
        } else if page_0 >= 0 {
            page_0 + 1
        } else {
            0
        };
        let row = vec![
            level.into_pyobject(py)?.into_any().unbind(),
            item.title.clone().into_pyobject(py)?.into_any().unbind(),
            sheet.into_pyobject(py)?.into_any().unbind(),
            dest.into_any().unbind(),
        ];
        out.push(row.into_pyobject(py)?.into_any().unbind());
        full_toc_rows(py, parsed, &item.children, level + 1, out)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Document methods
// ---------------------------------------------------------------------------

#[pymethods]
impl PyDocument {
    /// Close the document; reads past this fail, as does closing twice.
    fn close(&self) -> PyResult<()> {
        if self.closed.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return Err(PyValueError::new_err("document is closed"));
        }
        Ok(())
    }

    /// Whether the document is closed.
    #[getter]
    fn is_closed(&self) -> bool {
        self.closed.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Whether unsaved changes exist.
    #[getter]
    fn is_dirty(&self) -> PyResult<bool> {
        self.ensure_open()?;
        Ok(self.dirty.load(std::sync::atomic::Ordering::SeqCst))
    }

    /// Source path the document was opened from; empty for streams.
    #[getter]
    fn name(&self) -> PyResult<String> {
        self.ensure_open()?;
        Ok(self.name.clone())
    }

    /// Whether the document is a PDF.
    #[getter]
    fn is_pdf(&self) -> PyResult<bool> {
        self.ensure_open()?;
        Ok(true)
    }

    /// Whether the document holds a form: field count, else false.
    #[getter]
    fn is_form_pdf(&self, py: Python<'_>) -> PyResult<PyObject> {
        self.ensure_open()?;
        let parsed = snapshot(self)?;
        let fields = acroform_dict(&parsed)
            .as_ref()
            .and_then(|form| form.get(b"Fields").ok())
            .and_then(|o| o.as_array().ok())
            .map(|fields| fields.len())
            .unwrap_or(0);
        if fields == 0 {
            Ok(PyBool::new(py, false).to_owned().into_any().unbind())
        } else {
            Ok((fields as i32).into_pyobject(py)?.into_any().unbind())
        }
    }

    /// Whether the handle is a PDF.
    #[getter]
    fn this_is_pdf(&self) -> PyResult<bool> {
        self.ensure_open()?;
        Ok(false)
    }

    /// Opaque handle to the native document.
    #[getter]
    fn this(&self) -> PyFzDocument {
        PyFzDocument {
            token: self as *const Self as usize,
        }
    }

    /// Whether the bytes carry an encryption dictionary.
    #[getter]
    fn is_encrypted(&self) -> PyResult<bool> {
        self.ensure_open()?;
        Ok(raw_snapshot(self)?.is_encrypted())
    }

    /// Password state: 1 when a password is still needed, else 0.
    #[getter]
    fn needs_pass(&self) -> PyResult<i32> {
        self.ensure_open()?;
        let parsed = raw_snapshot(self)?;
        if !parsed.is_encrypted() {
            return Ok(0);
        }
        Ok(i32::from(parsed.authenticate_user_password("").is_err()))
    }

    /// Unlock with a password: 2 for user, 4 for owner, 0 for neither.
    fn authenticate(&self, password: &str) -> PyResult<i32> {
        self.ensure_open()?;
        let parsed = raw_snapshot(self)?;
        // No early return for unencrypted bytes: a file whose empty user
        // password opened it on load still verifies later passwords against
        // the retained encryption state (and a never-encrypted file fails
        // both checks, so the level below is 0 either way).
        let user = i32::from(parsed.authenticate_user_password(password).is_ok()) * 2;
        let owner = i32::from(parsed.authenticate_owner_password(password).is_ok()) * 4;
        let level = user | owner;
        if level != 0 {
            *self.password.lock().unwrap() = Some(password.to_string());
            if parsed.is_encrypted() {
                // The engine unlocks the encrypted bytes itself; a decrypted
                // re-serialisation would shed the encryption the readers below
                // still expect on `raw_bytes`.
                let bytes = self.raw_bytes.as_ref().clone();
                reparse_engine(self, &bytes)?;
            }
        }
        Ok(level)
    }

    /// Whether incremental saving applies.
    fn can_save_incrementally(&self) -> PyResult<i32> {
        self.ensure_open()?;
        Ok(i32::from(!raw_snapshot(self)?.is_encrypted()))
    }

    /// Permission bits.
    #[getter]
    fn permissions(&self) -> PyResult<i32> {
        self.ensure_open()?;
        Ok(permission_bits(&raw_snapshot(self)?))
    }

    /// Outline entries; `simple` drops the destination dicts.
    #[pyo3(signature = (simple=true))]
    fn get_toc(&self, py: Python<'_>, simple: bool) -> PyResult<Vec<PyObject>> {
        self.ensure_open()?;
        let parsed = snapshot(self)?;
        let bookmarks = pdf_manip::bookmarks::read_outline(&parsed).unwrap_or_default();
        let mut out = Vec::new();
        if simple {
            simple_toc_rows(py, &bookmarks, 1, &mut out)?;
        } else {
            full_toc_rows(py, &parsed, &bookmarks, 1, &mut out)?;
        }
        Ok(out)
    }

    /// Document outline: the head entry, empty when there is none.
    #[getter]
    fn outline(&self) -> PyResult<PyOutline> {
        self.ensure_open()?;
        let parsed = snapshot(self)?;
        let bookmarks = pdf_manip::bookmarks::read_outline(&parsed).unwrap_or_default();
        Ok(outline_head(&bookmarks))
    }

    /// Page by number (negative counts from the end).
    fn load_page(slf: Py<Self>, py: Python<'_>, page_id: i32) -> PyResult<PyPage> {
        let borrowed = slf.bind(py).borrow();
        borrowed.ensure_open()?;
        let engine = borrowed.engine()?;
        let count = engine.page_count();
        // The recorded surface raises ValueError here (unlike `doc[i]`,
        // which raises IndexError), so the shared index error is translated.
        let target = resolve_index(page_id, count, "page").map_err(|_| {
            PyValueError::new_err(format!("page index {page_id} out of range ({count} pages)"))
        })?;
        Ok(PyPage {
            doc: engine,
            index: target,
            refreshed: AtomicBool::new(false),
            clip: Mutex::new(None),
            parent: slf.clone_ref(py),
            lopdf: Arc::clone(&borrowed.lopdf),
            snapshot: Arc::clone(&borrowed.raw_bytes),
        })
    }

    /// Fresh view of a page.
    fn reload_page(slf: Py<Self>, py: Python<'_>, page: &Bound<'_, PyAny>) -> PyResult<PyPage> {
        let borrowed = slf.bind(py).borrow();
        borrowed.ensure_open()?;
        let engine = borrowed.engine()?;
        let index: usize = if let Ok(view) = page.extract::<PyRef<'_, PyPage>>() {
            view.compat_index()
        } else if let Ok(number) = page.extract::<i32>() {
            resolve_page(number, engine.page_count())?
        } else {
            let _ = page.getattr("number")?;
            return Err(validation_error(
                ValidationKind::BadXref,
                "reload_page needs a page or number",
            ));
        };
        if index >= engine.page_count() {
            return Err(PyIndexError::new_err("page index out of range"));
        }
        Ok(PyPage {
            doc: engine,
            index,
            refreshed: AtomicBool::new(false),
            clip: Mutex::new(None),
            parent: slf.clone_ref(py),
            lopdf: Arc::clone(&borrowed.lopdf),
            snapshot: Arc::clone(&borrowed.raw_bytes),
        })
    }

    /// Pages over a span.
    #[pyo3(signature = (start=None, stop=None, step=None))]
    fn pages(
        slf: Py<Self>,
        py: Python<'_>,
        start: Option<isize>,
        stop: Option<isize>,
        step: Option<isize>,
    ) -> PyResult<Vec<PyPage>> {
        let borrowed = slf.bind(py).borrow();
        borrowed.ensure_open()?;
        let engine = borrowed.engine()?;
        let count = engine.page_count() as isize;
        let step = step.unwrap_or(1);
        if step == 0 {
            return Err(PyValueError::new_err("step must not be zero"));
        }
        let normalise = |value: isize| -> isize {
            let value = if value < 0 { count + value } else { value };
            value.clamp(0, count)
        };
        let (start, stop) = if step > 0 {
            (
                normalise(start.unwrap_or(0)),
                normalise(stop.unwrap_or(count)),
            )
        } else {
            (
                start.map_or(count - 1, normalise),
                stop.map_or(-count - 1, |v| {
                    let value = if v < 0 { count + v } else { v };
                    value.clamp(-1, count - 1)
                }),
            )
        };
        let mut out = Vec::new();
        let mut at = start;
        while if step > 0 { at < stop } else { at > stop } {
            out.push(PyPage {
                doc: engine.clone(),
                index: at as usize,
                refreshed: AtomicBool::new(false),
                clip: Mutex::new(None),
                parent: slf.clone_ref(py),
                lopdf: Arc::clone(&borrowed.lopdf),
                snapshot: Arc::clone(&borrowed.raw_bytes),
            });
            at += step;
        }
        Ok(out)
    }

    /// Save to bytes.
    #[pyo3(signature = (*args, garbage=0, clean=0, deflate=0, deflate_images=0, deflate_fonts=0, incremental=0, ascii=0, expand=0, linear=0, no_new_id=0, appearance=0, pretty=0, encryption=1, permissions=4095, owner_pw=None, user_pw=None, preserve_metadata=1, use_objstms=0, compression_effort=0, raise_on_repair=0, reproducible=0))]
    #[allow(clippy::too_many_arguments)]
    fn tobytes<'py>(
        &self,
        py: Python<'py>,
        args: &Bound<'_, pyo3::types::PyTuple>,
        garbage: i32,
        clean: i32,
        deflate: i32,
        deflate_images: i32,
        deflate_fonts: i32,
        incremental: i32,
        ascii: i32,
        expand: i32,
        linear: i32,
        no_new_id: i32,
        appearance: i32,
        pretty: i32,
        encryption: i32,
        permissions: i32,
        owner_pw: Option<String>,
        user_pw: Option<String>,
        preserve_metadata: i32,
        use_objstms: i32,
        compression_effort: i32,
        raise_on_repair: i32,
        reproducible: i32,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let _ = (
            args.len(),
            garbage,
            clean,
            deflate,
            deflate_images,
            deflate_fonts,
            ascii,
            expand,
            no_new_id,
            appearance,
            pretty,
            preserve_metadata,
            use_objstms,
            compression_effort,
            raise_on_repair,
            reproducible,
        );
        self.ensure_open()?;
        if linear != 0 {
            return Err(PyValueError::new_err("linearization is not supported"));
        }
        if incremental != 0 && self.lopdf.lock().unwrap().is_none() {
            return Err(FzErrorArgument::new_err("nothing to append"));
        }
        let bytes = rendered_bytes(self, encryption, permissions, owner_pw, user_pw)?;
        Ok(PyBytes::new(py, &bytes))
    }
}

#[pymethods]
impl PyDocument {
    /// Write to bytes.
    #[pyo3(signature = (*args, garbage=0, clean=0, deflate=0, deflate_images=0, deflate_fonts=0, incremental=0, ascii=0, expand=0, linear=0, no_new_id=0, appearance=0, pretty=0, encryption=1, permissions=4095, owner_pw=None, user_pw=None, preserve_metadata=1, use_objstms=0, compression_effort=0, raise_on_repair=0, reproducible=0))]
    #[allow(clippy::too_many_arguments)]
    fn write<'py>(
        &self,
        py: Python<'py>,
        args: &Bound<'_, pyo3::types::PyTuple>,
        garbage: i32,
        clean: i32,
        deflate: i32,
        deflate_images: i32,
        deflate_fonts: i32,
        incremental: i32,
        ascii: i32,
        expand: i32,
        linear: i32,
        no_new_id: i32,
        appearance: i32,
        pretty: i32,
        encryption: i32,
        permissions: i32,
        owner_pw: Option<String>,
        user_pw: Option<String>,
        preserve_metadata: i32,
        use_objstms: i32,
        compression_effort: i32,
        raise_on_repair: i32,
        reproducible: i32,
    ) -> PyResult<Bound<'py, PyBytes>> {
        self.tobytes(
            py,
            args,
            garbage,
            clean,
            deflate,
            deflate_images,
            deflate_fonts,
            incremental,
            ascii,
            expand,
            linear,
            no_new_id,
            appearance,
            pretty,
            encryption,
            permissions,
            owner_pw,
            user_pw,
            preserve_metadata,
            use_objstms,
            compression_effort,
            raise_on_repair,
            reproducible,
        )
    }

    /// Save with cleanup defaults.
    #[pyo3(signature = (filename, garbage=3, clean=0, deflate=1, deflate_images=1, deflate_fonts=1, incremental=0, ascii=0, expand=0, linear=0, no_new_id=0, appearance=0, pretty=0, encryption=1, permissions=4095, owner_pw=None, user_pw=None, preserve_metadata=1, use_objstms=0, compression_effort=0, raise_on_repair=0, reproducible=0))]
    #[allow(clippy::too_many_arguments)]
    fn ez_save(
        &self,
        filename: &str,
        garbage: i32,
        clean: i32,
        deflate: i32,
        deflate_images: i32,
        deflate_fonts: i32,
        incremental: i32,
        ascii: i32,
        expand: i32,
        linear: i32,
        no_new_id: i32,
        appearance: i32,
        pretty: i32,
        encryption: i32,
        permissions: i32,
        owner_pw: Option<String>,
        user_pw: Option<String>,
        preserve_metadata: i32,
        use_objstms: i32,
        compression_effort: i32,
        raise_on_repair: i32,
        reproducible: i32,
    ) -> PyResult<()> {
        let _ = (
            garbage,
            clean,
            deflate,
            deflate_images,
            deflate_fonts,
            ascii,
            expand,
            no_new_id,
            appearance,
            pretty,
            preserve_metadata,
            use_objstms,
            compression_effort,
            raise_on_repair,
            reproducible,
        );
        compat_save_doc(
            self,
            filename,
            incremental != 0,
            linear != 0,
            encryption,
            permissions,
            owner_pw,
            user_pw,
        )
    }

    /// Append unsaved changes to the backing file.
    #[allow(non_snake_case)]
    fn saveIncr(&self) -> PyResult<()> {
        self.ensure_open()?;
        if self.name.is_empty() {
            return Err(PyValueError::new_err("no backing file to append to"));
        }
        if self.lopdf.lock().unwrap().is_none() {
            return Err(FzErrorArgument::new_err("nothing to append"));
        }
        let bytes = current_bytes(self)?;
        std::fs::write(&self.name, bytes)
            .map_err(|e| validation_error(ValidationKind::SaveFailed, e.to_string()))?;
        self.dirty.store(false, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    /// Save a compact snapshot.
    fn save_snapshot(&self, filename: &Bound<'_, PyAny>) -> PyResult<()> {
        let filename = filename
            .extract::<String>()
            .map_err(|_| validation_error(ValidationKind::BadXref, "snapshot needs a filename"))?;
        compat_save_doc(self, &filename, false, false, 1, 4095, None, None)
    }

    /// Whether a page number or chapter position exists.
    fn __contains__(&self, location: &Bound<'_, PyAny>) -> PyResult<bool> {
        self.ensure_open()?;
        if let Ok(page) = location.extract::<i32>() {
            return Ok(page >= 0 && (page as usize) < self.engine()?.page_count());
        }
        let Ok((chapter, page)) = location_tuple(location) else {
            return Ok(false);
        };
        if chapter != 0 || page < 0 {
            return Ok(false);
        }
        Ok((page as usize) < self.engine()?.page_count())
    }

    /// Delete the page at an index.
    fn __delitem__(&self, index: &Bound<'_, PyAny>) -> PyResult<()> {
        self.ensure_open()?;
        let index = index
            .extract::<i32>()
            .map_err(|_| validation_error(ValidationKind::BadXref, "page index must be an int"))?;
        let count = self.engine()?.page_count();
        let target = resolve_page(index, count)?;
        let mut guard = self.init_lopdf()?;
        let handle = guard.as_mut().expect("initialised above");
        if handle.is_encrypted() {
            let password = self.password.lock().unwrap().clone().unwrap_or_default();
            pdf_manip::encrypt::decrypt(handle, &password)
                .map_err(|_| PyValueError::new_err("document needs a password"))?;
        }
        let pages = handle.get_pages();
        let mut numbers: Vec<u32> = pages.keys().copied().collect();
        numbers.sort_unstable();
        let page_number = numbers[target];
        let page_id = pages[&page_number];
        let parent = handle
            .get_dictionary(page_id)
            .ok()
            .and_then(|d| d.get(b"Parent").ok()?.as_reference().ok());
        handle.objects.remove(&page_id);
        if let Some(parent_id) = parent {
            if let Ok(dict) = handle.get_dictionary_mut(parent_id) {
                if let Ok(kids) = dict.get_mut(b"Kids").and_then(|k| k.as_array_mut()) {
                    kids.retain(|kid| kid.as_reference().ok() != Some(page_id));
                }
                let count = dict
                    .get(b"Count")
                    .ok()
                    .and_then(|c| c.as_i64().ok())
                    .unwrap_or(0);
                dict.set(b"Count", LObject::Integer((count - 1).max(0)));
            }
        }
        let mut buf = Vec::new();
        handle
            .save_to(&mut buf)
            .map_err(|e| PdfluentIoError::new_err(format!("serialise failed: {e}")))?;
        drop(guard);
        reparse_engine(self, &buf)?;
        self.mark_dirty();
        Ok(())
    }

    /// Chapter count.
    #[getter]
    fn chapter_count(&self) -> PyResult<i32> {
        self.ensure_open()?;
        Ok(1)
    }

    /// Pages in a chapter.
    fn chapter_page_count(&self, chapter: i32) -> PyResult<usize> {
        self.ensure_open()?;
        if chapter != 0 {
            return Err(PyValueError::new_err(format!("no chapter {chapter}")));
        }
        Ok(self.engine()?.page_count())
    }

    /// Position behind a bookmark.
    fn find_bookmark(&self, py: Python<'_>, mark: i64) -> PyResult<PyObject> {
        self.ensure_open()?;
        let chapter = (mark >> 16) as i32;
        let page = (mark & 0xFFFF) as i32;
        Ok((chapter, page).into_pyobject(py)?.into_any().unbind())
    }

    /// Bookmark for a chapter position.
    fn make_bookmark(&self, location: &Bound<'_, PyAny>) -> PyResult<i64> {
        self.ensure_open()?;
        let (chapter, page) = location_tuple(location)?;
        Ok(((chapter as i64) << 16) | ((page as i64) & 0xFFFF))
    }

    /// Last chapter position.
    #[getter]
    fn last_location(&self, py: Python<'_>) -> PyResult<PyObject> {
        self.ensure_open()?;
        let last = self.engine()?.page_count().saturating_sub(1) as i32;
        Ok((0, last).into_pyobject(py)?.into_any().unbind())
    }

    /// Chapter position of a page.
    fn location_from_page_number(&self, py: Python<'_>, page: i32) -> PyResult<PyObject> {
        self.ensure_open()?;
        let target = resolve_page(page, self.engine()?.page_count())?;
        Ok((0, target as i32).into_pyobject(py)?.into_any().unbind())
    }

    /// Page number of a chapter position.
    fn page_number_from_location(&self, location: &Bound<'_, PyAny>) -> PyResult<i32> {
        self.ensure_open()?;
        let (chapter, page) = location_tuple(location)?;
        if chapter != 0 || page < 0 || page as usize >= self.engine()?.page_count() {
            return Err(PyValueError::new_err("bad location"));
        }
        Ok(page)
    }

    /// Chapter position after this one: empty past the end.
    fn next_location(&self, py: Python<'_>, location: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        self.ensure_open()?;
        let (chapter, page) = location_tuple(location)?;
        let count = self.engine()?.page_count() as i32;
        if chapter != 0 || page < 0 || page >= count {
            return Err(validation_error(ValidationKind::BadXref, "bad location"));
        }
        if page + 1 >= count {
            return Ok(PyList::empty(py).into_any().unbind());
        }
        Ok((0, page + 1).into_pyobject(py)?.into_any().unbind())
    }

    /// Chapter position before this one: empty before the start.
    fn prev_location(&self, py: Python<'_>, location: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        self.ensure_open()?;
        let (chapter, page) = location_tuple(location)?;
        let count = self.engine()?.page_count() as i32;
        if chapter != 0 || page < 0 || page >= count {
            return Err(validation_error(ValidationKind::BadXref, "bad location"));
        }
        if page == 0 {
            return Ok(PyList::empty(py).into_any().unbind());
        }
        Ok((0, page - 1).into_pyobject(py)?.into_any().unbind())
    }

    /// Page count of a document.
    fn page_count2(&self, doc: &Bound<'_, PyAny>) -> PyResult<usize> {
        let doc = doc
            .downcast::<PyDocument>()
            .map_err(|_| PyTypeError::new_err("page_count2() needs a Document"))?;
        let doc = doc.borrow();
        doc.ensure_open()?;
        Ok(doc.engine()?.page_count())
    }

    /// Convert a page span to PDF content.
    #[pyo3(signature = (from_page=0, to_page=-1, rotate=0))]
    fn convert_to_pdf(
        &self,
        py: Python<'_>,
        from_page: i32,
        to_page: i32,
        rotate: i32,
    ) -> PyResult<PyObject> {
        self.ensure_open()?;
        let bytes = self.current_bytes()?;
        let facade = FacadePdfDocument::from_bytes(&bytes)
            .map_err(|err| member_error(Member::ConvertToPdf, err))?;
        let out = facade
            .convert_to_pdf(from_page, to_page, rotate)
            .map_err(|err| member_error(Member::ConvertToPdf, err))?;
        Ok(PyBytes::new(py, &out).into_any().unbind())
    }

    /// Finish opening a fresh document.
    fn init_doc(&self) -> PyResult<()> {
        self.ensure_open()?;
        Ok(())
    }

    /// Resolve named destinations (they resolve on read).
    fn resolve_names(&self, py: Python<'_>) -> PyResult<PyObject> {
        self.ensure_open()?;
        let parsed = snapshot(self)?;
        let out = PyDict::new(py);
        let Some(root) = catalog_id(&parsed) else {
            return Ok(out.into_any().unbind());
        };
        let names = parsed
            .get_dictionary(root)
            .ok()
            .and_then(|catalog| catalog.get(b"Names").ok().cloned())
            .and_then(|obj| deref(&parsed, &obj));
        let Some(LObject::Dictionary(names)) = names else {
            return Ok(out.into_any().unbind());
        };
        let ids = page_ids(&parsed);
        for key in [b"Dests".as_slice(), b"EmbeddedFiles"] {
            let tree = names
                .get(key)
                .ok()
                .cloned()
                .and_then(|obj| deref(&parsed, &obj));
            let Some(tree) = tree else { continue };
            for (name, dest) in name_tree_entries(&parsed, &tree) {
                let page = dest_page(&parsed, &ids, &dest).map(|p| p as i32);
                match page {
                    Some(page) => out.set_item(name, page)?,
                    None => out.set_item(name, py.None())?,
                }
            }
        }
        Ok(out.into_any().unbind())
    }

    /// Reflow the document.
    #[pyo3(signature = (rect=None, width=0.0, height=0.0, fontsize=11.0))]
    fn layout(
        &self,
        rect: Option<&Bound<'_, PyAny>>,
        width: f64,
        height: f64,
        fontsize: f64,
    ) -> PyResult<()> {
        self.ensure_open()?;
        let _ = (rect, width, height, fontsize);
        Err(PdfluentError::new_err(
            "not supported in this version: Document.layout; see KNOWN_DIFFERENCES",
        ))
    }

    /// Recolor direct colour operators to a component count.
    #[pyo3(signature = (components=1))]
    fn recolor(&self, components: i32) -> PyResult<()> {
        self.ensure_open()?;
        if ![1, 3, 4].contains(&components) {
            return Err(validation_error(
                ValidationKind::DrawArg,
                format!("bad component count {components}"),
            ));
        }
        let mut guard = self.init_lopdf()?;
        let handle = guard.as_mut().expect("initialised above");
        let count = page_ids(handle).len();
        for index in 0..count {
            recolor_page_contents(handle, index, components)?;
        }
        let mut buf = Vec::new();
        handle
            .save_to(&mut buf)
            .map_err(|e| PdfluentIoError::new_err(format!("serialise failed: {e}")))?;
        drop(guard);
        reparse_engine(self, &buf)?;
        self.mark_dirty();
        Ok(())
    }

    /// Whether linearised for fast web view.
    #[getter]
    fn is_fast_webaccess(&self) -> PyResult<i32> {
        self.ensure_open()?;
        let parsed = raw_snapshot(self)?;
        Ok(parsed
            .objects
            .get(&(1, 0))
            .and_then(|o| o.as_dict().ok())
            .is_some_and(|dict| dict.get(b"Linearized").is_ok()) as i32)
    }

    /// Whether the document reflows.
    #[getter]
    fn is_reflowable(&self) -> PyResult<bool> {
        self.ensure_open()?;
        Ok(false)
    }

    /// Whether the document needed repair.
    #[getter]
    fn is_repaired(&self) -> PyResult<bool> {
        self.ensure_open()?;
        Ok(false)
    }

    /// Whether an object is a stream.
    #[pyo3(signature = (xref=0))]
    fn is_stream(&self, xref: i32) -> PyResult<bool> {
        self.ensure_open()?;
        let parsed = snapshot(self)?;
        Ok(object_by_xref(&parsed, xref)
            .ok()
            .is_some_and(|o| matches!(o, LObject::Stream(_))))
    }

    /// Document language.
    #[getter]
    fn language(&self) -> PyResult<Option<String>> {
        self.ensure_open()?;
        let found = catalog_text(&snapshot(self)?, b"Lang");
        if found.is_empty() {
            Ok(None)
        } else {
            Ok(Some(found))
        }
    }

    /// Layer interface configurations.
    fn layer_ui_configs(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        self.ensure_open()?;
        let bytes = self.current_bytes()?;
        let facade = FacadePdfDocument::from_bytes(&bytes).map_err(compat_err_to_py)?;
        let configs = facade.layer_ui_configs().map_err(compat_err_to_py)?;
        configs
            .iter()
            .map(|value| super::edit::facade_value_to_py(py, value))
            .collect()
    }

    /// Marked-content info.
    #[getter]
    fn markinfo(&self, py: Python<'_>) -> PyResult<PyObject> {
        self.ensure_open()?;
        let parsed = snapshot(self)?;
        let dict = PyDict::new(py);
        let info = catalog_id(&parsed)
            .and_then(|id| {
                parsed
                    .get_dictionary(id)
                    .ok()?
                    .get(b"MarkInfo")
                    .ok()
                    .cloned()
            })
            .and_then(|obj| deref(&parsed, &obj));
        if let Some(LObject::Dictionary(info)) = info {
            for key in [b"Marked".as_slice(), b"UserProperties", b"Suspects"] {
                let name = String::from_utf8_lossy(key).into_owned();
                match info.get(key) {
                    Ok(LObject::Boolean(b)) => dict.set_item(name, b)?,
                    Ok(other) => dict.set_item(name, object_source(other, false))?,
                    Err(_) => dict.set_item(name, false)?,
                }
            }
        }
        Ok(dict.into_any().unbind())
    }

    /// Whether appearances render on view (pass a value to set it).
    #[pyo3(signature = (value=None))]
    fn need_appearances(&self, value: Option<&Bound<'_, PyAny>>) -> PyResult<PyObject> {
        self.ensure_open()?;
        if let Some(value) = value {
            if !value.is_none() {
                let flag: bool = value.extract()?;
                let mut guard = self.init_lopdf()?;
                let handle = guard.as_mut().expect("initialised above");
                let root = handle
                    .trailer
                    .get(b"Root")
                    .ok()
                    .and_then(|o| o.as_reference().ok())
                    .ok_or_else(|| PyValueError::new_err("no catalog"))?;
                let form_id = handle
                    .get_dictionary(root)
                    .ok()
                    .and_then(|catalog| catalog.get(b"AcroForm").ok()?.as_reference().ok());
                if let Some(form_id) = form_id {
                    handle
                        .get_dictionary_mut(form_id)
                        .map_err(|_| PyValueError::new_err("no form"))?
                        .set(b"NeedAppearances", LObject::Boolean(flag));
                } else {
                    let fresh = handle.max_id + 1;
                    let mut form = lopdf::Dictionary::new();
                    form.set(b"NeedAppearances", LObject::Boolean(flag));
                    handle.objects.insert((fresh, 0), LObject::Dictionary(form));
                    handle.max_id = fresh;
                    handle
                        .get_dictionary_mut(root)
                        .map_err(|_| PyValueError::new_err("no catalog"))?
                        .set(b"AcroForm", LObject::Reference((fresh, 0)));
                }
                drop(guard);
                self.mark_dirty();
            }
        }
        Ok(Python::with_gil(|py| py.None()))
    }
}

/// Read a `(chapter, page)` location from a tuple or list.
fn location_tuple(location: &Bound<'_, PyAny>) -> PyResult<(i32, i32)> {
    if let Ok(pair) = location.extract::<(i32, i32)>() {
        return Ok(pair);
    }
    Err(PyTypeError::new_err(
        "location must be a (chapter, page) pair",
    ))
}

/// Local alias so the lane reads without the module prefix.
#[allow(clippy::too_many_arguments)]
fn compat_save_doc(
    doc: &PyDocument,
    filename: &str,
    incremental: bool,
    linear: bool,
    encryption: i32,
    permissions: i32,
    owner_pw: Option<String>,
    user_pw: Option<String>,
) -> PyResult<()> {
    compat_save(
        doc,
        filename,
        incremental,
        linear,
        encryption,
        permissions,
        owner_pw,
        user_pw,
    )
}

/// One optional-content group as a mapping.
/// The `/OCProperties` dictionary, resolved.
fn oc_properties(parsed: &LopdfDocument) -> Option<lopdf::Dictionary> {
    let root = catalog_id(parsed)?;
    parsed
        .get_dictionary(root)
        .ok()?
        .get(b"OCProperties")
        .ok()
        .cloned()
        .and_then(|obj| deref(parsed, &obj))
        .and_then(|obj| obj.as_dict().ok().cloned())
}

/// Optional-content groups: number, name, and on/off state each.
fn ocg_entries(parsed: &LopdfDocument) -> Vec<(i32, String, bool)> {
    let Some(props) = oc_properties(parsed) else {
        return Vec::new();
    };
    let groups = props
        .get(b"OCGs")
        .ok()
        .and_then(|o| o.as_array().ok())
        .cloned()
        .unwrap_or_default();
    let off: Vec<lopdf::ObjectId> = props
        .get(b"D")
        .ok()
        .cloned()
        .and_then(|obj| deref(parsed, &obj))
        .and_then(|obj| match obj {
            LObject::Dictionary(dict) => Some(dict),
            _ => None,
        })
        .and_then(|dict| dict.get(b"OFF").ok()?.as_array().ok().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|o| o.as_reference().ok())
        .collect();
    let mut out = Vec::new();
    for group in &groups {
        let Ok(id) = group.as_reference() else {
            continue;
        };
        let name = parsed
            .get_dictionary(id)
            .ok()
            .and_then(|d| d.get(b"Name").ok().cloned())
            .and_then(|o| match o {
                LObject::String(bytes, _) => Some(String::from_utf8_lossy(&bytes).into_owned()),
                _ => None,
            })
            .unwrap_or_default();
        out.push((id.0 as i32, name, !off.contains(&id)));
    }
    out
}

/// Layer configurations on the properties dictionary.
fn layer_configs(parsed: &LopdfDocument) -> Vec<lopdf::Dictionary> {
    let Some(props) = oc_properties(parsed) else {
        return Vec::new();
    };
    props
        .get(b"Configs")
        .ok()
        .and_then(|o| o.as_array().ok())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    deref(parsed, item).and_then(|obj| match obj {
                        LObject::Dictionary(dict) => Some(dict),
                        _ => None,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// One configuration as lowercase keys: names read bare.
fn config_dict(py: Python<'_>, config: &lopdf::Dictionary) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    for (key, value) in config.iter() {
        let name = String::from_utf8_lossy(key).to_lowercase();
        if let Ok(found) = value.as_name() {
            dict.set_item(name, String::from_utf8_lossy(found).into_owned())?;
        } else if let Ok(found) = value.as_str() {
            dict.set_item(name, String::from_utf8_lossy(found).into_owned())?;
        } else if let Ok(found) = value.as_bool() {
            dict.set_item(name, found)?;
        } else if let Ok(found) = value.as_i64() {
            dict.set_item(name, found)?;
        } else if let Ok(found) = value.as_float() {
            dict.set_item(name, found)?;
        }
    }
    Ok(dict.into_any().unbind())
}

#[pymethods]
impl PyDocument {
    /// Crop box of a page.
    fn page_cropbox(&self, pno: i32) -> PyResult<PyCompatRect> {
        self.ensure_open()?;
        let parsed = snapshot(self)?;
        let target = resolve_index(pno, page_ids(&parsed).len(), "page")?;
        named_box(&parsed, target, b"CropBox")
    }

    /// Object number of a page.
    fn page_xref(&self, pno: i32) -> PyResult<i32> {
        self.ensure_open()?;
        let parsed = snapshot(self)?;
        let ids = page_ids(&parsed);
        let count = ids.len();
        let target = if pno < 0 { count as i32 + pno } else { pno };
        if target < 0 || target as usize >= count {
            return Err(validation_error(
                ValidationKind::OutOfRange,
                format!("page number {pno} out of range ({count} pages)"),
            ));
        }
        Ok(ids[target as usize].0 as i32)
    }

    /// Page layout name.
    #[getter]
    fn pagelayout(&self) -> PyResult<String> {
        self.ensure_open()?;
        let found = catalog_text(&snapshot(self)?, b"PageLayout");
        Ok(if found.is_empty() {
            "SinglePage".to_string()
        } else {
            found
        })
    }

    /// Page mode name.
    #[getter]
    fn pagemode(&self) -> PyResult<String> {
        self.ensure_open()?;
        let found = catalog_text(&snapshot(self)?, b"PageMode");
        Ok(if found.is_empty() {
            "UseNone".to_string()
        } else {
            found
        })
    }

    /// Catalog object number.
    fn pdf_catalog(&self) -> PyResult<i32> {
        self.ensure_open()?;
        let parsed = snapshot(self)?;
        let Some(id) = catalog_id(&parsed) else {
            return Err(validation_error(ValidationKind::BadXref, "no catalog"));
        };
        Ok(id.0 as i32)
    }

    /// Trailer source.
    #[pyo3(signature = (compressed=0, ascii=0))]
    fn pdf_trailer(&self, compressed: i32, ascii: i32) -> PyResult<String> {
        self.ensure_open()?;
        let parsed = snapshot(self)?;
        if compressed != 0 {
            return Ok(packed_source(&LObject::Dictionary(parsed.trailer.clone())));
        }
        Ok(object_source(
            &LObject::Dictionary(parsed.trailer.clone()),
            ascii != 0,
        ))
    }

    /// Reserve a fresh object number.
    fn get_new_xref(&self) -> PyResult<i32> {
        self.ensure_open()?;
        let mut guard = self.init_lopdf()?;
        let handle = guard.as_mut().expect("initialised above");
        handle.max_id += 1;
        let fresh = handle.max_id;
        handle.objects.insert((fresh, 0), LObject::Null);
        drop(guard);
        self.mark_dirty();
        Ok(fresh as i32)
    }

    /// One layer configuration, if the document has layers.
    #[pyo3(signature = (config=None))]
    fn get_layer(&self, py: Python<'_>, config: Option<&Bound<'_, PyAny>>) -> PyResult<PyObject> {
        self.ensure_open()?;
        let parsed = snapshot(self)?;
        if ocg_entries(&parsed).is_empty() {
            return Ok(py.None());
        }
        let wanted = config
            .and_then(|found| found.extract::<i32>().ok())
            .unwrap_or(-1);
        if wanted == -1 {
            return Ok(PyDict::new(py).into_any().unbind());
        }
        let configs = layer_configs(&parsed);
        let target = resolve_layer(wanted, configs.len())?;
        config_dict(py, &configs[target])
    }

    /// Layer configurations.
    fn get_layers(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        self.ensure_open()?;
        let parsed = snapshot(self)?;
        layer_configs(&parsed)
            .iter()
            .map(|config| config_dict(py, config))
            .collect()
    }

    /// Optional-content groups by number.
    fn get_ocgs(&self, py: Python<'_>) -> PyResult<PyObject> {
        self.ensure_open()?;
        let parsed = snapshot(self)?;
        let out = PyDict::new(py);
        for (xref, name, on) in ocg_entries(&parsed) {
            let entry = PyDict::new(py);
            entry.set_item("name", name)?;
            entry.set_item("on", on)?;
            out.set_item(xref, entry)?;
        }
        Ok(out.into_any().unbind())
    }

    /// Optional-content state of an object: the membership reference, if any.
    fn get_oc(&self, xref: i32) -> PyResult<i32> {
        self.ensure_open()?;
        let parsed = snapshot(self)?;
        for (number, _, on) in ocg_entries(&parsed) {
            if number == xref {
                return Ok(i32::from(on));
            }
        }
        let obj = object_by_xref(&parsed, xref)?;
        let dict = match &obj {
            LObject::Dictionary(dict) => Some(dict.clone()),
            LObject::Stream(stream) => Some(stream.dict.clone()),
            _ => None,
        };
        if dict.is_some_and(|d| d.get(b"OC").is_ok()) {
            return Ok(1);
        }
        Err(validation_error(
            ValidationKind::BadXref,
            format!("xref {xref} is no optional-content group"),
        ))
    }

    /// Membership dict of an object.
    fn get_ocmd(&self, py: Python<'_>, xref: i32) -> PyResult<PyObject> {
        self.ensure_open()?;
        if xref == 0 {
            return Err(validation_error(
                ValidationKind::OutOfRange,
                "xref 0 holds no membership dict",
            ));
        }
        let parsed = snapshot(self)?;
        let obj = object_by_xref(&parsed, xref)?;
        let dict = match &obj {
            LObject::Dictionary(dict) => Some(dict.clone()),
            LObject::Stream(stream) => Some(stream.dict.clone()),
            _ => None,
        };
        let Some(dict) = dict else {
            return Err(PyValueError::new_err(format!("xref {xref} holds no dict")));
        };
        let is_ocmd =
            matches!(dict.get(b"Type"), Ok(LObject::Name(name)) if name.as_slice() == b"OCMD");
        if !is_ocmd {
            return Err(PyValueError::new_err(format!(
                "xref {xref} holds no membership dict"
            )));
        }
        let policy = match dict.get(b"P") {
            Ok(LObject::Name(name)) => String::from_utf8_lossy(name).into_owned(),
            _ => "AnyOn".to_string(),
        };
        let mut members = Vec::new();
        if let Ok(LObject::Array(items)) = dict.get(b"OCGs") {
            for item in items {
                if let Ok(id) = item.as_reference() {
                    members.push(id.0 as i32);
                }
            }
        }
        let out = PyDict::new(py);
        out.set_item("policy", policy)?;
        out.set_item("members", members)?;
        Ok(out.into_any().unbind())
    }

    /// Object numbers of the outline.
    fn get_outline_xrefs(&self) -> PyResult<Vec<i32>> {
        self.ensure_open()?;
        let parsed = snapshot(self)?;
        let Some(root) = catalog_id(&parsed) else {
            return Ok(Vec::new());
        };
        let mut current = parsed
            .get_dictionary(root)
            .ok()
            .and_then(|catalog| catalog.get(b"Outlines").ok()?.as_reference().ok())
            .and_then(|id| {
                parsed
                    .get_dictionary(id)
                    .ok()?
                    .get(b"First")
                    .ok()?
                    .as_reference()
                    .ok()
            });
        let mut out = Vec::new();
        let mut guard = 0;
        while let Some(id) = current {
            if guard > 4096 {
                break;
            }
            guard += 1;
            out.push(id.0 as i32);
            let Ok(dict) = parsed.get_dictionary(id) else {
                break;
            };
            if let Ok(first) = dict.get(b"First").and_then(|o| o.as_reference()) {
                let mut kids = Vec::new();
                collect_outline_ids(&parsed, first, &mut kids);
                out.append(&mut kids);
            }
            current = dict.get(b"Next").ok().and_then(|o| o.as_reference().ok());
        }
        Ok(out)
    }

    /// Page label runs.
    fn get_page_labels(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        self.ensure_open()?;
        let parsed = snapshot(self)?;
        let Some(root) = catalog_id(&parsed) else {
            return Ok(Vec::new());
        };
        let nums = parsed
            .get_dictionary(root)
            .ok()
            .and_then(|catalog| catalog.get(b"PageLabels").ok().cloned())
            .and_then(|obj| deref(&parsed, &obj));
        let Some(LObject::Dictionary(dict)) = nums else {
            return Ok(Vec::new());
        };
        let Ok(LObject::Array(nums)) = dict.get(b"Nums") else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for pair in nums.chunks(2) {
            let [start, entry] = pair else { continue };
            let Ok(start_page) = start.as_i64() else {
                continue;
            };
            let dict = match entry {
                LObject::Dictionary(dict) => Some(dict.clone()),
                LObject::Reference(id) => parsed.get_dictionary(*id).ok().cloned(),
                _ => None,
            };
            let Some(dict) = dict else { continue };
            let style = match dict.get(b"S") {
                Ok(LObject::Name(name)) => String::from_utf8_lossy(name).into_owned(),
                _ => "D".to_string(),
            };
            let prefix = match dict.get(b"P") {
                Ok(LObject::String(bytes, _)) => String::from_utf8_lossy(bytes).into_owned(),
                _ => String::new(),
            };
            let first_number = dict
                .get(b"St")
                .ok()
                .and_then(|o| o.as_i64().ok())
                .unwrap_or(1) as i32;
            let row = PyDict::new(py);
            row.set_item("startpage", start_page as i32)?;
            row.set_item("prefix", prefix)?;
            row.set_item("firstpagenum", first_number)?;
            row.set_item("style", style)?;
            out.push(row.into_any().unbind());
        }
        Ok(out)
    }

    /// Pages carrying a label.
    #[pyo3(signature = (label, only_one=false))]
    fn get_page_numbers(&self, label: &Bound<'_, PyAny>, only_one: bool) -> PyResult<Vec<i32>> {
        self.ensure_open()?;
        let Ok(label) = label.extract::<String>() else {
            return Ok(Vec::new());
        };
        let parsed = snapshot(self)?;
        let labels = display_labels(&parsed);
        let mut out = Vec::new();
        for (index, shown) in labels.into_iter().enumerate() {
            if shown == label {
                out.push(index as i32);
                if only_one {
                    break;
                }
            }
        }
        Ok(out)
    }

    /// External objects on a page.
    fn get_page_xobjects(&self, pno: i32) -> PyResult<Vec<i32>> {
        self.ensure_open()?;
        let parsed = snapshot(self)?;
        let target = resolve_index(pno, page_ids(&parsed).len(), "page")?;
        page_xobject_ids(&parsed, target)
    }

    /// Signature flags.
    fn get_sigflags(&self) -> PyResult<i32> {
        self.ensure_open()?;
        Ok(acroform_dict(&snapshot(self)?)
            .as_ref()
            .and_then(|form| form.get(b"SigFlags").ok()?.as_i64().ok())
            .unwrap_or(-1) as i32)
    }

    /// XML metadata packet.
    fn get_xml_metadata(&self) -> PyResult<String> {
        self.ensure_open()?;
        let parsed = snapshot(self)?;
        let Some(root) = catalog_id(&parsed) else {
            return Ok(String::new());
        };
        let stream_id = parsed
            .get_dictionary(root)
            .ok()
            .and_then(|catalog| catalog.get(b"Metadata").ok()?.as_reference().ok());
        let Some(stream_id) = stream_id else {
            return Ok(String::new());
        };
        let bytes = parsed
            .get_object(stream_id)
            .ok()
            .and_then(|o| o.as_stream().ok())
            .and_then(|s| s.decompressed_content().ok())
            .unwrap_or_default();
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Object number of the XML metadata.
    fn xref_xml_metadata(&self) -> PyResult<i32> {
        self.ensure_open()?;
        let parsed = snapshot(self)?;
        let Some(root) = catalog_id(&parsed) else {
            return Ok(0);
        };
        Ok(parsed
            .get_dictionary(root)
            .ok()
            .and_then(|catalog| catalog.get(b"Metadata").ok()?.as_reference().ok())
            .map(|id| id.0 as i32)
            .unwrap_or(0))
    }

    /// Incremental version count.
    #[getter]
    fn version_count(&self) -> PyResult<i32> {
        self.ensure_open()?;
        Ok(1)
    }

    /// Highest object number: gone, the count read lives on `xref_length`.
    #[getter]
    fn xref(&self) -> PyResult<i32> {
        self.ensure_open()?;
        Err(validation_error(
            ValidationKind::NoAttr,
            "use xref_length() for the object count",
        ))
    }

    /// Type and value of an object key.
    fn xref_get_key(&self, py: Python<'_>, xref: i32, key: &str) -> PyResult<PyObject> {
        self.ensure_open()?;
        let parsed = snapshot(self)?;
        let obj = object_by_xref(&parsed, xref)?;
        let dict = match &obj {
            LObject::Dictionary(dict) => Some(dict.clone()),
            LObject::Stream(stream) => Some(stream.dict.clone()),
            _ => None,
        };
        let Some(dict) = dict else {
            return Err(PyValueError::new_err(format!("xref {xref} holds no dict")));
        };
        let value = dict.get(key.as_bytes()).ok().cloned();
        let Some(value) = value else {
            return Ok(("null", "null").into_pyobject(py)?.into_any().unbind());
        };
        Ok((
            object_type_name(&value).to_string(),
            object_source(&value, false),
        )
            .into_pyobject(py)?
            .into_any()
            .unbind())
    }

    /// Keys of an object.
    fn xref_get_keys(&self, xref: i32) -> PyResult<Vec<String>> {
        self.ensure_open()?;
        let parsed = snapshot(self)?;
        let obj = object_by_xref(&parsed, xref)?;
        let dict = match &obj {
            LObject::Dictionary(dict) => Some(dict.clone()),
            LObject::Stream(stream) => Some(stream.dict.clone()),
            _ => None,
        };
        let Some(dict) = dict else {
            return Err(PyValueError::new_err(format!("xref {xref} holds no dict")));
        };
        Ok(dict
            .iter()
            .map(|(key, _)| String::from_utf8_lossy(key).into_owned())
            .collect())
    }

    /// Whether an object is a stream.
    #[pyo3(signature = (xref=0))]
    fn xref_is_stream(&self, xref: i32) -> PyResult<bool> {
        self.is_stream(xref)
    }

    /// Whether an object is an external object.
    fn xref_is_xobject(&self, xref: i32) -> PyResult<bool> {
        self.ensure_open()?;
        let parsed = snapshot(self)?;
        let obj = object_by_xref(&parsed, xref)?;
        let LObject::Stream(stream) = &obj else {
            return Ok(false);
        };
        Ok(matches!(
            stream.dict.get(b"Subtype").ok(),
            Some(LObject::Name(name))
                if name.as_slice() == b"Image"
                    || name.as_slice() == b"Form"
                    || name.as_slice() == b"PS"
        ))
    }

    /// Object count, the free head object included.
    fn xref_length(&self) -> PyResult<usize> {
        self.ensure_open()?;
        let parsed = snapshot(self)?;
        Ok(parsed.max_id as usize + 1)
    }

    /// Source of an object.
    #[pyo3(signature = (xref, compressed=0, ascii=0))]
    fn xref_object(&self, xref: i32, compressed: i32, ascii: i32) -> PyResult<String> {
        self.ensure_open()?;
        let parsed = snapshot(self)?;
        if xref == -1 {
            let trailer = LObject::Dictionary(parsed.trailer.clone());
            if compressed != 0 {
                return Ok(packed_source(&trailer));
            }
            return Ok(object_source(&trailer, ascii != 0));
        }
        let obj = object_by_xref(&parsed, xref).map_err(|_| {
            validation_error(ValidationKind::OutOfRange, format!("bad xref {xref}"))
        })?;
        if compressed != 0 {
            return Ok(packed_source(&obj));
        }
        Ok(object_source(&obj, ascii != 0))
    }

    /// Decoded stream bytes, if the object holds a stream.
    fn xref_stream(&self, py: Python<'_>, xref: i32) -> PyResult<PyObject> {
        self.ensure_open()?;
        let parsed = snapshot(self)?;
        let obj = object_by_xref(&parsed, xref)?;
        let LObject::Stream(stream) = &obj else {
            return Ok(py.None());
        };
        let bytes = stream
            .decompressed_content()
            .map_err(|_| PyRuntimeError::new_err(format!("cannot decode xref {xref}")))?;
        Ok(PyBytes::new(py, &bytes).into_any().unbind())
    }

    /// Raw stream bytes, if the object holds a stream.
    fn xref_stream_raw(&self, py: Python<'_>, xref: i32) -> PyResult<PyObject> {
        self.ensure_open()?;
        let parsed = snapshot(self)?;
        let obj = object_by_xref(&parsed, xref)?;
        let LObject::Stream(stream) = &obj else {
            return Ok(py.None());
        };
        Ok(PyBytes::new(py, &stream.content).into_any().unbind())
    }

    /// Switch the layer configuration.
    #[pyo3(signature = (config, as_default=0))]
    fn switch_layer(&self, config: i32, as_default: i32) -> PyResult<()> {
        self.ensure_open()?;
        let _ = as_default;
        let parsed = snapshot(self)?;
        if oc_properties(&parsed).is_none() {
            return Err(validation_error(
                ValidationKind::BadXref,
                "document has no optional content",
            ));
        }
        let configs = layer_configs(&parsed);
        if configs.is_empty() {
            if config == 0 || config == -1 {
                return Ok(());
            }
            return Err(validation_error(
                ValidationKind::BadXref,
                format!("config index {config} out of range (0 entries)"),
            ));
        }
        let target = resolve_layer(config, configs.len())?;
        let mut guard = self.init_lopdf()?;
        let handle = guard.as_mut().expect("initialised above");
        let root = handle
            .trailer
            .get(b"Root")
            .ok()
            .and_then(|o| o.as_reference().ok())
            .ok_or_else(|| validation_error(ValidationKind::BadXref, "no catalog"))?;
        let props_id = handle
            .get_dictionary(root)
            .ok()
            .and_then(|catalog| catalog.get(b"OCProperties").ok()?.as_reference().ok())
            .ok_or_else(|| validation_error(ValidationKind::BadXref, "no optional content"))?;
        let chosen = configs[target].clone();
        handle
            .get_dictionary_mut(props_id)
            .map_err(|_| validation_error(ValidationKind::BadXref, "no optional content"))?
            .set(b"D", LObject::Dictionary(chosen));
        drop(guard);
        self.mark_dirty();
        Ok(())
    }

    /// Embedded-file count.
    fn embfile_count(&self) -> PyResult<usize> {
        self.ensure_open()?;
        Ok(embedded_files(&snapshot(self)?).len())
    }

    /// Embedded-file names.
    fn embfile_names(&self) -> PyResult<Vec<String>> {
        self.ensure_open()?;
        Ok(embedded_files(&snapshot(self)?)
            .into_iter()
            .map(|(name, _)| name)
            .collect())
    }

    /// Embedded-file facts.
    fn embfile_info(&self, py: Python<'_>, item: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        self.ensure_open()?;
        let parsed = snapshot(self)?;
        let (name, id) = embfile_entry(&parsed, item)?;
        let dict = parsed.get_object(id).ok().and_then(|obj| match obj {
            LObject::Dictionary(dict) => Some(dict.clone()),
            LObject::Stream(stream) => Some(stream.dict.clone()),
            _ => None,
        });
        let file_string = |key: &[u8]| {
            dict.as_ref()
                .and_then(|d| d.get(key).ok().cloned())
                .and_then(|o| match o {
                    LObject::String(bytes, _) => Some(String::from_utf8_lossy(&bytes).into_owned()),
                    _ => None,
                })
        };
        let filename = file_string(b"F").unwrap_or_else(|| name.clone());
        let ufilename = file_string(b"UF").unwrap_or_else(|| filename.clone());
        let description = file_string(b"Desc").unwrap_or_default();
        let size = embfile_payload(&parsed, id)
            .map(|bytes| bytes.len())
            .unwrap_or(0);
        let out = PyDict::new(py);
        out.set_item("name", name)?;
        out.set_item("filename", filename)?;
        out.set_item("ufilename", ufilename)?;
        out.set_item("description", description)?;
        out.set_item("size", size)?;
        out.set_item("length", size)?;
        Ok(out.into_any().unbind())
    }

    /// Embedded-file bytes.
    fn embfile_get<'py>(
        &self,
        py: Python<'py>,
        item: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        self.ensure_open()?;
        let parsed = snapshot(self)?;
        let (_, id) = embfile_entry(&parsed, item)?;
        let bytes = embfile_payload(&parsed, id)
            .ok_or_else(|| PyKeyError::new_err("embedded file has no payload"))?;
        Ok(PyBytes::new(py, &bytes))
    }
}

/// Collect outline item ids below `first`, depth-first.
fn collect_outline_ids(parsed: &LopdfDocument, mut id: lopdf::ObjectId, out: &mut Vec<i32>) {
    for _ in 0..4096 {
        let Ok(dict) = parsed.get_dictionary(id) else {
            return;
        };
        out.push(id.0 as i32);
        if let Ok(first) = dict.get(b"First").and_then(|o| o.as_reference()) {
            collect_outline_ids(parsed, first, out);
        }
        let Ok(next) = dict.get(b"Next").and_then(|o| o.as_reference()) else {
            return;
        };
        id = next;
    }
}

/// Display labels for every page (ISO 32000 §12.4.2 numbering styles).
fn display_labels(parsed: &LopdfDocument) -> Vec<String> {
    let count = page_ids(parsed).len();
    let runs = label_runs(parsed);
    (0..count)
        .map(|index| {
            let run = runs
                .iter()
                .rev()
                .find(|(start, _, _, _)| *start as usize <= index);
            match run {
                Some((start, prefix, first, style)) => {
                    let value = first + (index as i32 - start);
                    format!("{prefix}{}", format_label_number(value, style))
                }
                None => String::new(),
            }
        })
        .collect()
}

/// Raw `/PageLabels` runs: `(start page, prefix, first number, style)`.
fn label_runs(parsed: &LopdfDocument) -> Vec<(i32, String, i32, String)> {
    let Some(root) = catalog_id(parsed) else {
        return Vec::new();
    };
    let nums = parsed
        .get_dictionary(root)
        .ok()
        .and_then(|catalog| catalog.get(b"PageLabels").ok().cloned())
        .and_then(|obj| deref(parsed, &obj));
    let Some(LObject::Dictionary(dict)) = nums else {
        return Vec::new();
    };
    let Ok(LObject::Array(nums)) = dict.get(b"Nums") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for pair in nums.chunks(2) {
        let [start, entry] = pair else { continue };
        let Ok(start_page) = start.as_i64() else {
            continue;
        };
        let dict = match entry {
            LObject::Dictionary(dict) => Some(dict.clone()),
            LObject::Reference(id) => parsed.get_dictionary(*id).ok().cloned(),
            _ => None,
        };
        let Some(dict) = dict else { continue };
        let style = match dict.get(b"S") {
            Ok(LObject::Name(name)) => String::from_utf8_lossy(name).into_owned(),
            _ => "D".to_string(),
        };
        let prefix = match dict.get(b"P") {
            Ok(LObject::String(bytes, _)) => String::from_utf8_lossy(bytes).into_owned(),
            _ => String::new(),
        };
        let first = dict
            .get(b"St")
            .ok()
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(1) as i32;
        out.push((start_page as i32, prefix, first, style));
    }
    out
}

/// Format one label number in a style letter.
fn format_label_number(value: i32, style: &str) -> String {
    if value <= 0 {
        return value.to_string();
    }
    match style {
        "A" => to_alpha(value, false),
        "a" => to_alpha(value, true),
        "R" => to_roman(value, false),
        "r" => to_roman(value, true),
        _ => value.to_string(),
    }
}

/// 1-based alphabetic numbering: 1 -> A, 27 -> AA.
fn to_alpha(mut value: i32, lower: bool) -> String {
    let mut out = Vec::new();
    while value > 0 {
        value -= 1;
        let digit = (value % 26) as u8;
        out.push(if lower { b'a' + digit } else { b'A' + digit });
        value /= 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// Roman numbering up to 3999.
fn to_roman(mut value: i32, lower: bool) -> String {
    const TABLE: &[(i32, &str)] = &[
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for (arabic, roman) in TABLE {
        while value >= *arabic {
            out.push_str(roman);
            value -= arabic;
        }
    }
    if lower {
        out.to_ascii_lowercase()
    } else {
        out
    }
}

/// Resolve an embedded-file handle (name or index) to its entry.
fn embfile_entry(
    parsed: &LopdfDocument,
    item: &Bound<'_, PyAny>,
) -> PyResult<(String, lopdf::ObjectId)> {
    let files = embedded_files(parsed);
    if let Ok(name) = item.extract::<String>() {
        return files
            .into_iter()
            .find(|(known, _)| known == &name)
            .ok_or_else(|| {
                validation_error(
                    ValidationKind::BadXref,
                    format!("no embedded file {name:?}"),
                )
            });
    }
    let index: i32 = item
        .extract()
        .map_err(|_| PyTypeError::new_err("embedded file needs a name or index"))?;
    let count = files.len();
    let target = if index < 0 {
        count as i32 + index
    } else {
        index
    };
    if target < 0 || target as usize >= count {
        return Err(validation_error(
            ValidationKind::BadXref,
            format!("embedded file index {index} out of range ({count} files)"),
        ));
    }
    Ok(files[target as usize].clone())
}

/// Payload bytes of one embedded file: the stream itself, or `/EF /F`
/// (falling back to `/UF`) behind a file-spec dict.
fn embfile_payload(parsed: &LopdfDocument, id: lopdf::ObjectId) -> Option<Vec<u8>> {
    if let Ok(stream) = parsed.get_object(id).ok()?.as_stream() {
        return stream.decompressed_content().ok();
    }
    let dict = parsed.get_dictionary(id).ok()?;
    let ef = dict.get(b"EF").ok().cloned()?;
    let ef = deref(parsed, &ef)?;
    let LObject::Dictionary(ef) = ef else {
        return None;
    };
    let stream_id = ef
        .get(b"F")
        .ok()
        .or_else(|| ef.get(b"UF").ok())
        .and_then(|o| o.as_reference().ok())?;
    parsed
        .get_object(stream_id)
        .ok()?
        .as_stream()
        .ok()?
        .decompressed_content()
        .ok()
}

// ---------------------------------------------------------------------------
// Page methods
// ---------------------------------------------------------------------------

/// Read a rectangle argument: a 4-sequence or an object with `x0..y1`.
fn parse_rect_arg(rect: &Bound<'_, PyAny>) -> PyResult<[f64; 4]> {
    if let Ok(seq) = rect.extract::<(f64, f64, f64, f64)>() {
        return Ok([seq.0, seq.1, seq.2, seq.3]);
    }
    if let Ok(seq) = rect.extract::<Vec<f64>>() {
        if seq.len() == 4 {
            return Ok([seq[0], seq[1], seq[2], seq[3]]);
        }
    }
    let edge = |name: &str| rect.getattr(name)?.extract::<f64>();
    Ok([edge("x0")?, edge("y0")?, edge("x1")?, edge("y1")?])
}

/// Width and height of one crop box.
fn crop_dims(crop: [f64; 4]) -> (f64, f64) {
    ((crop[2] - crop[0]).abs(), (crop[3] - crop[1]).abs())
}

#[pymethods]
impl PyPage {
    /// Page number (0-based): gone after `refresh`.
    #[getter]
    fn number(&self) -> Option<i32> {
        if self.compat_refreshed() {
            None
        } else {
            Some(self.index as i32)
        }
    }

    /// The owning document.
    #[getter]
    fn parent(&self, py: Python<'_>) -> Py<PyDocument> {
        self.parent.clone_ref(py)
    }

    /// Page rectangle.
    #[getter]
    fn rect(&self, py: Python<'_>) -> PyResult<PyCompatRect> {
        let parent = self.parent.bind(py).borrow();
        parent.ensure_open()?;
        let parsed = snapshot(&parent)?;
        let frames = page_frames(&parsed, self.index)?;
        let shown = flip_rect(frames.crop, frames.crop[3].max(frames.crop[1]));
        let (mut w, mut h) = (shown.x1 - shown.x0, shown.y1 - shown.y0);
        if matches!(frames.rotation, 90 | 270) {
            std::mem::swap(&mut w, &mut h);
        }
        Ok(PyCompatRect {
            x0: 0.0,
            y0: 0.0,
            x1: w,
            y1: h,
        })
    }

    /// Media box.
    #[getter]
    fn mediabox(&self, py: Python<'_>) -> PyResult<PyCompatRect> {
        let parent = self.parent.bind(py).borrow();
        parent.ensure_open()?;
        named_box(&snapshot(&parent)?, self.index, b"MediaBox")
    }

    /// Crop box.
    #[getter]
    fn cropbox(&self, py: Python<'_>) -> PyResult<PyCompatRect> {
        let parent = self.parent.bind(py).borrow();
        parent.ensure_open()?;
        named_box(&snapshot(&parent)?, self.index, b"CropBox")
    }

    /// Art box.
    #[getter]
    fn artbox(&self, py: Python<'_>) -> PyResult<PyCompatRect> {
        let parent = self.parent.bind(py).borrow();
        parent.ensure_open()?;
        named_box(&snapshot(&parent)?, self.index, b"ArtBox")
    }

    /// Bleed box.
    #[getter]
    fn bleedbox(&self, py: Python<'_>) -> PyResult<PyCompatRect> {
        let parent = self.parent.bind(py).borrow();
        parent.ensure_open()?;
        named_box(&snapshot(&parent)?, self.index, b"BleedBox")
    }

    /// Trim box.
    #[getter]
    fn trimbox(&self, py: Python<'_>) -> PyResult<PyCompatRect> {
        let parent = self.parent.bind(py).borrow();
        parent.ensure_open()?;
        named_box(&snapshot(&parent)?, self.index, b"TrimBox")
    }

    /// Content bounding box (the rotated crop box).
    fn bound(&self, py: Python<'_>) -> PyResult<PyCompatRect> {
        self.rect(py)
    }

    /// Media box size.
    #[getter]
    fn mediabox_size(&self, py: Python<'_>) -> PyResult<PyCompatPoint> {
        let media = self.mediabox(py)?;
        Ok(PyCompatPoint {
            x: (media.x1 - media.x0).abs(),
            y: (media.y1 - media.y0).abs(),
        })
    }

    /// Crop box offset within the media box.
    #[getter]
    fn cropbox_position(&self, py: Python<'_>) -> PyResult<PyCompatPoint> {
        let parent = self.parent.bind(py).borrow();
        parent.ensure_open()?;
        let frames = page_frames(&snapshot(&parent)?, self.index)?;
        Ok(PyCompatPoint {
            x: frames.crop[0].min(frames.crop[2]) - frames.media[0].min(frames.media[2]),
            y: frames.media[3].max(frames.media[1]) - frames.crop[3].max(frames.crop[1]),
        })
    }

    /// Matrix undoing the rotation.
    #[getter]
    fn derotation_matrix(&self, py: Python<'_>) -> PyResult<PyCompatMatrix> {
        let parent = self.parent.bind(py).borrow();
        parent.ensure_open()?;
        let frames = page_frames(&snapshot(&parent)?, self.index)?;
        let (w, h) = crop_dims(frames.crop);
        Ok(match frames.rotation.rem_euclid(360) {
            90 => PyCompatMatrix {
                a: 0.0,
                b: -1.0,
                c: 1.0,
                d: 0.0,
                e: 0.0,
                f: h,
            },
            180 => PyCompatMatrix {
                a: -1.0,
                b: 0.0,
                c: 0.0,
                d: -1.0,
                e: w,
                f: h,
            },
            270 => PyCompatMatrix {
                a: 0.0,
                b: 1.0,
                c: -1.0,
                d: 0.0,
                e: w,
                f: 0.0,
            },
            _ => PyCompatMatrix {
                a: 1.0,
                b: 0.0,
                c: 0.0,
                d: 1.0,
                e: 0.0,
                f: 0.0,
            },
        })
    }

    /// Matrix applying the rotation.
    #[getter]
    fn rotation_matrix(&self, py: Python<'_>) -> PyResult<PyCompatMatrix> {
        let parent = self.parent.bind(py).borrow();
        parent.ensure_open()?;
        let frames = page_frames(&snapshot(&parent)?, self.index)?;
        let (w, h) = crop_dims(frames.crop);
        Ok(match frames.rotation.rem_euclid(360) {
            90 => PyCompatMatrix {
                a: 0.0,
                b: 1.0,
                c: -1.0,
                d: 0.0,
                e: h,
                f: 0.0,
            },
            180 => PyCompatMatrix {
                a: -1.0,
                b: 0.0,
                c: 0.0,
                d: -1.0,
                e: w,
                f: h,
            },
            270 => PyCompatMatrix {
                a: 0.0,
                b: -1.0,
                c: 1.0,
                d: 0.0,
                e: 0.0,
                f: w,
            },
            _ => PyCompatMatrix {
                a: 1.0,
                b: 0.0,
                c: 0.0,
                d: 1.0,
                e: 0.0,
                f: 0.0,
            },
        })
    }

    /// Page transformation matrix (y-up PDF points to the y-down frame).
    #[getter]
    fn transformation_matrix(&self, py: Python<'_>) -> PyResult<PyCompatMatrix> {
        let parent = self.parent.bind(py).borrow();
        parent.ensure_open()?;
        let frames = page_frames(&snapshot(&parent)?, self.index)?;
        let (_, h) = crop_dims(frames.crop);
        Ok(PyCompatMatrix {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: -1.0,
            e: 0.0,
            f: h,
        })
    }

    /// Content object numbers.
    fn get_contents(&self, py: Python<'_>) -> PyResult<Vec<i32>> {
        let parent = self.parent.bind(py).borrow();
        parent.ensure_open()?;
        Ok(content_ids(&snapshot(&parent)?, self.index)?
            .into_iter()
            .map(|id| id.0 as i32)
            .collect())
    }

    /// Retained rendering of the page.
    #[pyo3(signature = (annots=1))]
    fn get_displaylist(&self, py: Python<'_>, annots: i32) -> PyResult<PyDisplayList> {
        let parent = self.parent.bind(py).borrow();
        parent.ensure_open()?;
        let _ = annots;
        Ok(PyDisplayList {
            rect: self.rect(py)?,
        })
    }

    /// Page label.
    fn get_label(&self, py: Python<'_>) -> PyResult<String> {
        let parent = self.parent.bind(py).borrow();
        parent.ensure_open()?;
        Ok(display_labels(&snapshot(&parent)?)
            .get(self.index)
            .cloned()
            .unwrap_or_default())
    }

    /// Reflowed layout, if any (fixed-layout PDF has none).
    #[pyo3(signature = (*args, **kwargs))]
    fn get_layout(
        &self,
        py: Python<'_>,
        args: &Bound<'_, pyo3::types::PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<PyObject> {
        let parent = self.parent.bind(py).borrow();
        parent.ensure_open()?;
        let _ = (args.len(), kwargs.is_some());
        Ok(py.None())
    }

    /// Optional-content items.
    fn get_oc_items(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        let parent = self.parent.bind(py).borrow();
        parent.ensure_open()?;
        Ok(Vec::new())
    }

    /// Whether contents are wrapped.
    #[getter]
    fn is_wrapped(&self, py: Python<'_>) -> PyResult<bool> {
        let parent = self.parent.bind(py).borrow();
        parent.ensure_open()?;
        let parsed = snapshot(&parent)?;
        let ids = content_ids(&parsed, self.index)?;
        if ids.is_empty() {
            return Ok(false);
        }
        for id in ids {
            let bytes = parsed
                .get_object(id)
                .ok()
                .and_then(|o| o.as_stream().ok())
                .and_then(|s| s.decompressed_content().ok())
                .unwrap_or_default();
            if !stream_wrapped(&bytes) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Page language.
    #[getter]
    fn language(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let parent = self.parent.bind(py).borrow();
        parent.ensure_open()?;
        let parsed = snapshot(&parent)?;
        let ids = page_ids(&parsed);
        let id = *ids.get(self.index).ok_or_else(|| {
            PyIndexError::new_err(format!("page index {} out of range", self.index))
        })?;
        Ok(inherited(&parsed, id, b"Lang").and_then(|obj| match obj {
            LObject::String(bytes, _) => Some(String::from_utf8_lossy(&bytes).into_owned()),
            _ => None,
        }))
    }

    /// Content stream bytes.
    fn read_contents<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let parent = self.parent.bind(py).borrow();
        parent.ensure_open()?;
        let parsed = snapshot(&parent)?;
        let mut out = Vec::new();
        for id in content_ids(&parsed, self.index)? {
            let bytes = parsed
                .get_object(id)
                .ok()
                .and_then(|o| o.as_stream().ok())
                .and_then(|s| s.decompressed_content().ok())
                .unwrap_or_default();
            if !out.is_empty() {
                out.push(b'\n');
            }
            out.extend_from_slice(&bytes);
        }
        Ok(PyBytes::new(py, &out))
    }

    /// Page object number.
    #[getter]
    fn xref(&self, py: Python<'_>) -> PyResult<i32> {
        let parent = self.parent.bind(py).borrow();
        parent.ensure_open()?;
        page_ids(&snapshot(&parent)?)
            .get(self.index)
            .copied()
            .map(|id| id.0 as i32)
            .ok_or_else(|| PyIndexError::new_err("page index out of range"))
    }

    /// Box change log: line-metric boxes in content-stream order.
    #[pyo3(signature = (layers=None))]
    fn get_bboxlog(&self, py: Python<'_>, layers: Option<&Bound<'_, PyAny>>) -> PyResult<PyObject> {
        let parent = self.parent.bind(py).borrow();
        parent.ensure_open()?;
        let bytes = parent.current_bytes()?;
        let facade = FacadePdfDocument::from_bytes(&bytes)
            .map_err(|err| member_error(Member::GetBboxlog, err))?;
        let page = FacadePage::new(&facade, self.index);
        let layers = layers
            .map(bboxlog_layers)
            .transpose()?
            .filter(|found| !matches!(found, FacadeValue::Null));
        let value = page
            .get_bboxlog(layers.as_ref())
            .map_err(|err| member_error(Member::GetBboxlog, err))?;
        super::edit::facade_value_to_py(py, &value)
    }

    /// Replay a retained rendering.
    fn run(&self, py: Python<'_>, dw: &Bound<'_, PyAny>, m: &Bound<'_, PyAny>) -> PyResult<()> {
        let parent = self.parent.bind(py).borrow();
        parent.ensure_open()?;
        let _ = m;
        let _ = dw.getattr("device")?;
        Err(PyRuntimeError::new_err("page replay is not supported"))
    }

    /// Recolor direct colour operators to a component count.
    #[pyo3(signature = (components=1))]
    fn recolor(&self, py: Python<'_>, components: i32) -> PyResult<()> {
        let parent = self.parent.bind(py).borrow();
        parent.ensure_open()?;
        if ![1, 3, 4].contains(&components) {
            return Err(validation_error(
                ValidationKind::DrawArg,
                format!("bad component count {components}"),
            ));
        }
        recolor_page_on(&parent, self.index, components)
    }

    /// Reload the page view.
    fn refresh(&self, py: Python<'_>) -> PyResult<()> {
        let parent = self.parent.bind(py).borrow();
        parent.ensure_open()?;
        self.mark_compat_refreshed();
        Ok(())
    }

    /// Clip contents to a rectangle.
    fn clip_to_rect(&self, py: Python<'_>, rect: &Bound<'_, PyAny>) -> PyResult<()> {
        let edges = parse_rect_arg(rect).map_err(|_| {
            validation_error(ValidationKind::Precondition, "rect must be rect-like")
        })?;
        let parent = self.parent.bind(py).borrow();
        parent.ensure_open()?;
        let frames = page_frames(&snapshot(&parent)?, self.index)?;
        let top = frames.crop[3].max(frames.crop[1]);
        let x = edges[0].min(edges[2]);
        let w = (edges[2] - edges[0]).abs();
        let y_bottom = top - edges[3].max(edges[1]);
        let h = (edges[3] - edges[1]).abs();
        let prefix = format!("q {x} {y_bottom} {w} {h} re W n\n");
        splice_page_contents(&parent, self.index, prefix.as_bytes(), b"Q\n")?;
        self.set_compat_clip(edges);
        Ok(())
    }

    /// Wrap contents for isolated drawing.
    fn wrap_contents(&self, py: Python<'_>) -> PyResult<()> {
        let parent = self.parent.bind(py).borrow();
        parent.ensure_open()?;
        splice_page_contents(&parent, self.index, b"q\n", b"Q\n")
    }
}

/// Whether a content stream opens with `q` and closes with `Q`.
fn stream_wrapped(bytes: &[u8]) -> bool {
    let rest = bytes
        .strip_prefix(b"q\n".as_slice())
        .or_else(|| bytes.strip_prefix(b"q\r\n".as_slice()))
        .or_else(|| bytes.strip_prefix(b"q ".as_slice()));
    let Some(rest) = rest else {
        return false;
    };
    rest.iter()
        .rev()
        .find(|byte| !byte.is_ascii_whitespace())
        .is_some_and(|byte| *byte == b'Q')
}

/// Recolor one page's content streams through the document handle.
fn recolor_page_on(parent: &PyDocument, index: usize, components: i32) -> PyResult<()> {
    let mut guard = parent.init_lopdf()?;
    let handle = guard.as_mut().expect("initialised above");
    recolor_page_contents(handle, index, components)?;
    let mut buf = Vec::new();
    handle
        .save_to(&mut buf)
        .map_err(|e| PdfluentIoError::new_err(format!("serialise failed: {e}")))?;
    drop(guard);
    reparse_engine(parent, &buf)?;
    parent.mark_dirty();
    Ok(())
}

/// Recolor one page's content streams in place.
fn recolor_page_contents(
    handle: &mut LopdfDocument,
    index: usize,
    components: i32,
) -> PyResult<()> {
    let ids = page_ids(handle);
    let id = *ids
        .get(index)
        .ok_or_else(|| PyIndexError::new_err(format!("page index {index} out of range")))?;
    let streams: Vec<lopdf::ObjectId> = match inherited(handle, id, b"Contents") {
        Some(LObject::Reference(ref_id)) => vec![ref_id],
        Some(LObject::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_reference().ok())
            .collect(),
        _ => Vec::new(),
    };
    for stream_id in streams {
        let stream = handle
            .get_object_mut(stream_id)
            .ok()
            .and_then(|o| o.as_stream_mut().ok())
            .ok_or_else(|| PyValueError::new_err("page content is no stream"))?;
        let content = stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone());
        stream.set_plain_content(recolor_stream(&content, components));
    }
    Ok(())
}

/// Recolor one content stream's direct colour operators, passing the rest
/// through byte-identical (named colourspaces stay untouched).
fn recolor_stream(content: &[u8], components: i32) -> Vec<u8> {
    use pdf_content_stream::ContentStreamParser;
    let (ops, _) = ContentStreamParser::new(content).collect_ops_lenient();
    if ops.is_empty() {
        return content.to_vec();
    }
    let mut out = Vec::with_capacity(content.len() + 16);
    let mut cursor = 0;
    for parsed in &ops {
        let start = parsed.byte_start.min(content.len());
        let end = parsed.byte_end.min(content.len());
        out.extend_from_slice(&content[cursor.min(start)..start]);
        match recolor_op(&parsed.op, components) {
            Some(replacement) => {
                let raw = &content[start..end];
                let ws = raw
                    .iter()
                    .take_while(|byte| byte.is_ascii_whitespace())
                    .count();
                out.extend_from_slice(&raw[..ws]);
                out.extend_from_slice(replacement.as_bytes());
            }
            None => out.extend_from_slice(&content[start..end]),
        }
        cursor = end.max(cursor);
    }
    out.extend_from_slice(&content[cursor.min(content.len())..]);
    out
}

/// Replacement source for one colour operator, if it changes spaces.
fn recolor_op(op: &pdf_content_stream::ContentOp, components: i32) -> Option<String> {
    use pdf_content_stream::ContentOp as Op;
    let (fill, from, values) = match *op {
        Op::SetFillGray(v) => (true, 1, vec![v]),
        Op::SetFillRgb(r, g, b) => (true, 3, vec![r, g, b]),
        Op::SetFillCmyk(c, m, y, k) => (true, 4, vec![c, m, y, k]),
        Op::SetStrokeGray(v) => (false, 1, vec![v]),
        Op::SetStrokeRgb(r, g, b) => (false, 3, vec![r, g, b]),
        Op::SetStrokeCmyk(c, m, y, k) => (false, 4, vec![c, m, y, k]),
        _ => return None,
    };
    if from == components {
        return None;
    }
    let converted = convert_color(&values, from, components);
    let name = match (fill, components) {
        (true, 1) => "g",
        (true, 3) => "rg",
        (true, _) => "k",
        (false, 1) => "G",
        (false, 3) => "RG",
        (false, _) => "K",
    };
    let mut out = converted
        .iter()
        .map(|v| color_num(*v))
        .collect::<Vec<_>>()
        .join(" ");
    out.push(' ');
    out.push_str(name);
    Some(out)
}

/// Convert one colour between component counts.
fn convert_color(values: &[f32], from: i32, to: i32) -> Vec<f32> {
    fn lum(r: f32, g: f32, b: f32) -> f32 {
        0.299 * r + 0.587 * g + 0.114 * b
    }
    let (r, g, b) = match from {
        1 => {
            let v = values.first().copied().unwrap_or(0.0);
            (v, v, v)
        }
        3 => (
            values.first().copied().unwrap_or(0.0),
            values.get(1).copied().unwrap_or(0.0),
            values.get(2).copied().unwrap_or(0.0),
        ),
        _ => {
            let (c, m, y, k) = (
                values.first().copied().unwrap_or(0.0),
                values.get(1).copied().unwrap_or(0.0),
                values.get(2).copied().unwrap_or(0.0),
                values.get(3).copied().unwrap_or(0.0),
            );
            (
                1.0 - (c + k).min(1.0),
                1.0 - (m + k).min(1.0),
                1.0 - (y + k).min(1.0),
            )
        }
    };
    match to {
        1 => vec![lum(r, g, b).clamp(0.0, 1.0)],
        3 => vec![r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0)],
        _ => {
            let k = (1.0 - r.max(g).max(b)).clamp(0.0, 1.0);
            if k >= 1.0 {
                vec![0.0, 0.0, 0.0, 1.0]
            } else {
                vec![
                    ((1.0 - r - k) / (1.0 - k)).clamp(0.0, 1.0),
                    ((1.0 - g - k) / (1.0 - k)).clamp(0.0, 1.0),
                    ((1.0 - b - k) / (1.0 - k)).clamp(0.0, 1.0),
                    k,
                ]
            }
        }
    }
}

/// One colour number, shortest round-trip form.
fn color_num(value: f32) -> String {
    let rounded = ((value.clamp(0.0, 1.0) * 10_000.0).round() / 10_000.0) as f64;
    let mut text = format!("{rounded:.4}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    if text.is_empty() {
        text.push('0');
    }
    text
}

/// Surround every content stream of a page with two byte runs.
fn splice_page_contents(
    parent: &PyDocument,
    index: usize,
    prefix: &[u8],
    suffix: &[u8],
) -> PyResult<()> {
    let mut guard = parent.init_lopdf()?;
    let handle = guard.as_mut().expect("initialised above");
    let ids = page_ids(handle);
    let id = *ids
        .get(index)
        .ok_or_else(|| PyIndexError::new_err(format!("page index {index} out of range")))?;
    let streams: Vec<lopdf::ObjectId> = match inherited(handle, id, b"Contents") {
        Some(LObject::Reference(ref_id)) => vec![ref_id],
        Some(LObject::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_reference().ok())
            .collect(),
        _ => Vec::new(),
    };
    for stream_id in streams {
        let stream = handle
            .get_object_mut(stream_id)
            .ok()
            .and_then(|o| o.as_stream_mut().ok())
            .ok_or_else(|| PyValueError::new_err("page content is no stream"))?;
        let mut content = stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone());
        let mut joined = Vec::with_capacity(prefix.len() + content.len() + suffix.len());
        joined.extend_from_slice(prefix);
        joined.append(&mut content);
        joined.extend_from_slice(suffix);
        stream.set_content(joined);
    }
    let mut buf = Vec::new();
    handle
        .save_to(&mut buf)
        .map_err(|e| PdfluentIoError::new_err(format!("serialise failed: {e}")))?;
    drop(guard);
    reparse_engine(parent, &buf)?;
    parent.mark_dirty();
    Ok(())
}
/// Register a name only when no earlier lane claimed it: the value-type and
/// image lanes register the real geometry/colourspace objects ahead of this
/// lane, and their instances must win over the stand-ins below.
fn set_if_absent<'py>(
    m: &Bound<'py, PyModule>,
    name: &str,
    value: impl IntoPyObject<'py>,
) -> PyResult<()> {
    if m.getattr(name).is_err() {
        m.setattr(name, value)?;
    }
    Ok(())
}

/// Glyph table rows with the recorded int/float shape: whole widths export
/// as int. The recording pins int 1 at symbol entries 188/190 and holds no
/// whole-valued floats, so nothing else changes shape.
fn glyph_rows(py: Python<'_>, table: &[(i32, f64)]) -> PyResult<Vec<PyObject>> {
    table
        .iter()
        .map(|(code, width)| {
            let number: PyObject = if width.fract() == 0.0 {
                (*width as i32)
                    .into_pyobject(py)
                    .map(|found| found.into_any().unbind())?
            } else {
                (*width)
                    .into_pyobject(py)
                    .map(|found| found.into_any().unbind())?
            };
            (*code, number)
                .into_pyobject(py)
                .map(|found| found.into_any().unbind())
        })
        .collect()
}

/// Register every module constant this lane owns, generated from
/// `pdfluent::compat::constants`. Names a lane registers as typed objects
/// (geometry's `Identity`, the image lane's colours-space presets and its
/// plain codes, the text lane's flags) are absent here on purpose: each name
/// has exactly one writer, so the typed objects win whatever order the
/// lanes register in.
fn register_constants(m: &Bound<'_, PyModule>) -> PyResult<()> {
    use pdfluent::compat::constants as rust;
    set_if_absent(m, "EPSILON", rust::EPSILON)?;
    set_if_absent(m, "FLT_EPSILON", rust::FLT_EPSILON)?;
    set_if_absent(m, "FZ_MAX_INF_RECT", rust::FZ_MAX_INF_RECT)?;
    set_if_absent(m, "FZ_MIN_INF_RECT", rust::FZ_MIN_INF_RECT)?;

    set_if_absent(m, "TEXT_ACCURATE_ASCENDERS", rust::TEXT_ACCURATE_ASCENDERS)?;
    set_if_absent(m, "TEXT_ACCURATE_BBOXES", rust::TEXT_ACCURATE_BBOXES)?;
    set_if_absent(
        m,
        "TEXT_ACCURATE_SIDE_BEARINGS",
        rust::TEXT_ACCURATE_SIDE_BEARINGS,
    )?;
    set_if_absent(m, "TEXT_ALIGN_CENTER", rust::TEXT_ALIGN_CENTER)?;
    set_if_absent(m, "TEXT_ALIGN_JUSTIFY", rust::TEXT_ALIGN_JUSTIFY)?;
    set_if_absent(m, "TEXT_ALIGN_LEFT", rust::TEXT_ALIGN_LEFT)?;
    set_if_absent(m, "TEXT_ALIGN_RIGHT", rust::TEXT_ALIGN_RIGHT)?;
    set_if_absent(
        m,
        "TEXT_CID_FOR_UNKNOWN_UNICODE",
        rust::TEXT_CID_FOR_UNKNOWN_UNICODE,
    )?;
    set_if_absent(m, "TEXT_CLIP", rust::TEXT_CLIP)?;
    set_if_absent(m, "TEXT_CLIP_RECT", rust::TEXT_CLIP_RECT)?;
    set_if_absent(m, "TEXT_COLLECT_STRUCTURE", rust::TEXT_COLLECT_STRUCTURE)?;
    set_if_absent(m, "TEXT_COLLECT_STYLES", rust::TEXT_COLLECT_STYLES)?;
    set_if_absent(m, "TEXT_COLLECT_VECTORS", rust::TEXT_COLLECT_VECTORS)?;

    set_if_absent(m, "TEXT_ENCODING_CYRILLIC", rust::TEXT_ENCODING_CYRILLIC)?;
    set_if_absent(m, "TEXT_ENCODING_GREEK", rust::TEXT_ENCODING_GREEK)?;
    set_if_absent(m, "TEXT_ENCODING_LATIN", rust::TEXT_ENCODING_LATIN)?;
    set_if_absent(m, "TEXT_FONT_BOLD", rust::TEXT_FONT_BOLD)?;
    set_if_absent(m, "TEXT_FONT_ITALIC", rust::TEXT_FONT_ITALIC)?;
    set_if_absent(m, "TEXT_FONT_MONOSPACED", rust::TEXT_FONT_MONOSPACED)?;
    set_if_absent(m, "TEXT_FONT_SERIFED", rust::TEXT_FONT_SERIFED)?;
    set_if_absent(m, "TEXT_FONT_SUPERSCRIPT", rust::TEXT_FONT_SUPERSCRIPT)?;
    set_if_absent(m, "TEXT_FUZZY_VECTORS", rust::TEXT_FUZZY_VECTORS)?;
    set_if_absent(m, "TEXT_IGNORE_ACTUALTEXT", rust::TEXT_IGNORE_ACTUALTEXT)?;

    set_if_absent(m, "TEXT_LAZY_VECTORS", rust::TEXT_LAZY_VECTORS)?;
    set_if_absent(m, "TEXT_MEDIABOX_CLIP", rust::TEXT_MEDIABOX_CLIP)?;
    set_if_absent(m, "TEXT_OUTPUT_HTML", rust::TEXT_OUTPUT_HTML)?;
    set_if_absent(m, "TEXT_OUTPUT_JSON", rust::TEXT_OUTPUT_JSON)?;
    set_if_absent(m, "TEXT_OUTPUT_TEXT", rust::TEXT_OUTPUT_TEXT)?;
    set_if_absent(m, "TEXT_OUTPUT_XHTML", rust::TEXT_OUTPUT_XHTML)?;
    set_if_absent(m, "TEXT_OUTPUT_XML", rust::TEXT_OUTPUT_XML)?;
    set_if_absent(m, "TEXT_PARAGRAPH_BREAK", rust::TEXT_PARAGRAPH_BREAK)?;
    set_if_absent(m, "TEXT_PRESERVE_IMAGES", rust::TEXT_PRESERVE_IMAGES)?;
    set_if_absent(m, "TEXT_PRESERVE_LIGATURES", rust::TEXT_PRESERVE_LIGATURES)?;
    set_if_absent(m, "TEXT_PRESERVE_SPANS", rust::TEXT_PRESERVE_SPANS)?;
    set_if_absent(
        m,
        "TEXT_PRESERVE_WHITESPACE",
        rust::TEXT_PRESERVE_WHITESPACE,
    )?;
    set_if_absent(m, "TEXT_SEGMENT", rust::TEXT_SEGMENT)?;
    set_if_absent(m, "TEXT_STEXT_SEGMENT", rust::TEXT_STEXT_SEGMENT)?;
    set_if_absent(m, "TEXT_TABLE_HUNT", rust::TEXT_TABLE_HUNT)?;
    set_if_absent(
        m,
        "TEXT_USE_CID_FOR_UNKNOWN_UNICODE",
        rust::TEXT_USE_CID_FOR_UNKNOWN_UNICODE,
    )?;
    set_if_absent(
        m,
        "TEXT_USE_GID_FOR_UNKNOWN_UNICODE",
        rust::TEXT_USE_GID_FOR_UNKNOWN_UNICODE,
    )?;
    set_if_absent(m, "TEXTFLAGS_BLOCKS", rust::TEXTFLAGS_BLOCKS)?;
    set_if_absent(m, "TEXTFLAGS_DICT", rust::TEXTFLAGS_DICT)?;
    set_if_absent(m, "TEXTFLAGS_HTML", rust::TEXTFLAGS_HTML)?;
    set_if_absent(m, "TEXTFLAGS_RAWDICT", rust::TEXTFLAGS_RAWDICT)?;
    set_if_absent(m, "TEXTFLAGS_WORDS", rust::TEXTFLAGS_WORDS)?;
    set_if_absent(m, "TEXTFLAGS_XHTML", rust::TEXTFLAGS_XHTML)?;
    set_if_absent(m, "TEXTFLAGS_XML", rust::TEXTFLAGS_XML)?;
    set_if_absent(m, "dictkey_align", rust::dictkey_align)?;
    set_if_absent(m, "dictkey_asc", rust::dictkey_asc)?;
    set_if_absent(m, "dictkey_bbox", rust::dictkey_bbox)?;
    set_if_absent(m, "dictkey_bidi", rust::dictkey_bidi)?;
    set_if_absent(m, "dictkey_blocks", rust::dictkey_blocks)?;
    set_if_absent(m, "dictkey_bpc", rust::dictkey_bpc)?;
    set_if_absent(m, "dictkey_c", rust::dictkey_c)?;
    set_if_absent(m, "dictkey_char_flags", rust::dictkey_char_flags)?;
    set_if_absent(m, "dictkey_chars", rust::dictkey_chars)?;
    set_if_absent(m, "dictkey_color", rust::dictkey_color)?;
    set_if_absent(m, "dictkey_colorspace", rust::dictkey_colorspace)?;
    set_if_absent(m, "dictkey_content", rust::dictkey_content)?;
    set_if_absent(m, "dictkey_creationDate", rust::dictkey_creationDate)?;
    set_if_absent(m, "dictkey_cs_name", rust::dictkey_cs_name)?;
    set_if_absent(m, "dictkey_da", rust::dictkey_da)?;
    set_if_absent(m, "dictkey_dashes", rust::dictkey_dashes)?;
    set_if_absent(m, "dictkey_desc", rust::dictkey_desc)?;
    set_if_absent(m, "dictkey_descr", rust::dictkey_descr)?;
    set_if_absent(m, "dictkey_dir", rust::dictkey_dir)?;
    set_if_absent(m, "dictkey_effect", rust::dictkey_effect)?;
    set_if_absent(m, "dictkey_ext", rust::dictkey_ext)?;
    set_if_absent(m, "dictkey_filename", rust::dictkey_filename)?;
    set_if_absent(m, "dictkey_fill", rust::dictkey_fill)?;
    set_if_absent(m, "dictkey_flags", rust::dictkey_flags)?;
    set_if_absent(m, "dictkey_font", rust::dictkey_font)?;
    set_if_absent(m, "dictkey_glyph", rust::dictkey_glyph)?;
    set_if_absent(m, "dictkey_height", rust::dictkey_height)?;
    set_if_absent(m, "dictkey_id", rust::dictkey_id)?;
    set_if_absent(m, "dictkey_image", rust::dictkey_image)?;
    set_if_absent(m, "dictkey_items", rust::dictkey_items)?;
    set_if_absent(m, "dictkey_length", rust::dictkey_length)?;
    set_if_absent(m, "dictkey_lines", rust::dictkey_lines)?;
    set_if_absent(m, "dictkey_matrix", rust::dictkey_matrix)?;
    set_if_absent(m, "dictkey_modDate", rust::dictkey_modDate)?;
    set_if_absent(m, "dictkey_name", rust::dictkey_name)?;
    set_if_absent(m, "dictkey_number", rust::dictkey_number)?;
    set_if_absent(m, "dictkey_origin", rust::dictkey_origin)?;
    set_if_absent(m, "dictkey_rect", rust::dictkey_rect)?;
    set_if_absent(m, "dictkey_size", rust::dictkey_size)?;
    set_if_absent(m, "dictkey_smask", rust::dictkey_smask)?;
    set_if_absent(m, "dictkey_spans", rust::dictkey_spans)?;
    set_if_absent(m, "dictkey_stroke", rust::dictkey_stroke)?;
    set_if_absent(m, "dictkey_style", rust::dictkey_style)?;
    set_if_absent(m, "dictkey_subject", rust::dictkey_subject)?;
    set_if_absent(m, "dictkey_text", rust::dictkey_text)?;
    set_if_absent(m, "dictkey_title", rust::dictkey_title)?;
    set_if_absent(m, "dictkey_type", rust::dictkey_type)?;
    set_if_absent(m, "dictkey_ufilename", rust::dictkey_ufilename)?;
    set_if_absent(m, "dictkey_width", rust::dictkey_width)?;
    set_if_absent(m, "dictkey_wmode", rust::dictkey_wmode)?;
    set_if_absent(m, "dictkey_xref", rust::dictkey_xref)?;
    set_if_absent(m, "dictkey_xres", rust::dictkey_xres)?;
    set_if_absent(m, "dictkey_yres", rust::dictkey_yres)?;
    set_if_absent(m, "PDF_STRUCT_BROKEN", rust::PDF_STRUCT_BROKEN)?;
    set_if_absent(m, "PDF_STRUCT_FIXED", rust::PDF_STRUCT_FIXED)?;
    set_if_absent(
        m,
        "PDF_STRUCT_HAS_ATTRIBUTES",
        rust::PDF_STRUCT_HAS_ATTRIBUTES,
    )?;
    set_if_absent(m, "PDF_STRUCT_HAS_CYCLE", rust::PDF_STRUCT_HAS_CYCLE)?;
    set_if_absent(
        m,
        "PDF_STRUCT_HAS_TABLE_ATTRIBUTES",
        rust::PDF_STRUCT_HAS_TABLE_ATTRIBUTES,
    )?;
    set_if_absent(
        m,
        "PDF_STRUCT_HAS_TABLE_SPAN_ATTRIBUTES",
        rust::PDF_STRUCT_HAS_TABLE_SPAN_ATTRIBUTES,
    )?;
    set_if_absent(m, "PDF_STRUCT_NOT_PRESENT", rust::PDF_STRUCT_NOT_PRESENT)?;
    set_if_absent(m, "PDF_STRUCT_PRESENT", rust::PDF_STRUCT_PRESENT)?;
    set_if_absent(m, "LINK_FLAG_B_VALID", rust::LINK_FLAG_B_VALID)?;
    set_if_absent(m, "LINK_FLAG_FIT_H", rust::LINK_FLAG_FIT_H)?;
    set_if_absent(m, "LINK_FLAG_FIT_V", rust::LINK_FLAG_FIT_V)?;
    set_if_absent(m, "LINK_FLAG_L_VALID", rust::LINK_FLAG_L_VALID)?;
    set_if_absent(m, "LINK_FLAG_R_IS_ZOOM", rust::LINK_FLAG_R_IS_ZOOM)?;
    set_if_absent(m, "LINK_FLAG_R_VALID", rust::LINK_FLAG_R_VALID)?;
    set_if_absent(m, "LINK_FLAG_T_VALID", rust::LINK_FLAG_T_VALID)?;
    set_if_absent(m, "LINK_GOTO", rust::LINK_GOTO)?;
    set_if_absent(m, "LINK_GOTOR", rust::LINK_GOTOR)?;
    set_if_absent(m, "LINK_LAUNCH", rust::LINK_LAUNCH)?;
    set_if_absent(m, "LINK_NAMED", rust::LINK_NAMED)?;
    set_if_absent(m, "LINK_NONE", rust::LINK_NONE)?;
    set_if_absent(m, "LINK_URI", rust::LINK_URI)?;
    set_if_absent(m, "PDF_ENCRYPT_AES_128", rust::PDF_ENCRYPT_AES_128)?;
    set_if_absent(m, "PDF_ENCRYPT_AES_256", rust::PDF_ENCRYPT_AES_256)?;
    set_if_absent(m, "PDF_ENCRYPT_KEEP", rust::PDF_ENCRYPT_KEEP)?;
    set_if_absent(m, "PDF_ENCRYPT_NONE", rust::PDF_ENCRYPT_NONE)?;
    set_if_absent(m, "PDF_ENCRYPT_RC4_128", rust::PDF_ENCRYPT_RC4_128)?;
    set_if_absent(m, "PDF_ENCRYPT_RC4_40", rust::PDF_ENCRYPT_RC4_40)?;
    set_if_absent(m, "PDF_ENCRYPT_UNKNOWN", rust::PDF_ENCRYPT_UNKNOWN)?;
    set_if_absent(m, "PDF_PERM_ACCESSIBILITY", rust::PDF_PERM_ACCESSIBILITY)?;
    set_if_absent(m, "PDF_PERM_ANNOTATE", rust::PDF_PERM_ANNOTATE)?;
    set_if_absent(m, "PDF_PERM_ASSEMBLE", rust::PDF_PERM_ASSEMBLE)?;
    set_if_absent(m, "PDF_PERM_COPY", rust::PDF_PERM_COPY)?;
    set_if_absent(m, "PDF_PERM_FORM", rust::PDF_PERM_FORM)?;
    set_if_absent(m, "PDF_PERM_MODIFY", rust::PDF_PERM_MODIFY)?;
    set_if_absent(m, "PDF_PERM_PRINT", rust::PDF_PERM_PRINT)?;
    set_if_absent(m, "PDF_PERM_PRINT_HQ", rust::PDF_PERM_PRINT_HQ)?;
    set_if_absent(m, "PDF_PAGE_LABEL_ALPHA_LC", rust::PDF_PAGE_LABEL_ALPHA_LC)?;
    set_if_absent(m, "PDF_PAGE_LABEL_ALPHA_UC", rust::PDF_PAGE_LABEL_ALPHA_UC)?;
    set_if_absent(m, "PDF_PAGE_LABEL_DECIMAL", rust::PDF_PAGE_LABEL_DECIMAL)?;
    set_if_absent(m, "PDF_PAGE_LABEL_NONE", rust::PDF_PAGE_LABEL_NONE)?;
    set_if_absent(m, "PDF_PAGE_LABEL_ROMAN_LC", rust::PDF_PAGE_LABEL_ROMAN_LC)?;
    set_if_absent(m, "PDF_PAGE_LABEL_ROMAN_UC", rust::PDF_PAGE_LABEL_ROMAN_UC)?;
    set_if_absent(
        m,
        "PDF_SIGNATURE_DEFAULT_APPEARANCE",
        rust::PDF_SIGNATURE_DEFAULT_APPEARANCE,
    )?;
    set_if_absent(
        m,
        "PDF_SIGNATURE_ERROR_DIGEST_FAILURE",
        rust::PDF_SIGNATURE_ERROR_DIGEST_FAILURE,
    )?;
    set_if_absent(
        m,
        "PDF_SIGNATURE_ERROR_NOT_SIGNED",
        rust::PDF_SIGNATURE_ERROR_NOT_SIGNED,
    )?;
    set_if_absent(
        m,
        "PDF_SIGNATURE_ERROR_NOT_TRUSTED",
        rust::PDF_SIGNATURE_ERROR_NOT_TRUSTED,
    )?;
    set_if_absent(
        m,
        "PDF_SIGNATURE_ERROR_NO_CERTIFICATE",
        rust::PDF_SIGNATURE_ERROR_NO_CERTIFICATE,
    )?;
    set_if_absent(
        m,
        "PDF_SIGNATURE_ERROR_NO_SIGNATURES",
        rust::PDF_SIGNATURE_ERROR_NO_SIGNATURES,
    )?;
    set_if_absent(
        m,
        "PDF_SIGNATURE_ERROR_OKAY",
        rust::PDF_SIGNATURE_ERROR_OKAY,
    )?;
    set_if_absent(
        m,
        "PDF_SIGNATURE_ERROR_SELF_SIGNED",
        rust::PDF_SIGNATURE_ERROR_SELF_SIGNED,
    )?;
    set_if_absent(
        m,
        "PDF_SIGNATURE_ERROR_SELF_SIGNED_IN_CHAIN",
        rust::PDF_SIGNATURE_ERROR_SELF_SIGNED_IN_CHAIN,
    )?;
    set_if_absent(
        m,
        "PDF_SIGNATURE_ERROR_UNKNOWN",
        rust::PDF_SIGNATURE_ERROR_UNKNOWN,
    )?;
    set_if_absent(m, "PDF_SIGNATURE_SHOW_DATE", rust::PDF_SIGNATURE_SHOW_DATE)?;
    set_if_absent(m, "PDF_SIGNATURE_SHOW_DN", rust::PDF_SIGNATURE_SHOW_DN)?;
    set_if_absent(
        m,
        "PDF_SIGNATURE_SHOW_GRAPHIC_NAME",
        rust::PDF_SIGNATURE_SHOW_GRAPHIC_NAME,
    )?;
    set_if_absent(
        m,
        "PDF_SIGNATURE_SHOW_LABELS",
        rust::PDF_SIGNATURE_SHOW_LABELS,
    )?;
    set_if_absent(m, "PDF_SIGNATURE_SHOW_LOGO", rust::PDF_SIGNATURE_SHOW_LOGO)?;
    set_if_absent(
        m,
        "PDF_SIGNATURE_SHOW_TEXT_NAME",
        rust::PDF_SIGNATURE_SHOW_TEXT_NAME,
    )?;
    set_if_absent(m, "SigFlag_AppendOnly", rust::SigFlag_AppendOnly)?;
    set_if_absent(m, "SigFlag_SignaturesExist", rust::SigFlag_SignaturesExist)?;
    set_if_absent(m, "PDF_ALERT_BUTTON_CANCEL", rust::PDF_ALERT_BUTTON_CANCEL)?;
    set_if_absent(
        m,
        "PDF_ALERT_BUTTON_GROUP_OK",
        rust::PDF_ALERT_BUTTON_GROUP_OK,
    )?;
    set_if_absent(
        m,
        "PDF_ALERT_BUTTON_GROUP_OK_CANCEL",
        rust::PDF_ALERT_BUTTON_GROUP_OK_CANCEL,
    )?;
    set_if_absent(
        m,
        "PDF_ALERT_BUTTON_GROUP_YES_NO",
        rust::PDF_ALERT_BUTTON_GROUP_YES_NO,
    )?;
    set_if_absent(
        m,
        "PDF_ALERT_BUTTON_GROUP_YES_NO_CANCEL",
        rust::PDF_ALERT_BUTTON_GROUP_YES_NO_CANCEL,
    )?;
    set_if_absent(m, "PDF_ALERT_BUTTON_NO", rust::PDF_ALERT_BUTTON_NO)?;
    set_if_absent(m, "PDF_ALERT_BUTTON_NONE", rust::PDF_ALERT_BUTTON_NONE)?;
    set_if_absent(m, "PDF_ALERT_BUTTON_OK", rust::PDF_ALERT_BUTTON_OK)?;
    set_if_absent(m, "PDF_ALERT_BUTTON_YES", rust::PDF_ALERT_BUTTON_YES)?;
    set_if_absent(m, "PDF_ALERT_ICON_ERROR", rust::PDF_ALERT_ICON_ERROR)?;
    set_if_absent(m, "PDF_ALERT_ICON_QUESTION", rust::PDF_ALERT_ICON_QUESTION)?;
    set_if_absent(m, "PDF_ALERT_ICON_STATUS", rust::PDF_ALERT_ICON_STATUS)?;
    set_if_absent(m, "PDF_ALERT_ICON_WARNING", rust::PDF_ALERT_ICON_WARNING)?;
    set_if_absent(
        m,
        "PDF_DOCUMENT_EVENT_ALERT",
        rust::PDF_DOCUMENT_EVENT_ALERT,
    )?;
    set_if_absent(
        m,
        "PDF_DOCUMENT_EVENT_EXEC_MENU_ITEM",
        rust::PDF_DOCUMENT_EVENT_EXEC_MENU_ITEM,
    )?;
    set_if_absent(
        m,
        "PDF_DOCUMENT_EVENT_LAUNCH_URL",
        rust::PDF_DOCUMENT_EVENT_LAUNCH_URL,
    )?;
    set_if_absent(
        m,
        "PDF_DOCUMENT_EVENT_MAIL_DOC",
        rust::PDF_DOCUMENT_EVENT_MAIL_DOC,
    )?;
    set_if_absent(
        m,
        "PDF_DOCUMENT_EVENT_PRINT",
        rust::PDF_DOCUMENT_EVENT_PRINT,
    )?;
    set_if_absent(
        m,
        "PDF_DOCUMENT_EVENT_SUBMIT",
        rust::PDF_DOCUMENT_EVENT_SUBMIT,
    )?;
    set_if_absent(m, "PDF_LAYER_UI_CHECKBOX", rust::PDF_LAYER_UI_CHECKBOX)?;
    set_if_absent(m, "PDF_LAYER_UI_LABEL", rust::PDF_LAYER_UI_LABEL)?;
    set_if_absent(m, "PDF_LAYER_UI_RADIOBOX", rust::PDF_LAYER_UI_RADIOBOX)?;
    set_if_absent(m, "PDF_OC_OFF", rust::PDF_OC_OFF)?;
    set_if_absent(m, "PDF_OC_ON", rust::PDF_OC_ON)?;
    set_if_absent(m, "PDF_OC_TOGGLE", rust::PDF_OC_TOGGLE)?;
    set_if_absent(m, "PDF_NOT_ZUGFERD", rust::PDF_NOT_ZUGFERD)?;
    set_if_absent(m, "PDF_ZUGFERD_BASIC", rust::PDF_ZUGFERD_BASIC)?;
    set_if_absent(m, "PDF_ZUGFERD_BASIC_WL", rust::PDF_ZUGFERD_BASIC_WL)?;
    set_if_absent(m, "PDF_ZUGFERD_COMFORT", rust::PDF_ZUGFERD_COMFORT)?;
    set_if_absent(m, "PDF_ZUGFERD_EXTENDED", rust::PDF_ZUGFERD_EXTENDED)?;
    set_if_absent(m, "PDF_ZUGFERD_MINIMUM", rust::PDF_ZUGFERD_MINIMUM)?;
    set_if_absent(m, "PDF_ZUGFERD_UNKNOWN", rust::PDF_ZUGFERD_UNKNOWN)?;
    set_if_absent(m, "PDF_ZUGFERD_XRECHNUNG", rust::PDF_ZUGFERD_XRECHNUNG)?;
    set_if_absent(
        m,
        "PDF_CLEAN_STRUCTURE_DROP",
        rust::PDF_CLEAN_STRUCTURE_DROP,
    )?;
    set_if_absent(
        m,
        "PDF_CLEAN_STRUCTURE_KEEP",
        rust::PDF_CLEAN_STRUCTURE_KEEP,
    )?;
    set_if_absent(m, "PDF_CLEAN_VECTORIZE_NO", rust::PDF_CLEAN_VECTORIZE_NO)?;
    set_if_absent(m, "PDF_CLEAN_VECTORIZE_YES", rust::PDF_CLEAN_VECTORIZE_YES)?;
    set_if_absent(m, "PDF_ANNOT_3D", rust::PDF_ANNOT_3D)?;
    set_if_absent(m, "PDF_ANNOT_CARET", rust::PDF_ANNOT_CARET)?;
    set_if_absent(m, "PDF_ANNOT_CIRCLE", rust::PDF_ANNOT_CIRCLE)?;
    set_if_absent(
        m,
        "PDF_ANNOT_FILE_ATTACHMENT",
        rust::PDF_ANNOT_FILE_ATTACHMENT,
    )?;
    set_if_absent(m, "PDF_ANNOT_FREE_TEXT", rust::PDF_ANNOT_FREE_TEXT)?;
    set_if_absent(m, "PDF_ANNOT_HIGHLIGHT", rust::PDF_ANNOT_HIGHLIGHT)?;
    set_if_absent(m, "PDF_ANNOT_INK", rust::PDF_ANNOT_INK)?;
    set_if_absent(m, "PDF_ANNOT_IS_HIDDEN", rust::PDF_ANNOT_IS_HIDDEN)?;
    set_if_absent(m, "PDF_ANNOT_IS_INVISIBLE", rust::PDF_ANNOT_IS_INVISIBLE)?;
    set_if_absent(m, "PDF_ANNOT_IS_LOCKED", rust::PDF_ANNOT_IS_LOCKED)?;
    set_if_absent(
        m,
        "PDF_ANNOT_IS_LOCKED_CONTENTS",
        rust::PDF_ANNOT_IS_LOCKED_CONTENTS,
    )?;
    set_if_absent(m, "PDF_ANNOT_IS_NO_ROTATE", rust::PDF_ANNOT_IS_NO_ROTATE)?;
    set_if_absent(m, "PDF_ANNOT_IS_NO_VIEW", rust::PDF_ANNOT_IS_NO_VIEW)?;
    set_if_absent(m, "PDF_ANNOT_IS_NO_ZOOM", rust::PDF_ANNOT_IS_NO_ZOOM)?;
    set_if_absent(m, "PDF_ANNOT_IS_PRINT", rust::PDF_ANNOT_IS_PRINT)?;
    set_if_absent(m, "PDF_ANNOT_IS_READ_ONLY", rust::PDF_ANNOT_IS_READ_ONLY)?;
    set_if_absent(
        m,
        "PDF_ANNOT_IS_TOGGLE_NO_VIEW",
        rust::PDF_ANNOT_IS_TOGGLE_NO_VIEW,
    )?;
    set_if_absent(m, "PDF_ANNOT_IT_DEFAULT", rust::PDF_ANNOT_IT_DEFAULT)?;
    set_if_absent(
        m,
        "PDF_ANNOT_IT_FREETEXT_CALLOUT",
        rust::PDF_ANNOT_IT_FREETEXT_CALLOUT,
    )?;
    set_if_absent(
        m,
        "PDF_ANNOT_IT_FREETEXT_TYPEWRITER",
        rust::PDF_ANNOT_IT_FREETEXT_TYPEWRITER,
    )?;
    set_if_absent(m, "PDF_ANNOT_IT_LINE_ARROW", rust::PDF_ANNOT_IT_LINE_ARROW)?;
    set_if_absent(
        m,
        "PDF_ANNOT_IT_LINE_DIMENSION",
        rust::PDF_ANNOT_IT_LINE_DIMENSION,
    )?;
    set_if_absent(
        m,
        "PDF_ANNOT_IT_POLYGON_CLOUD",
        rust::PDF_ANNOT_IT_POLYGON_CLOUD,
    )?;
    set_if_absent(
        m,
        "PDF_ANNOT_IT_POLYGON_DIMENSION",
        rust::PDF_ANNOT_IT_POLYGON_DIMENSION,
    )?;
    set_if_absent(
        m,
        "PDF_ANNOT_IT_POLYLINE_DIMENSION",
        rust::PDF_ANNOT_IT_POLYLINE_DIMENSION,
    )?;
    set_if_absent(
        m,
        "PDF_ANNOT_IT_STAMP_IMAGE",
        rust::PDF_ANNOT_IT_STAMP_IMAGE,
    )?;
    set_if_absent(
        m,
        "PDF_ANNOT_IT_STAMP_SNAPSHOT",
        rust::PDF_ANNOT_IT_STAMP_SNAPSHOT,
    )?;
    set_if_absent(m, "PDF_ANNOT_IT_UNKNOWN", rust::PDF_ANNOT_IT_UNKNOWN)?;
    set_if_absent(m, "PDF_ANNOT_LE_BUTT", rust::PDF_ANNOT_LE_BUTT)?;
    set_if_absent(m, "PDF_ANNOT_LE_CIRCLE", rust::PDF_ANNOT_LE_CIRCLE)?;
    set_if_absent(
        m,
        "PDF_ANNOT_LE_CLOSED_ARROW",
        rust::PDF_ANNOT_LE_CLOSED_ARROW,
    )?;
    set_if_absent(m, "PDF_ANNOT_LE_DIAMOND", rust::PDF_ANNOT_LE_DIAMOND)?;
    set_if_absent(m, "PDF_ANNOT_LE_NONE", rust::PDF_ANNOT_LE_NONE)?;
    set_if_absent(m, "PDF_ANNOT_LE_OPEN_ARROW", rust::PDF_ANNOT_LE_OPEN_ARROW)?;
    set_if_absent(
        m,
        "PDF_ANNOT_LE_R_CLOSED_ARROW",
        rust::PDF_ANNOT_LE_R_CLOSED_ARROW,
    )?;
    set_if_absent(
        m,
        "PDF_ANNOT_LE_R_OPEN_ARROW",
        rust::PDF_ANNOT_LE_R_OPEN_ARROW,
    )?;
    set_if_absent(m, "PDF_ANNOT_LE_SLASH", rust::PDF_ANNOT_LE_SLASH)?;
    set_if_absent(m, "PDF_ANNOT_LE_SQUARE", rust::PDF_ANNOT_LE_SQUARE)?;
    set_if_absent(m, "PDF_ANNOT_LINE", rust::PDF_ANNOT_LINE)?;
    set_if_absent(m, "PDF_ANNOT_LINK", rust::PDF_ANNOT_LINK)?;
    set_if_absent(m, "PDF_ANNOT_MOVIE", rust::PDF_ANNOT_MOVIE)?;
    set_if_absent(m, "PDF_ANNOT_POLYGON", rust::PDF_ANNOT_POLYGON)?;
    set_if_absent(m, "PDF_ANNOT_POLY_LINE", rust::PDF_ANNOT_POLY_LINE)?;
    set_if_absent(m, "PDF_ANNOT_POPUP", rust::PDF_ANNOT_POPUP)?;
    set_if_absent(m, "PDF_ANNOT_PRINTER_MARK", rust::PDF_ANNOT_PRINTER_MARK)?;
    set_if_absent(m, "PDF_ANNOT_PROJECTION", rust::PDF_ANNOT_PROJECTION)?;
    set_if_absent(m, "PDF_ANNOT_Q_CENTER", rust::PDF_ANNOT_Q_CENTER)?;
    set_if_absent(m, "PDF_ANNOT_Q_LEFT", rust::PDF_ANNOT_Q_LEFT)?;
    set_if_absent(m, "PDF_ANNOT_Q_RIGHT", rust::PDF_ANNOT_Q_RIGHT)?;
    set_if_absent(m, "PDF_ANNOT_REDACT", rust::PDF_ANNOT_REDACT)?;
    set_if_absent(m, "PDF_ANNOT_RICH_MEDIA", rust::PDF_ANNOT_RICH_MEDIA)?;
    set_if_absent(m, "PDF_ANNOT_SCREEN", rust::PDF_ANNOT_SCREEN)?;
    set_if_absent(m, "PDF_ANNOT_SOUND", rust::PDF_ANNOT_SOUND)?;
    set_if_absent(m, "PDF_ANNOT_SQUARE", rust::PDF_ANNOT_SQUARE)?;
    set_if_absent(m, "PDF_ANNOT_SQUIGGLY", rust::PDF_ANNOT_SQUIGGLY)?;
    set_if_absent(m, "PDF_ANNOT_STAMP", rust::PDF_ANNOT_STAMP)?;
    set_if_absent(m, "PDF_ANNOT_STRIKE_OUT", rust::PDF_ANNOT_STRIKE_OUT)?;
    set_if_absent(m, "PDF_ANNOT_TEXT", rust::PDF_ANNOT_TEXT)?;
    set_if_absent(m, "PDF_ANNOT_TRAP_NET", rust::PDF_ANNOT_TRAP_NET)?;
    set_if_absent(m, "PDF_ANNOT_UNDERLINE", rust::PDF_ANNOT_UNDERLINE)?;
    set_if_absent(m, "PDF_ANNOT_UNKNOWN", rust::PDF_ANNOT_UNKNOWN)?;
    set_if_absent(m, "PDF_ANNOT_WATERMARK", rust::PDF_ANNOT_WATERMARK)?;
    set_if_absent(m, "PDF_ANNOT_WIDGET", rust::PDF_ANNOT_WIDGET)?;
    set_if_absent(
        m,
        "PDF_BORDER_EFFECT_CLOUDY",
        rust::PDF_BORDER_EFFECT_CLOUDY,
    )?;
    set_if_absent(m, "PDF_BORDER_EFFECT_NONE", rust::PDF_BORDER_EFFECT_NONE)?;
    set_if_absent(
        m,
        "PDF_BORDER_STYLE_BEVELED",
        rust::PDF_BORDER_STYLE_BEVELED,
    )?;
    set_if_absent(m, "PDF_BORDER_STYLE_DASHED", rust::PDF_BORDER_STYLE_DASHED)?;
    set_if_absent(m, "PDF_BORDER_STYLE_INSET", rust::PDF_BORDER_STYLE_INSET)?;
    set_if_absent(m, "PDF_BORDER_STYLE_SOLID", rust::PDF_BORDER_STYLE_SOLID)?;
    set_if_absent(
        m,
        "PDF_BORDER_STYLE_UNDERLINE",
        rust::PDF_BORDER_STYLE_UNDERLINE,
    )?;
    set_if_absent(
        m,
        "PDF_BTN_FIELD_IS_NO_TOGGLE_TO_OFF",
        rust::PDF_BTN_FIELD_IS_NO_TOGGLE_TO_OFF,
    )?;
    set_if_absent(
        m,
        "PDF_BTN_FIELD_IS_PUSHBUTTON",
        rust::PDF_BTN_FIELD_IS_PUSHBUTTON,
    )?;
    set_if_absent(m, "PDF_BTN_FIELD_IS_RADIO", rust::PDF_BTN_FIELD_IS_RADIO)?;
    set_if_absent(
        m,
        "PDF_BTN_FIELD_IS_RADIOS_IN_UNISON",
        rust::PDF_BTN_FIELD_IS_RADIOS_IN_UNISON,
    )?;
    set_if_absent(m, "PDF_CH_FIELD_IS_COMBO", rust::PDF_CH_FIELD_IS_COMBO)?;
    set_if_absent(
        m,
        "PDF_CH_FIELD_IS_COMMIT_ON_SEL_CHANGE",
        rust::PDF_CH_FIELD_IS_COMMIT_ON_SEL_CHANGE,
    )?;
    set_if_absent(
        m,
        "PDF_CH_FIELD_IS_DO_NOT_SPELL_CHECK",
        rust::PDF_CH_FIELD_IS_DO_NOT_SPELL_CHECK,
    )?;
    set_if_absent(m, "PDF_CH_FIELD_IS_EDIT", rust::PDF_CH_FIELD_IS_EDIT)?;
    set_if_absent(
        m,
        "PDF_CH_FIELD_IS_MULTI_SELECT",
        rust::PDF_CH_FIELD_IS_MULTI_SELECT,
    )?;
    set_if_absent(m, "PDF_CH_FIELD_IS_SORT", rust::PDF_CH_FIELD_IS_SORT)?;
    set_if_absent(m, "PDF_FIELD_IS_NO_EXPORT", rust::PDF_FIELD_IS_NO_EXPORT)?;
    set_if_absent(m, "PDF_FIELD_IS_READ_ONLY", rust::PDF_FIELD_IS_READ_ONLY)?;
    set_if_absent(m, "PDF_FIELD_IS_REQUIRED", rust::PDF_FIELD_IS_REQUIRED)?;
    set_if_absent(m, "PDF_TX_FIELD_IS_COMB", rust::PDF_TX_FIELD_IS_COMB)?;
    set_if_absent(
        m,
        "PDF_TX_FIELD_IS_DO_NOT_SCROLL",
        rust::PDF_TX_FIELD_IS_DO_NOT_SCROLL,
    )?;
    set_if_absent(
        m,
        "PDF_TX_FIELD_IS_DO_NOT_SPELL_CHECK",
        rust::PDF_TX_FIELD_IS_DO_NOT_SPELL_CHECK,
    )?;
    set_if_absent(
        m,
        "PDF_TX_FIELD_IS_FILE_SELECT",
        rust::PDF_TX_FIELD_IS_FILE_SELECT,
    )?;
    set_if_absent(
        m,
        "PDF_TX_FIELD_IS_MULTILINE",
        rust::PDF_TX_FIELD_IS_MULTILINE,
    )?;
    set_if_absent(
        m,
        "PDF_TX_FIELD_IS_PASSWORD",
        rust::PDF_TX_FIELD_IS_PASSWORD,
    )?;
    set_if_absent(
        m,
        "PDF_TX_FIELD_IS_RICH_TEXT",
        rust::PDF_TX_FIELD_IS_RICH_TEXT,
    )?;
    set_if_absent(
        m,
        "PDF_WIDGET_TX_FORMAT_DATE",
        rust::PDF_WIDGET_TX_FORMAT_DATE,
    )?;
    set_if_absent(
        m,
        "PDF_WIDGET_TX_FORMAT_NONE",
        rust::PDF_WIDGET_TX_FORMAT_NONE,
    )?;
    set_if_absent(
        m,
        "PDF_WIDGET_TX_FORMAT_NUMBER",
        rust::PDF_WIDGET_TX_FORMAT_NUMBER,
    )?;
    set_if_absent(
        m,
        "PDF_WIDGET_TX_FORMAT_SPECIAL",
        rust::PDF_WIDGET_TX_FORMAT_SPECIAL,
    )?;
    set_if_absent(
        m,
        "PDF_WIDGET_TX_FORMAT_TIME",
        rust::PDF_WIDGET_TX_FORMAT_TIME,
    )?;
    set_if_absent(m, "PDF_WIDGET_TYPE_BUTTON", rust::PDF_WIDGET_TYPE_BUTTON)?;
    set_if_absent(
        m,
        "PDF_WIDGET_TYPE_CHECKBOX",
        rust::PDF_WIDGET_TYPE_CHECKBOX,
    )?;
    set_if_absent(
        m,
        "PDF_WIDGET_TYPE_COMBOBOX",
        rust::PDF_WIDGET_TYPE_COMBOBOX,
    )?;
    set_if_absent(m, "PDF_WIDGET_TYPE_LISTBOX", rust::PDF_WIDGET_TYPE_LISTBOX)?;
    set_if_absent(
        m,
        "PDF_WIDGET_TYPE_RADIOBUTTON",
        rust::PDF_WIDGET_TYPE_RADIOBUTTON,
    )?;
    set_if_absent(
        m,
        "PDF_WIDGET_TYPE_SIGNATURE",
        rust::PDF_WIDGET_TYPE_SIGNATURE,
    )?;
    set_if_absent(m, "PDF_WIDGET_TYPE_TEXT", rust::PDF_WIDGET_TYPE_TEXT)?;
    set_if_absent(m, "PDF_WIDGET_TYPE_UNKNOWN", rust::PDF_WIDGET_TYPE_UNKNOWN)?;
    set_if_absent(m, "STAMP_Approved", rust::STAMP_Approved)?;
    set_if_absent(m, "STAMP_AsIs", rust::STAMP_AsIs)?;
    set_if_absent(m, "STAMP_Confidential", rust::STAMP_Confidential)?;
    set_if_absent(m, "STAMP_Departmental", rust::STAMP_Departmental)?;
    set_if_absent(m, "STAMP_Draft", rust::STAMP_Draft)?;
    set_if_absent(m, "STAMP_Experimental", rust::STAMP_Experimental)?;
    set_if_absent(m, "STAMP_Expired", rust::STAMP_Expired)?;
    set_if_absent(m, "STAMP_Final", rust::STAMP_Final)?;
    set_if_absent(m, "STAMP_ForComment", rust::STAMP_ForComment)?;
    set_if_absent(m, "STAMP_ForPublicRelease", rust::STAMP_ForPublicRelease)?;
    set_if_absent(m, "STAMP_NotApproved", rust::STAMP_NotApproved)?;
    set_if_absent(
        m,
        "STAMP_NotForPublicRelease",
        rust::STAMP_NotForPublicRelease,
    )?;
    set_if_absent(m, "STAMP_Sold", rust::STAMP_Sold)?;
    set_if_absent(m, "STAMP_TopSecret", rust::STAMP_TopSecret)?;
    set_if_absent(m, "PDF_BM_Color", rust::PDF_BM_Color)?;
    set_if_absent(m, "PDF_BM_ColorBurn", rust::PDF_BM_ColorBurn)?;
    set_if_absent(m, "PDF_BM_ColorDodge", rust::PDF_BM_ColorDodge)?;
    set_if_absent(m, "PDF_BM_Darken", rust::PDF_BM_Darken)?;
    set_if_absent(m, "PDF_BM_Difference", rust::PDF_BM_Difference)?;
    set_if_absent(m, "PDF_BM_Exclusion", rust::PDF_BM_Exclusion)?;
    set_if_absent(m, "PDF_BM_HardLight", rust::PDF_BM_HardLight)?;
    set_if_absent(m, "PDF_BM_Hue", rust::PDF_BM_Hue)?;
    set_if_absent(m, "PDF_BM_Lighten", rust::PDF_BM_Lighten)?;
    set_if_absent(m, "PDF_BM_Luminosity", rust::PDF_BM_Luminosity)?;
    set_if_absent(m, "PDF_BM_Multiply", rust::PDF_BM_Multiply)?;
    set_if_absent(m, "PDF_BM_Normal", rust::PDF_BM_Normal)?;
    set_if_absent(m, "PDF_BM_Overlay", rust::PDF_BM_Overlay)?;
    set_if_absent(m, "PDF_BM_Saturation", rust::PDF_BM_Saturation)?;
    set_if_absent(m, "PDF_BM_Screen", rust::PDF_BM_Screen)?;
    set_if_absent(m, "PDF_BM_SoftLight", rust::PDF_BM_SoftLight)?;
    set_if_absent(m, "PDF_REDACT_IMAGE_NONE", rust::PDF_REDACT_IMAGE_NONE)?;
    set_if_absent(m, "PDF_REDACT_IMAGE_PIXELS", rust::PDF_REDACT_IMAGE_PIXELS)?;
    set_if_absent(m, "PDF_REDACT_IMAGE_REMOVE", rust::PDF_REDACT_IMAGE_REMOVE)?;
    set_if_absent(
        m,
        "PDF_REDACT_IMAGE_REMOVE_UNLESS_INVISIBLE",
        rust::PDF_REDACT_IMAGE_REMOVE_UNLESS_INVISIBLE,
    )?;
    set_if_absent(
        m,
        "PDF_REDACT_LINE_ART_NONE",
        rust::PDF_REDACT_LINE_ART_NONE,
    )?;
    set_if_absent(
        m,
        "PDF_REDACT_LINE_ART_REMOVE_IF_COVERED",
        rust::PDF_REDACT_LINE_ART_REMOVE_IF_COVERED,
    )?;
    set_if_absent(
        m,
        "PDF_REDACT_LINE_ART_REMOVE_IF_TOUCHED",
        rust::PDF_REDACT_LINE_ART_REMOVE_IF_TOUCHED,
    )?;
    set_if_absent(m, "PDF_REDACT_TEXT_NONE", rust::PDF_REDACT_TEXT_NONE)?;
    set_if_absent(m, "PDF_REDACT_TEXT_REMOVE", rust::PDF_REDACT_TEXT_REMOVE)?;
    set_if_absent(
        m,
        "PDF_REDACT_TEXT_REMOVE_INVISIBLE",
        rust::PDF_REDACT_TEXT_REMOVE_INVISIBLE,
    )?;
    set_if_absent(m, "UCDN_SCRIPT_ADLAM", rust::UCDN_SCRIPT_ADLAM)?;
    set_if_absent(m, "UCDN_SCRIPT_AHOM", rust::UCDN_SCRIPT_AHOM)?;
    set_if_absent(
        m,
        "UCDN_SCRIPT_ANATOLIAN_HIEROGLYPHS",
        rust::UCDN_SCRIPT_ANATOLIAN_HIEROGLYPHS,
    )?;
    set_if_absent(m, "UCDN_SCRIPT_ARABIC", rust::UCDN_SCRIPT_ARABIC)?;
    set_if_absent(m, "UCDN_SCRIPT_ARMENIAN", rust::UCDN_SCRIPT_ARMENIAN)?;
    set_if_absent(m, "UCDN_SCRIPT_AVESTAN", rust::UCDN_SCRIPT_AVESTAN)?;
    set_if_absent(m, "UCDN_SCRIPT_BALINESE", rust::UCDN_SCRIPT_BALINESE)?;
    set_if_absent(m, "UCDN_SCRIPT_BAMUM", rust::UCDN_SCRIPT_BAMUM)?;
    set_if_absent(m, "UCDN_SCRIPT_BASSA_VAH", rust::UCDN_SCRIPT_BASSA_VAH)?;
    set_if_absent(m, "UCDN_SCRIPT_BATAK", rust::UCDN_SCRIPT_BATAK)?;
    set_if_absent(m, "UCDN_SCRIPT_BENGALI", rust::UCDN_SCRIPT_BENGALI)?;
    set_if_absent(m, "UCDN_SCRIPT_BHAIKSUKI", rust::UCDN_SCRIPT_BHAIKSUKI)?;
    set_if_absent(m, "UCDN_SCRIPT_BOPOMOFO", rust::UCDN_SCRIPT_BOPOMOFO)?;
    set_if_absent(m, "UCDN_SCRIPT_BRAHMI", rust::UCDN_SCRIPT_BRAHMI)?;
    set_if_absent(m, "UCDN_SCRIPT_BRAILLE", rust::UCDN_SCRIPT_BRAILLE)?;
    set_if_absent(m, "UCDN_SCRIPT_BUGINESE", rust::UCDN_SCRIPT_BUGINESE)?;
    set_if_absent(m, "UCDN_SCRIPT_BUHID", rust::UCDN_SCRIPT_BUHID)?;
    set_if_absent(
        m,
        "UCDN_SCRIPT_CANADIAN_ABORIGINAL",
        rust::UCDN_SCRIPT_CANADIAN_ABORIGINAL,
    )?;
    set_if_absent(m, "UCDN_SCRIPT_CARIAN", rust::UCDN_SCRIPT_CARIAN)?;
    set_if_absent(
        m,
        "UCDN_SCRIPT_CAUCASIAN_ALBANIAN",
        rust::UCDN_SCRIPT_CAUCASIAN_ALBANIAN,
    )?;
    set_if_absent(m, "UCDN_SCRIPT_CHAKMA", rust::UCDN_SCRIPT_CHAKMA)?;
    set_if_absent(m, "UCDN_SCRIPT_CHAM", rust::UCDN_SCRIPT_CHAM)?;
    set_if_absent(m, "UCDN_SCRIPT_CHEROKEE", rust::UCDN_SCRIPT_CHEROKEE)?;
    set_if_absent(m, "UCDN_SCRIPT_CHORASMIAN", rust::UCDN_SCRIPT_CHORASMIAN)?;
    set_if_absent(m, "UCDN_SCRIPT_COMMON", rust::UCDN_SCRIPT_COMMON)?;
    set_if_absent(m, "UCDN_SCRIPT_COPTIC", rust::UCDN_SCRIPT_COPTIC)?;
    set_if_absent(m, "UCDN_SCRIPT_CUNEIFORM", rust::UCDN_SCRIPT_CUNEIFORM)?;
    set_if_absent(m, "UCDN_SCRIPT_CYPRIOT", rust::UCDN_SCRIPT_CYPRIOT)?;
    set_if_absent(
        m,
        "UCDN_SCRIPT_CYPRO_MINOAN",
        rust::UCDN_SCRIPT_CYPRO_MINOAN,
    )?;
    set_if_absent(m, "UCDN_SCRIPT_CYRILLIC", rust::UCDN_SCRIPT_CYRILLIC)?;
    set_if_absent(m, "UCDN_SCRIPT_DESERET", rust::UCDN_SCRIPT_DESERET)?;
    set_if_absent(m, "UCDN_SCRIPT_DEVANAGARI", rust::UCDN_SCRIPT_DEVANAGARI)?;
    set_if_absent(m, "UCDN_SCRIPT_DIVES_AKURU", rust::UCDN_SCRIPT_DIVES_AKURU)?;
    set_if_absent(m, "UCDN_SCRIPT_DOGRA", rust::UCDN_SCRIPT_DOGRA)?;
    set_if_absent(m, "UCDN_SCRIPT_DUPLOYAN", rust::UCDN_SCRIPT_DUPLOYAN)?;
    set_if_absent(
        m,
        "UCDN_SCRIPT_EGYPTIAN_HIEROGLYPHS",
        rust::UCDN_SCRIPT_EGYPTIAN_HIEROGLYPHS,
    )?;
    set_if_absent(m, "UCDN_SCRIPT_ELBASAN", rust::UCDN_SCRIPT_ELBASAN)?;
    set_if_absent(m, "UCDN_SCRIPT_ELYMAIC", rust::UCDN_SCRIPT_ELYMAIC)?;
    set_if_absent(m, "UCDN_SCRIPT_ETHIOPIC", rust::UCDN_SCRIPT_ETHIOPIC)?;
    set_if_absent(m, "UCDN_SCRIPT_GARAY", rust::UCDN_SCRIPT_GARAY)?;
    set_if_absent(m, "UCDN_SCRIPT_GEORGIAN", rust::UCDN_SCRIPT_GEORGIAN)?;
    set_if_absent(m, "UCDN_SCRIPT_GLAGOLITIC", rust::UCDN_SCRIPT_GLAGOLITIC)?;
    set_if_absent(m, "UCDN_SCRIPT_GOTHIC", rust::UCDN_SCRIPT_GOTHIC)?;
    set_if_absent(m, "UCDN_SCRIPT_GRANTHA", rust::UCDN_SCRIPT_GRANTHA)?;
    set_if_absent(m, "UCDN_SCRIPT_GREEK", rust::UCDN_SCRIPT_GREEK)?;
    set_if_absent(m, "UCDN_SCRIPT_GUJARATI", rust::UCDN_SCRIPT_GUJARATI)?;
    set_if_absent(
        m,
        "UCDN_SCRIPT_GUNJALA_GONDI",
        rust::UCDN_SCRIPT_GUNJALA_GONDI,
    )?;
    set_if_absent(m, "UCDN_SCRIPT_GURMUKHI", rust::UCDN_SCRIPT_GURMUKHI)?;
    set_if_absent(
        m,
        "UCDN_SCRIPT_GURUNG_KHEMA",
        rust::UCDN_SCRIPT_GURUNG_KHEMA,
    )?;
    set_if_absent(m, "UCDN_SCRIPT_HAN", rust::UCDN_SCRIPT_HAN)?;
    set_if_absent(m, "UCDN_SCRIPT_HANGUL", rust::UCDN_SCRIPT_HANGUL)?;
    set_if_absent(
        m,
        "UCDN_SCRIPT_HANIFI_ROHINGYA",
        rust::UCDN_SCRIPT_HANIFI_ROHINGYA,
    )?;
    set_if_absent(m, "UCDN_SCRIPT_HANUNOO", rust::UCDN_SCRIPT_HANUNOO)?;
    set_if_absent(m, "UCDN_SCRIPT_HATRAN", rust::UCDN_SCRIPT_HATRAN)?;
    set_if_absent(m, "UCDN_SCRIPT_HEBREW", rust::UCDN_SCRIPT_HEBREW)?;
    set_if_absent(m, "UCDN_SCRIPT_HIRAGANA", rust::UCDN_SCRIPT_HIRAGANA)?;
    set_if_absent(
        m,
        "UCDN_SCRIPT_IMPERIAL_ARAMAIC",
        rust::UCDN_SCRIPT_IMPERIAL_ARAMAIC,
    )?;
    set_if_absent(m, "UCDN_SCRIPT_INHERITED", rust::UCDN_SCRIPT_INHERITED)?;
    set_if_absent(
        m,
        "UCDN_SCRIPT_INSCRIPTIONAL_PAHLAVI",
        rust::UCDN_SCRIPT_INSCRIPTIONAL_PAHLAVI,
    )?;
    set_if_absent(
        m,
        "UCDN_SCRIPT_INSCRIPTIONAL_PARTHIAN",
        rust::UCDN_SCRIPT_INSCRIPTIONAL_PARTHIAN,
    )?;
    set_if_absent(m, "UCDN_SCRIPT_JAVANESE", rust::UCDN_SCRIPT_JAVANESE)?;
    set_if_absent(m, "UCDN_SCRIPT_KAITHI", rust::UCDN_SCRIPT_KAITHI)?;
    set_if_absent(m, "UCDN_SCRIPT_KANNADA", rust::UCDN_SCRIPT_KANNADA)?;
    set_if_absent(m, "UCDN_SCRIPT_KATAKANA", rust::UCDN_SCRIPT_KATAKANA)?;
    set_if_absent(m, "UCDN_SCRIPT_KAWI", rust::UCDN_SCRIPT_KAWI)?;
    set_if_absent(m, "UCDN_SCRIPT_KAYAH_LI", rust::UCDN_SCRIPT_KAYAH_LI)?;
    set_if_absent(m, "UCDN_SCRIPT_KHAROSHTHI", rust::UCDN_SCRIPT_KHAROSHTHI)?;
    set_if_absent(
        m,
        "UCDN_SCRIPT_KHITAN_SMALL_SCRIPT",
        rust::UCDN_SCRIPT_KHITAN_SMALL_SCRIPT,
    )?;
    set_if_absent(m, "UCDN_SCRIPT_KHMER", rust::UCDN_SCRIPT_KHMER)?;
    set_if_absent(m, "UCDN_SCRIPT_KHOJKI", rust::UCDN_SCRIPT_KHOJKI)?;
    set_if_absent(m, "UCDN_SCRIPT_KHUDAWADI", rust::UCDN_SCRIPT_KHUDAWADI)?;
    set_if_absent(m, "UCDN_SCRIPT_KIRAT_RAI", rust::UCDN_SCRIPT_KIRAT_RAI)?;
    set_if_absent(m, "UCDN_SCRIPT_LAO", rust::UCDN_SCRIPT_LAO)?;
    set_if_absent(m, "UCDN_SCRIPT_LATIN", rust::UCDN_SCRIPT_LATIN)?;
    set_if_absent(m, "UCDN_SCRIPT_LEPCHA", rust::UCDN_SCRIPT_LEPCHA)?;
    set_if_absent(m, "UCDN_SCRIPT_LIMBU", rust::UCDN_SCRIPT_LIMBU)?;
    set_if_absent(m, "UCDN_SCRIPT_LINEAR_A", rust::UCDN_SCRIPT_LINEAR_A)?;
    set_if_absent(m, "UCDN_SCRIPT_LINEAR_B", rust::UCDN_SCRIPT_LINEAR_B)?;
    set_if_absent(m, "UCDN_SCRIPT_LISU", rust::UCDN_SCRIPT_LISU)?;
    set_if_absent(m, "UCDN_SCRIPT_LYCIAN", rust::UCDN_SCRIPT_LYCIAN)?;
    set_if_absent(m, "UCDN_SCRIPT_LYDIAN", rust::UCDN_SCRIPT_LYDIAN)?;
    set_if_absent(m, "UCDN_SCRIPT_MAHAJANI", rust::UCDN_SCRIPT_MAHAJANI)?;
    set_if_absent(m, "UCDN_SCRIPT_MAKASAR", rust::UCDN_SCRIPT_MAKASAR)?;
    set_if_absent(m, "UCDN_SCRIPT_MALAYALAM", rust::UCDN_SCRIPT_MALAYALAM)?;
    set_if_absent(m, "UCDN_SCRIPT_MANDAIC", rust::UCDN_SCRIPT_MANDAIC)?;
    set_if_absent(m, "UCDN_SCRIPT_MANICHAEAN", rust::UCDN_SCRIPT_MANICHAEAN)?;
    set_if_absent(m, "UCDN_SCRIPT_MARCHEN", rust::UCDN_SCRIPT_MARCHEN)?;
    set_if_absent(
        m,
        "UCDN_SCRIPT_MASARAM_GONDI",
        rust::UCDN_SCRIPT_MASARAM_GONDI,
    )?;
    set_if_absent(m, "UCDN_SCRIPT_MEDEFAIDRIN", rust::UCDN_SCRIPT_MEDEFAIDRIN)?;
    set_if_absent(
        m,
        "UCDN_SCRIPT_MEETEI_MAYEK",
        rust::UCDN_SCRIPT_MEETEI_MAYEK,
    )?;
    set_if_absent(
        m,
        "UCDN_SCRIPT_MENDE_KIKAKUI",
        rust::UCDN_SCRIPT_MENDE_KIKAKUI,
    )?;
    set_if_absent(
        m,
        "UCDN_SCRIPT_MEROITIC_CURSIVE",
        rust::UCDN_SCRIPT_MEROITIC_CURSIVE,
    )?;
    set_if_absent(
        m,
        "UCDN_SCRIPT_MEROITIC_HIEROGLYPHS",
        rust::UCDN_SCRIPT_MEROITIC_HIEROGLYPHS,
    )?;
    set_if_absent(m, "UCDN_SCRIPT_MIAO", rust::UCDN_SCRIPT_MIAO)?;
    set_if_absent(m, "UCDN_SCRIPT_MODI", rust::UCDN_SCRIPT_MODI)?;
    set_if_absent(m, "UCDN_SCRIPT_MONGOLIAN", rust::UCDN_SCRIPT_MONGOLIAN)?;
    set_if_absent(m, "UCDN_SCRIPT_MRO", rust::UCDN_SCRIPT_MRO)?;
    set_if_absent(m, "UCDN_SCRIPT_MULTANI", rust::UCDN_SCRIPT_MULTANI)?;
    set_if_absent(m, "UCDN_SCRIPT_MYANMAR", rust::UCDN_SCRIPT_MYANMAR)?;
    set_if_absent(m, "UCDN_SCRIPT_NABATAEAN", rust::UCDN_SCRIPT_NABATAEAN)?;
    set_if_absent(m, "UCDN_SCRIPT_NAG_MUNDARI", rust::UCDN_SCRIPT_NAG_MUNDARI)?;
    set_if_absent(m, "UCDN_SCRIPT_NANDINAGARI", rust::UCDN_SCRIPT_NANDINAGARI)?;
    set_if_absent(m, "UCDN_SCRIPT_NEWA", rust::UCDN_SCRIPT_NEWA)?;
    set_if_absent(m, "UCDN_SCRIPT_NEW_TAI_LUE", rust::UCDN_SCRIPT_NEW_TAI_LUE)?;
    set_if_absent(m, "UCDN_SCRIPT_NKO", rust::UCDN_SCRIPT_NKO)?;
    set_if_absent(m, "UCDN_SCRIPT_NUSHU", rust::UCDN_SCRIPT_NUSHU)?;
    set_if_absent(
        m,
        "UCDN_SCRIPT_NYIAKENG_PUACHUE_HMONG",
        rust::UCDN_SCRIPT_NYIAKENG_PUACHUE_HMONG,
    )?;
    set_if_absent(m, "UCDN_SCRIPT_OGHAM", rust::UCDN_SCRIPT_OGHAM)?;
    set_if_absent(
        m,
        "UCDN_SCRIPT_OLD_HUNGARIAN",
        rust::UCDN_SCRIPT_OLD_HUNGARIAN,
    )?;
    set_if_absent(m, "UCDN_SCRIPT_OLD_ITALIC", rust::UCDN_SCRIPT_OLD_ITALIC)?;
    set_if_absent(
        m,
        "UCDN_SCRIPT_OLD_NORTH_ARABIAN",
        rust::UCDN_SCRIPT_OLD_NORTH_ARABIAN,
    )?;
    set_if_absent(m, "UCDN_SCRIPT_OLD_PERMIC", rust::UCDN_SCRIPT_OLD_PERMIC)?;
    set_if_absent(m, "UCDN_SCRIPT_OLD_PERSIAN", rust::UCDN_SCRIPT_OLD_PERSIAN)?;
    set_if_absent(m, "UCDN_SCRIPT_OLD_SOGDIAN", rust::UCDN_SCRIPT_OLD_SOGDIAN)?;
    set_if_absent(
        m,
        "UCDN_SCRIPT_OLD_SOUTH_ARABIAN",
        rust::UCDN_SCRIPT_OLD_SOUTH_ARABIAN,
    )?;
    set_if_absent(m, "UCDN_SCRIPT_OLD_TURKIC", rust::UCDN_SCRIPT_OLD_TURKIC)?;
    set_if_absent(m, "UCDN_SCRIPT_OLD_UYGHUR", rust::UCDN_SCRIPT_OLD_UYGHUR)?;
    set_if_absent(m, "UCDN_SCRIPT_OL_CHIKI", rust::UCDN_SCRIPT_OL_CHIKI)?;
    set_if_absent(m, "UCDN_SCRIPT_OL_ONAL", rust::UCDN_SCRIPT_OL_ONAL)?;
    set_if_absent(m, "UCDN_SCRIPT_ORIYA", rust::UCDN_SCRIPT_ORIYA)?;
    set_if_absent(m, "UCDN_SCRIPT_OSAGE", rust::UCDN_SCRIPT_OSAGE)?;
    set_if_absent(m, "UCDN_SCRIPT_OSMANYA", rust::UCDN_SCRIPT_OSMANYA)?;
    set_if_absent(
        m,
        "UCDN_SCRIPT_PAHAWH_HMONG",
        rust::UCDN_SCRIPT_PAHAWH_HMONG,
    )?;
    set_if_absent(m, "UCDN_SCRIPT_PALMYRENE", rust::UCDN_SCRIPT_PALMYRENE)?;
    set_if_absent(m, "UCDN_SCRIPT_PAU_CIN_HAU", rust::UCDN_SCRIPT_PAU_CIN_HAU)?;
    set_if_absent(m, "UCDN_SCRIPT_PHAGS_PA", rust::UCDN_SCRIPT_PHAGS_PA)?;
    set_if_absent(m, "UCDN_SCRIPT_PHOENICIAN", rust::UCDN_SCRIPT_PHOENICIAN)?;
    set_if_absent(
        m,
        "UCDN_SCRIPT_PSALTER_PAHLAVI",
        rust::UCDN_SCRIPT_PSALTER_PAHLAVI,
    )?;
    set_if_absent(m, "UCDN_SCRIPT_REJANG", rust::UCDN_SCRIPT_REJANG)?;
    set_if_absent(m, "UCDN_SCRIPT_RUNIC", rust::UCDN_SCRIPT_RUNIC)?;
    set_if_absent(m, "UCDN_SCRIPT_SAMARITAN", rust::UCDN_SCRIPT_SAMARITAN)?;
    set_if_absent(m, "UCDN_SCRIPT_SAURASHTRA", rust::UCDN_SCRIPT_SAURASHTRA)?;
    set_if_absent(m, "UCDN_SCRIPT_SHARADA", rust::UCDN_SCRIPT_SHARADA)?;
    set_if_absent(m, "UCDN_SCRIPT_SHAVIAN", rust::UCDN_SCRIPT_SHAVIAN)?;
    set_if_absent(m, "UCDN_SCRIPT_SIDDHAM", rust::UCDN_SCRIPT_SIDDHAM)?;
    set_if_absent(m, "UCDN_SCRIPT_SIGNWRITING", rust::UCDN_SCRIPT_SIGNWRITING)?;
    set_if_absent(m, "UCDN_SCRIPT_SINHALA", rust::UCDN_SCRIPT_SINHALA)?;
    set_if_absent(m, "UCDN_SCRIPT_SOGDIAN", rust::UCDN_SCRIPT_SOGDIAN)?;
    set_if_absent(
        m,
        "UCDN_SCRIPT_SORA_SOMPENG",
        rust::UCDN_SCRIPT_SORA_SOMPENG,
    )?;
    set_if_absent(m, "UCDN_SCRIPT_SOYOMBO", rust::UCDN_SCRIPT_SOYOMBO)?;
    set_if_absent(m, "UCDN_SCRIPT_SUNDANESE", rust::UCDN_SCRIPT_SUNDANESE)?;
    set_if_absent(m, "UCDN_SCRIPT_SUNUWAR", rust::UCDN_SCRIPT_SUNUWAR)?;
    set_if_absent(
        m,
        "UCDN_SCRIPT_SYLOTI_NAGRI",
        rust::UCDN_SCRIPT_SYLOTI_NAGRI,
    )?;
    set_if_absent(m, "UCDN_SCRIPT_SYRIAC", rust::UCDN_SCRIPT_SYRIAC)?;
    set_if_absent(m, "UCDN_SCRIPT_TAGALOG", rust::UCDN_SCRIPT_TAGALOG)?;
    set_if_absent(m, "UCDN_SCRIPT_TAGBANWA", rust::UCDN_SCRIPT_TAGBANWA)?;
    set_if_absent(m, "UCDN_SCRIPT_TAI_LE", rust::UCDN_SCRIPT_TAI_LE)?;
    set_if_absent(m, "UCDN_SCRIPT_TAI_THAM", rust::UCDN_SCRIPT_TAI_THAM)?;
    set_if_absent(m, "UCDN_SCRIPT_TAI_VIET", rust::UCDN_SCRIPT_TAI_VIET)?;
    set_if_absent(m, "UCDN_SCRIPT_TAKRI", rust::UCDN_SCRIPT_TAKRI)?;
    set_if_absent(m, "UCDN_SCRIPT_TAMIL", rust::UCDN_SCRIPT_TAMIL)?;
    set_if_absent(m, "UCDN_SCRIPT_TANGSA", rust::UCDN_SCRIPT_TANGSA)?;
    set_if_absent(m, "UCDN_SCRIPT_TANGUT", rust::UCDN_SCRIPT_TANGUT)?;
    set_if_absent(m, "UCDN_SCRIPT_TELUGU", rust::UCDN_SCRIPT_TELUGU)?;
    set_if_absent(m, "UCDN_SCRIPT_THAANA", rust::UCDN_SCRIPT_THAANA)?;
    set_if_absent(m, "UCDN_SCRIPT_THAI", rust::UCDN_SCRIPT_THAI)?;
    set_if_absent(m, "UCDN_SCRIPT_TIBETAN", rust::UCDN_SCRIPT_TIBETAN)?;
    set_if_absent(m, "UCDN_SCRIPT_TIFINAGH", rust::UCDN_SCRIPT_TIFINAGH)?;
    set_if_absent(m, "UCDN_SCRIPT_TIRHUTA", rust::UCDN_SCRIPT_TIRHUTA)?;
    set_if_absent(m, "UCDN_SCRIPT_TODHRI", rust::UCDN_SCRIPT_TODHRI)?;
    set_if_absent(m, "UCDN_SCRIPT_TOTO", rust::UCDN_SCRIPT_TOTO)?;
    set_if_absent(
        m,
        "UCDN_SCRIPT_TULU_TIGALARI",
        rust::UCDN_SCRIPT_TULU_TIGALARI,
    )?;
    set_if_absent(m, "UCDN_SCRIPT_UGARITIC", rust::UCDN_SCRIPT_UGARITIC)?;
    set_if_absent(m, "UCDN_SCRIPT_UNKNOWN", rust::UCDN_SCRIPT_UNKNOWN)?;
    set_if_absent(m, "UCDN_SCRIPT_VAI", rust::UCDN_SCRIPT_VAI)?;
    set_if_absent(m, "UCDN_SCRIPT_VITHKUQI", rust::UCDN_SCRIPT_VITHKUQI)?;
    set_if_absent(m, "UCDN_SCRIPT_WANCHO", rust::UCDN_SCRIPT_WANCHO)?;
    set_if_absent(m, "UCDN_SCRIPT_WARANG_CITI", rust::UCDN_SCRIPT_WARANG_CITI)?;
    set_if_absent(m, "UCDN_SCRIPT_YEZIDI", rust::UCDN_SCRIPT_YEZIDI)?;
    set_if_absent(m, "UCDN_SCRIPT_YI", rust::UCDN_SCRIPT_YI)?;
    set_if_absent(
        m,
        "UCDN_SCRIPT_ZANABAZAR_SQUARE",
        rust::UCDN_SCRIPT_ZANABAZAR_SQUARE,
    )?;
    set_if_absent(m, "PDF_CID_FONT_RESOURCE", rust::PDF_CID_FONT_RESOURCE)?;
    set_if_absent(m, "PDF_CJK_FONT_RESOURCE", rust::PDF_CJK_FONT_RESOURCE)?;
    set_if_absent(m, "PDF_FD_ALL_CAP", rust::PDF_FD_ALL_CAP)?;
    set_if_absent(m, "PDF_FD_FIXED_PITCH", rust::PDF_FD_FIXED_PITCH)?;
    set_if_absent(m, "PDF_FD_FORCE_BOLD", rust::PDF_FD_FORCE_BOLD)?;
    set_if_absent(m, "PDF_FD_ITALIC", rust::PDF_FD_ITALIC)?;
    set_if_absent(m, "PDF_FD_NONSYMBOLIC", rust::PDF_FD_NONSYMBOLIC)?;
    set_if_absent(m, "PDF_FD_SCRIPT", rust::PDF_FD_SCRIPT)?;
    set_if_absent(m, "PDF_FD_SERIF", rust::PDF_FD_SERIF)?;
    set_if_absent(m, "PDF_FD_SMALL_CAP", rust::PDF_FD_SMALL_CAP)?;
    set_if_absent(m, "PDF_FD_SYMBOLIC", rust::PDF_FD_SYMBOLIC)?;
    set_if_absent(
        m,
        "PDF_SIMPLE_ENCODING_CYRILLIC",
        rust::PDF_SIMPLE_ENCODING_CYRILLIC,
    )?;
    set_if_absent(
        m,
        "PDF_SIMPLE_ENCODING_GREEK",
        rust::PDF_SIMPLE_ENCODING_GREEK,
    )?;
    set_if_absent(
        m,
        "PDF_SIMPLE_ENCODING_LATIN",
        rust::PDF_SIMPLE_ENCODING_LATIN,
    )?;
    set_if_absent(
        m,
        "PDF_SIMPLE_FONT_RESOURCE",
        rust::PDF_SIMPLE_FONT_RESOURCE,
    )?;
    set_if_absent(
        m,
        "Base14_fontdict",
        rust::Base14_fontdict
            .iter()
            .copied()
            .collect::<HashMap<&str, &str>>(),
    )?;
    set_if_absent(m, "Base14_fontnames", rust::Base14_fontnames.to_vec())?;
    set_if_absent(m, "symbol_glyphs", glyph_rows(m.py(), rust::symbol_glyphs)?)?;
    set_if_absent(m, "zapf_glyphs", glyph_rows(m.py(), rust::zapf_glyphs)?)?;
    set_if_absent(
        m,
        "pdfcolor",
        rust::pdfcolor
            .iter()
            .map(|(name, rgb)| (*name, rgb.to_vec()))
            .collect::<HashMap<&str, Vec<f64>>>(),
    )?;
    set_if_absent(m, "INVALID_NAME_CHARS", rust::INVALID_NAME_CHARS.to_vec())?;
    set_if_absent(m, "PDF_ENUM_FALSE", rust::PDF_ENUM_FALSE)?;
    set_if_absent(m, "PDF_ENUM_LIMIT", rust::PDF_ENUM_LIMIT)?;
    set_if_absent(m, "PDF_ENUM_NULL", rust::PDF_ENUM_NULL)?;
    set_if_absent(m, "PDF_ENUM_TRUE", rust::PDF_ENUM_TRUE)?;
    set_if_absent(m, "PDF_LEXBUF_LARGE", rust::PDF_LEXBUF_LARGE)?;
    set_if_absent(m, "PDF_LEXBUF_SMALL", rust::PDF_LEXBUF_SMALL)?;
    set_if_absent(m, "PDF_MAX_GEN_NUMBER", rust::PDF_MAX_GEN_NUMBER)?;
    set_if_absent(m, "PDF_MAX_OBJECT_NUMBER", rust::PDF_MAX_OBJECT_NUMBER)?;
    set_if_absent(m, "PDF_MRANGE_CAP", rust::PDF_MRANGE_CAP)?;
    set_if_absent(m, "PDF_NUM_TOKENS", rust::PDF_NUM_TOKENS)?;
    set_if_absent(
        m,
        "PDF_PROCESSOR_REQUIRES_DECODED_IMAGES",
        rust::PDF_PROCESSOR_REQUIRES_DECODED_IMAGES,
    )?;
    set_if_absent(m, "PDF_TOK_CLOSE_ARRAY", rust::PDF_TOK_CLOSE_ARRAY)?;
    set_if_absent(m, "PDF_TOK_CLOSE_BRACE", rust::PDF_TOK_CLOSE_BRACE)?;
    set_if_absent(m, "PDF_TOK_CLOSE_DICT", rust::PDF_TOK_CLOSE_DICT)?;
    set_if_absent(m, "PDF_TOK_ENDOBJ", rust::PDF_TOK_ENDOBJ)?;
    set_if_absent(m, "PDF_TOK_ENDSTREAM", rust::PDF_TOK_ENDSTREAM)?;
    set_if_absent(m, "PDF_TOK_EOF", rust::PDF_TOK_EOF)?;
    set_if_absent(m, "PDF_TOK_ERROR", rust::PDF_TOK_ERROR)?;
    set_if_absent(m, "PDF_TOK_FALSE", rust::PDF_TOK_FALSE)?;
    set_if_absent(m, "PDF_TOK_INT", rust::PDF_TOK_INT)?;
    set_if_absent(m, "PDF_TOK_KEYWORD", rust::PDF_TOK_KEYWORD)?;
    set_if_absent(m, "PDF_TOK_NAME", rust::PDF_TOK_NAME)?;
    set_if_absent(m, "PDF_TOK_NEWOBJ", rust::PDF_TOK_NEWOBJ)?;
    set_if_absent(m, "PDF_TOK_NULL", rust::PDF_TOK_NULL)?;
    set_if_absent(m, "PDF_TOK_OBJ", rust::PDF_TOK_OBJ)?;
    set_if_absent(m, "PDF_TOK_OPEN_ARRAY", rust::PDF_TOK_OPEN_ARRAY)?;
    set_if_absent(m, "PDF_TOK_OPEN_BRACE", rust::PDF_TOK_OPEN_BRACE)?;
    set_if_absent(m, "PDF_TOK_OPEN_DICT", rust::PDF_TOK_OPEN_DICT)?;
    set_if_absent(m, "PDF_TOK_R", rust::PDF_TOK_R)?;
    set_if_absent(m, "PDF_TOK_REAL", rust::PDF_TOK_REAL)?;
    set_if_absent(m, "PDF_TOK_STARTXREF", rust::PDF_TOK_STARTXREF)?;
    set_if_absent(m, "PDF_TOK_STREAM", rust::PDF_TOK_STREAM)?;
    set_if_absent(m, "PDF_TOK_STRING", rust::PDF_TOK_STRING)?;
    set_if_absent(m, "PDF_TOK_TRAILER", rust::PDF_TOK_TRAILER)?;
    set_if_absent(m, "PDF_TOK_TRUE", rust::PDF_TOK_TRUE)?;
    set_if_absent(m, "PDF_TOK_XREF", rust::PDF_TOK_XREF)?;
    set_if_absent(m, "b", rust::b)?;
    set_if_absent(m, "f", rust::f)?;
    set_if_absent(m, "g", rust::g)?;
    set_if_absent(m, "matrix_like", rust::matrix_like)?;
    set_if_absent(m, "name", rust::name)?;
    set_if_absent(m, "point_like", rust::point_like)?;
    set_if_absent(m, "quad_like", rust::quad_like)?;
    set_if_absent(m, "r", rust::r)?;
    set_if_absent(m, "rect_like", rust::rect_like)?;
    set_if_absent(m, "trace_device_CLIP_PATH", rust::trace_device_CLIP_PATH)?;
    set_if_absent(
        m,
        "trace_device_CLIP_STROKE_PATH",
        rust::trace_device_CLIP_STROKE_PATH,
    )?;
    set_if_absent(m, "trace_device_FILL_PATH", rust::trace_device_FILL_PATH)?;
    set_if_absent(
        m,
        "trace_device_STROKE_PATH",
        rust::trace_device_STROKE_PATH,
    )?;
    Ok(())
}

/// Register the L2 lane's new module-level names.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // `open` is the `Document` class itself, so `pdfluent.open is
    // pdfluent.Document` holds the way the recorded surface shows it.
    let document = m.getattr("Document")?;
    m.setattr("open", document)?;
    m.add_class::<PyLink>()?;
    m.add_class::<PyLinkDest>()?;
    m.add_class::<PyOutline>()?;
    m.add_class::<PyDisplayList>()?;
    m.add_class::<PyFzDocument>()?;
    register_constants(m)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_dict_has_no_padding() {
        let dict = LObject::Dictionary(lopdf::dictionary! {
            "Size" => LObject::Integer(7),
            "Root" => LObject::Reference((5, 0)),
        });
        assert_eq!(packed_source(&dict), "<</Size 7/Root 5 0 R>>");
    }

    #[test]
    fn wrapped_stream_opens_with_q_and_closes_with_q() {
        assert!(stream_wrapped(b"q\nBT /F1 12 Tf ET\nQ\n"));
        assert!(stream_wrapped(b"q\nq\nBT ET\nQ\nQ\n"));
        assert!(!stream_wrapped(b"BT /F1 12 Tf ET\n"));
        assert!(!stream_wrapped(b""));
        assert!(!stream_wrapped(b"q\nBT ET\n"));
    }

    #[test]
    fn colour_numbers_trim() {
        assert_eq!(color_num(0.0), "0");
        assert_eq!(color_num(1.0), "1");
        assert_eq!(color_num(0.5), "0.5");
        assert_eq!(color_num(0.2999), "0.2999");
    }

    #[test]
    fn recolor_maps_rgb_to_gray_and_passes_the_rest_through() {
        let content = b"1 0 0 rg BT /F1 12 Tf (hi) Tj ET 0.5 g";
        let gray = recolor_stream(content, 1);
        assert_eq!(gray, b"0.299 g BT /F1 12 Tf (hi) Tj ET 0.5 g");
        let rgb = recolor_stream(content, 3);
        assert_eq!(rgb, b"1 0 0 rg BT /F1 12 Tf (hi) Tj ET 0.5 0.5 0.5 rg");
    }

    #[test]
    fn recolor_leaves_streams_without_colour_ops_alone() {
        let content = b"BT /F1 12 Tf 100 700 Td (Test page) Tj ET\n";
        assert_eq!(recolor_stream(content, 1), content);
    }
}
