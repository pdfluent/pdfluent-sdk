//! Compatible text extraction (lane L1).
// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use super::document::{Document, Page};
use super::edit::Annot;
use super::geometry::{MatrixLike, PointLike, Quad, QuadLike, Rect, RectLike};
use super::*;
use napi_derive::napi;
use pdfluent::compat as fc;

#[napi]
pub struct TextPage {
    pub(crate) shared: std::sync::Arc<Shared>,
    pub(crate) index: usize,
}

#[napi]
impl Document {
    /// Document.FormFonts from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "formFonts")]
    pub fn form_fonts(&self) -> napi::Result<Vec<serde_json::Value>> {
        let op = "formFonts";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.form_fonts().map_err(crate::error::facade(op))?;
        Ok(out.into_iter().map(|x| font_info_to_json(&x)).collect())
    }

    /// Document.extract_font from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "extractFont")]
    pub fn extract_font(
        &self,
        xref: Option<i32>,
        info_only: Option<bool>,
        name: Option<String>,
    ) -> napi::Result<serde_json::Value> {
        let op = "extractFont";
        self.shared.check_open(op)?;
        let xref_v = xref.unwrap_or(0);
        let info_only_v = info_only.unwrap_or(false);
        let guard = self.shared.lock(op)?;
        let out = guard
            .extract_font(xref_v, info_only_v, name.as_deref())
            .map_err(crate::error::facade(op))?;
        Ok(font_data_to_json(&out))
    }

    /// Document.get_char_widths from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getCharWidths")]
    pub fn get_char_widths(
        &self,
        xref: i32,
        limit: Option<i32>,
        index: Option<i32>,
        font_dict: Option<serde_json::Value>,
    ) -> napi::Result<Vec<f64>> {
        let op = "getCharWidths";
        self.shared.check_open(op)?;
        let limit_v = limit.unwrap_or(256);
        let index_v = index.unwrap_or(0);
        let font_dict_v = font_dict.as_ref().map(json_to_compat);
        let guard = self.shared.lock(op)?;
        let out = guard
            .get_char_widths(xref, limit_v, index_v, font_dict_v.as_ref())
            .map_err(crate::error::facade(op))?;
        Ok(out.into_iter().collect())
    }

    /// Document.get_page_fonts from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getPageFonts")]
    pub fn get_page_fonts(
        &self,
        page: i32,
        full: Option<bool>,
    ) -> napi::Result<Vec<serde_json::Value>> {
        let op = "getPageFonts";
        self.shared.check_open(op)?;
        let full_v = full.unwrap_or(false);
        let guard = self.shared.lock(op)?;
        let out = guard
            .get_page_fonts(page, full_v)
            .map_err(crate::error::facade(op))?;
        Ok(out.into_iter().map(|x| font_info_to_json(&x)).collect())
    }

    /// Document.get_page_text from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getPageText")]
    pub fn get_page_text(
        &self,
        page: i32,
        options: Option<serde_json::Value>,
        textpage: Option<&TextPage>,
    ) -> napi::Result<serde_json::Value> {
        let op = "getPageText";
        self.shared.check_open(op)?;
        let mut options_o = text_options_from_json(op, options.as_ref())?;
        let guard = self.shared.lock(op)?;
        if let Some(t) = textpage.as_ref() {
            if !std::sync::Arc::ptr_eq(&self.shared, &t.shared) {
                return Err(compat_error(
                    op,
                    fc::ErrorKind::BadValue,
                    format!("{op}: text page belongs to another document"),
                ));
            }
            let tp = fc::TextPage::new(fc::Page::new(&guard, t.index));
            options_o = options_o.with_textpage(tp);
        }
        let out = guard
            .get_page_text(page, &options_o)
            .map_err(crate::error::facade(op))?;
        let raw = options_o.format == fc::TextFormat::RawDict;
        text_output_to_json(&out, raw)
    }

    /// Document.search_page_for from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "searchPageFor")]
    pub fn search_page_for(
        &self,
        page: i32,
        needle: String,
        options: Option<serde_json::Value>,
        textpage: Option<&TextPage>,
    ) -> napi::Result<serde_json::Value> {
        let op = "searchPageFor";
        self.shared.check_open(op)?;
        let mut options_o = search_options_from_json(op, options.as_ref())?;
        let guard = self.shared.lock(op)?;
        if let Some(t) = textpage.as_ref() {
            if !std::sync::Arc::ptr_eq(&self.shared, &t.shared) {
                return Err(compat_error(
                    op,
                    fc::ErrorKind::BadValue,
                    format!("{op}: text page belongs to another document"),
                ));
            }
            let tp = fc::TextPage::new(fc::Page::new(&guard, t.index));
            options_o = options_o.with_textpage(tp);
        }
        let out = guard
            .search_page_for(page, &needle, &options_o)
            .map_err(crate::error::facade(op))?;
        search_hits_to_json(&out)
    }

    /// Document.xref_is_font from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "xrefIsFont")]
    pub fn xref_is_font(&self, xref: i32) -> napi::Result<bool> {
        let op = "xrefIsFont";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let out = guard.xref_is_font(xref).map_err(crate::error::facade(op))?;
        Ok(out)
    }
}

#[napi]
impl Page {
    /// Page.extend_textpage from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "extendTextpage")]
    pub fn extend_textpage(
        &self,
        target: &TextPage,
        flags: Option<i32>,
        matrix: Option<MatrixLike>,
    ) -> napi::Result<()> {
        let op = "extendTextpage";
        self.shared.check_open(op)?;
        if !std::sync::Arc::ptr_eq(&self.shared, &target.shared) {
            return Err(compat_error(
                op,
                fc::ErrorKind::BadValue,
                format!("{op}: text page belongs to another document"),
            ));
        }
        let flags_v = flags.unwrap_or(0);
        let matrix_v = matrix.as_ref().map(MatrixLike::to_compat);
        let guard = self.shared.lock(op)?;
        // The facade borrows the rebuilt view; writes back into it
        // are dropped with the temporary (see the method note).
        let mut target_v = fc::TextPage::new(fc::Page::new(&guard, target.index));
        let view = fc::Page::new(&guard, self.index);
        view.extend_textpage(&mut target_v, flags_v, matrix_v)
            .map_err(crate::error::facade(op))?;
        Ok(())
    }

    /// Page.find_tables from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "findTables")]
    pub fn find_tables(
        &self,
        options: Option<serde_json::Value>,
    ) -> napi::Result<serde_json::Value> {
        let op = "findTables";
        self.shared.check_open(op)?;
        let options_o = table_options_from_json(op, options.as_ref())?;
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view
            .find_tables(&options_o)
            .map_err(crate::error::facade(op))?;
        Ok(table_finder_to_json(&out))
    }

    /// Page.get_fonts from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getFonts")]
    pub fn get_fonts(&self, full: Option<bool>) -> napi::Result<Vec<serde_json::Value>> {
        let op = "getFonts";
        self.shared.check_open(op)?;
        let full_v = full.unwrap_or(false);
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view.get_fonts(full_v).map_err(crate::error::facade(op))?;
        Ok(out.into_iter().map(|x| font_info_to_json(&x)).collect())
    }

    /// Page.get_text from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getText")]
    pub fn get_text(
        &self,
        options: Option<serde_json::Value>,
        textpage: Option<&TextPage>,
    ) -> napi::Result<serde_json::Value> {
        let op = "getText";
        self.shared.check_open(op)?;
        let mut options_o = text_options_from_json(op, options.as_ref())?;
        let guard = self.shared.lock(op)?;
        if let Some(t) = textpage.as_ref() {
            if !std::sync::Arc::ptr_eq(&self.shared, &t.shared) {
                return Err(compat_error(
                    op,
                    fc::ErrorKind::BadValue,
                    format!("{op}: text page belongs to another document"),
                ));
            }
            let tp = fc::TextPage::new(fc::Page::new(&guard, t.index));
            options_o = options_o.with_textpage(tp);
        }
        let view = fc::Page::new(&guard, self.index);
        let out = view
            .get_text(&options_o)
            .map_err(crate::error::facade(op))?;
        let raw = options_o.format == fc::TextFormat::RawDict;
        text_output_to_json(&out, raw)
    }

    /// Page.get_text_blocks from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getTextBlocks")]
    pub fn get_text_blocks(
        &self,
        options: Option<serde_json::Value>,
        textpage: Option<&TextPage>,
    ) -> napi::Result<Vec<serde_json::Value>> {
        let op = "getTextBlocks";
        self.shared.check_open(op)?;
        let mut options_o = block_options_from_json(op, options.as_ref())?;
        let guard = self.shared.lock(op)?;
        if let Some(t) = textpage.as_ref() {
            if !std::sync::Arc::ptr_eq(&self.shared, &t.shared) {
                return Err(compat_error(
                    op,
                    fc::ErrorKind::BadValue,
                    format!("{op}: text page belongs to another document"),
                ));
            }
            let tp = fc::TextPage::new(fc::Page::new(&guard, t.index));
            options_o = options_o.with_textpage(tp);
        }
        let view = fc::Page::new(&guard, self.index);
        let out = view
            .get_text_blocks(&options_o)
            .map_err(crate::error::facade(op))?;
        Ok(out.into_iter().map(|x| text_block_to_json(&x)).collect())
    }

    /// Page.get_text_selection from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getTextSelection")]
    pub fn get_text_selection(
        &self,
        begin: PointLike,
        end: PointLike,
        clip: Option<RectLike>,
    ) -> napi::Result<String> {
        let op = "getTextSelection";
        self.shared.check_open(op)?;
        let clip_v = clip.as_ref().map(RectLike::to_compat);
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view
            .get_text_selection(begin.to_compat(), end.to_compat(), clip_v)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Page.get_text_words from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getTextWords")]
    pub fn get_text_words(
        &self,
        options: Option<serde_json::Value>,
        textpage: Option<&TextPage>,
    ) -> napi::Result<Vec<serde_json::Value>> {
        let op = "getTextWords";
        self.shared.check_open(op)?;
        let mut options_o = word_options_from_json(op, options.as_ref())?;
        let guard = self.shared.lock(op)?;
        if let Some(t) = textpage.as_ref() {
            if !std::sync::Arc::ptr_eq(&self.shared, &t.shared) {
                return Err(compat_error(
                    op,
                    fc::ErrorKind::BadValue,
                    format!("{op}: text page belongs to another document"),
                ));
            }
            let tp = fc::TextPage::new(fc::Page::new(&guard, t.index));
            options_o = options_o.with_textpage(tp);
        }
        let view = fc::Page::new(&guard, self.index);
        let out = view
            .get_text_words(&options_o)
            .map_err(crate::error::facade(op))?;
        Ok(out.into_iter().map(|x| text_word_to_json(&x)).collect())
    }

    /// Page.get_textbox from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getTextbox")]
    pub fn get_textbox(&self, rect: RectLike, textpage: Option<&TextPage>) -> napi::Result<String> {
        let op = "getTextbox";
        self.shared.check_open(op)?;
        if let Some(tp) = textpage.as_ref() {
            if !std::sync::Arc::ptr_eq(&self.shared, &tp.shared) {
                return Err(compat_error(
                    op,
                    fc::ErrorKind::BadValue,
                    format!("{op}: text page belongs to another document"),
                ));
            }
        }
        let guard = self.shared.lock(op)?;
        let textpage_v = textpage
            .as_ref()
            .map(|tp| fc::TextPage::new(fc::Page::new(&guard, tp.index)));
        let view = fc::Page::new(&guard, self.index);
        let out = view
            .get_textbox(rect.to_compat(), textpage_v)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// Page.get_textpage from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getTextpage")]
    pub fn get_textpage(
        &self,
        clip: Option<RectLike>,
        flags: Option<i32>,
        matrix: Option<MatrixLike>,
    ) -> napi::Result<TextPage> {
        let op = "getTextpage";
        self.shared.check_open(op)?;
        let clip_v = clip.as_ref().map(RectLike::to_compat);
        let flags_v = flags.unwrap_or(0);
        let matrix_v = matrix.as_ref().map(MatrixLike::to_compat);
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let _ = view
            .get_textpage(clip_v, flags_v, matrix_v)
            .map_err(crate::error::facade(op))?;
        Ok(TextPage {
            shared: self.shared.clone(),
            index: self.index,
        })
    }

    /// Page.get_textpage_ocr from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getTextpageOcr")]
    pub fn get_textpage_ocr(&self, options: Option<serde_json::Value>) -> napi::Result<TextPage> {
        let op = "getTextpageOcr";
        self.shared.check_open(op)?;
        let options_o = ocr_options_from_json(op, options.as_ref())?;
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let _ = view
            .get_textpage_ocr(&options_o)
            .map_err(crate::error::facade(op))?;
        Ok(TextPage {
            shared: self.shared.clone(),
            index: self.index,
        })
    }

    /// Page.get_texttrace from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "getTexttrace")]
    pub fn get_texttrace(&self) -> napi::Result<Vec<serde_json::Value>> {
        let op = "getTexttrace";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::Page::new(&guard, self.index);
        let out = view.get_texttrace().map_err(crate::error::facade(op))?;
        Ok(out.into_iter().map(|x| text_trace_to_json(&x)).collect())
    }

    /// Page.search_for from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "searchFor")]
    pub fn search_for(
        &self,
        needle: String,
        options: Option<serde_json::Value>,
        textpage: Option<&TextPage>,
    ) -> napi::Result<serde_json::Value> {
        let op = "searchFor";
        self.shared.check_open(op)?;
        let mut options_o = search_options_from_json(op, options.as_ref())?;
        let guard = self.shared.lock(op)?;
        if let Some(t) = textpage.as_ref() {
            if !std::sync::Arc::ptr_eq(&self.shared, &t.shared) {
                return Err(compat_error(
                    op,
                    fc::ErrorKind::BadValue,
                    format!("{op}: text page belongs to another document"),
                ));
            }
            let tp = fc::TextPage::new(fc::Page::new(&guard, t.index));
            options_o = options_o.with_textpage(tp);
        }
        let view = fc::Page::new(&guard, self.index);
        let out = view
            .search_for(&needle, &options_o)
            .map_err(crate::error::facade(op))?;
        search_hits_to_json(&out)
    }
}

#[napi]
impl TextPage {
    /// TextPage.__eq__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "eq")]
    pub fn eq(&self, other: &TextPage) -> napi::Result<bool> {
        let op = "eq";
        self.shared.check_open(op)?;
        other.shared.check_open(op)?;
        if std::sync::Arc::ptr_eq(&self.shared, &other.shared) {
            let guard = self.shared.lock(op)?;
            let a = fc::TextPage::new(fc::Page::new(&guard, self.index));
            let b_same = fc::TextPage::new(fc::Page::new(&guard, other.index));
            Ok(a == b_same)
        } else {
            let guard = self.shared.lock(op)?;
            let other_guard = other.shared.lock(op)?;
            let a = fc::TextPage::new(fc::Page::new(&guard, self.index));
            let b = fc::TextPage::new(fc::Page::new(&other_guard, other.index));
            Ok(a == b)
        }
    }

    /// TextPage.__ge__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "ge")]
    pub fn ge(&self, other: &TextPage) -> napi::Result<bool> {
        let op = "ge";
        self.shared.check_open(op)?;
        other.shared.check_open(op)?;
        if std::sync::Arc::ptr_eq(&self.shared, &other.shared) {
            let guard = self.shared.lock(op)?;
            let a = fc::TextPage::new(fc::Page::new(&guard, self.index));
            let b_same = fc::TextPage::new(fc::Page::new(&guard, other.index));
            a.compare(&b_same, fc::CompareOp::GreaterOrEqual)
                .map_err(crate::error::facade(op))
        } else {
            let guard = self.shared.lock(op)?;
            let other_guard = other.shared.lock(op)?;
            let a = fc::TextPage::new(fc::Page::new(&guard, self.index));
            let b = fc::TextPage::new(fc::Page::new(&other_guard, other.index));
            a.compare(&b, fc::CompareOp::GreaterOrEqual)
                .map_err(crate::error::facade(op))
        }
    }

    /// TextPage.__gt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "gt")]
    pub fn gt(&self, other: &TextPage) -> napi::Result<bool> {
        let op = "gt";
        self.shared.check_open(op)?;
        other.shared.check_open(op)?;
        if std::sync::Arc::ptr_eq(&self.shared, &other.shared) {
            let guard = self.shared.lock(op)?;
            let a = fc::TextPage::new(fc::Page::new(&guard, self.index));
            let b_same = fc::TextPage::new(fc::Page::new(&guard, other.index));
            a.compare(&b_same, fc::CompareOp::Greater)
                .map_err(crate::error::facade(op))
        } else {
            let guard = self.shared.lock(op)?;
            let other_guard = other.shared.lock(op)?;
            let a = fc::TextPage::new(fc::Page::new(&guard, self.index));
            let b = fc::TextPage::new(fc::Page::new(&other_guard, other.index));
            a.compare(&b, fc::CompareOp::Greater)
                .map_err(crate::error::facade(op))
        }
    }

    /// TextPage.__init__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(constructor)]
    pub fn new(page: &Page) -> napi::Result<Self> {
        let op = "TextPage";
        page.shared.check_open(op)?;
        Ok(Self {
            shared: page.shared.clone(),
            index: page.index,
        })
    }

    /// TextPage.__le__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "le")]
    pub fn le(&self, other: &TextPage) -> napi::Result<bool> {
        let op = "le";
        self.shared.check_open(op)?;
        other.shared.check_open(op)?;
        if std::sync::Arc::ptr_eq(&self.shared, &other.shared) {
            let guard = self.shared.lock(op)?;
            let a = fc::TextPage::new(fc::Page::new(&guard, self.index));
            let b_same = fc::TextPage::new(fc::Page::new(&guard, other.index));
            a.compare(&b_same, fc::CompareOp::LessOrEqual)
                .map_err(crate::error::facade(op))
        } else {
            let guard = self.shared.lock(op)?;
            let other_guard = other.shared.lock(op)?;
            let a = fc::TextPage::new(fc::Page::new(&guard, self.index));
            let b = fc::TextPage::new(fc::Page::new(&other_guard, other.index));
            a.compare(&b, fc::CompareOp::LessOrEqual)
                .map_err(crate::error::facade(op))
        }
    }

    /// TextPage.__lt__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "lt")]
    pub fn lt(&self, other: &TextPage) -> napi::Result<bool> {
        let op = "lt";
        self.shared.check_open(op)?;
        other.shared.check_open(op)?;
        if std::sync::Arc::ptr_eq(&self.shared, &other.shared) {
            let guard = self.shared.lock(op)?;
            let a = fc::TextPage::new(fc::Page::new(&guard, self.index));
            let b_same = fc::TextPage::new(fc::Page::new(&guard, other.index));
            a.compare(&b_same, fc::CompareOp::Less)
                .map_err(crate::error::facade(op))
        } else {
            let guard = self.shared.lock(op)?;
            let other_guard = other.shared.lock(op)?;
            let a = fc::TextPage::new(fc::Page::new(&guard, self.index));
            let b = fc::TextPage::new(fc::Page::new(&other_guard, other.index));
            a.compare(&b, fc::CompareOp::Less)
                .map_err(crate::error::facade(op))
        }
    }

    /// TextPage.__ne__ from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::should_implement_trait)]
    #[napi(js_name = "ne")]
    pub fn ne(&self, other: &TextPage) -> napi::Result<bool> {
        let op = "ne";
        self.shared.check_open(op)?;
        other.shared.check_open(op)?;
        if std::sync::Arc::ptr_eq(&self.shared, &other.shared) {
            let guard = self.shared.lock(op)?;
            let a = fc::TextPage::new(fc::Page::new(&guard, self.index));
            let b_same = fc::TextPage::new(fc::Page::new(&guard, other.index));
            Ok(a != b_same)
        } else {
            let guard = self.shared.lock(op)?;
            let other_guard = other.shared.lock(op)?;
            let a = fc::TextPage::new(fc::Page::new(&guard, self.index));
            let b = fc::TextPage::new(fc::Page::new(&other_guard, other.index));
            Ok(a != b)
        }
    }

    /// TextPage.extractBLOCKS from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "extractBLOCKS")]
    pub fn extract_blocks(&self) -> napi::Result<Vec<serde_json::Value>> {
        let op = "extractBLOCKS";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::TextPage::new(fc::Page::new(&guard, self.index));
        let out = view.extract_blocks().map_err(crate::error::facade(op))?;
        Ok(out.into_iter().map(|x| text_block_to_json(&x)).collect())
    }

    /// TextPage.extractDICT from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "extractDICT")]
    pub fn extract_dict(
        &self,
        cb: Option<serde_json::Value>,
        sort: Option<bool>,
    ) -> napi::Result<serde_json::Value> {
        let op = "extractDICT";
        self.shared.check_open(op)?;
        accept_default(op, cb.as_ref(), serde_json::Value::Null, "cb")?;
        let sort_v = sort.unwrap_or(false);
        let guard = self.shared.lock(op)?;
        let view = fc::TextPage::new(fc::Page::new(&guard, self.index));
        let out = view
            .extract_dict(sort_v)
            .map_err(crate::error::facade(op))?;
        Ok(text_dict_to_json(&out, false))
    }

    /// TextPage.extractHTML from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "extractHTML")]
    pub fn extract_html(&self) -> napi::Result<String> {
        let op = "extractHTML";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::TextPage::new(fc::Page::new(&guard, self.index));
        let out = view.extract_html().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// TextPage.extractIMGINFO from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "extractIMGINFO")]
    pub fn extract_imginfo(&self, hashes: Option<bool>) -> napi::Result<Vec<serde_json::Value>> {
        let op = "extractIMGINFO";
        self.shared.check_open(op)?;
        let hashes_v = hashes.unwrap_or(false);
        let guard = self.shared.lock(op)?;
        let view = fc::TextPage::new(fc::Page::new(&guard, self.index));
        let out = view
            .extract_imginfo(hashes_v)
            .map_err(crate::error::facade(op))?;
        Ok(out.into_iter().map(|x| placed_image_to_json(&x)).collect())
    }

    /// TextPage.extractJSON from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "extractJSON")]
    pub fn extract_json(
        &self,
        cb: Option<serde_json::Value>,
        sort: Option<bool>,
    ) -> napi::Result<String> {
        let op = "extractJSON";
        self.shared.check_open(op)?;
        accept_default(op, cb.as_ref(), serde_json::Value::Null, "cb")?;
        let sort_v = sort.unwrap_or(false);
        let guard = self.shared.lock(op)?;
        let view = fc::TextPage::new(fc::Page::new(&guard, self.index));
        let out = view
            .extract_json(sort_v)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// TextPage.extractRAWDICT from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "extractRAWDICT")]
    pub fn extract_rawdict(
        &self,
        cb: Option<serde_json::Value>,
        sort: Option<bool>,
    ) -> napi::Result<serde_json::Value> {
        let op = "extractRAWDICT";
        self.shared.check_open(op)?;
        accept_default(op, cb.as_ref(), serde_json::Value::Null, "cb")?;
        let sort_v = sort.unwrap_or(false);
        let guard = self.shared.lock(op)?;
        let view = fc::TextPage::new(fc::Page::new(&guard, self.index));
        let out = view
            .extract_rawdict(sort_v)
            .map_err(crate::error::facade(op))?;
        Ok(text_dict_to_json(&out, true))
    }

    /// TextPage.extractRAWJSON from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "extractRAWJSON")]
    pub fn extract_rawjson(
        &self,
        cb: Option<serde_json::Value>,
        sort: Option<bool>,
    ) -> napi::Result<String> {
        let op = "extractRAWJSON";
        self.shared.check_open(op)?;
        accept_default(op, cb.as_ref(), serde_json::Value::Null, "cb")?;
        let sort_v = sort.unwrap_or(false);
        let guard = self.shared.lock(op)?;
        let view = fc::TextPage::new(fc::Page::new(&guard, self.index));
        let out = view
            .extract_rawjson(sort_v)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// TextPage.extractSelection from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "extractSelection")]
    pub fn extract_selection(&self, begin: PointLike, end: PointLike) -> napi::Result<String> {
        let op = "extractSelection";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::TextPage::new(fc::Page::new(&guard, self.index));
        let out = view
            .extract_selection(begin.to_compat(), end.to_compat())
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// TextPage.extractTEXT from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "extractTEXT")]
    pub fn extract_text(&self, sort: Option<bool>) -> napi::Result<String> {
        let op = "extractTEXT";
        self.shared.check_open(op)?;
        let sort_v = sort.unwrap_or(false);
        let guard = self.shared.lock(op)?;
        let view = fc::TextPage::new(fc::Page::new(&guard, self.index));
        let out = view
            .extract_text(sort_v)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// TextPage.extractText from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "extractText")]
    pub fn extract_text_2(&self, sort: Option<bool>) -> napi::Result<String> {
        let op = "extractText";
        self.shared.check_open(op)?;
        let sort_v = sort.unwrap_or(false);
        let guard = self.shared.lock(op)?;
        let view = fc::TextPage::new(fc::Page::new(&guard, self.index));
        let out = view
            .extract_text(sort_v)
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// TextPage.extractTextbox from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "extractTextbox")]
    pub fn extract_textbox(&self, rect: RectLike) -> napi::Result<String> {
        let op = "extractTextbox";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::TextPage::new(fc::Page::new(&guard, self.index));
        let out = view
            .extract_textbox(rect.to_compat())
            .map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// TextPage.extractWORDS from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "extractWORDS")]
    pub fn extract_words(
        &self,
        delimiters: Option<String>,
    ) -> napi::Result<Vec<serde_json::Value>> {
        let op = "extractWORDS";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::TextPage::new(fc::Page::new(&guard, self.index));
        let out = view
            .extract_words(delimiters.as_deref())
            .map_err(crate::error::facade(op))?;
        Ok(out.into_iter().map(|x| text_word_to_json(&x)).collect())
    }

    /// TextPage.extractXHTML from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "extractXHTML")]
    pub fn extract_xhtml(&self) -> napi::Result<String> {
        let op = "extractXHTML";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::TextPage::new(fc::Page::new(&guard, self.index));
        let out = view.extract_xhtml().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// TextPage.extractXML from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "extractXML")]
    pub fn extract_xml(&self) -> napi::Result<String> {
        let op = "extractXML";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::TextPage::new(fc::Page::new(&guard, self.index));
        let out = view.extract_xml().map_err(crate::error::facade(op))?;
        Ok(out)
    }

    /// TextPage.poolsize from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "poolsize")]
    pub fn poolsize(&self) -> napi::Result<u32> {
        let op = "poolsize";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::TextPage::new(fc::Page::new(&guard, self.index));
        let out = view.poolsize().map_err(crate::error::facade(op))?;
        Ok(out as u32)
    }

    /// TextPage.rect from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(getter, js_name = "rect")]
    pub fn rect(&self) -> napi::Result<Rect> {
        let op = "rect";
        self.shared.check_open(op)?;
        let guard = self.shared.lock(op)?;
        let view = fc::TextPage::new(fc::Page::new(&guard, self.index));
        let out = view.rect().map_err(crate::error::facade(op))?;
        Ok(Rect { inner: out })
    }

    /// TextPage.search from the compatible API.
    #[allow(clippy::too_many_arguments)]
    #[napi(js_name = "search")]
    pub fn search(
        &self,
        needle: String,
        hit_max: Option<i32>,
        quads: Option<bool>,
    ) -> napi::Result<serde_json::Value> {
        let op = "search";
        self.shared.check_open(op)?;
        let hit_max_v = hit_max.unwrap_or(0);
        let quads_v = quads.unwrap_or(true);
        let guard = self.shared.lock(op)?;
        let view = fc::TextPage::new(fc::Page::new(&guard, self.index));
        let out = view
            .search(&needle, hit_max_v, quads_v)
            .map_err(crate::error::facade(op))?;
        search_hits_to_json(&out)
    }
}

/// module.CheckFont from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "checkFont")]
pub fn check_font(page: &Page, fontname: String) -> napi::Result<serde_json::Value> {
    let op = "checkFont";
    page.shared.check_open(op)?;
    let page_guard = page.shared.lock(op)?;
    let page_v = fc::Page::new(&page_guard, page.index);
    let out = fc::check_font(&page_v, &fontname).map_err(crate::error::facade(op))?;
    compat_to_json(&out)
}

/// module.CheckFontInfo from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "checkFontInfo")]
pub fn check_font_info(doc: &Document, xref: i32) -> napi::Result<serde_json::Value> {
    let op = "checkFontInfo";
    doc.shared.check_open(op)?;
    let doc_guard = doc.shared.lock(op)?;
    let out = fc::check_font_info(&doc_guard, xref).map_err(crate::error::facade(op))?;
    compat_to_json(&out)
}

/// module.Page__add_text_marker from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "pageAddTextMarker")]
pub fn page_add_text_marker(
    page: &Page,
    quads: Vec<QuadLike>,
    annot_kind: i32,
) -> napi::Result<Annot> {
    let op = "pageAddTextMarker";
    page.shared.check_open(op)?;
    let mut page_guard = page.shared.lock(op)?;
    let mut page_v = fc::PageMut::new(&mut page_guard, page.index);
    let quads_v: Vec<fc::Quad> = quads.iter().map(QuadLike::to_compat).collect();
    let out = fc::page_add_text_marker(&mut page_v, &quads_v, annot_kind)
        .map_err(crate::error::facade(op))?;
    Ok(Annot {
        shared: page.shared.clone(),
        page: page.index,
        xref: out.xref().map_err(crate::error::facade(op))?,
    })
}

/// module.UpdateFontInfo from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "updateFontInfo")]
pub fn update_font_info(doc: &Document, info: Vec<serde_json::Value>) -> napi::Result<()> {
    let op = "updateFontInfo";
    doc.shared.check_open(op)?;
    let mut doc_guard = doc.shared.lock(op)?;
    let info_v: Vec<fc::Value> = info.iter().map(json_to_compat).collect();
    fc::update_font_info(&mut doc_guard, &info_v).map_err(crate::error::facade(op))?;
    Ok(())
}

/// module.canon from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "canon")]
pub fn canon(ch: String) -> napi::Result<String> {
    let op = "canon";
    let ch_v = ch
        .chars()
        .next()
        .ok_or_else(|| wrong_type(op, format!("{op}: ch must not be empty")))?;
    let out = fc::canon(ch_v);
    Ok(out.to_string())
}

/// module.chartocanon from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "chartocanon")]
pub fn chartocanon(text: String) -> napi::Result<String> {
    let out = fc::chartocanon(&text);
    Ok(out)
}

/// module.css_for_font from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "cssForFont")]
pub fn css_for_font(
    fontcode: String,
    css: Option<String>,
    archive: Option<serde_json::Value>,
    name: Option<String>,
) -> napi::Result<String> {
    let op = "cssForFont";
    let archive_v = archive.map(|v| json_to_compat(&v));
    let out = fc::css_for_font(&fontcode, css.as_deref(), archive_v, name.as_deref())
        .map_err(crate::error::facade(op))?;
    Ok(out)
}

/// module.get_tessdata from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "getTessdata")]
pub fn get_tessdata(tessdata: Option<String>) -> napi::Result<String> {
    let op = "getTessdata";
    let out = fc::get_tessdata(tessdata.as_deref()).map_err(crate::error::facade(op))?;
    Ok(out)
}

/// module.get_text from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "getText")]
pub fn get_text(path: String, options: Option<serde_json::Value>) -> napi::Result<String> {
    let op = "getText";
    opts_key_default(
        options.as_ref(),
        "textpage",
        serde_json::Value::Null,
        "textpage",
    )?;
    opts_key_default(
        options.as_ref(),
        "_stats",
        serde_json::Value::Bool(false),
        "_stats",
    )?;
    let options_o = bulk_text_options_from_json(op, options.as_ref())?;
    let out = fc::get_text(&path, &options_o).map_err(crate::error::facade(op))?;
    Ok(out)
}

/// module.get_text_length from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "getTextLength")]
pub fn get_text_length(
    text: String,
    fontname: String,
    fontsize: Option<f64>,
    encoding: Option<i32>,
) -> napi::Result<f64> {
    let fontsize_v = fontsize.unwrap_or(11.0);
    let encoding_v = encoding.unwrap_or(0);
    let out = fc::get_text_length(&text, &fontname, fontsize_v, encoding_v);
    Ok(out)
}

/// module.glyph_name_to_unicode from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "glyphNameToUnicode")]
pub fn glyph_name_to_unicode(name: String) -> napi::Result<i32> {
    let out = fc::glyph_name_to_unicode(&name);
    Ok(out)
}

/// module.make_table from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "makeTable")]
pub fn make_table(
    rect: Option<RectLike>,
    cols: Option<i32>,
    rows: Option<i32>,
) -> napi::Result<Vec<serde_json::Value>> {
    let rect_v = rect
        .unwrap_or(RectLike {
            x0: 0.0,
            y0: 0.0,
            x1: 1.0,
            y1: 1.0,
        })
        .to_compat();
    let cols_v = cols.unwrap_or(1);
    let rows_v = rows.unwrap_or(1);
    let out = fc::make_table(rect_v, cols_v, rows_v);
    Ok(out.into_iter().map(|x| table_cell_to_json(&x)).collect())
}

/// module.on_highlight_char from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "onHighlightChar")]
pub fn on_highlight_char(
    hits: serde_json::Value,
    line: serde_json::Value,
    ch: serde_json::Value,
) -> napi::Result<serde_json::Value> {
    let op = "onHighlightChar";
    let hits_v = json_to_compat(&hits);
    let line_v = json_to_compat(&line);
    let ch_v = json_to_compat(&ch);
    let out = fc::on_highlight_char(&hits_v, &line_v, &ch_v).map_err(crate::error::facade(op))?;
    compat_to_json(&out)
}

/// module.recover_bbox_quad from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "recoverBboxQuad")]
pub fn recover_bbox_quad(
    direction: Vec<f64>,
    span: serde_json::Value,
    bbox: RectLike,
) -> napi::Result<Quad> {
    let op = "recoverBboxQuad";
    let direction_v: (f64, f64) = match direction.as_slice() {
        [a, b] => (*a, *b),
        _ => return Err(wrong_type(op, format!("{op}: direction needs two numbers"))),
    };
    let span_v = dict_span_from_json(op, &span)?;
    let out = fc::recover_bbox_quad(direction_v, &span_v, bbox.to_compat());
    Ok(Quad { inner: out })
}

/// module.recover_char_quad from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "recoverCharQuad")]
pub fn recover_char_quad(
    direction: Vec<f64>,
    span: serde_json::Value,
    ch: serde_json::Value,
) -> napi::Result<Quad> {
    let op = "recoverCharQuad";
    let direction_v: (f64, f64) = match direction.as_slice() {
        [a, b] => (*a, *b),
        _ => return Err(wrong_type(op, format!("{op}: direction needs two numbers"))),
    };
    let span_v = dict_span_from_json(op, &span)?;
    let ch_v = raw_char_from_json(op, &ch)?;
    let out = fc::recover_char_quad(direction_v, &span_v, &ch_v);
    Ok(Quad { inner: out })
}

/// module.recover_line_quad from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "recoverLineQuad")]
pub fn recover_line_quad(
    line: serde_json::Value,
    spans: Option<Vec<serde_json::Value>>,
) -> napi::Result<Quad> {
    let op = "recoverLineQuad";
    let line_v = dict_line_from_json(op, &line)?;
    let spans_v: Option<Vec<_>> = spans
        .as_ref()
        .map(|items| {
            items
                .iter()
                .map(|v| dict_span_from_json(op, v))
                .collect::<napi::Result<Vec<_>>>()
        })
        .transpose()?;
    let out = fc::recover_line_quad(&line_v, spans_v.as_deref());
    Ok(Quad { inner: out })
}

/// module.recover_quad from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "recoverQuad")]
pub fn recover_quad(direction: Vec<f64>, span: serde_json::Value) -> napi::Result<Quad> {
    let op = "recoverQuad";
    let direction_v: (f64, f64) = match direction.as_slice() {
        [a, b] => (*a, *b),
        _ => return Err(wrong_type(op, format!("{op}: direction needs two numbers"))),
    };
    let span_v = dict_span_from_json(op, &span)?;
    let out = fc::recover_quad(direction_v, &span_v);
    Ok(Quad { inner: out })
}

/// module.recover_span_quad from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "recoverSpanQuad")]
pub fn recover_span_quad(
    direction: Vec<f64>,
    span: serde_json::Value,
    chars: Option<Vec<serde_json::Value>>,
) -> napi::Result<Quad> {
    let op = "recoverSpanQuad";
    let direction_v: (f64, f64) = match direction.as_slice() {
        [a, b] => (*a, *b),
        _ => return Err(wrong_type(op, format!("{op}: direction needs two numbers"))),
    };
    let span_v = dict_span_from_json(op, &span)?;
    let chars_v: Option<Vec<_>> = chars
        .as_ref()
        .map(|items| {
            items
                .iter()
                .map(|v| raw_char_from_json(op, v))
                .collect::<napi::Result<Vec<_>>>()
        })
        .transpose()?;
    let out = fc::recover_span_quad(direction_v, &span_v, chars_v.as_deref());
    Ok(Quad { inner: out })
}

/// module.repair_mono_font from the compatible API.
///
/// Not yet wired: FontInfo cannot be built in this binding. Raises until the facade grows the accessor.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "repairMonoFont")]
pub fn repair_mono_font() -> napi::Result<serde_json::Value> {
    let op = "repairMonoFont";
    Err(compat_error(
        op,
        fc::ErrorKind::EngineFault,
        format!("{op}: FontInfo cannot be built in this binding"),
    ))
}

/// module.unicode_to_glyph_name from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "unicodeToGlyphName")]
pub fn unicode_to_glyph_name(code: i32) -> napi::Result<String> {
    let out = fc::unicode_to_glyph_name(code);
    Ok(out)
}
