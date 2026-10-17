//! Door lane L4: images and graphics (SPEC sections 7, 8, 9).
//!
//! The figure listing over the native document: figures with their rendered
//! pixels, per-page figure boxes, and one page's own image boxes. The
//! methods hang on the document and page classes beside their siblings;
//! only the figure record itself is registered here.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

use crate::{engine_err_to_py, extract_options, pdfluent_err_to_py, PyDocument, PyPage};
use pdf_engine::markdown_images::Figure;

/// One figure of the document, with its rendered pixels.
///
/// A placed image or a drawn vector cluster that stands beside prose: the
/// page, the box in PDF points, the file name the Markdown references it
/// by, and the box rendered at 150 dots per inch, encoded as PNG.
#[pyclass(name = "Figure")]
struct PyFigure {
    inner: Figure,
}

#[pymethods]
impl PyFigure {
    /// 0-based page index the figure stands on.
    #[getter]
    fn page(&self) -> usize {
        self.inner.page
    }

    /// Bounding box in PDF points, `[x0, y0, x1, y1]` with y up.
    #[getter]
    fn bbox(&self) -> [f64; 4] {
        self.inner.bbox
    }

    /// The figure's file name, `figure-<page>-<ordinal>.png`.
    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }

    /// The figure's box rendered at 150 dots per inch, as PNG bytes.
    #[getter]
    fn png_bytes(&self) -> &[u8] {
        &self.inner.png
    }
}

#[pymethods]
impl PyDocument {
    /// Every figure of the window, in reading order, with rendered pixels.
    ///
    /// The figure blocks ``extract_blocks`` lists under the same window,
    /// each with its box rendered at 150 dots per inch and encoded as PNG.
    /// ``first_page`` and ``page_count`` choose a window of pages;
    /// ``page_count=None`` means the rest of the document. ``pages`` is an
    /// explicit list of 0-based page indices instead of the window.
    ///
    /// Returns
    /// -------
    /// list[Figure]
    ///     One entry per figure, in reading order.
    #[pyo3(signature = (first_page = 0, page_count = None, pages = None))]
    fn extract_figures(
        &self,
        first_page: usize,
        page_count: Option<usize>,
        pages: Option<Vec<usize>>,
    ) -> PyResult<Vec<PyFigure>> {
        let options = extract_options(first_page, page_count, pages);
        Ok(self
            .engine()?
            .extract_figures(&options)
            .map_err(engine_err_to_py)?
            .into_iter()
            .map(|inner| PyFigure { inner })
            .collect())
    }

    /// Every page's figure boxes, in page order.
    ///
    /// The boxes the page places its images into, in drawing order, then
    /// the boxes of the vector clusters it draws, each
    /// ``[x0, y0, x1, y1]`` in PDF points. A scan of its own over the
    /// pages, capped at the first fifty: one entry per page either way,
    /// and pages past the cap report no figures.
    ///
    /// Returns
    /// -------
    /// list[list[list[float]]]
    ///     One list of boxes per page, in page order.
    fn figure_boxes(&self) -> PyResult<Vec<Vec<[f64; 4]>>> {
        self.engine()?
            .extract_image_boxes()
            .map_err(engine_err_to_py)
    }
}

#[pymethods]
impl PyPage {
    /// The boxes this page places its images into, in drawing order.
    ///
    /// Each ``[x0, y0, x1, y1]`` in PDF points. Placed images only -- the
    /// vector clusters the page draws are figures of the document, not
    /// pictures of the page.
    fn image_boxes(&self) -> PyResult<Vec<[f64; 4]>> {
        self.doc
            .page_image_boxes(self.index)
            .map_err(engine_err_to_py)
    }
}

/// Register this lane's native items on the module.
pub(crate) fn register(m: &Bound<PyModule>) -> PyResult<()> {
    m.add_class::<PyFigure>()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Plain embedded-image API: the facade's image calls on `Document`.
// ---------------------------------------------------------------------------

/// Open the plain facade over the document's current bytes.
///
/// The door holds the engine document plus a separate mutation handle;
/// the facade rebuilds from the bytes as they stand now, mutations
/// included, the way the Office exporters already do.
fn facade_of(doc: &PyDocument) -> PyResult<pdfluent::PdfDocument> {
    let raw = doc.current_bytes()?;
    pdfluent::PdfDocument::from_bytes(&raw).map_err(pdfluent_err_to_py)
}

#[pymethods]
impl PyDocument {
    /// Stored images on one page, as plain dicts.
    ///
    /// One dict per image XObject in the page's resources, in resource
    /// order, with the keys ``id``, ``width``, ``height``,
    /// ``bits_per_component``, ``components``, ``color_space``,
    /// ``color_space_name``, ``filter``, ``name`` and ``has_mask``.
    /// ``page`` is a 0-based index; the facade behind this call numbers
    /// pages from 1, and the boundary adds the one.
    ///
    /// Parameters
    /// ----------
    /// page:
    ///     0-based page index.
    ///
    /// Returns
    /// -------
    /// list[dict]
    ///     One dict per stored image, in resource order.
    ///
    /// Raises
    /// ------
    /// PdfluentError
    ///     With ``code == "E-PAGE-OUT-OF-RANGE"`` past the last page.
    fn page_images<'py>(&self, py: Python<'py>, page: usize) -> PyResult<Vec<Bound<'py, PyDict>>> {
        let facade = facade_of(self)?;
        let Some(number) = page.checked_add(1) else {
            return Err(pdfluent_err_to_py(pdfluent::Error::PageOutOfRange {
                requested: page,
                page_count: facade.page_count(),
            }));
        };
        let images = facade.page_images(number).map_err(pdfluent_err_to_py)?;
        images
            .iter()
            .map(|image| {
                let dict = PyDict::new(py);
                dict.set_item("id", image.id.as_u32())?;
                dict.set_item("width", image.width)?;
                dict.set_item("height", image.height)?;
                dict.set_item("bits_per_component", image.bits_per_component)?;
                dict.set_item("components", image.components)?;
                dict.set_item("color_space", image.color_space.name())?;
                dict.set_item("color_space_name", &image.color_space_name)?;
                dict.set_item("filter", &image.filter)?;
                dict.set_item("name", &image.name)?;
                dict.set_item("has_mask", image.has_mask)?;
                Ok(dict)
            })
            .collect()
    }

    /// One stored image's encoded bytes with its header facts.
    ///
    /// JPEG payloads pass through untouched; everything else arrives
    /// decoded and re-encoded as PNG. The dict carries ``width``,
    /// ``height``, ``bits_per_component``, ``components``,
    /// ``color_space``, ``color_space_name``, ``encoding`` (``"jpeg"`` or
    /// ``"png"``), ``data`` (bytes), ``masked``, ``xres`` and ``yres``.
    ///
    /// Parameters
    /// ----------
    /// image_id:
    ///     The ``id`` a ``page_images`` dict reported.
    ///
    /// Returns
    /// -------
    /// dict
    ///     Header facts plus the encoded ``data`` bytes.
    fn extract_image_data<'py>(
        &self,
        py: Python<'py>,
        image_id: u32,
    ) -> PyResult<Bound<'py, PyDict>> {
        let facade = facade_of(self)?;
        let image = facade
            .extract_image_data(pdfluent::ImageId::from(image_id))
            .map_err(pdfluent_err_to_py)?;
        let dict = PyDict::new(py);
        dict.set_item("width", image.width)?;
        dict.set_item("height", image.height)?;
        dict.set_item("bits_per_component", image.bits_per_component)?;
        dict.set_item("components", image.components)?;
        dict.set_item("color_space", image.color_space.name())?;
        dict.set_item("color_space_name", &image.color_space_name)?;
        dict.set_item("encoding", &image.encoding)?;
        dict.set_item("data", PyBytes::new(py, &image.data))?;
        dict.set_item("masked", image.masked)?;
        dict.set_item("xres", image.xres)?;
        dict.set_item("yres", image.yres)?;
        Ok(dict)
    }

    /// One stored image decoded to raw samples, top row first.
    ///
    /// Palettes and stencils are expanded. The dict carries ``width``,
    /// ``height``, ``components``, ``color_space`` and ``pixels`` (bytes,
    /// ``components`` bytes per pixel).
    ///
    /// Parameters
    /// ----------
    /// image_id:
    ///     The ``id`` a ``page_images`` dict reported.
    ///
    /// Returns
    /// -------
    /// dict
    ///     Size, layout, and the raw ``pixels`` bytes.
    fn decode_image_pixels<'py>(
        &self,
        py: Python<'py>,
        image_id: u32,
    ) -> PyResult<Bound<'py, PyDict>> {
        let facade = facade_of(self)?;
        let image = facade
            .decode_image_pixels(pdfluent::ImageId::from(image_id))
            .map_err(pdfluent_err_to_py)?;
        let dict = PyDict::new(py);
        dict.set_item("width", image.width)?;
        dict.set_item("height", image.height)?;
        dict.set_item("components", image.components)?;
        dict.set_item("color_space", image.color_space.name())?;
        dict.set_item("pixels", PyBytes::new(py, &image.pixels))?;
        Ok(dict)
    }
}
