//! Compatible page editing (lane L4).
// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use super::document::{Document, Link, Page};
use super::geometry::{Matrix, MatrixLike, Point, PointLike, QuadLike, Rect, RectLike};
use super::pixmap::{Colorspace, Pixmap};
use super::text::TextPage;
use super::*;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use pdfluent::compat as fc;

#[napi]
pub struct Annot {
    pub(crate) shared: std::sync::Arc<Shared>,
    pub(crate) page: usize,
    pub(crate) xref: i32,
}

#[napi]
pub struct Widget {
    pub(crate) inner: fc::Widget,
}

#[napi]
pub struct Shape {
    pub(crate) shared: std::sync::Arc<Shared>,
    pub(crate) page: usize,
}

#[napi]
pub struct Font {
    pub(crate) inner: fc::Font,
}

#[napi]
pub struct TextWriter {
    pub(crate) inner: fc::TextWriter,
}

#[napi]
impl Widget {
    /// Read the field name.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "fieldName")]
    pub fn field_name(&self) -> String {
        self.inner.field_name.clone()
    }

    /// Write the field name.
    #[allow(clippy::too_many_arguments)]
    #[napi(setter, js_name = "fieldName")]
    pub fn set_field_name(&mut self, value: String) {
        self.inner.field_name = value;
    }

    /// Read the field type code.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "fieldType")]
    pub fn field_type(&self) -> i32 {
        self.inner.field_type
    }

    /// Write the field type code.
    #[allow(clippy::too_many_arguments)]
    #[napi(setter, js_name = "fieldType")]
    pub fn set_field_type(&mut self, value: i32) {
        self.inner.field_type = value;
    }

    /// Read the field value.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "fieldValue")]
    pub fn field_value(&self) -> String {
        self.inner.field_value.clone()
    }

    /// Write the field value.
    #[allow(clippy::too_many_arguments)]
    #[napi(setter, js_name = "fieldValue")]
    pub fn set_field_value(&mut self, value: String) {
        self.inner.field_value = value;
    }

    /// Read the field box.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "rect")]
    pub fn rect(&self) -> Rect {
        Rect {
            inner: self.inner.rect,
        }
    }

    /// Write the field box.
    #[allow(clippy::too_many_arguments)]
    #[napi(setter, js_name = "rect")]
    pub fn set_rect(&mut self, value: RectLike) {
        self.inner.rect = value.to_compat();
    }
}

#[napi]
impl Annot {
    /// Annot.__bool__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "bool")]
    pub fn to_bool(&self) -> napi::Result<bool> {
        let op = "bool";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view.to_bool();
        Ok(out)
    }

    /// Annot.__eq__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "eq")]
    pub fn eq(&self, other: &Annot) -> bool {
        std::sync::Arc::ptr_eq(&self.shared, &other.shared)
            && self.page == other.page
            && self.xref == other.xref
    }

    /// Annot.__ge__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "ge")]
    pub fn ge(&self, _other: &Annot) -> napi::Result<bool> {
        let op = "ge";
        self.shared.check_open(op)?;
        Err(compat_error(
            op,
            fc::ErrorKind::BadValue,
            format!("{op}: ordering is not supported for Annot"),
        ))
    }

    /// Annot.__gt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "gt")]
    pub fn gt(&self, _other: &Annot) -> napi::Result<bool> {
        let op = "gt";
        self.shared.check_open(op)?;
        Err(compat_error(
            op,
            fc::ErrorKind::BadValue,
            format!("{op}: ordering is not supported for Annot"),
        ))
    }

    /// Annot.__init__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(constructor)]
    pub fn new(annot: &Annot) -> napi::Result<Self> {
        let op = "Annot";
        annot.shared.check_open(op)?;
        Ok(Self {
            shared: annot.shared.clone(),
            page: annot.page,
            xref: annot.xref,
        })
    }

    /// Annot.__le__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "le")]
    pub fn le(&self, _other: &Annot) -> napi::Result<bool> {
        let op = "le";
        self.shared.check_open(op)?;
        Err(compat_error(
            op,
            fc::ErrorKind::BadValue,
            format!("{op}: ordering is not supported for Annot"),
        ))
    }

    /// Annot.__lt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "lt")]
    pub fn lt(&self, _other: &Annot) -> napi::Result<bool> {
        let op = "lt";
        self.shared.check_open(op)?;
        Err(compat_error(
            op,
            fc::ErrorKind::BadValue,
            format!("{op}: ordering is not supported for Annot"),
        ))
    }

    /// Annot.__ne__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "ne")]
    pub fn ne(&self, other: &Annot) -> bool {
        !(std::sync::Arc::ptr_eq(&self.shared, &other.shared)
            && self.page == other.page
            && self.xref == other.xref)
    }

    /// Annot.apn_bbox from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "apnBbox")]
    pub fn apn_bbox(&self) -> napi::Result<Rect> {
        let op = "apnBbox";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view.apn_bbox().map_err(crate::error::facade(op))?;
        Ok(Rect { inner: out })
    }

    /// Annot.apn_matrix from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "apnMatrix")]
    pub fn apn_matrix(&self) -> napi::Result<Matrix> {
        let op = "apnMatrix";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view.apn_matrix().map_err(crate::error::facade(op))?;
        Ok(Matrix { inner: out })
    }

    /// Annot.blendmode from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "blendmode")]
    pub fn blendmode(&self) -> napi::Result<String> {
        let op = "blendmode";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view.blendmode().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Annot.border from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "border")]
    pub fn border(&self) -> napi::Result<serde_json::Value> {
        let op = "border";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view.border().map_err(crate::error::facade(op))?;
        Ok(border_to_json(&out))
    }

    /// Annot.clean_contents from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "cleanContents")]
    pub fn clean_contents(&self, sanitize: Option<bool>) -> napi::Result<()> {
        let op = "cleanContents";
        self.shared.check_open(op)?;
        let sanitize_v = sanitize.unwrap_or(true);
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let mut view = fc::Annot::new(&mut pm, self.xref);
        view.clean_contents(sanitize_v)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Annot.colors from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "colors")]
    pub fn colors(&self) -> napi::Result<serde_json::Value> {
        let op = "colors";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view.colors().map_err(crate::error::facade(op))?;
        Ok(annot_colors_to_json(&out))
    }

    /// Annot.delete_responses from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "deleteResponses")]
    pub fn delete_responses(&self) -> napi::Result<()> {
        let op = "deleteResponses";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let mut view = fc::Annot::new(&mut pm, self.xref);
        view.delete_responses().map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Annot.file_info from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "fileInfo")]
    pub fn file_info(&self) -> napi::Result<serde_json::Value> {
        let op = "fileInfo";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view.file_info().map_err(crate::error::facade(op))?;
        Ok(file_info_to_json(&out))
    }

    /// Annot.flags from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "flags")]
    pub fn flags(&self) -> napi::Result<i32> {
        let op = "flags";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view.flags().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Annot.get_file from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getFile")]
    pub fn get_file(&self) -> napi::Result<Buffer> {
        let op = "getFile";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view.get_file().map_err(crate::error::facade(op))?;
        Ok(Buffer::from(out))
    }

    /// Annot.get_oc from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getOc")]
    pub fn get_oc(&self) -> napi::Result<i32> {
        let op = "getOc";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view.get_oc().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Annot.get_parent from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getParent")]
    pub fn get_parent(&self) -> napi::Result<serde_json::Value> {
        let op = "getParent";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view.get_parent().map_err(crate::error::facade(op))?;
        Ok(annot_entry_to_json(&out))
    }

    /// Annot.get_pixmap from the compatible API.
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
        let mut guard = self.shared.lock(op)?;
        if let Some(c) = colorspace.as_ref() {
            options_o = options_o.with_colorspace(c.to_compat());
        }
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view
            .get_pixmap(&options_o)
            .map_err(crate::error::facade(op))?;
        Ok(Pixmap { inner: out })
    }

    /// Annot.get_sound from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getSound")]
    pub fn get_sound(&self) -> napi::Result<Buffer> {
        let op = "getSound";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view.get_sound().map_err(crate::error::facade(op))?;
        Ok(Buffer::from(out))
    }

    /// Annot.get_text from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getText")]
    pub fn get_text(
        &self,
        options: Option<serde_json::Value>,
        textpage: Option<&TextPage>,
    ) -> napi::Result<serde_json::Value> {
        let op = "getText";
        self.shared.check_open(op)?;
        let options_o = text_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        if textpage.is_some() {
            return Err(bad_value(
                op,
                format!("{op}: text page is not supported on this method in this build"),
            ));
        }
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view
            .get_text(&options_o)
            .map_err(crate::error::facade(op))?;
        let raw = options_o.format == fc::TextFormat::RawDict;
        text_output_to_json(&out, raw)
    }

    /// Annot.get_textbox from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getTextbox")]
    pub fn get_textbox(&self, rect: Option<RectLike>) -> napi::Result<String> {
        let op = "getTextbox";
        self.shared.check_open(op)?;
        let rect_v = rect.as_ref().map(RectLike::to_compat);
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view.get_textbox(rect_v).map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Annot.get_textpage from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getTextpage")]
    pub fn get_textpage(
        &self,
        clip: Option<RectLike>,
        flags: Option<i32>,
    ) -> napi::Result<TextPage> {
        let op = "getTextpage";
        self.shared.check_open(op)?;
        let clip_v = clip.as_ref().map(RectLike::to_compat);
        let flags_v = flags.unwrap_or(0);
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let _ = view
            .get_textpage(clip_v, flags_v)
            .map_err(crate::error::facade(op))?;
        Ok(TextPage {
            shared: self.shared.clone(),
            index: self.page,
        })
    }

    /// Annot.has_popup from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "hasPopup")]
    pub fn has_popup(&self) -> napi::Result<bool> {
        let op = "hasPopup";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view.has_popup().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Annot.info from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "info")]
    pub fn info(&self) -> napi::Result<serde_json::Value> {
        let op = "info";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view.info().map_err(crate::error::facade(op))?;
        Ok(annot_info_to_json(&out))
    }

    /// Annot.irt_xref from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "irtXref")]
    pub fn irt_xref(&self) -> napi::Result<i32> {
        let op = "irtXref";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view.irt_xref().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Annot.is_open from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "isOpen")]
    pub fn is_open(&self) -> napi::Result<bool> {
        let op = "isOpen";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view.is_open().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Annot.language from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "language")]
    pub fn language(&self) -> napi::Result<String> {
        let op = "language";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view.language().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Annot.line_ends from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "lineEnds")]
    pub fn line_ends(&self) -> napi::Result<serde_json::Value> {
        let op = "lineEnds";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view.line_ends().map_err(crate::error::facade(op))?;
        match out {
            Some(ends) => Ok(tuple_to_json_i32_i32(&ends)),
            None => Ok(serde_json::Value::Null),
        }
    }

    /// Annot.next from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "next")]
    pub fn next(&self) -> napi::Result<Option<Annot>> {
        let op = "next";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let mut view = fc::Annot::new(&mut pm, self.xref);
        let out = view.next().map_err(crate::error::facade(op))?;
        out.map(|x| -> napi::Result<Annot> {
            Ok(Annot {
                shared: self.shared.clone(),
                page: self.page,
                xref: x.xref().map_err(crate::error::facade(op))?,
            })
        })
        .transpose()
    }

    /// Annot.opacity from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "opacity")]
    pub fn opacity(&self) -> napi::Result<f64> {
        let op = "opacity";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view.opacity().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Annot.popup_rect from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "popupRect")]
    pub fn popup_rect(&self) -> napi::Result<Rect> {
        let op = "popupRect";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view.popup_rect().map_err(crate::error::facade(op))?;
        Ok(Rect { inner: out })
    }

    /// Annot.popup_xref from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "popupXref")]
    pub fn popup_xref(&self) -> napi::Result<i32> {
        let op = "popupXref";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view.popup_xref().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Annot.rect from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "rect")]
    pub fn rect(&self) -> napi::Result<Rect> {
        let op = "rect";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view.rect().map_err(crate::error::facade(op))?;
        Ok(Rect { inner: out })
    }

    /// Annot.rect_delta from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "rectDelta")]
    pub fn rect_delta(&self) -> napi::Result<Rect> {
        let op = "rectDelta";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view.rect_delta().map_err(crate::error::facade(op))?;
        Ok(Rect { inner: out })
    }

    /// Annot.rotation from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "rotation")]
    pub fn rotation(&self) -> napi::Result<i32> {
        let op = "rotation";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view.rotation().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Annot.set_apn_bbox from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setApnBbox")]
    pub fn set_apn_bbox(&self, bbox: RectLike) -> napi::Result<()> {
        let op = "setApnBbox";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let mut view = fc::Annot::new(&mut pm, self.xref);
        view.set_apn_bbox(bbox.to_compat())
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Annot.set_apn_matrix from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setApnMatrix")]
    pub fn set_apn_matrix(&self, matrix: MatrixLike) -> napi::Result<()> {
        let op = "setApnMatrix";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let mut view = fc::Annot::new(&mut pm, self.xref);
        view.set_apn_matrix(matrix.to_compat())
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Annot.set_blendmode from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setBlendmode")]
    pub fn set_blendmode(&self, blend_mode: String) -> napi::Result<()> {
        let op = "setBlendmode";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let mut view = fc::Annot::new(&mut pm, self.xref);
        view.set_blendmode(&blend_mode)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Annot.set_border from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setBorder")]
    pub fn set_border(&self, options: Option<serde_json::Value>) -> napi::Result<()> {
        let op = "setBorder";
        self.shared.check_open(op)?;
        opts_key_default(
            options.as_ref(),
            "border",
            serde_json::Value::Null,
            "border",
        )?;
        let options_o = border_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let mut view = fc::Annot::new(&mut pm, self.xref);
        view.set_border(&options_o)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Annot.set_colors from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setColors")]
    pub fn set_colors(&self, colors: Option<serde_json::Value>) -> napi::Result<()> {
        let op = "setColors";
        self.shared.check_open(op)?;
        let colors_o = match colors.as_ref() {
            Some(v) => annot_colors_from_json(op, v)?,
            None => fc::AnnotColors {
                stroke: Vec::new(),
                fill: Vec::new(),
            },
        };
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let mut view = fc::Annot::new(&mut pm, self.xref);
        view.set_colors(&colors_o)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Annot.set_flags from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setFlags")]
    pub fn set_flags(&self, flags: i32) -> napi::Result<()> {
        let op = "setFlags";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let mut view = fc::Annot::new(&mut pm, self.xref);
        view.set_flags(flags).map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Annot.set_info from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setInfo")]
    pub fn set_info(&self, options: Option<serde_json::Value>) -> napi::Result<()> {
        let op = "setInfo";
        self.shared.check_open(op)?;
        let options_o = annot_info_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let mut view = fc::Annot::new(&mut pm, self.xref);
        view.set_info(&options_o)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Annot.set_irt_xref from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setIrtXref")]
    pub fn set_irt_xref(&self, xref: i32) -> napi::Result<()> {
        let op = "setIrtXref";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let mut view = fc::Annot::new(&mut pm, self.xref);
        view.set_irt_xref(xref).map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Annot.set_language from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setLanguage")]
    pub fn set_language(&self, language: Option<String>) -> napi::Result<()> {
        let op = "setLanguage";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let mut view = fc::Annot::new(&mut pm, self.xref);
        view.set_language(language.as_deref())
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Annot.set_line_ends from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setLineEnds")]
    pub fn set_line_ends(&self, start: i32, end: i32) -> napi::Result<()> {
        let op = "setLineEnds";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let mut view = fc::Annot::new(&mut pm, self.xref);
        view.set_line_ends(start, end)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Annot.set_name from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setName")]
    pub fn set_name(&self, name: String) -> napi::Result<()> {
        let op = "setName";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let mut view = fc::Annot::new(&mut pm, self.xref);
        view.set_name(&name).map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Annot.set_oc from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setOc")]
    pub fn set_oc(&self, oc: Option<i32>) -> napi::Result<()> {
        let op = "setOc";
        self.shared.check_open(op)?;
        let oc_v = oc.unwrap_or(0);
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let mut view = fc::Annot::new(&mut pm, self.xref);
        view.set_oc(oc_v).map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Annot.set_opacity from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setOpacity")]
    pub fn set_opacity(&self, opacity: f64) -> napi::Result<()> {
        let op = "setOpacity";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let mut view = fc::Annot::new(&mut pm, self.xref);
        view.set_opacity(opacity)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Annot.set_open from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setOpen")]
    pub fn set_open(&self, is_open: bool) -> napi::Result<()> {
        let op = "setOpen";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let mut view = fc::Annot::new(&mut pm, self.xref);
        view.set_open(is_open).map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Annot.set_popup from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setPopup")]
    pub fn set_popup(&self, rect: RectLike) -> napi::Result<()> {
        let op = "setPopup";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let mut view = fc::Annot::new(&mut pm, self.xref);
        view.set_popup(rect.to_compat())
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Annot.set_rect from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setRect")]
    pub fn set_rect(&self, rect: RectLike) -> napi::Result<()> {
        let op = "setRect";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let mut view = fc::Annot::new(&mut pm, self.xref);
        view.set_rect(rect.to_compat())
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Annot.set_rotation from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setRotation")]
    pub fn set_rotation(&self, rotate: Option<i32>) -> napi::Result<()> {
        let op = "setRotation";
        self.shared.check_open(op)?;
        let rotate_v = rotate.unwrap_or(0);
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let mut view = fc::Annot::new(&mut pm, self.xref);
        view.set_rotation(rotate_v)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Annot.type from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "type")]
    pub fn annot_type(&self) -> napi::Result<serde_json::Value> {
        let op = "type";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view.annot_type().map_err(crate::error::facade(op))?;
        Ok(annot_type_to_json(&out))
    }

    /// Annot.update from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "update")]
    pub fn update(&self, options: Option<serde_json::Value>) -> napi::Result<()> {
        let op = "update";
        self.shared.check_open(op)?;
        let options_o = annot_update_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let mut view = fc::Annot::new(&mut pm, self.xref);
        view.update(&options_o).map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Annot.update_file from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "updateFile")]
    pub fn update_file(
        &self,
        data: Buffer,
        filename: Option<String>,
        display_name: Option<String>,
        description: Option<String>,
    ) -> napi::Result<()> {
        let op = "updateFile";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let mut view = fc::Annot::new(&mut pm, self.xref);
        view.update_file(
            data.as_ref(),
            filename.as_deref(),
            display_name.as_deref(),
            description.as_deref(),
        )
        .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Annot.update_timing_test from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "updateTimingTest")]
    pub fn update_timing_test(&self) -> napi::Result<f64> {
        let op = "updateTimingTest";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view
            .update_timing_test()
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Annot.vertices from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "vertices")]
    pub fn vertices(&self) -> napi::Result<Vec<Point>> {
        let op = "vertices";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view.vertices().map_err(crate::error::facade(op))?;
        Ok(out.into_iter().map(|x| Point { inner: x }).collect())
    }

    /// Annot.xref from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "xref")]
    pub fn xref(&self) -> napi::Result<i32> {
        let op = "xref";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut pm = fc::PageMut::new(&mut guard, self.page);
        let view = fc::Annot::new(&mut pm, self.xref);
        let out = view.xref().map_err(crate::error::facade(op))?;
        Ok(out)
    }
}

#[napi]
impl Document {
    /// Document.add_layer from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "addLayer")]
    pub fn add_layer(
        &self,
        name: String,
        creator: Option<String>,
        on: Option<bool>,
    ) -> napi::Result<()> {
        let op = "addLayer";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        guard
            .add_layer(&name, creator.as_deref(), on)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.add_ocg from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "addOcg")]
    pub fn add_ocg(
        &self,
        name: String,
        config: Option<i32>,
        on: Option<bool>,
        intent: Option<String>,
        usage: Option<String>,
    ) -> napi::Result<i32> {
        let op = "addOcg";
        self.shared.check_open(op)?;
        let config_v = config.unwrap_or(-1);
        let on_v = on.unwrap_or(true);
        let mut guard = self.shared.lock(op)?;
        let xref = guard
            .add_ocg(&name, config_v, on_v, intent.as_deref(), usage.as_deref())
            .map_err(crate::error::facade(op))?;
        Ok(xref)
    }

    /// Document.apply_css from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "applyCss")]
    pub fn apply_css(&self, css: String, append: Option<bool>) -> napi::Result<()> {
        let op = "applyCss";
        self.shared.check_open(op)?;
        let append_v = append.unwrap_or(true);
        let mut guard = self.shared.lock(op)?;
        guard
            .apply_css(&css, append_v)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.bake from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "bake")]
    pub fn bake(&self, options: Option<serde_json::Value>) -> napi::Result<()> {
        let op = "bake";
        self.shared.check_open(op)?;
        let options_o = bake_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        guard.bake(&options_o).map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.copy_page from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "copyPage")]
    pub fn copy_page(&self, page: i32, to: Option<i32>) -> napi::Result<()> {
        let op = "copyPage";
        self.shared.check_open(op)?;
        let to_v = to.unwrap_or(-1);
        let mut guard = self.shared.lock(op)?;
        guard
            .copy_page(page, to_v)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.del_toc_item from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "delTocItem")]
    pub fn del_toc_item(&self, index: i32) -> napi::Result<()> {
        let op = "delTocItem";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        guard
            .del_toc_item(index)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.del_xml_metadata from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "delXmlMetadata")]
    pub fn del_xml_metadata(&self) -> napi::Result<()> {
        let op = "delXmlMetadata";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        guard.del_xml_metadata().map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.delete_page from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "deletePage")]
    pub fn delete_page(&self, page: Option<i32>) -> napi::Result<()> {
        let op = "deletePage";
        self.shared.check_open(op)?;
        let page_v = page.unwrap_or(-1);
        let mut guard = self.shared.lock(op)?;
        guard
            .delete_page(page_v)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.delete_pages from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "deletePages")]
    pub fn delete_pages(&self, pages: serde_json::Value) -> napi::Result<()> {
        let op = "deletePages";
        self.shared.check_open(op)?;
        let pages_v = page_selection_from_json(op, &pages)?;
        let mut guard = self.shared.lock(op)?;
        guard
            .delete_pages(&pages_v)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.embfile_add from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "embfileAdd")]
    pub fn embfile_add(
        &self,
        name: String,
        data: Buffer,
        options: Option<serde_json::Value>,
    ) -> napi::Result<()> {
        let op = "embfileAdd";
        self.shared.check_open(op)?;
        opts_key_default(
            options.as_ref(),
            "ufilename",
            serde_json::Value::Null,
            "ufilename",
        )?;
        let options_o = emb_file_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        guard
            .embfile_add(&name, data.as_ref(), &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.embfile_del from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "embfileDel")]
    pub fn embfile_del(&self, item: serde_json::Value) -> napi::Result<()> {
        let op = "embfileDel";
        self.shared.check_open(op)?;
        let item_v = emb_item_from_json(op, &item)?;
        let mut guard = self.shared.lock(op)?;
        guard
            .embfile_del(&item_v)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.embfile_upd from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "embfileUpd")]
    pub fn embfile_upd(
        &self,
        item: serde_json::Value,
        data: Option<Buffer>,
        options: Option<serde_json::Value>,
    ) -> napi::Result<()> {
        let op = "embfileUpd";
        self.shared.check_open(op)?;
        opts_key_default(
            options.as_ref(),
            "ufilename",
            serde_json::Value::Null,
            "ufilename",
        )?;
        let item_v = emb_item_from_json(op, &item)?;
        let options_o = emb_file_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        guard
            .embfile_upd(&item_v, data.as_deref(), &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.fullcopy_page from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "fullcopyPage")]
    pub fn fullcopy_page(&self, page: i32, to: Option<i32>) -> napi::Result<()> {
        let op = "fullcopyPage";
        self.shared.check_open(op)?;
        let to_v = to.unwrap_or(-1);
        let mut guard = self.shared.lock(op)?;
        guard
            .fullcopy_page(page, to_v)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.has_annots from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "hasAnnots")]
    pub fn has_annots(&self) -> napi::Result<bool> {
        let op = "hasAnnots";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.has_annots().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.has_links from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "hasLinks")]
    pub fn has_links(&self) -> napi::Result<bool> {
        let op = "hasLinks";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.has_links().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.insert_file from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "insertFile")]
    pub fn insert_file(
        &self,
        path: String,
        options: Option<serde_json::Value>,
    ) -> napi::Result<()> {
        let op = "insertFile";
        self.shared.check_open(op)?;
        let options_o = insert_pdf_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        guard
            .insert_file(&path, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.insert_page from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "insertPage")]
    pub fn insert_page(&self, page: i32, options: Option<serde_json::Value>) -> napi::Result<i32> {
        let op = "insertPage";
        self.shared.check_open(op)?;
        let options_o = insert_page_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let out = guard
            .insert_page(page, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.insert_pdf from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "insertPdf")]
    pub fn insert_pdf(
        &self,
        source: &Document,
        options: Option<serde_json::Value>,
    ) -> napi::Result<()> {
        let op = "insertPdf";
        self.shared.check_open(op)?;
        opts_key_default(options.as_ref(), "_gmap", serde_json::Value::Null, "_gmap")?;
        if std::sync::Arc::ptr_eq(&self.shared, &source.shared) {
            return Err(compat_error(
                op,
                fc::ErrorKind::BadValue,
                format!("{op}: cannot use the same document as its own source"),
            ));
        }
        source.shared.check_open(op)?;
        let options_o = insert_pdf_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let source_guard = source.shared.lock(op)?;
        guard
            .insert_pdf(&source_guard, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.journal_can_do from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "journalCanDo")]
    pub fn journal_can_do(&self) -> napi::Result<serde_json::Value> {
        let op = "journalCanDo";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.journal_can_do().map_err(crate::error::facade(op))?;
        Ok(tuple_to_json_bool_bool(&out))
    }

    /// Document.journal_enable from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "journalEnable")]
    pub fn journal_enable(&self) -> napi::Result<()> {
        let op = "journalEnable";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        guard.journal_enable().map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.journal_is_enabled from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "journalIsEnabled")]
    pub fn journal_is_enabled(&self) -> napi::Result<bool> {
        let op = "journalIsEnabled";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard
            .journal_is_enabled()
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.journal_load from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "journalLoad")]
    pub fn journal_load(&self, path: String) -> napi::Result<()> {
        let op = "journalLoad";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        guard
            .journal_load(&path)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.journal_op_name from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "journalOpName")]
    pub fn journal_op_name(&self, step: i32) -> napi::Result<String> {
        let op = "journalOpName";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard
            .journal_op_name(step)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.journal_position from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "journalPosition")]
    pub fn journal_position(&self) -> napi::Result<i32> {
        let op = "journalPosition";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.journal_position().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.journal_redo from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "journalRedo")]
    pub fn journal_redo(&self) -> napi::Result<()> {
        let op = "journalRedo";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        guard.journal_redo().map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.journal_save from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "journalSave")]
    pub fn journal_save(&self, path: String) -> napi::Result<()> {
        let op = "journalSave";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        guard
            .journal_save(&path)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.journal_start_op from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "journalStartOp")]
    pub fn journal_start_op(&self, name: Option<String>) -> napi::Result<()> {
        let op = "journalStartOp";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        guard
            .journal_start_op(name.as_deref())
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.journal_stop_op from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "journalStopOp")]
    pub fn journal_stop_op(&self) -> napi::Result<()> {
        let op = "journalStopOp";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        guard.journal_stop_op().map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.journal_undo from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "journalUndo")]
    pub fn journal_undo(&self) -> napi::Result<()> {
        let op = "journalUndo";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        guard.journal_undo().map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.move_page from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "movePage")]
    pub fn move_page(&self, page: i32, to: Option<i32>) -> napi::Result<()> {
        let op = "movePage";
        self.shared.check_open(op)?;
        let to_v = to.unwrap_or(-1);
        let mut guard = self.shared.lock(op)?;
        guard
            .move_page(page, to_v)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.new_page from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "newPage")]
    pub fn new_page(
        &self,
        page: Option<i32>,
        width: Option<f64>,
        height: Option<f64>,
    ) -> napi::Result<Page> {
        let op = "newPage";
        self.shared.check_open(op)?;
        let page_v = page.unwrap_or(-1);
        let width_v = width.unwrap_or(595.0);
        let height_v = height.unwrap_or(842.0);
        let mut guard = self.shared.lock(op)?;
        let out = guard
            .new_page(page_v, width_v, height_v)
            .map_err(crate::error::facade(op))?;
        Ok(Page {
            shared: self.shared.clone(),
            index: out.index(),
        })
    }

    /// Document.page_annot_xrefs from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "pageAnnotXrefs")]
    pub fn page_annot_xrefs(&self, page: i32) -> napi::Result<Vec<i32>> {
        let op = "pageAnnotXrefs";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard
            .page_annot_xrefs(page)
            .map_err(crate::error::facade(op))?;
        Ok(out.into_iter().map(|row| row.xref).collect())
    }

    /// Document.repair from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "repair")]
    pub fn repair(&self) -> napi::Result<()> {
        let op = "repair";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        guard.repair().map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.resolve_link from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "resolveLink")]
    pub fn resolve_link(
        &self,
        uri: Option<String>,
        chapters: Option<i32>,
    ) -> napi::Result<serde_json::Value> {
        let op = "resolveLink";
        self.shared.check_open(op)?;
        let chapters_v = chapters.unwrap_or(0);
        let guard = self.shared.lock(op)?;
        let out = guard
            .resolve_link(uri.as_deref(), chapters_v)
            .map_err(crate::error::facade(op))?;
        compat_to_json(&out)
    }

    /// Document.scrub from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "scrub")]
    pub fn scrub(&self, options: Option<serde_json::Value>) -> napi::Result<()> {
        let op = "scrub";
        self.shared.check_open(op)?;
        let options_o = scrub_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        guard.scrub(&options_o).map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.select from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "select")]
    pub fn select(&self, pages: Vec<i32>) -> napi::Result<()> {
        let op = "select";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        guard.select(&pages).map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.set_language from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setLanguage")]
    pub fn set_language(&self, language: Option<String>) -> napi::Result<()> {
        let op = "setLanguage";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        guard
            .set_language(language.as_deref())
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.set_layer from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setLayer")]
    pub fn set_layer(&self, config: i32, options: Option<serde_json::Value>) -> napi::Result<()> {
        let op = "setLayer";
        self.shared.check_open(op)?;
        opts_key_default(
            options.as_ref(),
            "rbgroups",
            serde_json::Value::Null,
            "rbgroups",
        )?;
        opts_key_default(
            options.as_ref(),
            "locked",
            serde_json::Value::Null,
            "locked",
        )?;
        let options_o = layer_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        guard
            .set_layer(config, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.set_layer_ui_config from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setLayerUiConfig")]
    pub fn set_layer_ui_config(&self, number: i32, action: Option<i32>) -> napi::Result<()> {
        let op = "setLayerUiConfig";
        self.shared.check_open(op)?;
        let action_v = action.unwrap_or(0);
        let mut guard = self.shared.lock(op)?;
        guard
            .set_layer_ui_config(number, action_v)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.set_markinfo from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setMarkinfo")]
    pub fn set_markinfo(&self, markinfo: serde_json::Value) -> napi::Result<bool> {
        let op = "setMarkinfo";
        self.shared.check_open(op)?;
        let markinfo_v = json_to_compat(&markinfo);
        let mut guard = self.shared.lock(op)?;
        let out = guard
            .set_markinfo(&markinfo_v)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.set_metadata from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setMetadata")]
    pub fn set_metadata(&self, metadata: serde_json::Value) -> napi::Result<()> {
        let op = "setMetadata";
        self.shared.check_open(op)?;
        let metadata_v = doc_metadata_from_json(op, &metadata)?;
        let mut guard = self.shared.lock(op)?;
        guard
            .set_metadata(&metadata_v)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.set_oc from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setOc")]
    pub fn set_oc(&self, xref: i32, oc: i32) -> napi::Result<()> {
        let op = "setOc";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        guard.set_oc(xref, oc).map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.set_ocmd from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setOcmd")]
    pub fn set_ocmd(
        &self,
        xref: Option<i32>,
        options: Option<serde_json::Value>,
    ) -> napi::Result<i32> {
        let op = "setOcmd";
        self.shared.check_open(op)?;
        opts_key_default(options.as_ref(), "ve", serde_json::Value::Null, "ve")?;
        let xref_v = xref.unwrap_or(0);
        let options_o = ocmd_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let out = guard
            .set_ocmd(xref_v, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.set_page_labels from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setPageLabels")]
    pub fn set_page_labels(&self, labels: Vec<serde_json::Value>) -> napi::Result<()> {
        let op = "setPageLabels";
        self.shared.check_open(op)?;
        let mut labels_v = Vec::with_capacity(labels.len());
        for v in &labels {
            labels_v.push(page_label_from_json(op, v)?);
        }
        let mut guard = self.shared.lock(op)?;
        guard
            .set_page_labels(&labels_v)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.set_pagelayout from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setPagelayout")]
    pub fn set_pagelayout(&self, pagelayout: String) -> napi::Result<()> {
        let op = "setPagelayout";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        guard
            .set_pagelayout(&pagelayout)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.set_pagemode from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setPagemode")]
    pub fn set_pagemode(&self, pagemode: String) -> napi::Result<()> {
        let op = "setPagemode";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        guard
            .set_pagemode(&pagemode)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.set_toc from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setToc")]
    pub fn set_toc(&self, toc: Vec<serde_json::Value>, collapse: Option<i32>) -> napi::Result<i32> {
        let op = "setToc";
        self.shared.check_open(op)?;
        let mut toc_v = Vec::with_capacity(toc.len());
        for v in &toc {
            toc_v.push(toc_entry_from_json(op, v)?);
        }
        let collapse_v = collapse.unwrap_or(1);
        let mut guard = self.shared.lock(op)?;
        let out = guard
            .set_toc(&toc_v, collapse_v)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.set_toc_item from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setTocItem")]
    pub fn set_toc_item(&self, index: i32, options: Option<serde_json::Value>) -> napi::Result<()> {
        let op = "setTocItem";
        self.shared.check_open(op)?;
        opts_key_default(
            options.as_ref(),
            "dest_dict",
            serde_json::Value::Null,
            "dest_dict",
        )?;
        let options_o = toc_item_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        guard
            .set_toc_item(index, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.set_xml_metadata from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setXmlMetadata")]
    pub fn set_xml_metadata(&self, metadata: String) -> napi::Result<()> {
        let op = "setXmlMetadata";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        guard
            .set_xml_metadata(&metadata)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.subset_fonts from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "subsetFonts")]
    pub fn subset_fonts(
        &self,
        verbose: Option<bool>,
        fallback: Option<bool>,
    ) -> napi::Result<serde_json::Value> {
        let op = "subsetFonts";
        if verbose == Some(true) {
            return Err(compat_error(
                op,
                fc::ErrorKind::BadValue,
                format!("{op}: verbose is not supported in this build"),
            ));
        }
        if fallback == Some(true) {
            return Err(compat_error(
                op,
                fc::ErrorKind::BadValue,
                format!("{op}: fallback is not supported in this build"),
            ));
        }
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let out = guard.subset_fonts().map_err(crate::error::facade(op))?;
        Ok(font_subset_report_to_json(&out))
    }

    /// Document.update_object from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "updateObject")]
    pub fn update_object(&self, xref: i32, text: String, page: Option<i32>) -> napi::Result<()> {
        let op = "updateObject";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        guard
            .update_object(xref, &text, page)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.update_stream from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "updateStream")]
    pub fn update_stream(
        &self,
        xref: Option<i32>,
        data: Option<Buffer>,
        new_stream: Option<bool>,
        compress: Option<bool>,
    ) -> napi::Result<()> {
        let op = "updateStream";
        self.shared.check_open(op)?;
        let xref_v = xref.unwrap_or(0);
        let new_v = new_stream.unwrap_or(true);
        let compress_v = compress.unwrap_or(true);
        let mut guard = self.shared.lock(op)?;
        guard
            .update_stream(xref_v, data.as_deref(), new_v, compress_v)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.xref_copy from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "xrefCopy")]
    pub fn xref_copy(
        &self,
        source: i32,
        target: i32,
        keep: Option<Vec<String>>,
    ) -> napi::Result<()> {
        let op = "xrefCopy";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        guard
            .xref_copy(source, target, keep.as_deref())
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.xref_set_key from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "xrefSetKey")]
    pub fn xref_set_key(&self, xref: i32, key: String, value: String) -> napi::Result<()> {
        let op = "xrefSetKey";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        guard
            .xref_set_key(xref, &key, &value)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }
}

#[napi]
impl Font {
    /// Font.__eq__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "eq")]
    pub fn eq(&self, other: &Font) -> bool {
        self.inner == other.inner
    }

    /// Font.__ge__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "ge")]
    pub fn ge(&self, other: &Font) -> napi::Result<bool> {
        let op = "ge";
        self.inner
            .compare(&other.inner, fc::CompareOp::GreaterOrEqual)
            .map_err(crate::error::facade(op))
    }

    /// Font.__gt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "gt")]
    pub fn gt(&self, other: &Font) -> napi::Result<bool> {
        let op = "gt";
        self.inner
            .compare(&other.inner, fc::CompareOp::Greater)
            .map_err(crate::error::facade(op))
    }

    /// Font.__init__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(constructor)]
    pub fn new(
        options: Option<serde_json::Value>,
        fontbuffer: Option<Buffer>,
    ) -> napi::Result<Self> {
        let op = "Font";
        let mut o = font_options_from_json(op, options.as_ref())?;
        if let Some(buf) = fontbuffer.as_ref() {
            o = o.with_fontbuffer(Vec::<u8>::from(buf.clone()));
        }
        let out = fc::Font::new(&o).map_err(crate::error::facade(op))?;
        Ok(Self { inner: out })
    }

    /// Font.__le__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "le")]
    pub fn le(&self, other: &Font) -> napi::Result<bool> {
        let op = "le";
        self.inner
            .compare(&other.inner, fc::CompareOp::LessOrEqual)
            .map_err(crate::error::facade(op))
    }

    /// Font.__lt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "lt")]
    pub fn lt(&self, other: &Font) -> napi::Result<bool> {
        let op = "lt";
        self.inner
            .compare(&other.inner, fc::CompareOp::Less)
            .map_err(crate::error::facade(op))
    }

    /// Font.__ne__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "ne")]
    pub fn ne(&self, other: &Font) -> bool {
        self.inner != other.inner
    }

    /// Font.ascender from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "ascender")]
    pub fn ascender(&self) -> napi::Result<f64> {
        let op = "ascender";
        let out = self.inner.ascender().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Font.bbox from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "bbox")]
    pub fn bbox(&self) -> napi::Result<Rect> {
        let op = "bbox";
        let out = self.inner.bbox().map_err(crate::error::facade(op))?;
        Ok(Rect { inner: out })
    }

    /// Font.buffer from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "buffer")]
    pub fn buffer(&self) -> napi::Result<Buffer> {
        let op = "buffer";
        let out = self.inner.buffer().map_err(crate::error::facade(op))?;
        Ok(Buffer::from(out))
    }

    /// Font.char_lengths from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "charLengths")]
    pub fn char_lengths(
        &self,
        text: String,
        options: Option<serde_json::Value>,
    ) -> napi::Result<Vec<f64>> {
        let op = "charLengths";
        let options_o = font_measure_options_from_json(op, options.as_ref())?;
        let out = self
            .inner
            .char_lengths(&text, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(out.into_iter().collect())
    }

    /// Font.descender from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "descender")]
    pub fn descender(&self) -> napi::Result<f64> {
        let op = "descender";
        let out = self.inner.descender().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Font.flags from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "flags")]
    pub fn flags(&self) -> napi::Result<serde_json::Value> {
        let op = "flags";
        let out = self.inner.flags().map_err(crate::error::facade(op))?;
        Ok(font_flags_to_json(&out))
    }

    /// Font.glyph_advance from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "glyphAdvance")]
    pub fn glyph_advance(&self, chr: i32, options: Option<serde_json::Value>) -> napi::Result<f64> {
        let op = "glyphAdvance";
        let options_o = font_measure_options_from_json(op, options.as_ref())?;
        let out = self
            .inner
            .glyph_advance(chr, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Font.glyph_bbox from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "glyphBbox")]
    pub fn glyph_bbox(&self, chr: i32, options: Option<serde_json::Value>) -> napi::Result<Rect> {
        let op = "glyphBbox";
        let options_o = font_measure_options_from_json(op, options.as_ref())?;
        let out = self
            .inner
            .glyph_bbox(chr, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(Rect { inner: out })
    }

    /// Font.glyph_count from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "glyphCount")]
    pub fn glyph_count(&self) -> napi::Result<u32> {
        let op = "glyphCount";
        let out = self.inner.glyph_count().map_err(crate::error::facade(op))?;
        Ok(out as u32)
    }

    /// Font.glyph_name_to_unicode from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "glyphNameToUnicode")]
    pub fn glyph_name_to_unicode(&self, name: String) -> napi::Result<i32> {
        let op = "glyphNameToUnicode";
        let out = self
            .inner
            .glyph_name_to_unicode(&name)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Font.has_glyph from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "hasGlyph")]
    pub fn has_glyph(&self, chr: i32, options: Option<serde_json::Value>) -> napi::Result<i32> {
        let op = "hasGlyph";
        let options_o = font_measure_options_from_json(op, options.as_ref())?;
        let out = self
            .inner
            .has_glyph(chr, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Font.is_bold from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "isBold")]
    pub fn is_bold(&self) -> napi::Result<bool> {
        let op = "isBold";
        let out = self.inner.is_bold().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Font.is_italic from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "isItalic")]
    pub fn is_italic(&self) -> napi::Result<bool> {
        let op = "isItalic";
        let out = self.inner.is_italic().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Font.is_monospaced from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "isMonospaced")]
    pub fn is_monospaced(&self) -> napi::Result<bool> {
        let op = "isMonospaced";
        let out = self
            .inner
            .is_monospaced()
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Font.is_serif from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "isSerif")]
    pub fn is_serif(&self) -> napi::Result<bool> {
        let op = "isSerif";
        let out = self.inner.is_serif().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Font.is_writable from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "isWritable")]
    pub fn is_writable(&self) -> napi::Result<bool> {
        let op = "isWritable";
        let out = self.inner.is_writable().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Font.name from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "name")]
    pub fn name(&self) -> napi::Result<String> {
        let op = "name";
        let out = self.inner.name().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Font.text_length from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "textLength")]
    pub fn text_length(
        &self,
        text: String,
        options: Option<serde_json::Value>,
    ) -> napi::Result<f64> {
        let op = "textLength";
        let options_o = font_measure_options_from_json(op, options.as_ref())?;
        let out = self
            .inner
            .text_length(&text, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Font.unicode_to_glyph_name from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "unicodeToGlyphName")]
    pub fn unicode_to_glyph_name(&self, chr: i32) -> napi::Result<String> {
        let op = "unicodeToGlyphName";
        let out = self
            .inner
            .unicode_to_glyph_name(chr)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Font.valid_codepoints from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "validCodepoints")]
    pub fn valid_codepoints(&self) -> napi::Result<Vec<i32>> {
        let op = "validCodepoints";
        let out = self
            .inner
            .valid_codepoints()
            .map_err(crate::error::facade(op))?;
        Ok(out.into_iter().collect())
    }
}

#[napi]
impl Page {
    /// Page.add_caret_annot from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "addCaretAnnot")]
    pub fn add_caret_annot(&self, point: PointLike) -> napi::Result<Annot> {
        let op = "addCaretAnnot";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .add_caret_annot(point.to_compat())
            .map_err(crate::error::facade(op))?;
        Ok(Annot {
            shared: self.shared.clone(),
            page: self.index,
            xref: out.xref().map_err(crate::error::facade(op))?,
        })
    }

    /// Page.add_circle_annot from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "addCircleAnnot")]
    pub fn add_circle_annot(&self, rect: RectLike) -> napi::Result<Annot> {
        let op = "addCircleAnnot";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .add_circle_annot(rect.to_compat())
            .map_err(crate::error::facade(op))?;
        Ok(Annot {
            shared: self.shared.clone(),
            page: self.index,
            xref: out.xref().map_err(crate::error::facade(op))?,
        })
    }

    /// Page.add_file_annot from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "addFileAnnot")]
    pub fn add_file_annot(
        &self,
        point: PointLike,
        buffer: Buffer,
        filename: String,
        options: Option<serde_json::Value>,
    ) -> napi::Result<Annot> {
        let op = "addFileAnnot";
        self.shared.check_open(op)?;
        let o = file_annot_options_from_json(op, &filename, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .add_file_annot(point.to_compat(), buffer.as_ref(), &o)
            .map_err(crate::error::facade(op))?;
        Ok(Annot {
            shared: self.shared.clone(),
            page: self.index,
            xref: out.xref().map_err(crate::error::facade(op))?,
        })
    }

    /// Page.add_freetext_annot from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "addFreetextAnnot")]
    pub fn add_freetext_annot(
        &self,
        rect: RectLike,
        text: String,
        options: Option<serde_json::Value>,
    ) -> napi::Result<Annot> {
        let op = "addFreetextAnnot";
        self.shared.check_open(op)?;
        let options_o = free_text_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .add_freetext_annot(rect.to_compat(), &text, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(Annot {
            shared: self.shared.clone(),
            page: self.index,
            xref: out.xref().map_err(crate::error::facade(op))?,
        })
    }

    /// Page.add_highlight_annot from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "addHighlightAnnot")]
    pub fn add_highlight_annot(&self, options: Option<serde_json::Value>) -> napi::Result<Annot> {
        let op = "addHighlightAnnot";
        self.shared.check_open(op)?;
        let options_o = highlight_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .add_highlight_annot(&options_o)
            .map_err(crate::error::facade(op))?;
        Ok(Annot {
            shared: self.shared.clone(),
            page: self.index,
            xref: out.xref().map_err(crate::error::facade(op))?,
        })
    }

    /// Page.add_ink_annot from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "addInkAnnot")]
    pub fn add_ink_annot(&self, handwriting: Vec<Vec<PointLike>>) -> napi::Result<Annot> {
        let op = "addInkAnnot";
        self.shared.check_open(op)?;
        let handwriting_v: Vec<Vec<fc::Point>> = handwriting
            .into_iter()
            .map(|row| row.iter().map(PointLike::to_compat).collect())
            .collect();
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .add_ink_annot(&handwriting_v)
            .map_err(crate::error::facade(op))?;
        Ok(Annot {
            shared: self.shared.clone(),
            page: self.index,
            xref: out.xref().map_err(crate::error::facade(op))?,
        })
    }

    /// Page.add_line_annot from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "addLineAnnot")]
    pub fn add_line_annot(&self, p1: PointLike, p2: PointLike) -> napi::Result<Annot> {
        let op = "addLineAnnot";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .add_line_annot(p1.to_compat(), p2.to_compat())
            .map_err(crate::error::facade(op))?;
        Ok(Annot {
            shared: self.shared.clone(),
            page: self.index,
            xref: out.xref().map_err(crate::error::facade(op))?,
        })
    }

    /// Page.add_polygon_annot from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "addPolygonAnnot")]
    pub fn add_polygon_annot(&self, points: Vec<PointLike>) -> napi::Result<Annot> {
        let op = "addPolygonAnnot";
        self.shared.check_open(op)?;
        let points_v: Vec<fc::Point> = points.iter().map(PointLike::to_compat).collect();
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .add_polygon_annot(&points_v)
            .map_err(crate::error::facade(op))?;
        Ok(Annot {
            shared: self.shared.clone(),
            page: self.index,
            xref: out.xref().map_err(crate::error::facade(op))?,
        })
    }

    /// Page.add_polyline_annot from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "addPolylineAnnot")]
    pub fn add_polyline_annot(&self, points: Vec<PointLike>) -> napi::Result<Annot> {
        let op = "addPolylineAnnot";
        self.shared.check_open(op)?;
        let points_v: Vec<fc::Point> = points.iter().map(PointLike::to_compat).collect();
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .add_polyline_annot(&points_v)
            .map_err(crate::error::facade(op))?;
        Ok(Annot {
            shared: self.shared.clone(),
            page: self.index,
            xref: out.xref().map_err(crate::error::facade(op))?,
        })
    }

    /// Page.add_rect_annot from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "addRectAnnot")]
    pub fn add_rect_annot(&self, rect: RectLike) -> napi::Result<Annot> {
        let op = "addRectAnnot";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .add_rect_annot(rect.to_compat())
            .map_err(crate::error::facade(op))?;
        Ok(Annot {
            shared: self.shared.clone(),
            page: self.index,
            xref: out.xref().map_err(crate::error::facade(op))?,
        })
    }

    /// Page.add_redact_annot from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "addRedactAnnot")]
    pub fn add_redact_annot(
        &self,
        quad: QuadLike,
        options: Option<serde_json::Value>,
    ) -> napi::Result<Annot> {
        let op = "addRedactAnnot";
        self.shared.check_open(op)?;
        let options_o = redact_annot_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .add_redact_annot(quad.to_compat(), &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(Annot {
            shared: self.shared.clone(),
            page: self.index,
            xref: out.xref().map_err(crate::error::facade(op))?,
        })
    }

    /// Page.add_squiggly_annot from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "addSquigglyAnnot")]
    pub fn add_squiggly_annot(&self, options: Option<serde_json::Value>) -> napi::Result<Annot> {
        let op = "addSquigglyAnnot";
        self.shared.check_open(op)?;
        let options_o = highlight_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .add_squiggly_annot(&options_o)
            .map_err(crate::error::facade(op))?;
        Ok(Annot {
            shared: self.shared.clone(),
            page: self.index,
            xref: out.xref().map_err(crate::error::facade(op))?,
        })
    }

    /// Page.add_stamp_annot from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "addStampAnnot")]
    pub fn add_stamp_annot(&self, rect: RectLike, stamp: Option<i32>) -> napi::Result<Annot> {
        let op = "addStampAnnot";
        self.shared.check_open(op)?;
        let stamp_v = stamp.unwrap_or(0);
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .add_stamp_annot(rect.to_compat(), stamp_v)
            .map_err(crate::error::facade(op))?;
        Ok(Annot {
            shared: self.shared.clone(),
            page: self.index,
            xref: out.xref().map_err(crate::error::facade(op))?,
        })
    }

    /// Page.add_strikeout_annot from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "addStrikeoutAnnot")]
    pub fn add_strikeout_annot(&self, options: Option<serde_json::Value>) -> napi::Result<Annot> {
        let op = "addStrikeoutAnnot";
        self.shared.check_open(op)?;
        let options_o = highlight_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .add_strikeout_annot(&options_o)
            .map_err(crate::error::facade(op))?;
        Ok(Annot {
            shared: self.shared.clone(),
            page: self.index,
            xref: out.xref().map_err(crate::error::facade(op))?,
        })
    }

    /// Page.add_text_annot from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "addTextAnnot")]
    pub fn add_text_annot(
        &self,
        point: PointLike,
        text: String,
        icon: String,
    ) -> napi::Result<Annot> {
        let op = "addTextAnnot";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .add_text_annot(point.to_compat(), &text, &icon)
            .map_err(crate::error::facade(op))?;
        Ok(Annot {
            shared: self.shared.clone(),
            page: self.index,
            xref: out.xref().map_err(crate::error::facade(op))?,
        })
    }

    /// Page.add_underline_annot from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "addUnderlineAnnot")]
    pub fn add_underline_annot(&self, options: Option<serde_json::Value>) -> napi::Result<Annot> {
        let op = "addUnderlineAnnot";
        self.shared.check_open(op)?;
        let options_o = highlight_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .add_underline_annot(&options_o)
            .map_err(crate::error::facade(op))?;
        Ok(Annot {
            shared: self.shared.clone(),
            page: self.index,
            xref: out.xref().map_err(crate::error::facade(op))?,
        })
    }

    /// Page.add_widget from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "addWidget")]
    pub fn add_widget(&self, widget: &Widget) -> napi::Result<Annot> {
        let op = "addWidget";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .add_widget(&widget.inner)
            .map_err(crate::error::facade(op))?;
        Ok(Annot {
            shared: self.shared.clone(),
            page: self.index,
            xref: out.xref().map_err(crate::error::facade(op))?,
        })
    }

    /// Page.annot_names from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "annotNames")]
    pub fn annot_names(&self) -> napi::Result<Vec<String>> {
        let op = "annotNames";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let view = fc::PageMut::new(&mut guard, self.index);
        let out = view.annot_names().map_err(crate::error::facade(op))?;
        Ok(out.into_iter().collect())
    }

    /// Page.annot_xrefs from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "annotXrefs")]
    pub fn annot_xrefs(&self) -> napi::Result<Vec<serde_json::Value>> {
        let op = "annotXrefs";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let view = fc::PageMut::new(&mut guard, self.index);
        let out = view.annot_xrefs().map_err(crate::error::facade(op))?;
        Ok(out.into_iter().map(|x| annot_xref_to_json(&x)).collect())
    }

    /// Page.annots from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "annots")]
    pub fn annots(&self, types: Option<Vec<i32>>) -> napi::Result<Vec<serde_json::Value>> {
        let op = "annots";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .annots(types.as_deref())
            .map_err(crate::error::facade(op))?;
        Ok(out.into_iter().map(|x| annot_entry_to_json(&x)).collect())
    }

    /// Page.apply_redactions from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "applyRedactions")]
    pub fn apply_redactions(&self, options: Option<serde_json::Value>) -> napi::Result<bool> {
        let op = "applyRedactions";
        self.shared.check_open(op)?;
        let options_o = redact_apply_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .apply_redactions(&options_o)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Page.clean_contents from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "cleanContents")]
    pub fn clean_contents(&self, sanitize: Option<bool>) -> napi::Result<()> {
        let op = "cleanContents";
        self.shared.check_open(op)?;
        let sanitize_v = sanitize.unwrap_or(true);
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        view.clean_contents(sanitize_v)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Page.cluster_drawings from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "clusterDrawings")]
    pub fn cluster_drawings(
        &self,
        options: Option<serde_json::Value>,
    ) -> napi::Result<Vec<serde_json::Value>> {
        let op = "clusterDrawings";
        self.shared.check_open(op)?;
        let options_o = cluster_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .cluster_drawings(&options_o)
            .map_err(crate::error::facade(op))?;
        out.into_iter()
            .map(|x| drawing_cluster_to_json(&x))
            .collect()
    }

    /// Page.delete_annot from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "deleteAnnot")]
    pub fn delete_annot(&self, annot: &Annot) -> napi::Result<()> {
        let op = "deleteAnnot";
        self.shared.check_open(op)?;
        if !std::sync::Arc::ptr_eq(&self.shared, &annot.shared) {
            return Err(compat_error(
                op,
                fc::ErrorKind::BadValue,
                format!("{op}: annotation belongs to another document"),
            ));
        }
        let mut guard = self.shared.lock(op)?;
        let entry = {
            let mut pm = fc::PageMut::new(&mut guard, annot.page);
            let view = fc::Annot::new(&mut pm, annot.xref);
            fc::AnnotEntry {
                xref: annot.xref,
                annot_type: view.annot_type().map_err(crate::error::facade(op))?,
                rect: view.rect().map_err(crate::error::facade(op))?,
            }
        };
        let mut pm = fc::PageMut::new(&mut guard, self.index);
        pm.delete_annot(&entry).map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Page.delete_image from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "deleteImage")]
    pub fn delete_image(&self, xref: i32) -> napi::Result<()> {
        let op = "deleteImage";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        view.delete_image(xref).map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Page.delete_link from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "deleteLink")]
    pub fn delete_link(&self, link: &Link) -> napi::Result<()> {
        let op = "deleteLink";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        view.delete_link(&link.inner)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Page.delete_widget from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "deleteWidget")]
    pub fn delete_widget(&self, widget: &Widget) -> napi::Result<Widget> {
        let op = "deleteWidget";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .delete_widget(&widget.inner)
            .map_err(crate::error::facade(op))?;
        Ok(Widget { inner: out })
    }

    /// Page.draw_bezier from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "drawBezier")]
    pub fn draw_bezier(
        &self,
        p1: PointLike,
        p2: PointLike,
        p3: PointLike,
        p4: PointLike,
        options: Option<serde_json::Value>,
    ) -> napi::Result<Point> {
        let op = "drawBezier";
        self.shared.check_open(op)?;
        let options_o = draw_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .draw_bezier(
                p1.to_compat(),
                p2.to_compat(),
                p3.to_compat(),
                p4.to_compat(),
                &options_o,
            )
            .map_err(crate::error::facade(op))?;
        Ok(Point { inner: out })
    }

    /// Page.draw_circle from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "drawCircle")]
    pub fn draw_circle(
        &self,
        center: PointLike,
        radius: f64,
        options: Option<serde_json::Value>,
    ) -> napi::Result<Point> {
        let op = "drawCircle";
        self.shared.check_open(op)?;
        let options_o = draw_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .draw_circle(center.to_compat(), radius, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(Point { inner: out })
    }

    /// Page.draw_curve from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "drawCurve")]
    pub fn draw_curve(
        &self,
        p1: PointLike,
        p2: PointLike,
        p3: PointLike,
        options: Option<serde_json::Value>,
    ) -> napi::Result<Point> {
        let op = "drawCurve";
        self.shared.check_open(op)?;
        let options_o = draw_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .draw_curve(p1.to_compat(), p2.to_compat(), p3.to_compat(), &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(Point { inner: out })
    }

    /// Page.draw_line from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "drawLine")]
    pub fn draw_line(
        &self,
        p1: PointLike,
        p2: PointLike,
        options: Option<serde_json::Value>,
    ) -> napi::Result<Point> {
        let op = "drawLine";
        self.shared.check_open(op)?;
        let options_o = draw_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .draw_line(p1.to_compat(), p2.to_compat(), &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(Point { inner: out })
    }

    /// Page.draw_oval from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "drawOval")]
    pub fn draw_oval(
        &self,
        rect: RectLike,
        options: Option<serde_json::Value>,
    ) -> napi::Result<Point> {
        let op = "drawOval";
        self.shared.check_open(op)?;
        let options_o = draw_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .draw_oval(rect.to_compat(), &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(Point { inner: out })
    }

    /// Page.draw_polyline from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "drawPolyline")]
    pub fn draw_polyline(
        &self,
        points: Vec<PointLike>,
        options: Option<serde_json::Value>,
    ) -> napi::Result<Point> {
        let op = "drawPolyline";
        self.shared.check_open(op)?;
        let points_v: Vec<fc::Point> = points.iter().map(PointLike::to_compat).collect();
        let options_o = draw_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .draw_polyline(&points_v, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(Point { inner: out })
    }

    /// Page.draw_quad from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "drawQuad")]
    pub fn draw_quad(
        &self,
        quad: QuadLike,
        options: Option<serde_json::Value>,
    ) -> napi::Result<Point> {
        let op = "drawQuad";
        self.shared.check_open(op)?;
        let options_o = draw_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .draw_quad(quad.to_compat(), &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(Point { inner: out })
    }

    /// Page.draw_rect from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "drawRect")]
    pub fn draw_rect(
        &self,
        rect: RectLike,
        options: Option<serde_json::Value>,
        radius: Option<f64>,
    ) -> napi::Result<Point> {
        let op = "drawRect";
        self.shared.check_open(op)?;
        let options_o = draw_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .draw_rect(rect.to_compat(), &options_o, radius)
            .map_err(crate::error::facade(op))?;
        Ok(Point { inner: out })
    }

    /// Page.draw_sector from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "drawSector")]
    pub fn draw_sector(
        &self,
        center: PointLike,
        point: PointLike,
        beta: f64,
        full_sector: Option<bool>,
        options: Option<serde_json::Value>,
    ) -> napi::Result<Point> {
        let op = "drawSector";
        self.shared.check_open(op)?;
        let full_sector_v = full_sector.unwrap_or(true);
        let options_o = draw_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .draw_sector(
                center.to_compat(),
                point.to_compat(),
                beta,
                full_sector_v,
                &options_o,
            )
            .map_err(crate::error::facade(op))?;
        Ok(Point { inner: out })
    }

    /// Page.draw_squiggle from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "drawSquiggle")]
    pub fn draw_squiggle(
        &self,
        p1: PointLike,
        p2: PointLike,
        breadth: Option<f64>,
        options: Option<serde_json::Value>,
    ) -> napi::Result<Point> {
        let op = "drawSquiggle";
        self.shared.check_open(op)?;
        let breadth_v = breadth.unwrap_or(2.0);
        let options_o = draw_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .draw_squiggle(p1.to_compat(), p2.to_compat(), breadth_v, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(Point { inner: out })
    }

    /// Page.draw_zigzag from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "drawZigzag")]
    pub fn draw_zigzag(
        &self,
        p1: PointLike,
        p2: PointLike,
        breadth: Option<f64>,
        options: Option<serde_json::Value>,
    ) -> napi::Result<Point> {
        let op = "drawZigzag";
        self.shared.check_open(op)?;
        let breadth_v = breadth.unwrap_or(2.0);
        let options_o = draw_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .draw_zigzag(p1.to_compat(), p2.to_compat(), breadth_v, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(Point { inner: out })
    }

    /// Page.first_annot from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "firstAnnot")]
    pub fn first_annot(&self) -> napi::Result<Option<serde_json::Value>> {
        let op = "firstAnnot";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let view = fc::PageMut::new(&mut guard, self.index);
        let out = view.first_annot().map_err(crate::error::facade(op))?;
        Ok(out.map(|x| annot_entry_to_json(&x)))
    }

    /// Page.first_link from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "firstLink")]
    pub fn first_link(&self) -> napi::Result<Option<Link>> {
        let op = "firstLink";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let view = fc::PageMut::new(&mut guard, self.index);
        let out = view.first_link().map_err(crate::error::facade(op))?;
        Ok(out.map(|x| Link { inner: x }))
    }

    /// Page.first_widget from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "firstWidget")]
    pub fn first_widget(&self) -> napi::Result<Option<Widget>> {
        let op = "firstWidget";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let view = fc::PageMut::new(&mut guard, self.index);
        let out = view.first_widget().map_err(crate::error::facade(op))?;
        Ok(out.map(|x| Widget { inner: x }))
    }

    /// Page.get_cdrawings from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getCdrawings")]
    pub fn get_cdrawings(
        &self,
        options: Option<serde_json::Value>,
    ) -> napi::Result<Vec<serde_json::Value>> {
        let op = "getCdrawings";
        self.shared.check_open(op)?;
        opts_key_default(
            options.as_ref(),
            "method",
            serde_json::Value::Null,
            "method",
        )?;
        let options_o = cdrawings_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .get_cdrawings(&options_o)
            .map_err(crate::error::facade(op))?;
        out.into_iter().map(|x| drawing_to_json(&x)).collect()
    }

    /// Page.get_drawings from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getDrawings")]
    pub fn get_drawings(&self, extended: Option<bool>) -> napi::Result<Vec<serde_json::Value>> {
        let op = "getDrawings";
        self.shared.check_open(op)?;
        let extended_v = extended.unwrap_or(false);
        let mut guard = self.shared.lock(op)?;
        let view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .get_drawings(extended_v)
            .map_err(crate::error::facade(op))?;
        out.into_iter().map(|x| drawing_to_json(&x)).collect()
    }

    /// Page.get_links from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getLinks")]
    pub fn get_links(&self) -> napi::Result<Vec<Link>> {
        let op = "getLinks";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let view = fc::PageMut::new(&mut guard, self.index);
        let out = view.get_links().map_err(crate::error::facade(op))?;
        Ok(out.into_iter().map(|x| Link { inner: x }).collect())
    }

    /// Page.insert_font from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "insertFont")]
    pub fn insert_font(&self, options: Option<serde_json::Value>) -> napi::Result<i32> {
        let op = "insertFont";
        self.shared.check_open(op)?;
        let options_o = insert_font_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .insert_font(&options_o)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Page.insert_htmlbox from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "insertHtmlbox")]
    pub fn insert_htmlbox(
        &self,
        rect: RectLike,
        text: String,
        options: Option<serde_json::Value>,
    ) -> napi::Result<serde_json::Value> {
        let op = "insertHtmlbox";
        self.shared.check_open(op)?;
        opts_key_default(
            options.as_ref(),
            "_scale_word_width",
            serde_json::Value::Bool(true),
            "_scale_word_width",
        )?;
        opts_key_default(
            options.as_ref(),
            "_verbose",
            serde_json::Value::Bool(false),
            "_verbose",
        )?;
        let options_o = html_box_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .insert_htmlbox(rect.to_compat(), &text, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(tuple_to_json_f64_f64(&out))
    }

    /// Page.insert_image from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "insertImage")]
    pub fn insert_image(
        &self,
        rect: RectLike,
        options: Option<serde_json::Value>,
        pixmap: Option<&Pixmap>,
        stream: Option<Buffer>,
    ) -> napi::Result<i32> {
        let op = "insertImage";
        self.shared.check_open(op)?;
        let mut options_o = image_insert_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        if let Some(p) = pixmap.as_ref() {
            options_o = options_o.with_pixmap(p.inner.clone());
        }
        if let Some(s) = stream.as_ref() {
            options_o = options_o.with_stream(Vec::<u8>::from(s.clone()));
        }
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .insert_image(rect.to_compat(), &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Page.insert_link from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "insertLink")]
    pub fn insert_link(&self, link: &Link, mark: Option<bool>) -> napi::Result<()> {
        let op = "insertLink";
        self.shared.check_open(op)?;
        let mark_v = mark.unwrap_or(true);
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        view.insert_link(&link.inner, mark_v)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Page.insert_text from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "insertText")]
    pub fn insert_text(
        &self,
        point: PointLike,
        text: serde_json::Value,
        options: Option<serde_json::Value>,
    ) -> napi::Result<i32> {
        let op = "insertText";
        self.shared.check_open(op)?;
        let text_v = text_arg_from_json(op, &text)?;
        let options_o = insert_text_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .insert_text(point.to_compat(), &text_v, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Page.insert_textbox from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "insertTextbox")]
    pub fn insert_textbox(
        &self,
        rect: RectLike,
        text: serde_json::Value,
        options: Option<serde_json::Value>,
    ) -> napi::Result<f64> {
        let op = "insertTextbox";
        self.shared.check_open(op)?;
        let text_v = text_arg_from_json(op, &text)?;
        let options_o = text_box_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .insert_textbox(rect.to_compat(), &text_v, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Page.links from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "links")]
    pub fn links(&self, kinds: Option<Vec<i32>>) -> napi::Result<Vec<Link>> {
        let op = "links";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .links(kinds.as_deref())
            .map_err(crate::error::facade(op))?;
        Ok(out.into_iter().map(|x| Link { inner: x }).collect())
    }

    /// Page.load_annot from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "loadAnnot")]
    pub fn load_annot(&self, ident: serde_json::Value) -> napi::Result<Annot> {
        let op = "loadAnnot";
        self.shared.check_open(op)?;
        let ident_v = annot_ident_from_json(op, &ident)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .load_annot(&ident_v)
            .map_err(crate::error::facade(op))?;
        Ok(Annot {
            shared: self.shared.clone(),
            page: self.index,
            xref: out.xref().map_err(crate::error::facade(op))?,
        })
    }

    /// Page.load_links from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "loadLinks")]
    pub fn load_links(&self) -> napi::Result<Vec<Link>> {
        let op = "loadLinks";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let view = fc::PageMut::new(&mut guard, self.index);
        let out = view.load_links().map_err(crate::error::facade(op))?;
        Ok(out.into_iter().map(|x| Link { inner: x }).collect())
    }

    /// Page.load_widget from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "loadWidget")]
    pub fn load_widget(&self, xref: i32) -> napi::Result<Widget> {
        let op = "loadWidget";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let view = fc::PageMut::new(&mut guard, self.index);
        let out = view.load_widget(xref).map_err(crate::error::facade(op))?;
        Ok(Widget { inner: out })
    }

    /// Page.new_shape from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "newShape")]
    pub fn new_shape(&self) -> napi::Result<Shape> {
        let op = "newShape";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let _ = view.new_shape().map_err(crate::error::facade(op))?;
        Ok(Shape {
            shared: self.shared.clone(),
            page: self.index,
        })
    }

    /// Page.remove_rotation from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "removeRotation")]
    pub fn remove_rotation(&self) -> napi::Result<()> {
        let op = "removeRotation";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        view.remove_rotation()
            .map(|_| ())
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Page.replace_image from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "replaceImage")]
    pub fn replace_image(&self, xref: i32, options: Option<serde_json::Value>) -> napi::Result<()> {
        let op = "replaceImage";
        self.shared.check_open(op)?;
        let options_o = replace_image_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        view.replace_image(xref, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Page.set_artbox from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setArtbox")]
    pub fn set_artbox(&self, rect: RectLike) -> napi::Result<()> {
        let op = "setArtbox";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        view.set_artbox(rect.to_compat())
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Page.set_bleedbox from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setBleedbox")]
    pub fn set_bleedbox(&self, rect: RectLike) -> napi::Result<()> {
        let op = "setBleedbox";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        view.set_bleedbox(rect.to_compat())
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Page.set_contents from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setContents")]
    pub fn set_contents(&self, xref: i32) -> napi::Result<()> {
        let op = "setContents";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        view.set_contents(xref).map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Page.set_cropbox from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setCropbox")]
    pub fn set_cropbox(&self, rect: RectLike) -> napi::Result<()> {
        let op = "setCropbox";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        view.set_cropbox(rect.to_compat())
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Page.set_language from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setLanguage")]
    pub fn set_language(&self, language: Option<String>) -> napi::Result<()> {
        let op = "setLanguage";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        view.set_language(language.as_deref())
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Page.set_mediabox from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setMediabox")]
    pub fn set_mediabox(&self, rect: RectLike) -> napi::Result<()> {
        let op = "setMediabox";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        view.set_mediabox(rect.to_compat())
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Page.set_rotation from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setRotation")]
    pub fn set_rotation(&self, rotation: i32) -> napi::Result<()> {
        let op = "setRotation";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        view.set_rotation(rotation)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Page.set_trimbox from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setTrimbox")]
    pub fn set_trimbox(&self, rect: RectLike) -> napi::Result<()> {
        let op = "setTrimbox";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        view.set_trimbox(rect.to_compat())
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Page.show_pdf_page from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "showPdfPage")]
    pub fn show_pdf_page(
        &self,
        rect: RectLike,
        source: &Document,
        options: Option<serde_json::Value>,
    ) -> napi::Result<i32> {
        let op = "showPdfPage";
        self.shared.check_open(op)?;
        if std::sync::Arc::ptr_eq(&self.shared, &source.shared) {
            return Err(compat_error(
                op,
                fc::ErrorKind::BadValue,
                format!("{op}: cannot use the same document as its own source"),
            ));
        }
        source.shared.check_open(op)?;
        let options_o = show_page_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let source_guard = source.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .show_pdf_page(rect.to_compat(), &source_guard, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Page.update_link from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "updateLink")]
    pub fn update_link(&self, link: &Link) -> napi::Result<()> {
        let op = "updateLink";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        view.update_link(&link.inner)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Page.widgets from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "widgets")]
    pub fn widgets(&self, types: Option<Vec<i32>>) -> napi::Result<Vec<Widget>> {
        let op = "widgets";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let view = fc::PageMut::new(&mut guard, self.index);
        let out = view
            .widgets(types.as_deref())
            .map_err(crate::error::facade(op))?;
        Ok(out.into_iter().map(|x| Widget { inner: x }).collect())
    }

    /// Page.write_text from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "writeText")]
    pub fn write_text(&self, options: Option<serde_json::Value>) -> napi::Result<()> {
        let op = "writeText";
        self.shared.check_open(op)?;
        let options_o = write_text_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        view.write_text(&options_o)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }
}

#[napi]
impl Shape {
    /// Shape.__eq__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "eq")]
    pub fn eq(&self, other: &Shape) -> bool {
        std::sync::Arc::ptr_eq(&self.shared, &other.shared) && self.page == other.page
    }

    /// Shape.__ge__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "ge")]
    pub fn ge(&self, _other: &Shape) -> napi::Result<bool> {
        let op = "ge";
        self.shared.check_open(op)?;
        Err(compat_error(
            op,
            fc::ErrorKind::BadValue,
            format!("{op}: ordering is not supported for Shape"),
        ))
    }

    /// Shape.__gt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "gt")]
    pub fn gt(&self, _other: &Shape) -> napi::Result<bool> {
        let op = "gt";
        self.shared.check_open(op)?;
        Err(compat_error(
            op,
            fc::ErrorKind::BadValue,
            format!("{op}: ordering is not supported for Shape"),
        ))
    }

    /// Shape.__init__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(constructor)]
    pub fn new(page: &Page) -> napi::Result<Self> {
        let op = "Shape";
        page.shared.check_open(op)?;
        Ok(Self {
            shared: page.shared.clone(),
            page: page.index,
        })
    }

    /// Shape.__le__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "le")]
    pub fn le(&self, _other: &Shape) -> napi::Result<bool> {
        let op = "le";
        self.shared.check_open(op)?;
        Err(compat_error(
            op,
            fc::ErrorKind::BadValue,
            format!("{op}: ordering is not supported for Shape"),
        ))
    }

    /// Shape.__lt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "lt")]
    pub fn lt(&self, _other: &Shape) -> napi::Result<bool> {
        let op = "lt";
        self.shared.check_open(op)?;
        Err(compat_error(
            op,
            fc::ErrorKind::BadValue,
            format!("{op}: ordering is not supported for Shape"),
        ))
    }

    /// Shape.__ne__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "ne")]
    pub fn ne(&self, other: &Shape) -> bool {
        !(std::sync::Arc::ptr_eq(&self.shared, &other.shared) && self.page == other.page)
    }

    /// Shape.commit from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "commit")]
    pub fn commit(&self, overlay: Option<bool>) -> napi::Result<()> {
        let op = "commit";
        self.shared.check_open(op)?;
        let overlay_v = overlay.unwrap_or(true);
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::Shape::new(&mut guard, self.page);
        view.commit(overlay_v).map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Shape.draw_bezier from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "drawBezier")]
    pub fn draw_bezier(
        &self,
        p1: PointLike,
        p2: PointLike,
        p3: PointLike,
        p4: PointLike,
    ) -> napi::Result<Point> {
        let op = "drawBezier";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::Shape::new(&mut guard, self.page);
        let out = view
            .draw_bezier(
                p1.to_compat(),
                p2.to_compat(),
                p3.to_compat(),
                p4.to_compat(),
            )
            .map_err(crate::error::facade(op))?;
        Ok(Point { inner: out })
    }

    /// Shape.draw_circle from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "drawCircle")]
    pub fn draw_circle(&self, center: PointLike, radius: f64) -> napi::Result<Point> {
        let op = "drawCircle";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::Shape::new(&mut guard, self.page);
        let out = view
            .draw_circle(center.to_compat(), radius)
            .map_err(crate::error::facade(op))?;
        Ok(Point { inner: out })
    }

    /// Shape.draw_curve from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "drawCurve")]
    pub fn draw_curve(&self, p1: PointLike, p2: PointLike, p3: PointLike) -> napi::Result<Point> {
        let op = "drawCurve";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::Shape::new(&mut guard, self.page);
        let out = view
            .draw_curve(p1.to_compat(), p2.to_compat(), p3.to_compat())
            .map_err(crate::error::facade(op))?;
        Ok(Point { inner: out })
    }

    /// Shape.draw_line from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "drawLine")]
    pub fn draw_line(&self, p1: PointLike, p2: PointLike) -> napi::Result<Point> {
        let op = "drawLine";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::Shape::new(&mut guard, self.page);
        let out = view
            .draw_line(p1.to_compat(), p2.to_compat())
            .map_err(crate::error::facade(op))?;
        Ok(Point { inner: out })
    }

    /// Shape.draw_oval from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "drawOval")]
    pub fn draw_oval(&self, rect: RectLike) -> napi::Result<Point> {
        let op = "drawOval";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::Shape::new(&mut guard, self.page);
        let out = view
            .draw_oval(rect.to_compat())
            .map_err(crate::error::facade(op))?;
        Ok(Point { inner: out })
    }

    /// Shape.draw_polyline from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "drawPolyline")]
    pub fn draw_polyline(&self, points: Vec<PointLike>) -> napi::Result<Point> {
        let op = "drawPolyline";
        self.shared.check_open(op)?;
        let points_v: Vec<fc::Point> = points.iter().map(PointLike::to_compat).collect();
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::Shape::new(&mut guard, self.page);
        let out = view
            .draw_polyline(&points_v)
            .map_err(crate::error::facade(op))?;
        Ok(Point { inner: out })
    }

    /// Shape.draw_quad from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "drawQuad")]
    pub fn draw_quad(&self, quad: QuadLike) -> napi::Result<Point> {
        let op = "drawQuad";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::Shape::new(&mut guard, self.page);
        let out = view
            .draw_quad(quad.to_compat())
            .map_err(crate::error::facade(op))?;
        Ok(Point { inner: out })
    }

    /// Shape.draw_rect from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "drawRect")]
    pub fn draw_rect(&self, rect: RectLike, radius: Option<f64>) -> napi::Result<Point> {
        let op = "drawRect";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::Shape::new(&mut guard, self.page);
        let out = view
            .draw_rect(rect.to_compat(), radius)
            .map_err(crate::error::facade(op))?;
        Ok(Point { inner: out })
    }

    /// Shape.draw_sector from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "drawSector")]
    pub fn draw_sector(
        &self,
        center: PointLike,
        point: PointLike,
        beta: f64,
        full_sector: Option<bool>,
    ) -> napi::Result<Point> {
        let op = "drawSector";
        self.shared.check_open(op)?;
        let full_sector_v = full_sector.unwrap_or(true);
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::Shape::new(&mut guard, self.page);
        let out = view
            .draw_sector(center.to_compat(), point.to_compat(), beta, full_sector_v)
            .map_err(crate::error::facade(op))?;
        Ok(Point { inner: out })
    }

    /// Shape.draw_squiggle from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "drawSquiggle")]
    pub fn draw_squiggle(
        &self,
        p1: PointLike,
        p2: PointLike,
        breadth: Option<f64>,
    ) -> napi::Result<Point> {
        let op = "drawSquiggle";
        self.shared.check_open(op)?;
        let breadth_v = breadth.unwrap_or(2.0);
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::Shape::new(&mut guard, self.page);
        let out = view
            .draw_squiggle(p1.to_compat(), p2.to_compat(), breadth_v)
            .map_err(crate::error::facade(op))?;
        Ok(Point { inner: out })
    }

    /// Shape.draw_zigzag from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "drawZigzag")]
    pub fn draw_zigzag(
        &self,
        p1: PointLike,
        p2: PointLike,
        breadth: Option<f64>,
    ) -> napi::Result<Point> {
        let op = "drawZigzag";
        self.shared.check_open(op)?;
        let breadth_v = breadth.unwrap_or(2.0);
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::Shape::new(&mut guard, self.page);
        let out = view
            .draw_zigzag(p1.to_compat(), p2.to_compat(), breadth_v)
            .map_err(crate::error::facade(op))?;
        Ok(Point { inner: out })
    }

    /// Shape.finish from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "finish")]
    pub fn finish(&self, options: Option<serde_json::Value>) -> napi::Result<()> {
        let op = "finish";
        self.shared.check_open(op)?;
        let options_o = shape_finish_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::Shape::new(&mut guard, self.page);
        view.finish(&options_o).map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Shape.horizontal_angle from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "horizontalAngle")]
    pub fn horizontal_angle(&self, center: PointLike, point: PointLike) -> napi::Result<f64> {
        let op = "horizontalAngle";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let view = fc::Shape::new(&mut guard, self.page);
        let out = view.horizontal_angle(center.to_compat(), point.to_compat());
        Ok(out)
    }

    /// Shape.insert_text from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "insertText")]
    pub fn insert_text(
        &self,
        point: PointLike,
        text: serde_json::Value,
        options: Option<serde_json::Value>,
    ) -> napi::Result<i32> {
        let op = "insertText";
        self.shared.check_open(op)?;
        let text_v = text_arg_from_json(op, &text)?;
        let options_o = insert_text_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::Shape::new(&mut guard, self.page);
        let out = view
            .insert_text(point.to_compat(), &text_v, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Shape.insert_textbox from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "insertTextbox")]
    pub fn insert_textbox(
        &self,
        rect: RectLike,
        text: serde_json::Value,
        options: Option<serde_json::Value>,
    ) -> napi::Result<f64> {
        let op = "insertTextbox";
        self.shared.check_open(op)?;
        let text_v = text_arg_from_json(op, &text)?;
        let options_o = text_box_options_from_json(op, options.as_ref())?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::Shape::new(&mut guard, self.page);
        let out = view
            .insert_textbox(rect.to_compat(), &text_v, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Shape.updateRect from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "updateRect")]
    pub fn update_rect(&self, rect: RectLike) -> napi::Result<()> {
        let op = "updateRect";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::Shape::new(&mut guard, self.page);
        view.update_rect(rect.to_compat())
            .map_err(crate::error::facade(op))?;
        Ok(())
    }
}

#[napi]
impl TextWriter {
    /// TextWriter.__eq__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "eq")]
    pub fn eq(&self, other: &TextWriter) -> bool {
        self.inner == other.inner
    }

    /// TextWriter.__ge__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "ge")]
    pub fn ge(&self, other: &TextWriter) -> napi::Result<bool> {
        let op = "ge";
        self.inner
            .compare(&other.inner, fc::CompareOp::GreaterOrEqual)
            .map_err(crate::error::facade(op))
    }

    /// TextWriter.__gt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "gt")]
    pub fn gt(&self, other: &TextWriter) -> napi::Result<bool> {
        let op = "gt";
        self.inner
            .compare(&other.inner, fc::CompareOp::Greater)
            .map_err(crate::error::facade(op))
    }

    /// TextWriter.__init__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(constructor)]
    pub fn new(rect: RectLike, options: Option<serde_json::Value>) -> napi::Result<Self> {
        let op = "TextWriter";
        let o = text_writer_options_from_json(op, options.as_ref())?;
        Ok(Self {
            inner: fc::TextWriter::new(rect.to_compat(), &o),
        })
    }

    /// TextWriter.__le__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "le")]
    pub fn le(&self, other: &TextWriter) -> napi::Result<bool> {
        let op = "le";
        self.inner
            .compare(&other.inner, fc::CompareOp::LessOrEqual)
            .map_err(crate::error::facade(op))
    }

    /// TextWriter.__lt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "lt")]
    pub fn lt(&self, other: &TextWriter) -> napi::Result<bool> {
        let op = "lt";
        self.inner
            .compare(&other.inner, fc::CompareOp::Less)
            .map_err(crate::error::facade(op))
    }

    /// TextWriter.__ne__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "ne")]
    pub fn ne(&self, other: &TextWriter) -> bool {
        self.inner != other.inner
    }

    /// TextWriter.append from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "append")]
    pub fn append(
        &mut self,
        pos: PointLike,
        text: String,
        options: Option<serde_json::Value>,
        font: Option<&Font>,
    ) -> napi::Result<serde_json::Value> {
        let op = "append";
        let mut options_o = append_options_from_json(op, options.as_ref())?;
        if let Some(f) = font.as_ref() {
            options_o = options_o.with_font(&f.inner);
        }
        let out = self
            .inner
            .append(pos.to_compat(), &text, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(tuple_to_json_rect_point(&out))
    }

    /// TextWriter.appendv from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "appendv")]
    pub fn appendv(
        &mut self,
        pos: PointLike,
        text: String,
        options: Option<serde_json::Value>,
        font: Option<&Font>,
    ) -> napi::Result<serde_json::Value> {
        let op = "appendv";
        let mut options_o = append_options_from_json(op, options.as_ref())?;
        if let Some(f) = font.as_ref() {
            options_o = options_o.with_font(&f.inner);
        }
        let out = self
            .inner
            .appendv(pos.to_compat(), &text, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(tuple_to_json_rect_point(&out))
    }

    /// TextWriter.clean_rtl from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "cleanRtl")]
    pub fn clean_rtl(&self, text: String) -> napi::Result<String> {
        let out = self.inner.clean_rtl(&text);
        Ok(out)
    }

    /// TextWriter.fill_textbox from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "fillTextbox")]
    pub fn fill_textbox(
        &mut self,
        rect: RectLike,
        text: serde_json::Value,
        options: Option<serde_json::Value>,
        font: Option<&Font>,
    ) -> napi::Result<Vec<serde_json::Value>> {
        let op = "fillTextbox";
        let text_v = text_arg_from_json(op, &text)?;
        let mut options_o = fill_text_box_options_from_json(op, options.as_ref())?;
        if let Some(f) = font.as_ref() {
            options_o = options_o.with_font(&f.inner);
        }
        let out = self
            .inner
            .fill_textbox(rect.to_compat(), &text_v, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(out
            .into_iter()
            .map(|x| tuple_to_json_string_f64(&x))
            .collect())
    }

    /// TextWriter.write_text from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "writeText")]
    pub fn write_text(&self, page: &Page, options: Option<serde_json::Value>) -> napi::Result<()> {
        let op = "writeText";
        page.shared.check_open(op)?;
        let mut page_guard = page.shared.lock(op)?;
        let mut page_v = fc::PageMut::new(&mut page_guard, page.index);
        let options_o = flush_options_from_json(op, options.as_ref())?;
        self.inner
            .write_text(&mut page_v, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }
}

#[napi]
impl Widget {
    /// Widget.__eq__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "eq")]
    pub fn eq(&self, other: &Widget) -> bool {
        self.inner == other.inner
    }

    /// Widget.__ge__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "ge")]
    pub fn ge(&self, other: &Widget) -> napi::Result<bool> {
        let op = "ge";
        self.inner
            .compare(&other.inner, fc::CompareOp::GreaterOrEqual)
            .map_err(crate::error::facade(op))
    }

    /// Widget.__gt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "gt")]
    pub fn gt(&self, other: &Widget) -> napi::Result<bool> {
        let op = "gt";
        self.inner
            .compare(&other.inner, fc::CompareOp::Greater)
            .map_err(crate::error::facade(op))
    }

    /// Widget.__init__ from the compatible API.
    #[allow(clippy::too_many_arguments, clippy::new_without_default)]
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: fc::Widget::new(),
        }
    }

    /// Widget.__le__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "le")]
    pub fn le(&self, other: &Widget) -> napi::Result<bool> {
        let op = "le";
        self.inner
            .compare(&other.inner, fc::CompareOp::LessOrEqual)
            .map_err(crate::error::facade(op))
    }

    /// Widget.__lt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "lt")]
    pub fn lt(&self, other: &Widget) -> napi::Result<bool> {
        let op = "lt";
        self.inner
            .compare(&other.inner, fc::CompareOp::Less)
            .map_err(crate::error::facade(op))
    }

    /// Widget.__ne__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "ne")]
    pub fn ne(&self, other: &Widget) -> bool {
        self.inner != other.inner
    }

    /// Widget.button_states from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "buttonStates")]
    pub fn button_states(&self) -> napi::Result<Vec<String>> {
        let op = "buttonStates";
        let out = self
            .inner
            .button_states()
            .map_err(crate::error::facade(op))?;
        Ok(out.into_iter().collect())
    }

    /// Widget.next from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "next")]
    pub fn next(&self) -> napi::Result<Option<Widget>> {
        let out = self.inner.next();
        Ok(out.map(|x| Widget { inner: x }))
    }

    /// Widget.on_state from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "onState")]
    pub fn on_state(&self) -> napi::Result<String> {
        let op = "onState";
        let out = self.inner.on_state().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Widget.reset from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "reset")]
    pub fn reset(&mut self) -> napi::Result<()> {
        let op = "reset";
        self.inner.reset().map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Widget.update from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "update")]
    pub fn update(&self, sync_flags: Option<serde_json::Value>) -> napi::Result<()> {
        let op = "update";
        accept_default(
            op,
            sync_flags.as_ref(),
            serde_json::Value::Bool(false),
            "sync_flags",
        )?;
        self.inner.update().map_err(crate::error::facade(op))?;
        Ok(())
    }
}
