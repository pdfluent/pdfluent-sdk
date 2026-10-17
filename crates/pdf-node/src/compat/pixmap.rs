//! Compatible images and rasters (lane L3).
// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use super::document::{Document, Page};
use super::geometry::{IRect, IRectLike, Matrix, MatrixLike, QuadLike, Rect, RectLike};
use super::*;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use pdfluent::compat as fc;

#[napi]
pub struct Pixmap {
    pub(crate) inner: fc::Pixmap,
}

#[napi]
pub struct Colorspace {
    pub(crate) inner: fc::Colorspace,
}

impl Colorspace {
    pub(crate) fn to_compat(&self) -> fc::Colorspace {
        self.inner
    }
}

#[napi]
impl Colorspace {
    /// Colorspace.__eq__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "eq")]
    pub fn eq(&self, other: &Colorspace) -> bool {
        self.inner.name() == other.inner.name()
    }

    /// Colorspace.__ge__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "ge")]
    pub fn ge(&self, other: &Colorspace) -> napi::Result<bool> {
        let op = "ge";
        self.inner
            .compare(&other.inner, fc::CompareOp::GreaterOrEqual)
            .map_err(crate::error::facade(op))
    }

    /// Colorspace.__gt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "gt")]
    pub fn gt(&self, other: &Colorspace) -> napi::Result<bool> {
        let op = "gt";
        self.inner
            .compare(&other.inner, fc::CompareOp::Greater)
            .map_err(crate::error::facade(op))
    }

    /// Colorspace.__init__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(constructor)]
    pub fn new(kind: i32) -> Self {
        Self {
            inner: fc::Colorspace::new(kind),
        }
    }

    /// Colorspace.__le__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "le")]
    pub fn le(&self, other: &Colorspace) -> napi::Result<bool> {
        let op = "le";
        self.inner
            .compare(&other.inner, fc::CompareOp::LessOrEqual)
            .map_err(crate::error::facade(op))
    }

    /// Colorspace.__lt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "lt")]
    pub fn lt(&self, other: &Colorspace) -> napi::Result<bool> {
        let op = "lt";
        self.inner
            .compare(&other.inner, fc::CompareOp::Less)
            .map_err(crate::error::facade(op))
    }

    /// Colorspace.__ne__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "ne")]
    pub fn ne(&self, other: &Colorspace) -> bool {
        self.inner.name() != other.inner.name()
    }

    /// Colorspace.n from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "n")]
    pub fn n(&self) -> napi::Result<i32> {
        let out = self.inner.n();
        Ok(out)
    }

    /// Colorspace.name from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "name")]
    pub fn name(&self) -> napi::Result<String> {
        let out = self.inner.name();
        Ok(out.to_string())
    }
}

#[napi]
impl Document {
    /// Document.extract_image from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "extractImage")]
    pub fn extract_image(&self, xref: i32) -> napi::Result<serde_json::Value> {
        let op = "extractImage";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard
            .extract_image(xref)
            .map_err(crate::error::facade(op))?;
        Ok(image_data_to_json(&out))
    }

    /// Document.get_page_images from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getPageImages")]
    pub fn get_page_images(
        &self,
        page: i32,
        full: Option<bool>,
    ) -> napi::Result<Vec<serde_json::Value>> {
        let op = "getPageImages";
        self.shared.check_open(op)?;
        let full_v = full.unwrap_or(false);
        let guard = self.shared.lock(op)?;
        let out = guard
            .get_page_images(page, full_v)
            .map_err(crate::error::facade(op))?;
        Ok(out.into_iter().map(|x| image_ref_to_json(&x)).collect())
    }

    /// Document.get_page_pixmap from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getPagePixmap")]
    pub fn get_page_pixmap(
        &self,
        page: i32,
        options: Option<serde_json::Value>,
        colorspace: Option<&Colorspace>,
    ) -> napi::Result<Pixmap> {
        let op = "getPagePixmap";
        self.shared.check_open(op)?;
        let mut options_o = render_options_from_json(op, options.as_ref())?;
        let guard = self.shared.lock(op)?;
        if let Some(c) = colorspace.as_ref() {
            options_o = options_o.with_colorspace(c.to_compat());
        }
        let out = guard
            .get_page_pixmap(page, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(Pixmap { inner: out })
    }

    /// Document.rewrite_images from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "rewriteImages")]
    pub fn rewrite_images(&self, options: Option<serde_json::Value>) -> napi::Result<()> {
        let op = "rewriteImages";
        self.shared.check_open(op)?;
        let options_o = rewrite_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        guard
            .rewrite_images(&options_o)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.xref_is_image from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "xrefIsImage")]
    pub fn xref_is_image(&self, xref: i32) -> napi::Result<bool> {
        let op = "xrefIsImage";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard
            .xref_is_image(xref)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }
}

#[napi]
impl Page {
    /// Page.get_image_bbox from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getImageBbox")]
    pub fn get_image_bbox(&self, name: String, transform: Option<bool>) -> napi::Result<Rect> {
        let op = "getImageBbox";
        self.shared.check_open(op)?;
        let transform_v = transform.unwrap_or(false);
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view
            .get_image_bbox(&name, transform_v)
            .map_err(crate::error::facade(op))?;
        Ok(Rect { inner: out })
    }

    /// Page.get_image_info from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getImageInfo")]
    pub fn get_image_info(
        &self,
        hashes: Option<bool>,
        xrefs: Option<bool>,
    ) -> napi::Result<Vec<serde_json::Value>> {
        let op = "getImageInfo";
        self.shared.check_open(op)?;
        let hashes_v = hashes.unwrap_or(false);
        let xrefs_v = xrefs.unwrap_or(false);
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view
            .get_image_info(hashes_v, xrefs_v)
            .map_err(crate::error::facade(op))?;
        Ok(out.into_iter().map(|x| placed_image_to_json(&x)).collect())
    }

    /// Page.get_image_rects from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getImageRects")]
    pub fn get_image_rects(
        &self,
        name: String,
        transform: Option<bool>,
    ) -> napi::Result<Vec<Rect>> {
        let op = "getImageRects";
        self.shared.check_open(op)?;
        let transform_v = transform.unwrap_or(false);
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view
            .get_image_rects(&name, transform_v)
            .map_err(crate::error::facade(op))?;
        Ok(out.into_iter().map(|x| Rect { inner: x }).collect())
    }

    /// Page.get_images from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getImages")]
    pub fn get_images(&self, full: Option<bool>) -> napi::Result<Vec<serde_json::Value>> {
        let op = "getImages";
        self.shared.check_open(op)?;
        let full_v = full.unwrap_or(false);
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view.get_images(full_v).map_err(crate::error::facade(op))?;
        Ok(out.into_iter().map(|x| image_ref_to_json(&x)).collect())
    }

    /// Page.get_pixmap from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getPixmap")]
    pub fn get_pixmap(
        &self,
        options: Option<serde_json::Value>,
        colorspace: Option<&Colorspace>,
    ) -> napi::Result<Pixmap> {
        let op = "getPixmap";
        self.shared.check_open(op)?;
        let mut options_o = render_options_from_json(op, options.as_ref())?;
        let guard = self.shared.lock(op)?;
        if let Some(c) = colorspace.as_ref() {
            options_o = options_o.with_colorspace(c.to_compat());
        }
        let view = fc::Page::new(&guard, self.index);
        let out = view
            .get_pixmap(&options_o)
            .map_err(crate::error::facade(op))?;
        Ok(Pixmap { inner: out })
    }

    /// Page.get_svg_image from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getSvgImage")]
    pub fn get_svg_image(
        &self,
        matrix: Option<MatrixLike>,
        text_as_path: Option<bool>,
    ) -> napi::Result<String> {
        let op = "getSvgImage";
        self.shared.check_open(op)?;
        let matrix_v = matrix.as_ref().map(MatrixLike::to_compat);
        let text_as_path_v = text_as_path.unwrap_or(true);
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view
            .get_svg_image(matrix_v, text_as_path_v)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }
}

#[napi]
impl Pixmap {
    /// Pixmap.__eq__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "eq")]
    pub fn eq(&self, other: &Pixmap) -> bool {
        self.inner == other.inner
    }

    /// Pixmap.__ge__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "ge")]
    pub fn ge(&self, other: &Pixmap) -> napi::Result<bool> {
        let op = "ge";
        self.inner
            .compare(&other.inner, fc::CompareOp::GreaterOrEqual)
            .map_err(crate::error::facade(op))
    }

    /// Pixmap.__gt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "gt")]
    pub fn gt(&self, other: &Pixmap) -> napi::Result<bool> {
        let op = "gt";
        self.inner
            .compare(&other.inner, fc::CompareOp::Greater)
            .map_err(crate::error::facade(op))
    }

    /// Pixmap.__init__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(constructor)]
    pub fn new(
        colorspace: &Colorspace,
        bounds: IRectLike,
        alpha: Option<bool>,
    ) -> napi::Result<Self> {
        let op = "Pixmap";
        let out = fc::Pixmap::new(
            colorspace.to_compat(),
            bounds.to_compat(),
            alpha.unwrap_or(false),
        )
        .map_err(crate::error::facade(op))?;
        Ok(Self { inner: out })
    }

    /// Open an image pixmap from encoded bytes.
    #[allow(clippy::too_many_arguments, clippy::should_implement_trait)]
    #[napi(js_name = "fromBytes", factory)]
    pub fn from_bytes(data: Buffer) -> napi::Result<Self> {
        let op = "fromBytes";
        let bytes = Vec::<u8>::from(data);
        let out = fc::Pixmap::from_bytes(&bytes).map_err(crate::error::facade(op))?;
        Ok(Self { inner: out })
    }

    /// Open an image pixmap from a file path.
    #[allow(clippy::too_many_arguments, clippy::should_implement_trait)]
    #[napi(js_name = "fromFile", factory)]
    pub fn from_file(path: String) -> napi::Result<Self> {
        let op = "fromFile";
        let out = fc::Pixmap::from_file(&path).map_err(crate::error::facade(op))?;
        Ok(Self { inner: out })
    }

    /// Copy a pixmap, optionally changing its alpha flag.
    #[allow(clippy::too_many_arguments, clippy::should_implement_trait)]
    #[napi(js_name = "fromPixmap", factory)]
    pub fn from_pixmap(src: &Pixmap, alpha: Option<bool>) -> napi::Result<Self> {
        let op = "fromPixmap";
        let bounds = src.inner.irect();
        let mut out = fc::Pixmap::new(
            src.inner.colorspace(),
            bounds,
            alpha.unwrap_or(src.inner.alpha()),
        )
        .map_err(crate::error::facade(op))?;
        out.copy(&src.inner, bounds)
            .map_err(crate::error::facade(op))?;
        Ok(Self { inner: out })
    }

    /// Convert a pixmap to another colorspace.
    ///
    /// Not yet wired: the facade has no convert entry point. Raises until it grows one.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "convert", factory)]
    pub fn convert(_colorspace: &Colorspace, _src: &Pixmap) -> napi::Result<Self> {
        let op = "convert";
        Err(compat_error(
            op,
            fc::ErrorKind::EngineFault,
            format!("{op}: pixmap conversion is not available in this build"),
        ))
    }

    /// Pixmap.__le__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "le")]
    pub fn le(&self, other: &Pixmap) -> napi::Result<bool> {
        let op = "le";
        self.inner
            .compare(&other.inner, fc::CompareOp::LessOrEqual)
            .map_err(crate::error::facade(op))
    }

    /// Pixmap.__len__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::len_without_is_empty)]
    #[napi(js_name = "len")]
    pub fn len(&self) -> napi::Result<u32> {
        let out = self.inner.len();
        Ok(out as u32)
    }

    /// Pixmap.__lt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "lt")]
    pub fn lt(&self, other: &Pixmap) -> napi::Result<bool> {
        let op = "lt";
        self.inner
            .compare(&other.inner, fc::CompareOp::Less)
            .map_err(crate::error::facade(op))
    }

    /// Pixmap.__ne__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "ne")]
    pub fn ne(&self, other: &Pixmap) -> bool {
        self.inner != other.inner
    }

    /// Pixmap.alpha from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "alpha")]
    pub fn alpha(&self) -> napi::Result<bool> {
        let out = self.inner.alpha();
        Ok(out)
    }

    /// Pixmap.clear_with from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "clearWith")]
    pub fn clear_with(&mut self, value: Option<u32>, bbox: Option<IRectLike>) -> napi::Result<()> {
        let op = "clearWith";
        let value_v = value.map(|v| v as u8);
        let bbox_v = bbox.as_ref().map(IRectLike::to_compat);
        self.inner
            .clear_with(value_v, bbox_v)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Pixmap.color_count from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "colorCount")]
    pub fn color_count(&self, colors: Option<i32>, clip: Option<IRectLike>) -> napi::Result<i32> {
        let op = "colorCount";
        let colors_v = colors.unwrap_or(0);
        let clip_v = clip.as_ref().map(IRectLike::to_compat);
        let out = self
            .inner
            .color_count(colors_v, clip_v)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Pixmap.color_topusage from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "colorTopusage")]
    pub fn color_topusage(&self, clip: Option<IRectLike>) -> napi::Result<serde_json::Value> {
        let op = "colorTopusage";
        let clip_v = clip.as_ref().map(IRectLike::to_compat);
        let out = self
            .inner
            .color_topusage(clip_v)
            .map_err(crate::error::facade(op))?;
        Ok(tuple_to_json_i32_vecu8(&out))
    }

    /// Pixmap.colorspace from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "colorspace")]
    pub fn colorspace(&self) -> napi::Result<Colorspace> {
        let out = self.inner.colorspace();
        Ok(Colorspace { inner: out })
    }

    /// Pixmap.copy from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "copy")]
    pub fn copy(&mut self, source: &Pixmap, bbox: IRectLike) -> napi::Result<()> {
        let op = "copy";
        self.inner
            .copy(&source.inner, bbox.to_compat())
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Pixmap.digest from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "digest")]
    pub fn digest(&self) -> napi::Result<Buffer> {
        let out = self.inner.digest();
        Ok(Buffer::from(out))
    }

    /// Pixmap.gamma_with from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "gammaWith")]
    pub fn gamma_with(&mut self, gamma: f64) -> napi::Result<()> {
        let op = "gammaWith";
        self.inner
            .gamma_with(gamma)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Pixmap.h from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "h")]
    pub fn h(&self) -> napi::Result<i32> {
        let out = self.inner.h();
        Ok(out)
    }

    /// Pixmap.height from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "height")]
    pub fn height(&self) -> napi::Result<i32> {
        let out = self.inner.height();
        Ok(out)
    }

    /// Pixmap.invert_irect from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "invertIrect")]
    pub fn invert_irect(&mut self, bbox: Option<IRectLike>) -> napi::Result<()> {
        let op = "invertIrect";
        let bbox_v = bbox.as_ref().map(IRectLike::to_compat);
        self.inner
            .invert_irect(bbox_v)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Pixmap.irect from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "irect")]
    pub fn irect(&self) -> napi::Result<IRect> {
        let out = self.inner.irect();
        Ok(IRect { inner: out })
    }

    /// Pixmap.is_monochrome from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "isMonochrome")]
    pub fn is_monochrome(&self) -> napi::Result<bool> {
        let out = self.inner.is_monochrome();
        Ok(out)
    }

    /// Pixmap.is_unicolor from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "isUnicolor")]
    pub fn is_unicolor(&self) -> napi::Result<bool> {
        let out = self.inner.is_unicolor();
        Ok(out)
    }

    /// Pixmap.n from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "n")]
    pub fn n(&self) -> napi::Result<i32> {
        let out = self.inner.n();
        Ok(out)
    }

    /// Pixmap.pdfocr_save from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "pdfocrSave")]
    pub fn pdfocr_save(
        &self,
        path: String,
        compress: Option<bool>,
        language: Option<String>,
        tessdata: Option<String>,
    ) -> napi::Result<()> {
        let op = "pdfocrSave";
        let compress_v = compress.unwrap_or(true);
        self.inner
            .pdfocr_save(&path, compress_v, language.as_deref(), tessdata.as_deref())
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Pixmap.pdfocr_tobytes from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "pdfocrTobytes")]
    pub fn pdfocr_tobytes(
        &self,
        compress: Option<bool>,
        language: String,
        tessdata: Option<String>,
    ) -> napi::Result<Buffer> {
        let op = "pdfocrTobytes";
        let compress_v = compress.unwrap_or(true);
        let out = self
            .inner
            .pdfocr_tobytes(compress_v, &language, tessdata.as_deref())
            .map_err(crate::error::facade(op))?;
        Ok(Buffer::from(out))
    }

    /// Pixmap.pil_image from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "pilImage")]
    pub fn pil_image(&self) -> napi::Result<serde_json::Value> {
        let op = "pilImage";
        let out = self.inner.pil_image().map_err(crate::error::facade(op))?;
        compat_to_json(&out)
    }

    /// Pixmap.pil_save from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "pilSave")]
    pub fn pil_save(&self, path: String, format: Option<String>) -> napi::Result<()> {
        let op = "pilSave";
        self.inner
            .pil_save(&path, format.as_deref())
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Pixmap.pil_tobytes from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "pilTobytes")]
    pub fn pil_tobytes(&self, format: String) -> napi::Result<Buffer> {
        let op = "pilTobytes";
        let out = self
            .inner
            .pil_tobytes(&format)
            .map_err(crate::error::facade(op))?;
        Ok(Buffer::from(out))
    }

    /// Pixmap.pixel from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "pixel")]
    pub fn pixel(&self, x: i32, y: i32) -> napi::Result<Buffer> {
        let op = "pixel";
        let out = self.inner.pixel(x, y).map_err(crate::error::facade(op))?;
        Ok(Buffer::from(out))
    }

    /// Pixmap.samples from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "samples")]
    pub fn samples(&self) -> napi::Result<Buffer> {
        let out = self.inner.samples();
        Ok(Buffer::from(out.to_vec()))
    }

    /// Pixmap.samples_mv from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "samplesMv")]
    pub fn samples_mv(&self) -> napi::Result<Buffer> {
        let out = self.inner.samples_mv();
        Ok(Buffer::from(out.to_vec()))
    }

    /// Pixmap.samples_ptr from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "samplesPtr")]
    pub fn samples_ptr(&self) -> napi::Result<f64> {
        let out = self.inner.samples_ptr();
        Ok(out as usize as f64)
    }

    /// Pixmap.save from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "save")]
    pub fn save(
        &self,
        path: String,
        output: Option<String>,
        jpg_quality: Option<i32>,
    ) -> napi::Result<()> {
        let op = "save";
        let jpg_quality_v = jpg_quality.unwrap_or(95);
        self.inner
            .save(&path, output.as_deref(), jpg_quality_v)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Pixmap.set_alpha from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setAlpha")]
    pub fn set_alpha(&mut self, options: Option<serde_json::Value>) -> napi::Result<()> {
        let op = "setAlpha";
        let options_o = alpha_options_from_json(op, options.as_ref())?;
        self.inner
            .set_alpha(&options_o)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Pixmap.set_dpi from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setDpi")]
    pub fn set_dpi(&mut self, xres: i32, yres: i32) -> napi::Result<()> {
        let op = "setDpi";
        self.inner
            .set_dpi(xres, yres)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Pixmap.set_origin from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setOrigin")]
    pub fn set_origin(&mut self, x: i32, y: i32) -> napi::Result<()> {
        let op = "setOrigin";
        self.inner
            .set_origin(x, y)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Pixmap.set_pixel from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setPixel")]
    pub fn set_pixel(&mut self, x: i32, y: i32, color: Buffer) -> napi::Result<()> {
        let op = "setPixel";
        self.inner
            .set_pixel(x, y, color.as_ref())
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Pixmap.set_rect from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setRect")]
    pub fn set_rect(&mut self, bbox: IRectLike, color: Buffer) -> napi::Result<()> {
        let op = "setRect";
        self.inner
            .set_rect(bbox.to_compat(), color.as_ref())
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Pixmap.shrink from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "shrink")]
    pub fn shrink(&mut self, factor: i32) -> napi::Result<()> {
        let op = "shrink";
        self.inner
            .shrink(factor)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Pixmap.size from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "size")]
    pub fn size(&self) -> napi::Result<u32> {
        let out = self.inner.size();
        Ok(out as u32)
    }

    /// Pixmap.stride from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "stride")]
    pub fn stride(&self) -> napi::Result<u32> {
        let out = self.inner.stride();
        Ok(out as u32)
    }

    /// Pixmap.tint_with from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "tintWith")]
    pub fn tint_with(&mut self, black: i32, white: i32) -> napi::Result<()> {
        let op = "tintWith";
        self.inner
            .tint_with(black, white)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Pixmap.tobytes from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "tobytes")]
    pub fn tobytes(&self, output: String, jpg_quality: Option<i32>) -> napi::Result<Buffer> {
        let op = "tobytes";
        let jpg_quality_v = jpg_quality.unwrap_or(95);
        let out = self
            .inner
            .tobytes(&output, jpg_quality_v)
            .map_err(crate::error::facade(op))?;
        Ok(Buffer::from(out))
    }

    /// Pixmap.w from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "w")]
    pub fn w(&self) -> napi::Result<i32> {
        let out = self.inner.w();
        Ok(out)
    }

    /// Pixmap.warp from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "warp")]
    pub fn warp(&self, quad: QuadLike, width: i32, height: i32) -> napi::Result<Pixmap> {
        let op = "warp";
        let out = self
            .inner
            .warp(quad.to_compat(), width, height)
            .map_err(crate::error::facade(op))?;
        Ok(Pixmap { inner: out })
    }

    /// Pixmap.width from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "width")]
    pub fn width(&self) -> napi::Result<i32> {
        let out = self.inner.width();
        Ok(out)
    }

    /// Pixmap.x from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "x")]
    pub fn x(&self) -> napi::Result<i32> {
        let out = self.inner.x();
        Ok(out)
    }

    /// Pixmap.xres from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "xres")]
    pub fn xres(&self) -> napi::Result<i32> {
        let out = self.inner.xres();
        Ok(out)
    }

    /// Pixmap.y from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "y")]
    pub fn y(&self) -> napi::Result<i32> {
        let out = self.inner.y();
        Ok(out)
    }

    /// Pixmap.yres from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "yres")]
    pub fn yres(&self) -> napi::Result<i32> {
        let out = self.inner.yres();
        Ok(out)
    }
}

/// module.calc_image_matrix from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "calcImageMatrix")]
pub fn calc_image_matrix(
    width: f64,
    height: f64,
    target: RectLike,
    rotate: i32,
    keep: bool,
) -> napi::Result<Matrix> {
    let out = fc::calc_image_matrix(width, height, target.to_compat(), rotate, keep);
    Ok(Matrix { inner: out })
}

/// module.image_profile from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "imageProfile")]
pub fn image_profile(data: Buffer) -> napi::Result<serde_json::Value> {
    let op = "imageProfile";
    let out = fc::image_profile(data.as_ref()).map_err(crate::error::facade(op))?;
    Ok(image_profile_to_json(&out))
}
