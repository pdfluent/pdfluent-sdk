//! Compatible images and rasters (lane L3): image classes and `Page` render methods.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::compat::pixmap as core;
use pdfluent::compat::Value as CompatValue;
use pdfluent::compat::{
    geometry::{IRect, Matrix, Point, Quad, Rect},
    ErrorKind,
};
use pyo3::exceptions::{
    PyAssertionError, PyException, PyIndexError, PyKeyError, PyModuleNotFoundError, PyRuntimeError,
    PyTypeError, PyValueError, PyZeroDivisionError,
};
use pyo3::prelude::*;
use pyo3::pyclass::CompareOp as PyCompareOp;
use pyo3::types::{PyBool, PyBytes, PyDict, PyMemoryView, PyTuple};

use crate::compat::errors::{FzErrorFormat, FzErrorSystem};
use crate::{PyDocument, PyPage};

/// Which Python class an engine fault becomes at one call site.
///
/// Bare exceptions and assertion failures are raised directly at their
/// call sites instead of travelling through here.
#[derive(Debug, Clone, Copy)]
enum FaultMap {
    /// Unspecified runtime failure.
    Runtime,
    /// System-level image failure.
    System,
}

/// Map a compat failure onto the recorded Python exception.
fn compat_err(error: pdfluent::Error, fault: FaultMap, context: &str) -> PyErr {
    let message = format!("{context}: {error}");
    match error.compat_kind().cloned() {
        Some(ErrorKind::WrongType) => PyTypeError::new_err(message),
        Some(ErrorKind::BadValue | ErrorKind::EmptyFile) => PyValueError::new_err(message),
        Some(ErrorKind::UnknownKey) => PyKeyError::new_err(message),
        Some(ErrorKind::IndexOutOfRange) => PyIndexError::new_err(message),
        Some(ErrorKind::MissingFile) => FzErrorSystem::new_err(message),
        Some(ErrorKind::UnreadableFile) => FzErrorFormat::new_err(message),
        Some(ErrorKind::EngineFault) => match fault {
            FaultMap::Runtime => PyRuntimeError::new_err(message),
            FaultMap::System => FzErrorSystem::new_err(message),
        },
        Some(ErrorKind::MissingComponent) => PyModuleNotFoundError::new_err(message),
        Some(ErrorKind::Deprecation) | Some(_) => PyException::new_err(message),
        None => PyRuntimeError::new_err(message),
    }
}

/// Read an integer box: duck-typed corners or a four-sequence.
fn coerce_irect(obj: &Bound<'_, PyAny>) -> Option<IRect> {
    if let (Ok(x0), Ok(y0), Ok(x1), Ok(y1)) = (
        box_coord(obj, "x0"),
        box_coord(obj, "y0"),
        box_coord(obj, "x1"),
        box_coord(obj, "y1"),
    ) {
        return Some(IRect { x0, y0, x1, y1 });
    }
    let items: Vec<Bound<'_, PyAny>> = obj.extract().ok()?;
    if items.len() != 4 {
        return None;
    }
    let mut out = [0i32; 4];
    for (slot, item) in out.iter_mut().zip(items.iter()) {
        *slot = int_coord(item)?;
    }
    Some(IRect {
        x0: out[0],
        y0: out[1],
        x1: out[2],
        y1: out[3],
    })
}

/// Read one integer corner attribute, rounding floats.
fn box_coord(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<i32> {
    let value = obj.getattr(name)?;
    int_coord(&value)
        .ok_or_else(|| PyTypeError::new_err(format!("box corner '{name}' is not a number")))
}

/// Read one integer coordinate, rounding floats.
fn int_coord(obj: &Bound<'_, PyAny>) -> Option<i32> {
    if let Ok(v) = obj.extract::<i32>() {
        return Some(v);
    }
    obj.extract::<f64>().ok().map(|v| v.round() as i32)
}

/// Read a float box: duck-typed corners or a four-sequence.
fn coerce_rect(obj: &Bound<'_, PyAny>) -> Option<Rect> {
    if let (Ok(x0), Ok(y0), Ok(x1), Ok(y1)) = (
        float_attr(obj, "x0"),
        float_attr(obj, "y0"),
        float_attr(obj, "x1"),
        float_attr(obj, "y1"),
    ) {
        return Some(Rect { x0, y0, x1, y1 });
    }
    let items: Vec<Bound<'_, PyAny>> = obj.extract().ok()?;
    if items.len() != 4 {
        return None;
    }
    let mut out = [0.0f64; 4];
    for (slot, item) in out.iter_mut().zip(items.iter()) {
        *slot = float_value(item)?;
    }
    Some(Rect {
        x0: out[0],
        y0: out[1],
        x1: out[2],
        y1: out[3],
    })
}

/// Read one float attribute.
fn float_attr(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<f64> {
    let value = obj.getattr(name)?;
    float_value(&value).ok_or_else(|| PyTypeError::new_err(format!("'{name}' is not a number")))
}

/// Read one float value, accepting integers.
fn float_value(obj: &Bound<'_, PyAny>) -> Option<f64> {
    if let Ok(v) = obj.extract::<f64>() {
        return Some(v);
    }
    obj.extract::<i32>().ok().map(f64::from)
}

/// Read a matrix: duck-typed coefficients or a six-sequence.
fn coerce_matrix(obj: &Bound<'_, PyAny>) -> Option<Matrix> {
    if let (Ok(a), Ok(b), Ok(c), Ok(d), Ok(e), Ok(f)) = (
        float_attr(obj, "a"),
        float_attr(obj, "b"),
        float_attr(obj, "c"),
        float_attr(obj, "d"),
        float_attr(obj, "e"),
        float_attr(obj, "f"),
    ) {
        return Some(Matrix { a, b, c, d, e, f });
    }
    let items: Vec<Bound<'_, PyAny>> = obj.extract().ok()?;
    if items.len() != 6 {
        return None;
    }
    let mut out = [0.0f64; 6];
    for (slot, item) in out.iter_mut().zip(items.iter()) {
        *slot = float_value(item)?;
    }
    Some(Matrix {
        a: out[0],
        b: out[1],
        c: out[2],
        d: out[3],
        e: out[4],
        f: out[5],
    })
}

/// Read a colour: a sequence of channel bytes; non-sequences and
/// out-of-range values are type errors, length is the core's business.
fn extract_color(obj: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    let items: Vec<Bound<'_, PyAny>> = obj
        .extract()
        .map_err(|_| PyTypeError::new_err("colour must be a sequence of bytes"))?;
    let mut out = Vec::with_capacity(items.len());
    for item in &items {
        out.push(
            item.extract::<u8>()
                .map_err(|_| PyTypeError::new_err("colour values must be bytes"))?,
        );
    }
    Ok(out)
}

/// Construct a sibling-lane rectangle through the public package.
///
/// Geometry classes belong to the lane that owns them; resolving them at
/// call time keeps this lane compiling while that lane lands.
fn public_rect(py: Python<'_>, x0: f64, y0: f64, x1: f64, y1: f64) -> PyResult<Py<PyAny>> {
    let module = py.import("pdfluent")?;
    Ok(module.getattr("Rect")?.call1((x0, y0, x1, y1))?.unbind())
}

/// Construct a sibling-lane matrix through the public package.
fn public_matrix(
    py: Python<'_>,
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
) -> PyResult<Py<PyAny>> {
    let module = py.import("pdfluent")?;
    Ok(module
        .getattr("Matrix")?
        .call1((a, b, c, d, e, f))?
        .unbind())
}

/// Read a shared instance through the public package.
fn public_attr(py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
    Ok(py.import("pdfluent")?.getattr(name)?.unbind())
}

/// Compatible colour space.
#[pyclass(name = "Colorspace")]
pub(crate) struct PyColorspace {
    inner: core::Colorspace,
}

#[pymethods]
impl PyColorspace {
    #[new]
    fn new(kind: &Bound<'_, PyAny>) -> Self {
        // The recordings never fail here: 2 and 3 select grey and CMYK,
        // everything else reads as RGB.
        let code = kind.extract::<i32>().unwrap_or(1);
        Self {
            inner: core::Colorspace::new(code),
        }
    }

    /// Channel count, excluding alpha.
    #[getter]
    fn n(&self) -> i32 {
        self.inner.n()
    }

    /// Recorded colour-space name.
    #[getter]
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn __str__(&self) -> PyResult<String> {
        self.tag()
    }

    fn __repr__(&self) -> PyResult<String> {
        self.tag()
    }

    fn __richcmp__(
        &self,
        other: &Bound<'_, PyAny>,
        op: PyCompareOp,
        py: Python<'_>,
    ) -> PyResult<Py<PyAny>> {
        let own = self.inner;
        match op {
            PyCompareOp::Eq => {
                let same = other
                    .extract::<PyRef<'_, Self>>()
                    .map(|o| o.inner == own)
                    .unwrap_or(false);
                Ok(pyo3::types::PyBool::new(py, same)
                    .to_owned()
                    .into_any()
                    .unbind())
            }
            PyCompareOp::Ne => {
                let same = other
                    .extract::<PyRef<'_, Self>>()
                    .map(|o| o.inner == own)
                    .unwrap_or(false);
                Ok(pyo3::types::PyBool::new(py, !same)
                    .to_owned()
                    .into_any()
                    .unbind())
            }
            _ => Err(PyTypeError::new_err("colorspaces cannot be ordered")),
        }
    }
}

impl PyColorspace {
    /// Recorded tag: constant name plus colour-space name.
    fn tag(&self) -> PyResult<String> {
        let constant = match self.inner {
            core::Colorspace::Gray => "CS_GRAY",
            core::Colorspace::Rgb => "CS_RGB",
            core::Colorspace::Cmyk => "CS_CMYK",
            _ => return Err(crate::compat::unknown_variant("colour space")),
        };
        Ok(format!("Colorspace({constant}) - {}", self.inner.name()))
    }
}

/// Compatible pixel buffer.
#[pyclass(name = "Pixmap")]
pub(crate) struct PyPixmap {
    inner: core::Pixmap,
}

/// Read a pixmap argument: this lane's `Pixmap`, whatever lane it came
/// from. Other lanes take pixmaps by duck-typed handle and funnel through
/// here rather than naming this class.
pub(crate) fn coerce_pixmap(value: &Bound<'_, PyAny>) -> PyResult<core::Pixmap> {
    if let Ok(own) = value.extract::<PyRef<'_, PyPixmap>>() {
        return Ok(own.inner.clone());
    }
    Err(PyTypeError::new_err("pixmap must be a Pixmap".to_string()))
}

impl PyPixmap {
    /// Colour space in colour-space position: class or kind code.
    fn colorspace_arg(obj: &Bound<'_, PyAny>) -> Option<core::Colorspace> {
        if let Ok(known) = obj.extract::<PyRef<'_, PyColorspace>>() {
            return Some(known.inner);
        }
        obj.extract::<i32>().ok().map(core::Colorspace::new)
    }

    /// Alpha flag: integers only, anything else is a type error.
    fn alpha_flag(obj: &Bound<'_, PyAny>) -> PyResult<bool> {
        obj.extract::<i32>()
            .map(|v| v != 0)
            .map_err(|_| PyTypeError::new_err("alpha flag must be an integer"))
    }

    /// Bytes-like argument: bytes, bytearray, or anything with `tobytes`.
    fn bytes_arg(obj: &Bound<'_, PyAny>) -> PyResult<Option<Vec<u8>>> {
        // Byte extraction alone accepts any int sequence, boxes included;
        // only true bytes-likes count here (l3-pixmap-init-blank).
        if obj.is_instance_of::<pyo3::types::PyBytes>()
            || obj.is_instance_of::<pyo3::types::PyByteArray>()
        {
            if let Ok(bytes) = obj.extract::<Vec<u8>>() {
                return Ok(Some(bytes));
            }
        }
        if obj.hasattr("tobytes").unwrap_or(false) {
            let out = obj.call_method0("tobytes")?;
            return Ok(Some(out.extract::<Vec<u8>>().map_err(|_| {
                PyTypeError::new_err("tobytes did not return bytes")
            })?));
        }
        Ok(None)
    }

    /// Path-like argument: text or `__fspath__`.
    fn path_arg(obj: &Bound<'_, PyAny>) -> Option<String> {
        if let Ok(text) = obj.extract::<String>() {
            return Some(text);
        }
        // As above: only true bytes-likes read as byte paths, so boxes and
        // other int sequences fall through to the ValueError below.
        if obj.is_instance_of::<pyo3::types::PyBytes>()
            || obj.is_instance_of::<pyo3::types::PyByteArray>()
        {
            if let Ok(bytes) = obj.extract::<Vec<u8>>() {
                return Some(String::from_utf8_lossy(&bytes).into_owned());
            }
        }
        let fspath = obj.getattr("__fspath__").ok()?;
        let out = fspath.call0().ok()?;
        if let Ok(text) = out.extract::<String>() {
            return Some(text);
        }
        out.extract::<Vec<u8>>()
            .ok()
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Single-argument construction: copy, decode, or load.
    fn from_single(obj: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(src) = obj.extract::<PyRef<'_, Self>>() {
            let inner = ensure_alpha(&src.inner, true)
                .map_err(|e| compat_err(e, FaultMap::System, "cannot copy pixmap"))?;
            return Ok(Self { inner });
        }
        // Undecodable bytes travel as the facade's parse failure, which
        // carries no compat kind; pin them to the recorded format error.
        let decode_err = |e: pdfluent::Error, context: &str| {
            if matches!(e, pdfluent::Error::InvalidPdf { .. }) {
                FzErrorFormat::new_err(format!("{context}: {e}"))
            } else {
                compat_err(e, FaultMap::System, context)
            }
        };
        if let Some(bytes) = Self::bytes_arg(obj)? {
            let inner = core::Pixmap::from_bytes(&bytes)
                .map_err(|e| decode_err(e, "cannot decode image"))?;
            return Ok(Self { inner });
        }
        if let Some(path) = Self::path_arg(obj) {
            let inner =
                core::Pixmap::from_file(&path).map_err(|e| decode_err(e, "cannot load image"))?;
            return Ok(Self { inner });
        }
        if coerce_irect(obj).is_some() || Self::colorspace_arg(obj).is_some() {
            return Err(PyValueError::new_err(
                "a colour space and a box need each other",
            ));
        }
        Err(PyException::new_err("unrecognized Pixmap arguments"))
    }

    /// Two-argument construction: blank, convert, or copy with a flag.
    fn from_double(first: &Bound<'_, PyAny>, second: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Some(space) = Self::colorspace_arg(first) {
            if let Some(area) = coerce_irect(second) {
                let inner = core::Pixmap::new(space, area, false)
                    .map_err(|e| compat_err(e, FaultMap::System, "cannot make pixmap"))?;
                return Ok(Self { inner });
            }
            if let Ok(src) = second.extract::<PyRef<'_, Self>>() {
                return Self::convert(&src.inner, space, None);
            }
            return Err(PyException::new_err("unrecognized Pixmap arguments"));
        }
        if let Ok(src) = first.extract::<PyRef<'_, Self>>() {
            let alpha = Self::alpha_flag(second)?;
            let inner = ensure_alpha(&src.inner, alpha)
                .map_err(|e| compat_err(e, FaultMap::System, "cannot copy pixmap"))?;
            return Ok(Self { inner });
        }
        if let Ok(doc) = first.extract::<PyRef<'_, PyDocument>>() {
            let xref = second
                .extract::<i32>()
                .map_err(|_| PyException::new_err("unrecognized Pixmap arguments"))?;
            let guard = doc.init_lopdf()?;
            let parsed = guard.as_ref().unwrap();
            let inner = core::pixmap_from_image_in(parsed, xref)
                .map_err(|e| compat_err(e, FaultMap::System, "cannot read image"))?;
            return Ok(Self { inner });
        }
        Err(PyException::new_err("unrecognized Pixmap arguments"))
    }

    /// Three-argument construction: blank or convert with an alpha flag.
    fn from_triple(
        first: &Bound<'_, PyAny>,
        second: &Bound<'_, PyAny>,
        third: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        if let Some(space) = Self::colorspace_arg(first) {
            if let Some(area) = coerce_irect(second) {
                let alpha = Self::alpha_flag(third)?;
                let inner = core::Pixmap::new(space, area, alpha)
                    .map_err(|e| compat_err(e, FaultMap::System, "cannot make pixmap"))?;
                return Ok(Self { inner });
            }
            if let Ok(src) = second.extract::<PyRef<'_, Self>>() {
                let alpha = Self::alpha_flag(third)?;
                return Self::convert(&src.inner, space, Some(alpha));
            }
        }
        Err(PyException::new_err("unrecognized Pixmap arguments"))
    }

    /// Row-major RGB bytes of the samples, whatever the colour space.
    fn pixmap_rgb(&self) -> PyResult<(Vec<u8>, u32, u32)> {
        let (w, h) = (self.inner.width(), self.inner.height());
        if w <= 0 || h <= 0 {
            return Err(PyValueError::new_err("pixmap has no pixels"));
        }
        let stride = self.inner.stride();
        let samples = self.inner.samples();
        let channels = self.inner.n().max(1) as usize;
        let colour = self.inner.colorspace().components() as usize;
        let mut rgb = Vec::with_capacity(w as usize * h as usize * 3);
        for row in 0..h as usize {
            let line = samples.get(row * stride..).unwrap_or(&[]);
            for px in 0..w as usize {
                let cell = line.get(px * channels..).unwrap_or(&[]);
                let at = |i: usize| cell.get(i).copied().unwrap_or(0);
                match colour {
                    1 => rgb.extend_from_slice(&[at(0), at(0), at(0)]),
                    4 => rgb.extend_from_slice(&cmyk_to_rgb(at(0), at(1), at(2), at(3))),
                    _ => rgb.extend_from_slice(&[at(0), at(1), at(2)]),
                }
            }
        }
        Ok((rgb, w as u32, h as u32))
    }

    /// One-page searchable PDF of the pixmap: the samples as the page
    /// image plus the recognizer's invisible text layer.
    fn pdfocr_bytes(
        &self,
        compress: bool,
        language: &str,
        tessdata: Option<&str>,
    ) -> PyResult<Vec<u8>> {
        #[cfg(feature = "ocr-cli")]
        return self.pdfocr_bytes_ocr(compress, language, tessdata);
        #[cfg(not(feature = "ocr-cli"))]
        {
            let _ = (compress, language, tessdata);
            Err(PyModuleNotFoundError::new_err(
                "cannot recognize text: text recognition is not available in this build",
            ))
        }
    }

    /// OCR engine path behind `pdfocr_bytes`: single-image document plus
    /// the recognizer's invisible text layer.
    #[cfg(feature = "ocr-cli")]
    fn pdfocr_bytes_ocr(
        &self,
        compress: bool,
        language: &str,
        tessdata: Option<&str>,
    ) -> PyResult<Vec<u8>> {
        {
            use lopdf::{dictionary, Document as LoDoc, Object, Stream};
            let (rgb, w, h) = self.pixmap_rgb()?;
            let mut image = Stream::new(
                dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Image",
                    "Width" => i64::from(w),
                    "Height" => i64::from(h),
                    "ColorSpace" => "DeviceRGB",
                    "BitsPerComponent" => 8,
                },
                rgb.clone(),
            );
            if compress {
                image.compress().map_err(|e| {
                    PyRuntimeError::new_err(format!("cannot compress page image: {e}"))
                })?;
            }
            let mut doc = LoDoc::with_version("1.7");
            let image_id = doc.new_object_id();
            doc.objects.insert(image_id, Object::Stream(image));
            let content = format!("q\n{w} 0 0 {h} 0 0 cm\n/Im0 Do\nQ\n").into_bytes();
            let content_id = doc.new_object_id();
            doc.objects.insert(
                content_id,
                Object::Stream(Stream::new(dictionary! {}, content)),
            );
            let pages_id = doc.new_object_id();
            let page_id = doc.new_object_id();
            doc.objects.insert(
                page_id,
                Object::Dictionary(dictionary! {
                    "Type" => "Page",
                    "Parent" => pages_id,
                    "MediaBox" => vec![0.into(), 0.into(), Object::Integer(i64::from(w)), Object::Integer(i64::from(h))],
                    "Contents" => content_id,
                    "Resources" => dictionary! {
                        "XObject" => dictionary! { "Im0" => image_id },
                    },
                }),
            );
            doc.objects.insert(
                pages_id,
                Object::Dictionary(dictionary! {
                    "Type" => "Pages",
                    "Kids" => vec![Object::Reference(page_id)],
                    "Count" => 1,
                }),
            );
            let catalog_id = doc.new_object_id();
            doc.objects.insert(
                catalog_id,
                Object::Dictionary(dictionary! {
                    "Type" => "Catalog",
                    "Pages" => pages_id,
                }),
            );
            doc.trailer.set("Root", catalog_id);
            if let Some(dir) = tessdata {
                std::env::set_var("TESSDATA_PREFIX", dir);
            }
            let engine = pdf_ocr::TesseractCliEngine::new(language)
                .map_err(|e| PyRuntimeError::new_err(format!("cannot recognize text: {e}")))?;
            let dpi = self.inner.xres().max(1) as u32;
            let config = pdf_ocr::OcrConfig {
                dpi,
                ..pdf_ocr::OcrConfig::default()
            };
            pdf_ocr::make_searchable(&mut doc, &engine, &config, |_, _, _| {
                Ok((rgb.clone(), w, h))
            })
            .map_err(|e| PyRuntimeError::new_err(format!("cannot recognize text: {e}")))?;
            let mut out = Vec::new();
            doc.save_to(&mut out)
                .map_err(|e| PyRuntimeError::new_err(format!("cannot serialise OCR pdf: {e}")))?;
            Ok(out)
        }
    }

    /// Convert a pixmap into another colour space, keeping resolution.
    fn convert(src: &core::Pixmap, space: core::Colorspace, alpha: Option<bool>) -> PyResult<Self> {
        let want_alpha = alpha.unwrap_or_else(|| src.alpha());
        if space == src.colorspace() {
            let inner = ensure_alpha(src, want_alpha)
                .map_err(|e| compat_err(e, FaultMap::System, "cannot copy pixmap"))?;
            return Ok(Self { inner });
        }
        let channels = src.n().max(1) as usize;
        let mut samples = Vec::with_capacity(
            (src.width().max(0) as usize)
                * (src.height().max(0) as usize)
                * (3 + usize::from(want_alpha)),
        );
        for px in src.samples().chunks_exact(channels) {
            let (r, g, b) = match src.colorspace() {
                core::Colorspace::Gray => (px[0], px[0], px[0]),
                core::Colorspace::Rgb => (px[0], px[1], px[2]),
                core::Colorspace::Cmyk => {
                    let rgb = cmyk_to_rgb(px[0], px[1], px[2], px[3]);
                    (rgb[0], rgb[1], rgb[2])
                }
                _ => return Err(crate::compat::unknown_variant("source colour space")),
            };
            match space {
                core::Colorspace::Gray => samples.push(luma(r, g, b)),
                core::Colorspace::Rgb => samples.extend_from_slice(&[r, g, b]),
                core::Colorspace::Cmyk => samples.extend_from_slice(&rgb_to_cmyk(r, g, b)),
                _ => return Err(crate::compat::unknown_variant("target colour space")),
            }
            if want_alpha {
                samples.push(if src.alpha() { px[channels - 1] } else { 255 });
            }
        }
        let inner = core::Pixmap::from_samples(
            space,
            src.irect(),
            want_alpha,
            samples,
            src.xres(),
            src.yres(),
        )
        .map_err(|e| compat_err(e, FaultMap::System, "cannot convert pixmap"))?;
        Ok(Self { inner })
    }

    /// Box argument or a type error.
    fn box_arg(obj: &Bound<'_, PyAny>) -> PyResult<IRect> {
        coerce_irect(obj)
            .ok_or_else(|| PyTypeError::new_err("box must be a rectangle or four integers"))
    }

    /// Optional box argument or a type error.
    fn opt_box_arg(obj: Option<&Bound<'_, PyAny>>) -> PyResult<Option<IRect>> {
        obj.map(Self::box_arg).transpose()
    }

    /// Build the imaging-library image for the `pil_*` family.
    fn pil_object(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let module = py.import("PIL.Image")?;
        let mode = match (self.inner.colorspace(), self.inner.alpha()) {
            (core::Colorspace::Gray, false) => "L",
            (core::Colorspace::Gray, true) => "LA",
            (core::Colorspace::Rgb, false) => "RGB",
            (core::Colorspace::Rgb, true) => "RGBA",
            (core::Colorspace::Cmyk, _) => "CMYK",
            _ => return Err(crate::compat::unknown_variant("colour space")),
        };
        let bytes = PyBytes::new(py, self.inner.samples());
        let size = (self.inner.width(), self.inner.height());
        let image = module.call_method1("frombytes", (mode, size, bytes))?;
        if self.inner.colorspace() == core::Colorspace::Cmyk && self.inner.alpha() {
            // No CMYK-with-alpha mode exists; drop what cannot travel.
            let _ = image;
            let rgb = module.call_method1(
                "frombytes",
                ("RGB", size, PyBytes::new(py, &self.flattened_rgb())),
            )?;
            return Ok(rgb.unbind());
        }
        Ok(image.unbind())
    }

    /// RGB flattening for the CMYK-with-alpha imaging-library corner.
    fn flattened_rgb(&self) -> Vec<u8> {
        let channels = self.inner.n().max(1) as usize;
        let mut out = Vec::with_capacity(
            (self.inner.width().max(0) as usize) * (self.inner.height().max(0) as usize) * 3,
        );
        for px in self.inner.samples().chunks_exact(channels) {
            let rgb = cmyk_to_rgb(px[0], px[1], px[2], px[3]);
            if self.inner.alpha() {
                let a = u32::from(px[channels - 1]);
                for v in rgb {
                    out.push(((u32::from(v) * a + 255 * (255 - a) + 127) / 255).min(255) as u8);
                }
            } else {
                out.extend_from_slice(&rgb);
            }
        }
        out
    }
}

/// Copy a pixmap, ensuring or dropping the alpha channel.
fn ensure_alpha(src: &core::Pixmap, want: bool) -> Result<core::Pixmap, pdfluent::Error> {
    if src.alpha() == want {
        return Ok(src.clone());
    }
    let mut out = core::Pixmap::new(src.colorspace(), src.irect(), want)?;
    let area = src.irect();
    out.copy(src, area)?;
    out.set_dpi(src.xres(), src.yres())?;
    Ok(out)
}

/// Rec.601 luma, rounded half up. Mirrors the core conversion.
fn luma(r: u8, g: u8, b: u8) -> u8 {
    ((u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114 + 500) / 1000).min(255) as u8
}

/// Naive CMYK ink complements to sRGB, rounded half up. Mirrors the core.
fn cmyk_to_rgb(c: u8, m: u8, y: u8, k: u8) -> [u8; 3] {
    let inv = 255 - u32::from(k);
    [
        ((255 - u32::from(c)) * inv + 127) / 255,
        ((255 - u32::from(m)) * inv + 127) / 255,
        ((255 - u32::from(y)) * inv + 127) / 255,
    ]
    .map(|v| v.min(255) as u8)
}

/// Naive sRGB complements to CMYK ink, rounded half up. Mirrors the core.
fn rgb_to_cmyk(r: u8, g: u8, b: u8) -> [u8; 4] {
    let (r, g, b) = (u32::from(r), u32::from(g), u32::from(b));
    let k = 255 - r.max(g).max(b);
    if k == 255 {
        return [0, 0, 0, 255];
    }
    let denom = 255 - k;
    [
        ((255 - r - k) * 255 + denom / 2) / denom,
        ((255 - g - k) * 255 + denom / 2) / denom,
        ((255 - b - k) * 255 + denom / 2) / denom,
        k,
    ]
    .map(|v| v.min(255) as u8)
}

#[pymethods]
impl PyPixmap {
    #[new]
    #[pyo3(signature = (*args, **kwargs))]
    fn new(args: &Bound<'_, PyTuple>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        if kwargs.is_some_and(|k| !k.is_empty()) {
            return Err(PyTypeError::new_err("Pixmap takes no keyword arguments"));
        }
        match args.len() {
            0 => Err(PyException::new_err(
                "Pixmap needs a source, or a colour space and a box",
            )),
            1 => Self::from_single(&args.get_item(0)?),
            2 => Self::from_double(&args.get_item(0)?, &args.get_item(1)?),
            3 => Self::from_triple(&args.get_item(0)?, &args.get_item(1)?, &args.get_item(2)?),
            _ => Err(PyException::new_err("too many arguments for Pixmap")),
        }
    }

    /// Width in pixels.
    #[getter]
    fn width(&self) -> i32 {
        self.inner.width()
    }

    /// Width in pixels.
    #[getter]
    fn w(&self) -> i32 {
        self.inner.w()
    }

    /// Height in pixels.
    #[getter]
    fn height(&self) -> i32 {
        self.inner.height()
    }

    /// Height in pixels.
    #[getter]
    fn h(&self) -> i32 {
        self.inner.h()
    }

    /// Components per pixel.
    #[getter]
    fn n(&self) -> i32 {
        self.inner.n()
    }

    /// Whether an alpha channel exists, as an integer flag.
    #[getter]
    fn alpha(&self) -> i32 {
        i32::from(self.inner.alpha())
    }

    /// Row length in bytes.
    #[getter]
    fn stride(&self) -> usize {
        self.inner.stride()
    }

    /// Sample length in bytes.
    #[getter]
    fn size(&self) -> usize {
        self.inner.size()
    }

    /// Horizontal box origin.
    #[getter]
    fn x(&self) -> i32 {
        self.inner.x()
    }

    /// Vertical box origin.
    #[getter]
    fn y(&self) -> i32 {
        self.inner.y()
    }

    /// Horizontal resolution.
    #[getter]
    fn xres(&self) -> i32 {
        self.inner.xres()
    }

    /// Vertical resolution.
    #[getter]
    fn yres(&self) -> i32 {
        self.inner.yres()
    }

    /// Pixel colour space: the shared preset instance.
    #[getter]
    fn colorspace(&self, py: Python<'_>) -> Py<PyAny> {
        let name = match self.inner.colorspace() {
            core::Colorspace::Gray => Some("csGRAY"),
            core::Colorspace::Rgb => Some("csRGB"),
            core::Colorspace::Cmyk => Some("csCMYK"),
            // No shared preset for a space this binding does not know: hand
            // out a fresh instance of the real value instead of a neighbour.
            _ => None,
        };
        name.and_then(|name| public_attr(py, name).ok())
            .unwrap_or_else(|| {
                PyColorspace {
                    inner: self.inner.colorspace(),
                }
                .into_pyobject(py)
                .map(|bound| bound.into_any().unbind())
                .unwrap_or_else(|_| py.None())
            })
    }

    /// Raw samples.
    #[getter]
    fn samples(&self, py: Python<'_>) -> Py<PyAny> {
        PyBytes::new(py, self.inner.samples()).into_any().unbind()
    }

    /// Raw samples as a view.
    #[getter]
    fn samples_mv(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let bytes = PyBytes::new(py, self.inner.samples());
        Ok(PyMemoryView::from(bytes.as_any())?.into_any().unbind())
    }

    /// Pointer to the samples.
    #[getter]
    fn samples_ptr(&self) -> usize {
        self.inner.samples_ptr() as usize
    }

    /// Pixel box as a four-tuple.
    #[getter]
    fn irect(&self) -> (i32, i32, i32, i32) {
        let area = self.inner.irect();
        (area.x0, area.y0, area.x1, area.y1)
    }

    /// Sample digest.
    #[getter]
    fn digest(&self, py: Python<'_>) -> Py<PyAny> {
        PyBytes::new(py, &self.inner.digest()).into_any().unbind()
    }

    /// Whether single-colour, as an integer flag.
    #[getter]
    fn is_monochrome(&self) -> i32 {
        i32::from(self.inner.is_monochrome())
    }

    /// Whether one flat colour.
    #[getter]
    fn is_unicolor(&self) -> bool {
        self.inner.is_unicolor()
    }

    fn __len__(&self) -> usize {
        self.inner.len()
    }

    /// Identity comparison: equality is identity, ordering raises.
    ///
    /// The recordings pin `False`/`True` for equality against a non-pixmap
    /// and a `TypeError` raise for ordering even when the slot is invoked
    /// directly, so ordering raises instead of returning `NotImplemented`.
    fn __richcmp__(
        slf: PyRef<'_, Self>,
        other: &Bound<'_, PyAny>,
        op: PyCompareOp,
        py: Python<'_>,
    ) -> PyResult<Py<PyAny>> {
        let peer = other.extract::<PyRef<'_, Self>>().ok();
        let same = peer
            .as_ref()
            .is_some_and(|o| std::ptr::eq(slf.as_ptr(), o.as_ptr()));
        match op {
            PyCompareOp::Eq => Ok(PyBool::new(py, same).to_owned().into_any().unbind()),
            PyCompareOp::Ne => Ok(PyBool::new(py, !same).to_owned().into_any().unbind()),
            _ => Err(PyTypeError::new_err("pixmaps cannot be ordered")),
        }
    }

    /// Fill a box with a value.
    #[pyo3(signature = (value=None, clip=None))]
    fn clear_with(&mut self, value: Option<u8>, clip: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let area = Self::opt_box_arg(clip)?;
        self.inner
            .clear_with(value, area)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "cannot clear pixmap"))
    }

    /// Distinct colour count.
    #[pyo3(signature = (colors=0, clip=None))]
    fn color_count(&self, colors: i32, clip: Option<&Bound<'_, PyAny>>) -> PyResult<i32> {
        let area = Self::opt_box_arg(clip)?;
        self.inner
            .color_count(colors, area)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "cannot count colours"))
    }

    /// Most used colour and its share of the pixels.
    #[pyo3(signature = (clip=None))]
    fn color_topusage(
        &self,
        py: Python<'_>,
        clip: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<(f64, Py<PyAny>)> {
        let area = Self::opt_box_arg(clip)?;
        let (count, color) = self
            .inner
            .color_topusage(area)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "cannot rank colours"))?;
        let total = (self.inner.width().max(0) as f64) * (self.inner.height().max(0) as f64);
        let share = if total > 0.0 {
            f64::from(count) / total
        } else {
            0.0
        };
        Ok((share, PyBytes::new(py, &color).into_any().unbind()))
    }

    /// Copy a box from another pixmap.
    fn copy(&mut self, src: &Self, bbox: &Bound<'_, PyAny>) -> PyResult<()> {
        let area = Self::box_arg(bbox)?;
        self.inner
            .copy(&src.inner, area)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "cannot copy"))
    }

    /// Apply gamma correction.
    fn gamma_with(&mut self, gamma: f64) -> PyResult<()> {
        self.inner
            .gamma_with(gamma)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "cannot apply gamma"))
    }

    /// Invert samples in a box.
    #[pyo3(signature = (bbox=None))]
    fn invert_irect(&mut self, bbox: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let area = Self::opt_box_arg(bbox)?;
        self.inner
            .invert_irect(area)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "cannot invert"))
    }

    /// Sample at a pixel.
    fn pixel(&self, x: i32, y: i32) -> PyResult<Vec<i32>> {
        self.inner
            .pixel(x, y)
            .map(|px| px.into_iter().map(i32::from).collect())
            .map_err(|e| compat_err(e, FaultMap::Runtime, "cannot read pixel"))
    }

    /// Save to a file.
    #[pyo3(signature = (filename, output=None, jpg_quality=95))]
    fn save(&self, filename: &str, output: Option<&str>, jpg_quality: i32) -> PyResult<()> {
        self.inner
            .save(filename, output, jpg_quality)
            .map_err(|e| compat_err(e, FaultMap::System, "cannot save pixmap"))
    }

    /// Set the alpha channel.
    #[pyo3(signature = (alphavalues=None, premultiply=1, opaque=None, matte=None))]
    fn set_alpha(
        &mut self,
        alphavalues: Option<&Bound<'_, PyAny>>,
        premultiply: i32,
        opaque: Option<&Bound<'_, PyAny>>,
        matte: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let values = match alphavalues {
            None => None,
            Some(value) => {
                if let Ok(bytes) = value.extract::<Vec<u8>>() {
                    Some(bytes)
                } else if value.hasattr("tobytes").unwrap_or(false) {
                    let out = value.call_method0("tobytes")?;
                    Some(
                        out.extract::<Vec<u8>>()
                            .map_err(|_| PyAssertionError::new_err("alpha values must be bytes"))?,
                    )
                } else {
                    return Err(PyAssertionError::new_err("alpha values must be bytes"));
                }
            }
        };
        let optional = |obj: Option<&Bound<'_, PyAny>>| -> PyResult<Option<CompatValue>> {
            obj.map(|o| {
                extract_color(o).map(|colors| {
                    CompatValue::List(
                        colors
                            .into_iter()
                            .map(|v| CompatValue::Int(i64::from(v)))
                            .collect(),
                    )
                })
            })
            .transpose()
        };
        let mut options = core::AlphaOptions::default();
        options.values = values;
        options.premultiply = premultiply != 0;
        options.opaque = optional(opaque)?;
        options.matte = optional(matte)?;
        self.inner
            .set_alpha(&options)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "cannot set alpha"))
    }

    /// Set the resolution.
    fn set_dpi(&mut self, xres: i32, yres: i32) -> PyResult<()> {
        self.inner
            .set_dpi(xres, yres)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "cannot set resolution"))
    }

    /// Set the box origin.
    fn set_origin(&mut self, x: i32, y: i32) -> PyResult<()> {
        self.inner
            .set_origin(x, y)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "cannot set origin"))
    }

    /// Set one pixel.
    fn set_pixel(&mut self, x: i32, y: i32, color: &Bound<'_, PyAny>) -> PyResult<()> {
        let color = extract_color(color)?;
        self.inner
            .set_pixel(x, y, &color)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "cannot set pixel"))
    }

    /// Fill a box with a colour.
    fn set_rect(&mut self, bbox: &Bound<'_, PyAny>, color: &Bound<'_, PyAny>) -> PyResult<bool> {
        let area = Self::box_arg(bbox)?;
        let color = extract_color(color)?;
        self.inner
            .set_rect(area, &color)
            .map(|_| true)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "cannot fill box"))
    }

    /// Downsample by a factor.
    fn shrink(&mut self, factor: i32) -> PyResult<()> {
        self.inner
            .shrink(factor)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "cannot shrink"))
    }

    /// Tint the samples.
    fn tint_with(&mut self, black: i32, white: i32) -> PyResult<()> {
        self.inner
            .tint_with(black, white)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "cannot tint"))
    }

    /// Encode to bytes.
    #[pyo3(signature = (output="png", jpg_quality=95))]
    fn tobytes(&self, py: Python<'_>, output: &str, jpg_quality: i32) -> PyResult<Py<PyAny>> {
        let bytes = self
            .inner
            .tobytes(output, jpg_quality)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "cannot encode pixmap"))?;
        Ok(PyBytes::new(py, &bytes).into_any().unbind())
    }

    /// Warped copy over a quad.
    ///
    /// The quad travels as a plain four-sequence; anything else, quad
    /// objects included, is refused.
    fn warp(&self, quad: &Bound<'_, PyAny>, width: i32, height: i32) -> PyResult<Self> {
        let is_seq = quad.is_instance_of::<pyo3::types::PyList>()
            || quad.is_instance_of::<pyo3::types::PyTuple>();
        if !is_seq {
            return Err(PyTypeError::new_err(
                "warp quad must be a sequence of four points",
            ));
        }
        let items: Vec<Bound<'_, PyAny>> = quad
            .extract()
            .map_err(|_| PyTypeError::new_err("warp quad must hold four points"))?;
        if items.len() != 4 {
            return Err(PyTypeError::new_err("warp quad must hold four points"));
        }
        let mut corners = [(0.0f64, 0.0f64); 4];
        for (slot, item) in corners.iter_mut().zip(items.iter()) {
            *slot = warp_point(item)?;
        }
        let quad = Quad {
            ul: Point {
                x: corners[0].0,
                y: corners[0].1,
            },
            ur: Point {
                x: corners[1].0,
                y: corners[1].1,
            },
            ll: Point {
                x: corners[2].0,
                y: corners[2].1,
            },
            lr: Point {
                x: corners[3].0,
                y: corners[3].1,
            },
        };
        let inner = self
            .inner
            .warp(quad, width, height)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "cannot warp"))?;
        Ok(Self { inner })
    }

    /// Save an OCR PDF of the pixmap.
    #[pyo3(signature = (filename, compress=true, language=None, tessdata=None))]
    fn pdfocr_save(
        &self,
        filename: &str,
        compress: bool,
        language: Option<&str>,
        tessdata: Option<&str>,
    ) -> PyResult<()> {
        let bytes = self.pdfocr_bytes(compress, language.unwrap_or("eng"), tessdata)?;
        std::fs::write(filename, bytes)
            .map_err(|e| FzErrorSystem::new_err(format!("cannot write OCR pdf {filename}: {e}")))?;
        Ok(())
    }

    /// OCR PDF bytes of the pixmap.
    #[pyo3(signature = (compress=true, language="eng", tessdata=None))]
    fn pdfocr_tobytes(
        &self,
        py: Python<'_>,
        compress: bool,
        language: &str,
        tessdata: Option<&str>,
    ) -> PyResult<Py<PyAny>> {
        let bytes = self.pdfocr_bytes(compress, language, tessdata)?;
        Ok(PyBytes::new(py, &bytes).into_any().unbind())
    }

    /// Imaging-library image of the samples.
    fn pil_image(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.pil_object(py)
    }

    /// Encode through the imaging library.
    #[pyo3(signature = (*args, **kwargs))]
    fn pil_tobytes(
        &self,
        py: Python<'_>,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        let image = self.pil_object(py)?;
        let empty;
        let kwargs = match &kwargs {
            Some(bound) => bound,
            None => {
                empty = PyDict::new(py);
                &empty
            }
        };
        if kwargs.get_item("format")?.is_none() {
            let first = (0..args.len())
                .find_map(|i| args.get_item(i).ok())
                .and_then(|item| item.extract::<String>().ok());
            kwargs.set_item("format", first.as_deref().unwrap_or("PNG"))?;
        }
        let buffer = py.import("io")?.call_method0("BytesIO")?;
        image.call_method(py, "save", (buffer.clone(),), Some(kwargs))?;
        Ok(buffer.call_method0("getvalue")?.unbind())
    }

    /// Save through the imaging library.
    #[pyo3(signature = (*args, **kwargs))]
    fn pil_save(
        &self,
        py: Python<'_>,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        let image = self.pil_object(py)?;
        let empty;
        let kwargs = match &kwargs {
            Some(bound) => bound,
            None => {
                empty = PyDict::new(py);
                &empty
            }
        };
        let target = (0..args.len())
            .find_map(|i| args.get_item(i).ok())
            .map(|item| item.unbind())
            .or_else(|| {
                kwargs
                    .get_item("filename")
                    .ok()
                    .flatten()
                    .map(|item| item.unbind())
            })
            .ok_or_else(|| PyTypeError::new_err("pil_save needs a filename"))?;
        image.call_method(py, "save", (target,), Some(kwargs))?;
        Ok(())
    }
}

/// Read one warp point: a two-sequence or duck-typed coordinates.
fn warp_point(obj: &Bound<'_, PyAny>) -> PyResult<(f64, f64)> {
    if let (Ok(x), Ok(y)) = (float_attr(obj, "x"), float_attr(obj, "y")) {
        return Ok((x, y));
    }
    let items: Vec<Bound<'_, PyAny>> = obj
        .extract()
        .map_err(|_| PyTypeError::new_err("warp points need two numbers"))?;
    if items.len() != 2 {
        return Err(PyTypeError::new_err("warp points need two numbers"));
    }
    let x = float_value(&items[0])
        .ok_or_else(|| PyTypeError::new_err("warp points need two numbers"))?;
    let y = float_value(&items[1])
        .ok_or_else(|| PyTypeError::new_err("warp points need two numbers"))?;
    Ok((x, y))
}

/// Colour space in render position: class, kind code, or nothing.
/// Anything else falls back to the default instead of failing.
fn render_colorspace(obj: &Bound<'_, PyAny>) -> Option<core::Colorspace> {
    if let Ok(known) = obj.extract::<PyRef<'_, PyColorspace>>() {
        return Some(known.inner);
    }
    if obj.is_none() {
        return None;
    }
    obj.extract::<i32>().ok().map(core::Colorspace::new)
}

/// Build render options from binding arguments, leniently.
///
/// Unparseable matrices, resolutions, colour spaces, and clips fall back
/// to their defaults.
#[allow(clippy::too_many_arguments)]
fn render_args(
    matrix: Option<&Bound<'_, PyAny>>,
    dpi: Option<&Bound<'_, PyAny>>,
    colorspace: Option<&Bound<'_, PyAny>>,
    clip: Option<&Bound<'_, PyAny>>,
    alpha: bool,
    annots: bool,
) -> PyResult<core::RenderOptions> {
    let mut options = core::RenderOptions::default();
    if let Some(found) = matrix.and_then(coerce_matrix) {
        options = options.with_matrix(found);
    }
    if let Some(found) = dpi.and_then(float_value) {
        options = options.with_dpi(found);
    }
    if let Some(found) = colorspace.and_then(render_colorspace) {
        options = options.with_colorspace(found);
    }
    if let Some(area) = clip.and_then(coerce_rect) {
        // Clips share the pixmap frame: top-left, y down, no flip. The
        // clipped thumbnail pins the top region (l3-page-get-pixmap-clip).
        options = options.with_clip(area);
    }
    options = options.with_alpha(alpha).with_annots(annots);
    Ok(options)
}

/// Image rows as tuples, with a trailing zero for full listings.
fn image_rows(py: Python<'_>, rows: Vec<core::ImageRef>, full: bool) -> PyResult<Py<PyAny>> {
    let out = pyo3::types::PyList::empty(py);
    for row in rows {
        if full {
            // Trailing field is 0 in every recording, including rows with a
            // mask; this listing is page-direct, so no referring object.
            out.append((
                row.xref,
                row.mask,
                row.width,
                row.height,
                row.bits,
                row.colorspace_name,
                row.alt,
                row.name,
                row.filter,
                0,
            ))?;
        } else {
            out.append((
                row.xref,
                row.mask,
                row.width,
                row.height,
                row.bits,
                row.colorspace_name,
                row.alt,
                row.name,
                row.filter,
            ))?;
        }
    }
    Ok(out.into_any().unbind())
}

impl PyPage {
    /// Object store over the engine's source bytes.
    fn parsed(&self) -> PyResult<lopdf::Document> {
        let bytes: &[u8] = self.doc.pdf().data().as_ref();
        lopdf::Document::load_mem(bytes)
            .map_err(|e| PyRuntimeError::new_err(format!("cannot re-read page bytes: {e}")))
    }
}

#[pymethods]
impl PyDocument {
    /// Image bytes and header.
    fn extract_image(&self, py: Python<'_>, xref: i32) -> PyResult<Py<PyAny>> {
        let guard = self.init_lopdf()?;
        let doc = guard.as_ref().unwrap();
        let data = core::extract_image_in(doc, xref)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "cannot extract image"))?;
        let smask = if data.masked {
            core::image_mask_xref(doc, xref).unwrap_or(0)
        } else {
            0
        };
        let dict = PyDict::new(py);
        dict.set_item("bpc", data.bits)?;
        dict.set_item("colorspace", data.colorspace)?;
        dict.set_item("cs-name", data.colorspace_name)?;
        dict.set_item("ext", data.ext)?;
        dict.set_item("height", data.height)?;
        dict.set_item("image", PyBytes::new(py, &data.data))?;
        dict.set_item("size", data.size)?;
        dict.set_item("smask", smask)?;
        dict.set_item("width", data.width)?;
        dict.set_item("xres", data.xres)?;
        dict.set_item("yres", data.yres)?;
        Ok(dict.into_any().unbind())
    }

    /// Images on a page.
    #[pyo3(signature = (pno, full=false))]
    fn get_page_images(&self, py: Python<'_>, pno: i32, full: bool) -> PyResult<Py<PyAny>> {
        let guard = self.init_lopdf()?;
        let doc = guard.as_ref().unwrap();
        let page = core::page_number_id(doc, pno)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "bad page number"))?;
        let rows = core::page_images_in(doc, page)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "cannot list images"))?;
        image_rows(py, rows, full)
    }

    /// Rendered pixmap of a page.
    #[pyo3(signature = (pno, *, matrix=None, dpi=None, colorspace=None, clip=None, alpha=false, annots=true))]
    #[allow(clippy::too_many_arguments)]
    fn get_page_pixmap(
        &self,
        pno: i32,
        matrix: Option<&Bound<'_, PyAny>>,
        dpi: Option<&Bound<'_, PyAny>>,
        colorspace: Option<&Bound<'_, PyAny>>,
        clip: Option<&Bound<'_, PyAny>>,
        alpha: bool,
        annots: bool,
    ) -> PyResult<PyPixmap> {
        if pno < 0 {
            return Err(PyValueError::new_err("bad page number"));
        }
        let index = pno as usize;
        let options = render_args(matrix, dpi, colorspace, clip, alpha, annots)?;
        let engine = self.engine()?;
        let inner = core::render_pixmap(&engine, index, &options)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "cannot render page"))?;
        Ok(PyPixmap { inner })
    }

    /// Rewrite images in place.
    #[pyo3(signature = (dpi_threshold=None, dpi_target=0, quality=0, lossy=true, lossless=true, bitonal=true, color=true, gray=true, set_to_gray=false, options=None))]
    #[allow(clippy::too_many_arguments)]
    fn rewrite_images(
        &self,
        dpi_threshold: Option<i32>,
        dpi_target: i32,
        quality: i32,
        lossy: bool,
        lossless: bool,
        bitonal: bool,
        color: bool,
        gray: bool,
        set_to_gray: bool,
        options: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let _ = options;
        let mut options = core::RewriteOptions::default();
        if let Some(threshold) = dpi_threshold {
            options = options.with_dpi_threshold(threshold);
        }
        options = options
            .with_dpi_target(dpi_target)
            .with_quality(quality)
            .with_lossy(lossy)
            .with_lossless(lossless)
            .with_bitonal(bitonal)
            .with_color(color)
            .with_gray(gray)
            .with_set_to_gray(set_to_gray);
        let mut guard = self.init_lopdf()?;
        let doc = guard.as_mut().unwrap();
        core::rewrite_images_in(doc, &options)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "cannot rewrite images"))
    }

    /// Whether an object is an image.
    fn xref_is_image(&self, xref: i32) -> PyResult<bool> {
        let guard = self.init_lopdf()?;
        let doc = guard.as_ref().unwrap();
        core::xref_is_image_in(doc, xref)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "bad reference"))
    }
}

#[pymethods]
impl PyPage {
    /// Images on the page.
    #[pyo3(signature = (full=false))]
    fn get_images(&self, py: Python<'_>, full: bool) -> PyResult<Py<PyAny>> {
        let doc = self.parsed()?;
        let page = core::page_object_id(&doc, self.index)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "bad page number"))?;
        let rows = core::page_images_in(&doc, page)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "cannot list images"))?;
        image_rows(py, rows, full)
    }

    /// Placed images with facts.
    #[pyo3(signature = (hashes=false, xrefs=false))]
    fn get_image_info(&self, py: Python<'_>, hashes: bool, xrefs: bool) -> PyResult<Py<PyAny>> {
        let with = hashes || xrefs;
        let doc = self.parsed()?;
        let page = core::page_object_id(&doc, self.index)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "bad page number"))?;
        let placements = core::page_placements_in(&doc, page, with)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "cannot list images"))?;
        let out = pyo3::types::PyList::empty(py);
        for placement in placements {
            let info = &placement.image;
            let dict = PyDict::new(py);
            dict.set_item(
                "bbox",
                (info.bbox.x0, info.bbox.y0, info.bbox.x1, info.bbox.y1),
            )?;
            dict.set_item("bpc", info.bits)?;
            dict.set_item("colorspace", placement.comps)?;
            dict.set_item("cs-name", &info.colorspace_name)?;
            if with {
                if let Some(digest) = &info.digest {
                    dict.set_item("digest", PyBytes::new(py, digest))?;
                }
                dict.set_item("xref", placement.xref)?;
            }
            dict.set_item("has-mask", info.has_mask)?;
            dict.set_item("height", info.height)?;
            dict.set_item("number", info.number)?;
            dict.set_item("size", info.size)?;
            dict.set_item(
                "transform",
                (
                    info.transform.a,
                    info.transform.b,
                    info.transform.c,
                    info.transform.d,
                    info.transform.e,
                    info.transform.f,
                ),
            )?;
            dict.set_item("width", info.width)?;
            dict.set_item("xres", info.xres)?;
            dict.set_item("yres", info.yres)?;
            out.append(dict)?;
        }
        Ok(out.into_any().unbind())
    }

    /// Box of a named image, with its matrix on request.
    #[pyo3(signature = (name, transform=0))]
    fn get_image_bbox(&self, py: Python<'_>, name: &str, transform: i32) -> PyResult<Py<PyAny>> {
        let doc = self.parsed()?;
        let page = core::page_object_id(&doc, self.index)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "bad page number"))?;
        let (bbox, matrix) = core::page_image_bbox_in(&doc, page, name)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "cannot find image"))?;
        let rect = public_rect(py, bbox.x0, bbox.y0, bbox.x1, bbox.y1)?;
        if transform != 0 {
            let mapped = public_matrix(
                py, matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f,
            )?;
            Ok((rect, mapped).into_pyobject(py)?.into_any().unbind())
        } else {
            Ok(rect)
        }
    }

    /// Boxes of a named image, with matrices on request.
    #[pyo3(signature = (name, transform=false))]
    fn get_image_rects(&self, py: Python<'_>, name: &str, transform: bool) -> PyResult<Py<PyAny>> {
        let doc = self.parsed()?;
        let page = core::page_object_id(&doc, self.index)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "bad page number"))?;
        let placements = core::page_placements_in(&doc, page, false)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "cannot list images"))?;
        let matches: Vec<_> = placements.iter().filter(|p| p.name == name).collect();
        if matches.is_empty() {
            return Err(PyValueError::new_err(format!("unknown image '{name}'")));
        }
        let with_matrix = transform;
        let out = pyo3::types::PyList::empty(py);
        for placement in matches {
            let bbox = &placement.image.bbox;
            let rect = public_rect(py, bbox.x0, bbox.y0, bbox.x1, bbox.y1)?;
            if with_matrix {
                let matrix = &placement.image.transform;
                let mapped = public_matrix(
                    py, matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f,
                )?;
                out.append((rect, mapped).into_pyobject(py)?.into_any())?;
            } else {
                out.append(rect)?;
            }
        }
        Ok(out.into_any().unbind())
    }

    /// Non-image XObjects on the page.
    fn get_xobjects(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let doc = self.parsed()?;
        let page = core::page_object_id(&doc, self.index)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "bad page number"))?;
        let rows = core::page_form_xobject_rows(&doc, page)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "cannot list objects"))?;
        let out = pyo3::types::PyList::empty(py);
        for (xref, name, generation, bbox) in rows {
            out.append((xref, name, generation, [bbox[0], bbox[1], bbox[2], bbox[3]]))?;
        }
        Ok(out.into_any().unbind())
    }

    /// Rendered pixmap of the page.
    #[pyo3(signature = (*, matrix=None, dpi=None, colorspace=None, clip=None, alpha=false, annots=true))]
    #[allow(clippy::too_many_arguments)]
    fn get_pixmap(
        &self,
        matrix: Option<&Bound<'_, PyAny>>,
        dpi: Option<&Bound<'_, PyAny>>,
        colorspace: Option<&Bound<'_, PyAny>>,
        clip: Option<&Bound<'_, PyAny>>,
        alpha: bool,
        annots: bool,
    ) -> PyResult<PyPixmap> {
        let options = render_args(matrix, dpi, colorspace, clip, alpha, annots)?;
        let inner = core::render_pixmap(&self.doc, self.index, &options)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "cannot render page"))?;
        Ok(PyPixmap { inner })
    }

    /// SVG image of the page.
    #[pyo3(signature = (matrix=None, text_as_path=1))]
    fn get_svg_image(
        &self,
        matrix: Option<&Bound<'_, PyAny>>,
        text_as_path: i32,
    ) -> PyResult<String> {
        let matrix = matrix.and_then(coerce_matrix);
        core::svg_for_page(&self.doc, self.index, matrix, text_as_path != 0)
            .map_err(|e| compat_err(e, FaultMap::Runtime, "cannot depict page"))
    }
}

/// Matrix placing an image into a target box.
#[pyfunction]
#[pyo3(signature = (width, height, target, rotate, keep))]
fn calc_image_matrix(
    py: Python<'_>,
    width: f64,
    height: f64,
    target: &Bound<'_, PyAny>,
    rotate: i32,
    keep: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    if width == 0.0 || height == 0.0 {
        return Err(PyZeroDivisionError::new_err(
            "calc_image_matrix() needs a nonzero image size",
        ));
    }
    let rect =
        coerce_rect(target).ok_or_else(|| PyTypeError::new_err("target must be a rectangle"))?;
    let matrix = core::calc_image_matrix(width, height, rect, rotate, keep.is_truthy()?);
    public_matrix(
        py, matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f,
    )
}

/// Sniffed facts for image bytes: empty reads bare, anything else refuses.
#[pyfunction]
fn image_profile(py: Python<'_>, img: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    if img.is_none() || img.extract::<String>().is_ok() {
        return Err(PyValueError::new_err("img must be image bytes"));
    }
    let bytes: Vec<u8> = img
        .extract()
        .map_err(|_| PyTypeError::new_err("img must be image bytes"))?;
    if bytes.is_empty() {
        return Ok(py.None());
    }
    Err(PyTypeError::new_err(
        "image_profile() needs a profile this build does not keep",
    ))
}

/// Register the L3 lane's new module-level names.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    use pdfluent::compat::constants as lane_constants;
    m.add_class::<PyPixmap>()?;
    m.add_class::<PyColorspace>()?;
    m.add("CS_CMYK", lane_constants::CS_CMYK)?;
    m.add("CS_GRAY", lane_constants::CS_GRAY)?;
    m.add("CS_RGB", lane_constants::CS_RGB)?;
    m.add(
        "csCMYK",
        PyColorspace {
            inner: core::Colorspace::Cmyk,
        },
    )?;
    m.add(
        "csGRAY",
        PyColorspace {
            inner: core::Colorspace::Gray,
        },
    )?;
    m.add(
        "csRGB",
        PyColorspace {
            inner: core::Colorspace::Rgb,
        },
    )?;
    m.add("FZ_RECOMPRESS_FAX", lane_constants::FZ_RECOMPRESS_FAX)?;
    m.add("FZ_RECOMPRESS_J2K", lane_constants::FZ_RECOMPRESS_J2K)?;
    m.add("FZ_RECOMPRESS_JPEG", lane_constants::FZ_RECOMPRESS_JPEG)?;
    m.add(
        "FZ_RECOMPRESS_LOSSLESS",
        lane_constants::FZ_RECOMPRESS_LOSSLESS,
    )?;
    m.add("FZ_RECOMPRESS_NEVER", lane_constants::FZ_RECOMPRESS_NEVER)?;
    m.add("FZ_RECOMPRESS_SAME", lane_constants::FZ_RECOMPRESS_SAME)?;
    m.add("FZ_SUBSAMPLE_AVERAGE", lane_constants::FZ_SUBSAMPLE_AVERAGE)?;
    m.add("FZ_SUBSAMPLE_BICUBIC", lane_constants::FZ_SUBSAMPLE_BICUBIC)?;
    m.add_function(pyo3::wrap_pyfunction!(calc_image_matrix, m)?)?;
    m.add_function(pyo3::wrap_pyfunction!(image_profile, m)?)?;
    Ok(())
}
