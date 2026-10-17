//! Compatible document operations (lane L2).
// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use super::geometry::{Matrix, Point, Rect, RectLike};
use super::*;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use pdfluent::compat as fc;

#[napi]
pub struct Document {
    pub(crate) shared: std::sync::Arc<Shared>,
}

#[napi]
pub struct Page {
    pub(crate) shared: std::sync::Arc<Shared>,
    pub(crate) index: usize,
}

#[napi]
pub struct Link {
    pub(crate) inner: fc::Link,
}

#[napi]
impl Link {
    /// Read the link kind code.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "kind")]
    pub fn kind(&self) -> i32 {
        self.inner.kind
    }

    /// Write the link kind code.
    #[allow(clippy::too_many_arguments)]
    #[napi(setter, js_name = "kind")]
    pub fn set_kind(&mut self, value: i32) {
        self.inner.kind = value;
    }

    /// Read the link cross-reference.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "xref")]
    pub fn xref(&self) -> i32 {
        self.inner.xref
    }

    /// Read the link identifier.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "id")]
    pub fn id(&self) -> String {
        self.inner.id.clone()
    }
}

/// Normalize a start/stop/step slice against a page count.
fn slice_bounds(count: i32, start: Option<i32>, stop: Option<i32>, step: i32) -> (i32, i32) {
    let norm = |v: i32| {
        let x = if v < 0 { count + v } else { v };
        if step > 0 {
            x.clamp(0, count)
        } else {
            x.clamp(-1, count - 1)
        }
    };
    let lo = start.map_or(if step > 0 { 0 } else { count - 1 }, norm);
    let hi = stop.map_or(if step > 0 { count } else { -1 }, norm);
    (lo, hi)
}

#[napi]
impl Document {
    /// Document.__contains__ from the compatible API.
    ///
    /// Not yet wired: the facade cannot serve it (unconstructible:Location). Raises until the facade grows the accessor.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "contains")]
    pub fn __contains__(&self) -> napi::Result<serde_json::Value> {
        let op = "contains";
        Err(compat_error(
            op,
            fc::ErrorKind::EngineFault,
            format!("{op}: the facade cannot serve it (unconstructible:Location)"),
        ))
    }

    /// Document.__delitem__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "delitem")]
    pub fn delete_at(&self, index: i32) -> napi::Result<()> {
        let op = "delitem";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        guard.delete_at(index).map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.__enter__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "enter")]
    pub fn enter(&self) -> napi::Result<()> {
        let op = "enter";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        guard.enter();
        Ok(())
    }

    /// Document.__eq__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "eq")]
    pub fn eq(&self, other: &Document) -> napi::Result<bool> {
        let op = "eq";
        self.shared.check_open(op)?;
        other.shared.check_open(op)?;
        if std::sync::Arc::ptr_eq(&self.shared, &other.shared) {
            let guard = self.shared.lock(op)?;
            let a = &*guard;
            let b_same = &*guard;
            Ok(a == b_same)
        } else {
            let guard = self.shared.lock(op)?;
            let other_guard = other.shared.lock(op)?;
            let a = &*guard;
            let b = &*other_guard;
            Ok(a == b)
        }
    }

    /// Document.__exit__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "exit")]
    pub fn exit(&self) -> napi::Result<()> {
        let op = "exit";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        guard.exit().map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.__ge__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "ge")]
    pub fn ge(&self, other: &Document) -> napi::Result<bool> {
        let op = "ge";
        self.shared.check_open(op)?;
        other.shared.check_open(op)?;
        if std::sync::Arc::ptr_eq(&self.shared, &other.shared) {
            let guard = self.shared.lock(op)?;
            let a = &*guard;
            let b_same = &*guard;
            a.compare(b_same, fc::CompareOp::GreaterOrEqual)
                .map_err(crate::error::facade(op))
        } else {
            let guard = self.shared.lock(op)?;
            let other_guard = other.shared.lock(op)?;
            let a = &*guard;
            let b = &*other_guard;
            a.compare(b, fc::CompareOp::GreaterOrEqual)
                .map_err(crate::error::facade(op))
        }
    }

    /// Document.__getitem__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getitem")]
    pub fn at(&self, index: Option<i32>) -> napi::Result<Page> {
        let op = "getitem";
        self.shared.check_open(op)?;
        let index_v = index.unwrap_or(0);
        let guard = self.shared.lock(op)?;
        let out = guard.at(index_v).map_err(crate::error::facade(op))?;
        Ok(Page {
            shared: self.shared.clone(),
            index: out.index(),
        })
    }

    /// Document.__gt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "gt")]
    pub fn gt(&self, other: &Document) -> napi::Result<bool> {
        let op = "gt";
        self.shared.check_open(op)?;
        other.shared.check_open(op)?;
        if std::sync::Arc::ptr_eq(&self.shared, &other.shared) {
            let guard = self.shared.lock(op)?;
            let a = &*guard;
            let b_same = &*guard;
            a.compare(b_same, fc::CompareOp::Greater)
                .map_err(crate::error::facade(op))
        } else {
            let guard = self.shared.lock(op)?;
            let other_guard = other.shared.lock(op)?;
            let a = &*guard;
            let b = &*other_guard;
            a.compare(b, fc::CompareOp::Greater)
                .map_err(crate::error::facade(op))
        }
    }

    /// Document.__init__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(constructor)]
    pub fn new(
        filename: Option<String>,
        stream: Option<Buffer>,
        filetype: Option<String>,
        rect: Option<RectLike>,
        width: Option<f64>,
        height: Option<f64>,
        fontsize: Option<f64>,
        archive: Option<serde_json::Value>,
    ) -> napi::Result<Self> {
        let op = "Document";
        let mut o = fc::DocumentOpenOptions::default();
        if let Some(v) = filename.as_ref() {
            o = o.with_filename(v);
        }
        if let Some(v) = stream.as_ref() {
            o = o.with_data(Vec::<u8>::from(v.clone()));
        }
        if let Some(v) = filetype.as_ref() {
            o = o.with_filetype(v);
        }
        if let Some(v) = rect {
            o = o.with_rect(v.to_compat());
        }
        if width.is_some() || height.is_some() {
            o = o.with_size(width.unwrap_or(0.0), height.unwrap_or(0.0));
        }
        if let Some(v) = fontsize {
            o = o.with_fontsize(v);
        }
        if let Some(v) = archive.as_ref() {
            o = o.with_archive(json_to_compat(v));
        }
        let doc = FacadePdfDocument::open_compat(&o).map_err(crate::error::facade(op))?;
        Ok(Self {
            shared: std::sync::Arc::new(Shared::new(doc)),
        })
    }

    /// Document.__le__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "le")]
    pub fn le(&self, other: &Document) -> napi::Result<bool> {
        let op = "le";
        self.shared.check_open(op)?;
        other.shared.check_open(op)?;
        if std::sync::Arc::ptr_eq(&self.shared, &other.shared) {
            let guard = self.shared.lock(op)?;
            let a = &*guard;
            let b_same = &*guard;
            a.compare(b_same, fc::CompareOp::LessOrEqual)
                .map_err(crate::error::facade(op))
        } else {
            let guard = self.shared.lock(op)?;
            let other_guard = other.shared.lock(op)?;
            let a = &*guard;
            let b = &*other_guard;
            a.compare(b, fc::CompareOp::LessOrEqual)
                .map_err(crate::error::facade(op))
        }
    }

    /// Document.__len__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::len_without_is_empty)]
    #[napi(js_name = "len")]
    pub fn len(&self) -> napi::Result<u32> {
        let op = "len";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.len();
        Ok(out as u32)
    }

    /// Document.__lt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "lt")]
    pub fn lt(&self, other: &Document) -> napi::Result<bool> {
        let op = "lt";
        self.shared.check_open(op)?;
        other.shared.check_open(op)?;
        if std::sync::Arc::ptr_eq(&self.shared, &other.shared) {
            let guard = self.shared.lock(op)?;
            let a = &*guard;
            let b_same = &*guard;
            a.compare(b_same, fc::CompareOp::Less)
                .map_err(crate::error::facade(op))
        } else {
            let guard = self.shared.lock(op)?;
            let other_guard = other.shared.lock(op)?;
            let a = &*guard;
            let b = &*other_guard;
            a.compare(b, fc::CompareOp::Less)
                .map_err(crate::error::facade(op))
        }
    }

    /// Document.__ne__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "ne")]
    pub fn ne(&self, other: &Document) -> napi::Result<bool> {
        let op = "ne";
        self.shared.check_open(op)?;
        other.shared.check_open(op)?;
        if std::sync::Arc::ptr_eq(&self.shared, &other.shared) {
            let guard = self.shared.lock(op)?;
            let a = &*guard;
            let b_same = &*guard;
            Ok(a != b_same)
        } else {
            let guard = self.shared.lock(op)?;
            let other_guard = other.shared.lock(op)?;
            let a = &*guard;
            let b = &*other_guard;
            Ok(a != b)
        }
    }

    /// Document.authenticate from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "authenticate")]
    pub fn authenticate(&self, password: String) -> napi::Result<i32> {
        let op = "authenticate";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard
            .authenticate(&password)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.can_save_incrementally from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "canSaveIncrementally")]
    pub fn can_save_incrementally(&self) -> napi::Result<bool> {
        let op = "canSaveIncrementally";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard
            .can_save_incrementally()
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.chapter_count from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "chapterCount")]
    pub fn chapter_count(&self) -> napi::Result<u32> {
        let op = "chapterCount";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.chapter_count().map_err(crate::error::facade(op))?;
        Ok(out as u32)
    }

    /// Document.chapter_page_count from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "chapterPageCount")]
    pub fn chapter_page_count(&self, chapter: i32) -> napi::Result<u32> {
        let op = "chapterPageCount";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard
            .chapter_page_count(chapter)
            .map_err(crate::error::facade(op))?;
        Ok(out as u32)
    }

    /// Document.close from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "close")]
    pub fn close(&self) -> napi::Result<()> {
        self.shared.close();
        Ok(())
    }

    /// Document.convert_to_pdf from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "convertToPdf")]
    pub fn convert_to_pdf(
        &self,
        from_page: Option<i32>,
        to_page: Option<i32>,
        rotate: Option<i32>,
    ) -> napi::Result<Buffer> {
        let op = "convertToPdf";
        self.shared.check_open(op)?;
        let from_page_v = from_page.unwrap_or(0);
        let to_page_v = to_page.unwrap_or(-1);
        let rotate_v = rotate.unwrap_or(0);
        let guard = self.shared.lock(op)?;
        let out = guard
            .convert_to_pdf(from_page_v, to_page_v, rotate_v)
            .map_err(crate::error::facade(op))?;
        Ok(Buffer::from(out))
    }

    /// Document.embfile_count from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "embfileCount")]
    pub fn embfile_count(&self) -> napi::Result<u32> {
        let op = "embfileCount";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.embfile_count().map_err(crate::error::facade(op))?;
        Ok(out as u32)
    }

    /// Document.embfile_get from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "embfileGet")]
    pub fn embfile_get(&self, item: serde_json::Value) -> napi::Result<Buffer> {
        let op = "embfileGet";
        self.shared.check_open(op)?;
        let item_v = emb_item_from_json(op, &item)?;
        let guard = self.shared.lock(op)?;
        let out = guard
            .embfile_get(&item_v)
            .map_err(crate::error::facade(op))?;
        Ok(Buffer::from(out))
    }

    /// Document.embfile_info from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "embfileInfo")]
    pub fn embfile_info(&self, item: serde_json::Value) -> napi::Result<serde_json::Value> {
        let op = "embfileInfo";
        self.shared.check_open(op)?;
        let item_v = emb_item_from_json(op, &item)?;
        let guard = self.shared.lock(op)?;
        let out = guard
            .embfile_info(&item_v)
            .map_err(crate::error::facade(op))?;
        Ok(embedded_file_to_json(&out))
    }

    /// Document.embfile_names from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "embfileNames")]
    pub fn embfile_names(&self) -> napi::Result<Vec<String>> {
        let op = "embfileNames";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.embfile_names().map_err(crate::error::facade(op))?;
        Ok(out.into_iter().collect())
    }

    /// Document.ez_save from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "ezSave")]
    pub fn ez_save(&self, path: String, options: Option<serde_json::Value>) -> napi::Result<()> {
        let op = "ezSave";
        self.shared.check_open(op)?;
        let options_o = save_settings_from_json(op, options.as_ref())?;
        let guard = self.shared.lock(op)?;
        guard
            .ez_save(&path, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.find_bookmark from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "findBookmark")]
    pub fn find_bookmark(&self, mark: f64) -> napi::Result<serde_json::Value> {
        let op = "findBookmark";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard
            .find_bookmark(mark as i64)
            .map_err(crate::error::facade(op))?;
        Ok(location_to_json(&out))
    }

    /// Document.get_layer from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getLayer")]
    pub fn get_layer(&self, config: Option<i32>) -> napi::Result<serde_json::Value> {
        let op = "getLayer";
        self.shared.check_open(op)?;
        let config_v = config.unwrap_or(-1);
        let guard = self.shared.lock(op)?;
        let out = guard
            .get_layer(config_v)
            .map_err(crate::error::facade(op))?;
        Ok(ocg_to_json(&out))
    }

    /// Document.get_layers from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getLayers")]
    pub fn get_layers(&self) -> napi::Result<Vec<serde_json::Value>> {
        let op = "getLayers";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.get_layers().map_err(crate::error::facade(op))?;
        Ok(out.into_iter().map(|x| ocg_to_json(&x)).collect())
    }

    /// Document.get_new_xref from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getNewXref")]
    pub fn get_new_xref(&self) -> napi::Result<i32> {
        let op = "getNewXref";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let out = guard.get_new_xref().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.get_oc from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getOc")]
    pub fn get_oc(&self, xref: i32) -> napi::Result<i32> {
        let op = "getOc";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.get_oc(xref).map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.get_ocgs from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getOcgs")]
    pub fn get_ocgs(&self) -> napi::Result<Vec<serde_json::Value>> {
        let op = "getOcgs";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.get_ocgs().map_err(crate::error::facade(op))?;
        Ok(out.into_iter().map(|x| ocg_to_json(&x)).collect())
    }

    /// Document.get_ocmd from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getOcmd")]
    pub fn get_ocmd(&self, xref: i32) -> napi::Result<serde_json::Value> {
        let op = "getOcmd";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.get_ocmd(xref).map_err(crate::error::facade(op))?;
        Ok(ocmd_info_to_json(&out))
    }

    /// Document.get_outline_xrefs from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getOutlineXrefs")]
    pub fn get_outline_xrefs(&self) -> napi::Result<Vec<i32>> {
        let op = "getOutlineXrefs";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard
            .get_outline_xrefs()
            .map_err(crate::error::facade(op))?;
        Ok(out.into_iter().collect())
    }

    /// Document.get_page_labels from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getPageLabels")]
    pub fn get_page_labels(&self) -> napi::Result<Vec<serde_json::Value>> {
        let op = "getPageLabels";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.get_page_labels().map_err(crate::error::facade(op))?;
        Ok(out.into_iter().map(|x| page_label_to_json(&x)).collect())
    }

    /// Document.get_page_numbers from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getPageNumbers")]
    pub fn get_page_numbers(
        &self,
        label: String,
        only_one: Option<bool>,
    ) -> napi::Result<Vec<i32>> {
        let op = "getPageNumbers";
        self.shared.check_open(op)?;
        let only_one_v = only_one.unwrap_or(false);
        let guard = self.shared.lock(op)?;
        let out = guard
            .get_page_numbers(&label, only_one_v)
            .map_err(crate::error::facade(op))?;
        Ok(out.into_iter().collect())
    }

    /// Document.get_page_xobjects from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getPageXobjects")]
    pub fn get_page_xobjects(&self, page: i32) -> napi::Result<Vec<i32>> {
        let op = "getPageXobjects";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard
            .get_page_xobjects(page)
            .map_err(crate::error::facade(op))?;
        Ok(out.into_iter().collect())
    }

    /// Document.get_sigflags from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getSigflags")]
    pub fn get_sigflags(&self) -> napi::Result<i32> {
        let op = "getSigflags";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.get_sigflags().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.get_toc from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getToc")]
    pub fn get_toc(&self, simple: Option<bool>) -> napi::Result<Vec<serde_json::Value>> {
        let op = "getToc";
        self.shared.check_open(op)?;
        let simple_v = simple.unwrap_or(true);
        let guard = self.shared.lock(op)?;
        let out = guard.get_toc(simple_v).map_err(crate::error::facade(op))?;
        Ok(out.into_iter().map(|x| toc_entry_to_json(&x)).collect())
    }

    /// Document.get_xml_metadata from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getXmlMetadata")]
    pub fn get_xml_metadata(&self) -> napi::Result<String> {
        let op = "getXmlMetadata";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.get_xml_metadata().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.init_doc from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "initDoc")]
    pub fn init_doc(&self) -> napi::Result<()> {
        let op = "initDoc";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        guard.init_doc().map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.is_dirty from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "isDirty")]
    pub fn is_dirty(&self) -> napi::Result<bool> {
        let op = "isDirty";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.is_dirty().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.is_fast_webaccess from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "isFastWebaccess")]
    pub fn is_fast_webaccess(&self) -> napi::Result<bool> {
        let op = "isFastWebaccess";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard
            .is_fast_webaccess()
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.is_form_pdf from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "isFormPdf")]
    pub fn is_form_pdf(&self) -> napi::Result<bool> {
        let op = "isFormPdf";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.is_form_pdf().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.is_pdf from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "isPdf")]
    pub fn is_pdf(&self) -> napi::Result<bool> {
        let op = "isPdf";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.is_pdf().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.is_reflowable from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "isReflowable")]
    pub fn is_reflowable(&self) -> napi::Result<bool> {
        let op = "isReflowable";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.is_reflowable().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.is_repaired from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "isRepaired")]
    pub fn is_repaired(&self) -> napi::Result<bool> {
        let op = "isRepaired";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.is_repaired().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.is_stream from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "isStream")]
    pub fn is_stream(&self, xref: Option<i32>) -> napi::Result<bool> {
        let op = "isStream";
        self.shared.check_open(op)?;
        let xref_v = xref.unwrap_or(0);
        let guard = self.shared.lock(op)?;
        let out = guard.is_stream(xref_v).map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.language from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "language")]
    pub fn language(&self) -> napi::Result<String> {
        let op = "language";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.language().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.last_location from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "lastLocation")]
    pub fn last_location(&self) -> napi::Result<serde_json::Value> {
        let op = "lastLocation";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.last_location().map_err(crate::error::facade(op))?;
        Ok(location_to_json(&out))
    }

    /// Document.layer_ui_configs from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "layerUiConfigs")]
    pub fn layer_ui_configs(&self) -> napi::Result<Vec<serde_json::Value>> {
        let op = "layerUiConfigs";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.layer_ui_configs().map_err(crate::error::facade(op))?;
        out.into_iter().map(|x| compat_to_json(&x)).collect()
    }

    /// Document.layout from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "layout")]
    pub fn layout(
        &self,
        rect: Option<RectLike>,
        width: Option<f64>,
        height: Option<f64>,
        fontsize: Option<f64>,
    ) -> napi::Result<()> {
        let op = "layout";
        self.shared.check_open(op)?;
        let rect_v = rect.as_ref().map(RectLike::to_compat);
        let width_v = width.unwrap_or(0.0);
        let height_v = height.unwrap_or(0.0);
        let fontsize_v = fontsize.unwrap_or(11.0);
        let mut guard = self.shared.lock(op)?;
        guard
            .layout(rect_v, width_v, height_v, fontsize_v)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.load_page from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "loadPage")]
    pub fn load_page(&self, number: i32) -> napi::Result<Page> {
        let op = "loadPage";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.load_page(number).map_err(crate::error::facade(op))?;
        Ok(Page {
            shared: self.shared.clone(),
            index: out.index(),
        })
    }

    /// Document.location_from_page_number from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "locationFromPageNumber")]
    pub fn location_from_page_number(&self, page: i32) -> napi::Result<serde_json::Value> {
        let op = "locationFromPageNumber";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard
            .location_from_page_number(page)
            .map_err(crate::error::facade(op))?;
        Ok(location_to_json(&out))
    }

    /// Document.make_bookmark from the compatible API.
    ///
    /// Not yet wired: the facade cannot serve it (unconstructible:Location). Raises until the facade grows the accessor.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "makeBookmark")]
    pub fn make_bookmark(&self) -> napi::Result<serde_json::Value> {
        let op = "makeBookmark";
        Err(compat_error(
            op,
            fc::ErrorKind::EngineFault,
            format!("{op}: the facade cannot serve it (unconstructible:Location)"),
        ))
    }

    /// Document.markinfo from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "markinfo")]
    pub fn markinfo(&self) -> napi::Result<serde_json::Value> {
        let op = "markinfo";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.markinfo().map_err(crate::error::facade(op))?;
        compat_to_json(&out)
    }

    /// Document.name from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "name")]
    pub fn name(&self) -> napi::Result<String> {
        let op = "name";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.name().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.need_appearances from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "needAppearances")]
    pub fn need_appearances(&self, value: Option<serde_json::Value>) -> napi::Result<bool> {
        let op = "needAppearances";
        self.shared.check_open(op)?;
        accept_default(op, value.as_ref(), serde_json::Value::Null, "value")?;
        let guard = self.shared.lock(op)?;
        let out = guard.need_appearances().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.needs_pass from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "needsPass")]
    pub fn needs_pass(&self) -> napi::Result<i32> {
        let op = "needsPass";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.needs_pass().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.next_location from the compatible API.
    ///
    /// Not yet wired: the facade cannot serve it (unconstructible:Location). Raises until the facade grows the accessor.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "nextLocation")]
    pub fn next_location(&self) -> napi::Result<serde_json::Value> {
        let op = "nextLocation";
        Err(compat_error(
            op,
            fc::ErrorKind::EngineFault,
            format!("{op}: the facade cannot serve it (unconstructible:Location)"),
        ))
    }

    /// Document.outline from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "outline")]
    pub fn outline(&self) -> napi::Result<Vec<serde_json::Value>> {
        let op = "outline";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.outline().map_err(crate::error::facade(op))?;
        Ok(out.into_iter().map(|x| toc_entry_to_json(&x)).collect())
    }

    /// Document.page_count from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "pageCount")]
    pub fn page_count(&self) -> napi::Result<u32> {
        let op = "pageCount";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.page_count();
        Ok(out as u32)
    }

    /// Document.page_count2 from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "pageCount2")]
    pub fn page_count2(&self) -> napi::Result<serde_json::Value> {
        let op = "pageCount2";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.page_count2().map_err(crate::error::facade(op))?;
        Ok(tuple_to_json_usize_usize(&out))
    }

    /// Document.page_cropbox from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "pageCropbox")]
    pub fn page_cropbox(&self, page: i32) -> napi::Result<Rect> {
        let op = "pageCropbox";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.page_cropbox(page).map_err(crate::error::facade(op))?;
        Ok(Rect { inner: out })
    }

    /// Document.page_number_from_location from the compatible API.
    ///
    /// Not yet wired: the facade cannot serve it (unconstructible:Location). Raises until the facade grows the accessor.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "pageNumberFromLocation")]
    pub fn page_number_from_location(&self) -> napi::Result<serde_json::Value> {
        let op = "pageNumberFromLocation";
        Err(compat_error(
            op,
            fc::ErrorKind::EngineFault,
            format!("{op}: the facade cannot serve it (unconstructible:Location)"),
        ))
    }

    /// Document.page_xref from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "pageXref")]
    pub fn page_xref(&self, page: i32) -> napi::Result<i32> {
        let op = "pageXref";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.page_xref(page).map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.pagelayout from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "pagelayout")]
    pub fn pagelayout(&self) -> napi::Result<String> {
        let op = "pagelayout";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.pagelayout().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.pagemode from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "pagemode")]
    pub fn pagemode(&self) -> napi::Result<String> {
        let op = "pagemode";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.pagemode().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.pages from the compatible API.
    /// Iterate pages over a start/stop/step slice.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "pages")]
    pub fn pages(
        &self,
        start: Option<i32>,
        stop: Option<i32>,
        step: Option<i32>,
    ) -> napi::Result<Vec<Page>> {
        let op = "pages";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let count = guard.page_count() as i32;
        let step = step.unwrap_or(1);
        if step == 0 {
            return Err(compat_error(
                op,
                fc::ErrorKind::BadValue,
                format!("{op}: step must not be zero"),
            ));
        }
        let (mut i, end) = slice_bounds(count, start, stop, step);
        let mut out = Vec::new();
        if step > 0 {
            while i < end {
                out.push(Page {
                    shared: self.shared.clone(),
                    index: i as usize,
                });
                i += step;
            }
        } else {
            while i > end {
                out.push(Page {
                    shared: self.shared.clone(),
                    index: i as usize,
                });
                i += step;
            }
        }
        Ok(out)
    }

    /// Document.pdf_catalog from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "pdfCatalog")]
    pub fn pdf_catalog(&self) -> napi::Result<String> {
        let op = "pdfCatalog";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.pdf_catalog().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.pdf_trailer from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "pdfTrailer")]
    pub fn pdf_trailer(
        &self,
        compressed: Option<bool>,
        ascii: Option<bool>,
    ) -> napi::Result<String> {
        let op = "pdfTrailer";
        self.shared.check_open(op)?;
        let compressed_v = compressed.unwrap_or(false);
        let ascii_v = ascii.unwrap_or(false);
        let guard = self.shared.lock(op)?;
        let out = guard
            .pdf_trailer(compressed_v, ascii_v)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.permissions from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "permissions")]
    pub fn permissions(&self) -> napi::Result<i32> {
        let op = "permissions";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.permissions().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.prev_location from the compatible API.
    ///
    /// Not yet wired: the facade cannot serve it (unconstructible:Location). Raises until the facade grows the accessor.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "prevLocation")]
    pub fn prev_location(&self) -> napi::Result<serde_json::Value> {
        let op = "prevLocation";
        Err(compat_error(
            op,
            fc::ErrorKind::EngineFault,
            format!("{op}: the facade cannot serve it (unconstructible:Location)"),
        ))
    }

    /// Document.recolor from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "recolor")]
    pub fn recolor(&self, components: Option<i32>) -> napi::Result<()> {
        let op = "recolor";
        self.shared.check_open(op)?;
        let components_v = components.unwrap_or(1);
        let mut guard = self.shared.lock(op)?;
        guard
            .recolor(components_v)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.reload_page from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "reloadPage")]
    pub fn reload_page(&self, page: &Page) -> napi::Result<Page> {
        let op = "reloadPage";
        self.shared.check_open(op)?;
        if !std::sync::Arc::ptr_eq(&self.shared, &page.shared) {
            return Err(compat_error(
                op,
                fc::ErrorKind::BadValue,
                format!("{op}: page belongs to another document"),
            ));
        }
        let guard = self.shared.lock(op)?;
        let page_v = fc::Page::new(&guard, page.index);
        let out = guard
            .reload_page(page_v)
            .map_err(crate::error::facade(op))?;
        Ok(Page {
            shared: self.shared.clone(),
            index: out.index(),
        })
    }

    /// Document.resolve_names from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "resolveNames")]
    pub fn resolve_names(&self) -> napi::Result<()> {
        let op = "resolveNames";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        guard.resolve_names().map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.save from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "save")]
    pub fn save_full(&self, path: String, options: Option<serde_json::Value>) -> napi::Result<()> {
        let op = "save";
        self.shared.check_open(op)?;
        let options_o = save_settings_from_json(op, options.as_ref())?;
        let guard = self.shared.lock(op)?;
        guard
            .save_full(&path, &options_o)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.saveIncr from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "saveIncr")]
    pub fn save_incr(&self) -> napi::Result<()> {
        let op = "saveIncr";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        guard.save_incr().map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.save_snapshot from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "saveSnapshot")]
    pub fn save_snapshot(&self, path: String) -> napi::Result<()> {
        let op = "saveSnapshot";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        guard
            .save_snapshot(&path)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.switch_layer from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "switchLayer")]
    pub fn switch_layer(&self, config: i32, as_default: Option<bool>) -> napi::Result<()> {
        let op = "switchLayer";
        self.shared.check_open(op)?;
        let as_default_v = as_default.unwrap_or(false);
        let mut guard = self.shared.lock(op)?;
        guard
            .switch_layer(config, as_default_v)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Document.this from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "this")]
    pub fn this(&self) -> napi::Result<u32> {
        let op = "this";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.this();
        Ok(out as u32)
    }

    /// Document.this_is_pdf from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "thisIsPdf")]
    pub fn this_is_pdf(&self) -> napi::Result<bool> {
        let op = "thisIsPdf";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.this_is_pdf().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.tobytes from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "tobytes")]
    pub fn tobytes(&self, options: Option<serde_json::Value>) -> napi::Result<Buffer> {
        let op = "tobytes";
        self.shared.check_open(op)?;
        let options_o = save_settings_from_json(op, options.as_ref())?;
        let guard = self.shared.lock(op)?;
        let out = guard
            .tobytes(&options_o)
            .map_err(crate::error::facade(op))?;
        Ok(Buffer::from(out))
    }

    /// Document.version_count from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "versionCount")]
    pub fn version_count(&self) -> napi::Result<u32> {
        let op = "versionCount";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.version_count().map_err(crate::error::facade(op))?;
        Ok(out as u32)
    }

    /// Document.write from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "write")]
    pub fn write(&self, options: Option<serde_json::Value>) -> napi::Result<Buffer> {
        let op = "write";
        self.shared.check_open(op)?;
        let options_o = save_settings_from_json(op, options.as_ref())?;
        let guard = self.shared.lock(op)?;
        let out = guard.write(&options_o).map_err(crate::error::facade(op))?;
        Ok(Buffer::from(out))
    }

    /// Document.xref from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "xref")]
    pub fn xref(&self) -> napi::Result<i32> {
        let op = "xref";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.xref().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.xref_get_key from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "xrefGetKey")]
    pub fn xref_get_key(&self, xref: i32, key: String) -> napi::Result<serde_json::Value> {
        let op = "xrefGetKey";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard
            .xref_get_key(xref, &key)
            .map_err(crate::error::facade(op))?;
        Ok(tuple_to_json_string_string(&out))
    }

    /// Document.xref_get_keys from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "xrefGetKeys")]
    pub fn xref_get_keys(&self, xref: i32) -> napi::Result<Vec<String>> {
        let op = "xrefGetKeys";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard
            .xref_get_keys(xref)
            .map_err(crate::error::facade(op))?;
        Ok(out.into_iter().collect())
    }

    /// Document.xref_is_stream from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "xrefIsStream")]
    pub fn xref_is_stream(&self, xref: Option<i32>) -> napi::Result<bool> {
        let op = "xrefIsStream";
        self.shared.check_open(op)?;
        let xref_v = xref.unwrap_or(0);
        let guard = self.shared.lock(op)?;
        let out = guard
            .xref_is_stream(xref_v)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.xref_is_xobject from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "xrefIsXobject")]
    pub fn xref_is_xobject(&self, xref: i32) -> napi::Result<bool> {
        let op = "xrefIsXobject";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard
            .xref_is_xobject(xref)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.xref_length from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "xrefLength")]
    pub fn xref_length(&self) -> napi::Result<u32> {
        let op = "xrefLength";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.xref_length().map_err(crate::error::facade(op))?;
        Ok(out as u32)
    }

    /// Document.xref_object from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "xrefObject")]
    pub fn xref_object(
        &self,
        xref: i32,
        compressed: Option<bool>,
        ascii: Option<bool>,
    ) -> napi::Result<String> {
        let op = "xrefObject";
        self.shared.check_open(op)?;
        let compressed_v = compressed.unwrap_or(false);
        let ascii_v = ascii.unwrap_or(false);
        let guard = self.shared.lock(op)?;
        let out = guard
            .xref_object(xref, compressed_v, ascii_v)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Document.xref_stream from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "xrefStream")]
    pub fn xref_stream(&self, xref: i32) -> napi::Result<Buffer> {
        let op = "xrefStream";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.xref_stream(xref).map_err(crate::error::facade(op))?;
        Ok(Buffer::from(out))
    }

    /// Document.xref_stream_raw from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "xrefStreamRaw")]
    pub fn xref_stream_raw(&self, xref: i32) -> napi::Result<Buffer> {
        let op = "xrefStreamRaw";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard
            .xref_stream_raw(xref)
            .map_err(crate::error::facade(op))?;
        Ok(Buffer::from(out))
    }

    /// Document.xref_xml_metadata from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "xrefXmlMetadata")]
    pub fn xref_xml_metadata(&self) -> napi::Result<i32> {
        let op = "xrefXmlMetadata";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard
            .xref_xml_metadata()
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }
}

#[napi]
impl Link {
    /// Link.__eq__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "eq")]
    pub fn eq(&self, other: &Link) -> bool {
        self.inner == other.inner
    }

    /// Link.__ge__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "ge")]
    pub fn ge(&self, other: &Link) -> napi::Result<bool> {
        let op = "ge";
        self.inner
            .compare(&other.inner, fc::CompareOp::GreaterOrEqual)
            .map_err(crate::error::facade(op))
    }

    /// Link.__gt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "gt")]
    pub fn gt(&self, other: &Link) -> napi::Result<bool> {
        let op = "gt";
        self.inner
            .compare(&other.inner, fc::CompareOp::Greater)
            .map_err(crate::error::facade(op))
    }

    /// Link.__init__ from the compatible API.
    #[allow(clippy::too_many_arguments, clippy::new_without_default)]
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: fc::Link::new(),
        }
    }

    /// Link.__le__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "le")]
    pub fn le(&self, other: &Link) -> napi::Result<bool> {
        let op = "le";
        self.inner
            .compare(&other.inner, fc::CompareOp::LessOrEqual)
            .map_err(crate::error::facade(op))
    }

    /// Link.__lt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "lt")]
    pub fn lt(&self, other: &Link) -> napi::Result<bool> {
        let op = "lt";
        self.inner
            .compare(&other.inner, fc::CompareOp::Less)
            .map_err(crate::error::facade(op))
    }

    /// Link.__ne__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "ne")]
    pub fn ne(&self, other: &Link) -> bool {
        self.inner != other.inner
    }

    /// Link.border from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "border")]
    pub fn border(&self) -> napi::Result<serde_json::Value> {
        let out = self.inner.border();
        Ok(border_to_json(&out))
    }

    /// Link.colors from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "colors")]
    pub fn colors(&self) -> napi::Result<serde_json::Value> {
        let out = self.inner.colors();
        Ok(annot_colors_to_json(&out))
    }

    /// Link.dest from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "dest")]
    pub fn dest(&self) -> napi::Result<Option<serde_json::Value>> {
        let out = self.inner.dest();
        Ok(out.map(|x| link_dest_to_json(&x)))
    }

    /// Link.flags from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "flags")]
    pub fn flags(&self) -> napi::Result<i32> {
        let out = self.inner.flags();
        Ok(out)
    }

    /// Link.is_external from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "isExternal")]
    pub fn is_external(&self) -> napi::Result<bool> {
        let out = self.inner.is_external();
        Ok(out)
    }

    /// Link.next from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "next")]
    pub fn next(&self) -> napi::Result<Option<Link>> {
        let out = self.inner.next();
        Ok(out.map(|x| Link { inner: x }))
    }

    /// Link.rect from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "rect")]
    pub fn rect(&self) -> napi::Result<Rect> {
        let out = self.inner.rect();
        Ok(Rect { inner: out })
    }

    /// Link.set_border from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setBorder")]
    pub fn set_border(&mut self, options: Option<serde_json::Value>) -> napi::Result<()> {
        let op = "setBorder";
        opts_key_default(
            options.as_ref(),
            "border",
            serde_json::Value::Null,
            "border",
        )?;
        let options_o = border_options_from_json(op, options.as_ref())?;
        self.inner.set_border(&options_o);
        Ok(())
    }

    /// Link.set_colors from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setColors")]
    pub fn set_colors(&mut self, colors: Option<serde_json::Value>) -> napi::Result<()> {
        let op = "setColors";
        let colors_o = match colors.as_ref() {
            Some(v) => annot_colors_from_json(op, v)?,
            None => fc::AnnotColors {
                stroke: Vec::new(),
                fill: Vec::new(),
            },
        };
        self.inner.set_colors(&colors_o);
        Ok(())
    }

    /// Link.set_flags from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "setFlags")]
    pub fn set_flags(&mut self, flags: i32) -> napi::Result<()> {
        self.inner.set_flags(flags);
        Ok(())
    }

    /// Link.uri from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "uri")]
    pub fn uri(&self) -> napi::Result<String> {
        let out = self.inner.uri();
        Ok(out)
    }
}

#[napi]
impl Page {
    /// Page.__eq__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "eq")]
    pub fn eq(&self, other: &Page) -> napi::Result<bool> {
        let op = "eq";
        self.shared.check_open(op)?;
        other.shared.check_open(op)?;
        if std::sync::Arc::ptr_eq(&self.shared, &other.shared) {
            let guard = self.shared.lock(op)?;
            let a = fc::Page::new(&guard, self.index);
            let b_same = fc::Page::new(&guard, other.index);
            Ok(a == b_same)
        } else {
            let guard = self.shared.lock(op)?;
            let other_guard = other.shared.lock(op)?;
            let a = fc::Page::new(&guard, self.index);
            let b = fc::Page::new(&other_guard, other.index);
            Ok(a == b)
        }
    }

    /// Page.__ge__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "ge")]
    pub fn ge(&self, other: &Page) -> napi::Result<bool> {
        let op = "ge";
        self.shared.check_open(op)?;
        other.shared.check_open(op)?;
        if std::sync::Arc::ptr_eq(&self.shared, &other.shared) {
            let guard = self.shared.lock(op)?;
            let a = fc::Page::new(&guard, self.index);
            let b_same = fc::Page::new(&guard, other.index);
            a.compare(&b_same, fc::CompareOp::GreaterOrEqual)
                .map_err(crate::error::facade(op))
        } else {
            let guard = self.shared.lock(op)?;
            let other_guard = other.shared.lock(op)?;
            let a = fc::Page::new(&guard, self.index);
            let b = fc::Page::new(&other_guard, other.index);
            a.compare(&b, fc::CompareOp::GreaterOrEqual)
                .map_err(crate::error::facade(op))
        }
    }

    /// Page.__gt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "gt")]
    pub fn gt(&self, other: &Page) -> napi::Result<bool> {
        let op = "gt";
        self.shared.check_open(op)?;
        other.shared.check_open(op)?;
        if std::sync::Arc::ptr_eq(&self.shared, &other.shared) {
            let guard = self.shared.lock(op)?;
            let a = fc::Page::new(&guard, self.index);
            let b_same = fc::Page::new(&guard, other.index);
            a.compare(&b_same, fc::CompareOp::Greater)
                .map_err(crate::error::facade(op))
        } else {
            let guard = self.shared.lock(op)?;
            let other_guard = other.shared.lock(op)?;
            let a = fc::Page::new(&guard, self.index);
            let b = fc::Page::new(&other_guard, other.index);
            a.compare(&b, fc::CompareOp::Greater)
                .map_err(crate::error::facade(op))
        }
    }

    /// Page.__init__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(constructor)]
    pub fn new(page: u32, document: &Document) -> napi::Result<Self> {
        let op = "Page";
        document.shared.check_open(op)?;
        Ok(Self {
            shared: document.shared.clone(),
            index: page as usize,
        })
    }

    /// Page.__le__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "le")]
    pub fn le(&self, other: &Page) -> napi::Result<bool> {
        let op = "le";
        self.shared.check_open(op)?;
        other.shared.check_open(op)?;
        if std::sync::Arc::ptr_eq(&self.shared, &other.shared) {
            let guard = self.shared.lock(op)?;
            let a = fc::Page::new(&guard, self.index);
            let b_same = fc::Page::new(&guard, other.index);
            a.compare(&b_same, fc::CompareOp::LessOrEqual)
                .map_err(crate::error::facade(op))
        } else {
            let guard = self.shared.lock(op)?;
            let other_guard = other.shared.lock(op)?;
            let a = fc::Page::new(&guard, self.index);
            let b = fc::Page::new(&other_guard, other.index);
            a.compare(&b, fc::CompareOp::LessOrEqual)
                .map_err(crate::error::facade(op))
        }
    }

    /// Page.__lt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "lt")]
    pub fn lt(&self, other: &Page) -> napi::Result<bool> {
        let op = "lt";
        self.shared.check_open(op)?;
        other.shared.check_open(op)?;
        if std::sync::Arc::ptr_eq(&self.shared, &other.shared) {
            let guard = self.shared.lock(op)?;
            let a = fc::Page::new(&guard, self.index);
            let b_same = fc::Page::new(&guard, other.index);
            a.compare(&b_same, fc::CompareOp::Less)
                .map_err(crate::error::facade(op))
        } else {
            let guard = self.shared.lock(op)?;
            let other_guard = other.shared.lock(op)?;
            let a = fc::Page::new(&guard, self.index);
            let b = fc::Page::new(&other_guard, other.index);
            a.compare(&b, fc::CompareOp::Less)
                .map_err(crate::error::facade(op))
        }
    }

    /// Page.__ne__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "ne")]
    pub fn ne(&self, other: &Page) -> napi::Result<bool> {
        let op = "ne";
        self.shared.check_open(op)?;
        other.shared.check_open(op)?;
        if std::sync::Arc::ptr_eq(&self.shared, &other.shared) {
            let guard = self.shared.lock(op)?;
            let a = fc::Page::new(&guard, self.index);
            let b_same = fc::Page::new(&guard, other.index);
            Ok(a != b_same)
        } else {
            let guard = self.shared.lock(op)?;
            let other_guard = other.shared.lock(op)?;
            let a = fc::Page::new(&guard, self.index);
            let b = fc::Page::new(&other_guard, other.index);
            Ok(a != b)
        }
    }

    /// Page.artbox from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "artbox")]
    pub fn artbox(&self) -> napi::Result<Rect> {
        let op = "artbox";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view.artbox().map_err(crate::error::facade(op))?;
        Ok(Rect { inner: out })
    }

    /// Page.bleedbox from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "bleedbox")]
    pub fn bleedbox(&self) -> napi::Result<Rect> {
        let op = "bleedbox";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view.bleedbox().map_err(crate::error::facade(op))?;
        Ok(Rect { inner: out })
    }

    /// Page.bound from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "bound")]
    pub fn bound(&self) -> napi::Result<Rect> {
        let op = "bound";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view.bound().map_err(crate::error::facade(op))?;
        Ok(Rect { inner: out })
    }

    /// Page.clip_to_rect from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "clipToRect")]
    pub fn clip_to_rect(&self, rect: RectLike) -> napi::Result<()> {
        let op = "clipToRect";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        view.clip_to_rect(rect.to_compat())
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Page.cropbox from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "cropbox")]
    pub fn cropbox(&self) -> napi::Result<Rect> {
        let op = "cropbox";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view.cropbox().map_err(crate::error::facade(op))?;
        Ok(Rect { inner: out })
    }

    /// Page.cropbox_position from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "cropboxPosition")]
    pub fn cropbox_position(&self) -> napi::Result<Point> {
        let op = "cropboxPosition";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view.cropbox_position().map_err(crate::error::facade(op))?;
        Ok(Point { inner: out })
    }

    /// Page.derotation_matrix from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "derotationMatrix")]
    pub fn derotation_matrix(&self) -> napi::Result<Matrix> {
        let op = "derotationMatrix";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view.derotation_matrix().map_err(crate::error::facade(op))?;
        Ok(Matrix { inner: out })
    }

    /// Page.get_bboxlog from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getBboxlog")]
    pub fn get_bboxlog(
        &self,
        layers: Option<serde_json::Value>,
    ) -> napi::Result<serde_json::Value> {
        let op = "getBboxlog";
        self.shared.check_open(op)?;
        let layers_v = layers.as_ref().map(json_to_compat);
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view
            .get_bboxlog(layers_v.as_ref())
            .map_err(crate::error::facade(op))?;
        compat_to_json(&out)
    }

    /// Page.get_contents from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getContents")]
    pub fn get_contents(&self) -> napi::Result<Vec<i32>> {
        let op = "getContents";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view.get_contents().map_err(crate::error::facade(op))?;
        Ok(out.into_iter().collect())
    }

    /// Page.get_displaylist from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getDisplaylist")]
    pub fn get_displaylist(&self) -> napi::Result<serde_json::Value> {
        let op = "getDisplaylist";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view
            .get_displaylist(true)
            .map_err(crate::error::facade(op))?;
        Ok(serde_json::json!({ "rect": geom_rect(&out.rect()) }))
    }

    /// Page.get_label from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getLabel")]
    pub fn get_label(&self) -> napi::Result<String> {
        let op = "getLabel";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view.get_label().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Page.get_layout from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getLayout")]
    pub fn get_layout(
        &self,
        options: Option<serde_json::Value>,
    ) -> napi::Result<Option<serde_json::Value>> {
        let op = "getLayout";
        self.shared.check_open(op)?;
        let options_o = layout_options_from_json(op, options.as_ref())?;
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view
            .get_layout(&options_o)
            .map_err(crate::error::facade(op))?;
        out.map(|x| compat_to_json(&x)).transpose()
    }

    /// Page.get_oc_items from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getOcItems")]
    pub fn get_oc_items(&self) -> napi::Result<Vec<serde_json::Value>> {
        let op = "getOcItems";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view.get_oc_items().map_err(crate::error::facade(op))?;
        out.into_iter().map(|x| compat_to_json(&x)).collect()
    }

    /// Page.get_xobjects from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getXobjects")]
    pub fn get_xobjects(&self) -> napi::Result<Vec<i32>> {
        let op = "getXobjects";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view.get_xobjects().map_err(crate::error::facade(op))?;
        Ok(out.into_iter().collect())
    }

    /// Page.is_wrapped from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "isWrapped")]
    pub fn is_wrapped(&self) -> napi::Result<bool> {
        let op = "isWrapped";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view.is_wrapped().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Page.language from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "language")]
    pub fn language(&self) -> napi::Result<String> {
        let op = "language";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view.language().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Page.mediabox from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "mediabox")]
    pub fn mediabox(&self) -> napi::Result<Rect> {
        let op = "mediabox";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view.mediabox().map_err(crate::error::facade(op))?;
        Ok(Rect { inner: out })
    }

    /// Page.mediabox_size from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "mediaboxSize")]
    pub fn mediabox_size(&self) -> napi::Result<serde_json::Value> {
        let op = "mediaboxSize";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view.mediabox_size().map_err(crate::error::facade(op))?;
        Ok(tuple_to_json_f64_f64(&out))
    }

    /// Page.read_contents from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "readContents")]
    pub fn read_contents(&self) -> napi::Result<Buffer> {
        let op = "readContents";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view.read_contents().map_err(crate::error::facade(op))?;
        Ok(Buffer::from(out))
    }

    /// Page.recolor from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "recolor")]
    pub fn recolor(&self, components: Option<i32>) -> napi::Result<()> {
        let op = "recolor";
        self.shared.check_open(op)?;
        let components_v = components.unwrap_or(1);
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        view.recolor(components_v)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Page.rect from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "rect")]
    pub fn rect(&self) -> napi::Result<Rect> {
        let op = "rect";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view.rect().map_err(crate::error::facade(op))?;
        Ok(Rect { inner: out })
    }

    /// Page.refresh from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "refresh")]
    pub fn refresh(&self) -> napi::Result<()> {
        let op = "refresh";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        view.refresh().map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Page.rotation from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "rotation")]
    pub fn rotation(&self) -> napi::Result<i32> {
        let op = "rotation";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view.rotation().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Page.rotation_matrix from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "rotationMatrix")]
    pub fn rotation_matrix(&self) -> napi::Result<Matrix> {
        let op = "rotationMatrix";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view.rotation_matrix().map_err(crate::error::facade(op))?;
        Ok(Matrix { inner: out })
    }

    /// Page.run from the compatible API.
    ///
    /// Not yet wired: display lists are sealed in this build. Raises until the facade grows the accessor.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "run")]
    pub fn run(&self) -> napi::Result<serde_json::Value> {
        let op = "run";
        Err(compat_error(
            op,
            fc::ErrorKind::EngineFault,
            format!("{op}: display lists are sealed in this build"),
        ))
    }

    /// Page.transformation_matrix from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "transformationMatrix")]
    pub fn transformation_matrix(&self) -> napi::Result<Matrix> {
        let op = "transformationMatrix";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view
            .transformation_matrix()
            .map_err(crate::error::facade(op))?;
        Ok(Matrix { inner: out })
    }

    /// Page.trimbox from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "trimbox")]
    pub fn trimbox(&self) -> napi::Result<Rect> {
        let op = "trimbox";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view.trimbox().map_err(crate::error::facade(op))?;
        Ok(Rect { inner: out })
    }

    /// Page.wrap_contents from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "wrapContents")]
    pub fn wrap_contents(&self) -> napi::Result<()> {
        let op = "wrapContents";
        self.shared.check_open(op)?;
        let mut guard = self.shared.lock(op)?;
        let mut view = fc::PageMut::new(&mut guard, self.index);
        view.wrap_contents().map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Page.xref from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "xref")]
    pub fn xref(&self) -> napi::Result<i32> {
        let op = "xref";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view.xref().map_err(crate::error::facade(op))?;
        Ok(out)
    }
}
