//! Compatible module entry points.
// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use super::document::{Document, Page};
use super::edit::Annot;
use super::geometry::{
    IRect, Matrix, MatrixLike, Point, PointLike, Quad, QuadLike, Rect, RectLike,
};
use super::*;
use napi_derive::napi;
use pdfluent::compat as fc;

/// module.ASSERT_PDF from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "assertPDF")]
pub fn assert_pdf(cond: bool) -> napi::Result<()> {
    fc::assert_pdf(cond);
    Ok(())
}

/// module.CheckColor from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "checkColor")]
pub fn check_color(color: Option<Vec<f64>>) -> napi::Result<()> {
    let op = "checkColor";
    fc::check_color(color.as_deref()).map_err(crate::error::facade(op))?;
    Ok(())
}

/// module.CheckMarkerArg from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "checkMarkerArg")]
pub fn check_marker_arg(quads: serde_json::Value) -> napi::Result<serde_json::Value> {
    let op = "checkMarkerArg";
    let quads_v = json_to_compat(&quads);
    let out = fc::check_marker_arg(&quads_v).map_err(crate::error::facade(op))?;
    compat_to_json(&out)
}

/// module.CheckMorph from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "checkMorph")]
pub fn check_morph(value: serde_json::Value) -> napi::Result<bool> {
    let value_v = json_to_compat(&value);
    let out = fc::check_morph(&value_v);
    Ok(out)
}

/// module.CheckParent from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "checkParent")]
pub fn check_parent(value: serde_json::Value) -> napi::Result<()> {
    let op = "checkParent";
    let value_v = json_to_compat(&value);
    fc::check_parent(&value_v).map_err(crate::error::facade(op))?;
    Ok(())
}

/// module.CheckQuad from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "checkQuad")]
pub fn check_quad(value: serde_json::Value) -> napi::Result<bool> {
    let value_v = json_to_compat(&value);
    let out = fc::check_quad(&value_v);
    Ok(out)
}

/// module.CheckRect from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "checkRect")]
pub fn check_rect(value: serde_json::Value) -> napi::Result<bool> {
    let value_v = json_to_compat(&value);
    let out = fc::check_rect(&value_v);
    Ok(out)
}

/// module.ColorCode from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "colorCode")]
pub fn color_code(color: serde_json::Value, format: String) -> napi::Result<String> {
    let color_v = json_to_compat(&color);
    let out = fc::color_code(&color_v, &format);
    Ok(out)
}

/// module.ConversionHeader from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "conversionHeader")]
pub fn conversion_header(kind: String, filename: Option<String>) -> napi::Result<String> {
    let out = fc::conversion_header(&kind, filename.as_deref());
    Ok(out)
}

/// module.ConversionTrailer from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "conversionTrailer")]
pub fn conversion_trailer(kind: String) -> napi::Result<String> {
    let out = fc::conversion_trailer(&kind);
    Ok(out)
}

/// module.EMPTY_IRECT from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "emptyIRECT")]
pub fn empty_irect() -> napi::Result<IRect> {
    let out = fc::empty_irect();
    Ok(IRect { inner: out })
}

/// module.EMPTY_QUAD from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "emptyQUAD")]
pub fn empty_quad() -> napi::Result<Quad> {
    let out = fc::empty_quad();
    Ok(Quad { inner: out })
}

/// module.EMPTY_RECT from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "emptyRECT")]
pub fn empty_rect() -> napi::Result<Rect> {
    let out = fc::empty_rect();
    Ok(Rect { inner: out })
}

/// module.ENSURE_OPERATION from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "ensureOPERATION")]
pub fn ensure_operation(doc: &Document) -> napi::Result<()> {
    let op = "ensureOPERATION";
    doc.shared.check_open(op)?;
    let doc_guard = doc.shared.lock(op)?;
    fc::ensure_operation(&doc_guard).map_err(crate::error::facade(op))?;
    Ok(())
}

/// module.INFINITE_IRECT from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "infiniteIRECT")]
pub fn infinite_irect() -> napi::Result<IRect> {
    let out = fc::infinite_irect();
    Ok(IRect { inner: out })
}

/// module.INFINITE_QUAD from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "infiniteQUAD")]
pub fn infinite_quad() -> napi::Result<Quad> {
    let out = fc::infinite_quad();
    Ok(Quad { inner: out })
}

/// module.INFINITE_RECT from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "infiniteRECT")]
pub fn infinite_rect() -> napi::Result<Rect> {
    let out = fc::infinite_rect();
    Ok(Rect { inner: out })
}

/// module.PDF_NAME from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "pdfNAME")]
pub fn pdf_name(value: serde_json::Value) -> napi::Result<String> {
    let value_v = json_to_compat(&value);
    let out = fc::pdf_name(&value_v);
    Ok(out)
}

/// module.PySequence_Check from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "pysequenceCheck")]
pub fn py_sequence_check(value: serde_json::Value) -> napi::Result<bool> {
    let value_v = json_to_compat(&value);
    let out = fc::py_sequence_check(&value_v);
    Ok(out)
}

/// module.PySequence_Size from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "pysequenceSize")]
pub fn py_sequence_size(value: serde_json::Value) -> napi::Result<u32> {
    let value_v = json_to_compat(&value);
    let out = fc::py_sequence_size(&value_v);
    Ok(out as u32)
}

/// module.PyUnicode_DecodeRawUnicodeEscape from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "pyunicodeDecodeRawUnicodeEscape")]
pub fn py_unicode_decode_raw_unicode_escape(text: String, errors: String) -> napi::Result<String> {
    let out = fc::py_unicode_decode_raw_unicode_escape(&text, &errors);
    Ok(out)
}

/// module.RAISEPY from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "rAISEPY")]
pub fn raisepy(message: String, exc: serde_json::Value) -> napi::Result<()> {
    let op = "rAISEPY";
    let exc_v = json_to_compat(&exc);
    fc::raisepy(&message, &exc_v).map_err(crate::error::facade(op))?;
    Ok(())
}

/// module.annot_postprocess from the compatible API.
///
/// Not yet wired: page and annotation borrows cannot combine in this binding. Raises until the facade grows the accessor.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "annotPostprocess")]
pub fn annot_postprocess() -> napi::Result<serde_json::Value> {
    let op = "annotPostprocess";
    Err(compat_error(
        op,
        fc::ErrorKind::EngineFault,
        format!("{op}: page and annotation borrows cannot combine in this binding"),
    ))
}

/// module.annot_preprocess from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "annotPreprocess")]
pub fn annot_preprocess(page: &Page) -> napi::Result<i32> {
    let op = "annotPreprocess";
    page.shared.check_open(op)?;
    let page_guard = page.shared.lock(op)?;
    let page_v = fc::Page::new(&page_guard, page.index);
    let out = fc::annot_preprocess(&page_v).map_err(crate::error::facade(op))?;
    Ok(out)
}

/// module.apply_pages from the compatible API.
///
/// Not yet wired: page callbacks cannot cross into this binding. Raises until the facade grows the accessor.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "applyPages")]
pub fn apply_pages() -> napi::Result<serde_json::Value> {
    let op = "applyPages";
    Err(compat_error(
        op,
        fc::ErrorKind::EngineFault,
        format!("{op}: page callbacks cannot cross into this binding"),
    ))
}

/// module.args_match from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "argsMatch")]
pub fn args_match(
    args: Vec<serde_json::Value>,
    types: Vec<serde_json::Value>,
) -> napi::Result<bool> {
    let args_v: Vec<fc::Value> = args.iter().map(json_to_compat).collect();
    let types_v: Vec<fc::Value> = types.iter().map(json_to_compat).collect();
    let out = fc::args_match(&args_v, &types_v);
    Ok(out)
}

/// module.colors_pdf_dict from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "colorsPdfDict")]
pub fn colors_pdf_dict() -> napi::Result<serde_json::Value> {
    let out = fc::colors_pdf_dict();
    compat_to_json(&out)
}

/// module.colors_wx_list from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "colorsWxList")]
pub fn colors_wx_list() -> napi::Result<Vec<String>> {
    let out = fc::colors_wx_list();
    Ok(out.into_iter().collect())
}

/// module.compute_scissor from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "computeScissor")]
pub fn compute_scissor(device: serde_json::Value) -> napi::Result<serde_json::Value> {
    let device_v = json_to_compat(&device);
    let out = fc::compute_scissor(&device_v);
    compat_to_json(&out)
}

/// module.dest_is_valid from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "destIsValid")]
pub fn dest_is_valid(
    dest: serde_json::Value,
    page_count: i32,
    page_object_nums: Vec<i32>,
    names: Vec<String>,
) -> napi::Result<bool> {
    let dest_v = json_to_compat(&dest);
    let out = fc::dest_is_valid(&dest_v, page_count, &page_object_nums, &names);
    Ok(out)
}

/// module.dest_is_valid_page from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "destIsValidPage")]
pub fn dest_is_valid_page(
    obj: serde_json::Value,
    page_object_nums: Vec<i32>,
    page_count: i32,
) -> napi::Result<bool> {
    let obj_v = json_to_compat(&obj);
    let out = fc::dest_is_valid_page(&obj_v, &page_object_nums, page_count);
    Ok(out)
}

/// module.detect_super_script from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "detectSuperScript")]
pub fn detect_super_script(line: serde_json::Value, ch: serde_json::Value) -> napi::Result<bool> {
    let line_v = json_to_compat(&line);
    let ch_v = json_to_compat(&ch);
    let out = fc::detect_super_script(&line_v, &ch_v);
    Ok(out)
}

/// module.dir_str from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "dirStr")]
pub fn dir_str(value: serde_json::Value) -> napi::Result<String> {
    let value_v = json_to_compat(&value);
    let out = fc::dir_str(&value_v);
    Ok(out)
}

/// module.exception_info from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "exceptionInfo")]
pub fn exception_info() -> napi::Result<serde_json::Value> {
    let out = fc::exception_info();
    compat_to_json(&out)
}

/// module.extra_FzDocument_insert_pdf from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "extraFzDocumentInsertPdf")]
pub fn extra_insert_pdf(
    doc: &Document,
    src: &Document,
    options: Option<serde_json::Value>,
) -> napi::Result<()> {
    let op = "extraFzDocumentInsertPdf";
    if std::sync::Arc::ptr_eq(&doc.shared, &src.shared) {
        return Err(compat_error(
            op,
            fc::ErrorKind::BadValue,
            format!("{op}: cannot use the same document as its own source"),
        ));
    }
    opts_key_default(
        options.as_ref(),
        "graft_map",
        serde_json::Value::Null,
        "graft_map",
    )?;
    doc.shared.check_open(op)?;
    let mut doc_guard = doc.shared.lock(op)?;
    src.shared.check_open(op)?;
    let src_guard = src.shared.lock(op)?;
    let options_o = insert_pdf_options_from_json(op, options.as_ref())?;
    fc::extra_insert_pdf(&mut doc_guard, &src_guard, &options_o)
        .map_err(crate::error::facade(op))?;
    Ok(())
}

/// module.find_string from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "findString")]
pub fn find_string(text: String, needle: String) -> napi::Result<Option<u32>> {
    let out = fc::find_string(&text, &needle);
    Ok(out.map(|x| x as u32))
}

/// module.format_g from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "formatG")]
pub fn format_g(value: f64, fmt: String) -> napi::Result<String> {
    let out = fc::format_g(value, &fmt);
    Ok(out)
}

/// module.getTJstr from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "getTJstr")]
pub fn get_tjstr(
    text: String,
    glyphs: Option<Vec<i32>>,
    simple: bool,
    ordering: i32,
) -> napi::Result<String> {
    let out = fc::get_tjstr(&text, glyphs.as_deref(), simple, ordering);
    Ok(out)
}

/// module.get_env_bool from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "getEnvBool")]
pub fn get_env_bool(name: String, default_value: bool) -> napi::Result<bool> {
    let out = fc::get_env_bool(&name, default_value);
    Ok(out)
}

/// module.get_env_int from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "getEnvInt")]
pub fn get_env_int(name: String, default_value: i32) -> napi::Result<i32> {
    let out = fc::get_env_int(&name, default_value);
    Ok(out)
}

/// module.get_highlight_selection from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "getHighlightSelection")]
pub fn get_highlight_selection(
    page: &Page,
    start: Option<PointLike>,
    stop: Option<PointLike>,
    clip: Option<RectLike>,
) -> napi::Result<Vec<Quad>> {
    let op = "getHighlightSelection";
    page.shared.check_open(op)?;
    let page_guard = page.shared.lock(op)?;
    let page_v = fc::Page::new(&page_guard, page.index);
    let start_v = start.as_ref().map(PointLike::to_compat);
    let stop_v = stop.as_ref().map(PointLike::to_compat);
    let clip_v = clip.as_ref().map(RectLike::to_compat);
    let out = fc::get_highlight_selection(&page_v, start_v, stop_v, clip_v)
        .map_err(crate::error::facade(op))?;
    Ok(out.into_iter().map(|x| Quad { inner: x }).collect())
}

/// module.get_pdf_now from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "getPdfNow")]
pub fn get_pdf_now() -> napi::Result<String> {
    let out = fc::get_pdf_now();
    Ok(out)
}

/// module.get_pdf_str from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "getPdfStr")]
pub fn get_pdf_str(text: String) -> napi::Result<String> {
    let out = fc::get_pdf_str(&text);
    Ok(out)
}

/// module.hdist from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "hdist")]
pub fn hdist(
    direction: serde_json::Value,
    a: serde_json::Value,
    b: serde_json::Value,
) -> napi::Result<f64> {
    let direction_v = json_to_compat(&direction);
    let a_v = json_to_compat(&a);
    let b_v = json_to_compat(&b);
    let out = fc::hdist(&direction_v, &a_v, &b_v);
    Ok(out)
}

/// module.log from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "log")]
pub fn log(text: String, caller: Option<i32>) -> napi::Result<()> {
    let caller_v = caller.unwrap_or(1);
    fc::log(&text, caller_v);
    Ok(())
}

/// module.make_escape from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "makeEscape")]
pub fn make_escape(ch: String) -> napi::Result<String> {
    let op = "makeEscape";
    let ch_v = ch
        .chars()
        .next()
        .ok_or_else(|| wrong_type(op, format!("{op}: ch must not be empty")))?;
    let out = fc::make_escape(ch_v);
    Ok(out)
}

/// module.make_story_elpos from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "makeStoryElpos")]
pub fn make_story_elpos() -> napi::Result<serde_json::Value> {
    let out = fc::make_story_elpos();
    compat_to_json(&out)
}

/// module.match_string from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "matchString")]
pub fn match_string(h0: serde_json::Value, n0: serde_json::Value) -> napi::Result<bool> {
    let h0_v = json_to_compat(&h0);
    let n0_v = json_to_compat(&n0);
    let out = fc::match_string(&h0_v, &n0_v);
    Ok(out)
}

/// module.message from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "message")]
pub fn message(text: String) -> napi::Result<()> {
    fc::message(&text);
    Ok(())
}

/// module.message_warning from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "messageWarning")]
pub fn message_warning(text: String) -> napi::Result<()> {
    fc::message_warning(&text);
    Ok(())
}

/// module.no_recommend_layout from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "noRecommendLayout")]
pub fn no_recommend_layout() -> napi::Result<bool> {
    let out = fc::no_recommend_layout();
    Ok(out)
}

/// module.page_merge from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "pageMerge")]
pub fn page_merge(
    target: &Document,
    source: &Document,
    page_from: i32,
    page_to: i32,
    options: Option<serde_json::Value>,
) -> napi::Result<()> {
    let op = "pageMerge";
    if std::sync::Arc::ptr_eq(&target.shared, &source.shared) {
        return Err(compat_error(
            op,
            fc::ErrorKind::BadValue,
            format!("{op}: cannot use the same document as its own source"),
        ));
    }
    target.shared.check_open(op)?;
    let mut target_guard = target.shared.lock(op)?;
    source.shared.check_open(op)?;
    let source_guard = source.shared.lock(op)?;
    let options_o = page_merge_options_from_json(op, options.as_ref())?;
    fc::page_merge(
        &mut target_guard,
        &source_guard,
        page_from,
        page_to,
        &options_o,
    )
    .map_err(crate::error::facade(op))?;
    Ok(())
}

/// module.paper_rect from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "paperRect")]
pub fn paper_rect(name: String) -> napi::Result<Rect> {
    let op = "paperRect";
    let out = fc::paper_rect(&name).map_err(crate::error::facade(op))?;
    Ok(Rect { inner: out })
}

/// module.paper_size from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "paperSize")]
pub fn paper_size(name: String) -> napi::Result<serde_json::Value> {
    let op = "paperSize";
    let out = fc::paper_size(&name).map_err(crate::error::facade(op))?;
    Ok(tuple_to_json_f64_f64(&out))
}

/// module.paper_sizes from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "paperSizes")]
pub fn paper_sizes() -> napi::Result<Vec<String>> {
    let out = fc::paper_sizes();
    Ok(out.into_iter().collect())
}

/// module.pdf_lookup_page_loc from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "pdfLookupPageLoc")]
pub fn pdf_lookup_page_loc(
    doc: &Document,
    needle: serde_json::Value,
) -> napi::Result<serde_json::Value> {
    let op = "pdfLookupPageLoc";
    doc.shared.check_open(op)?;
    let doc_guard = doc.shared.lock(op)?;
    let needle_v = json_to_compat(&needle);
    let out = fc::pdf_lookup_page_loc(&doc_guard, &needle_v).map_err(crate::error::facade(op))?;
    compat_to_json(&out)
}

/// module.pdfobj_string from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "pdfobjString")]
pub fn pdfobj_string(obj: serde_json::Value, prefix: String) -> napi::Result<String> {
    let obj_v = json_to_compat(&obj);
    let out = fc::pdfobj_string(&obj_v, &prefix);
    Ok(out)
}

/// module.planish_line from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "planishLine")]
pub fn planish_line(p1: PointLike, p2: PointLike) -> napi::Result<Matrix> {
    let out = fc::planish_line(p1.to_compat(), p2.to_compat());
    Ok(Matrix { inner: out })
}

/// module.restore_aliases from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "restoreAliases")]
pub fn restore_aliases() -> napi::Result<()> {
    fc::restore_aliases();
    Ok(())
}

/// module.sRGB_to_pdf from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "srgbToPdf")]
pub fn srgb_to_pdf(srgb: i32) -> napi::Result<serde_json::Value> {
    let out = fc::srgb_to_pdf(srgb);
    Ok(tuple_to_json_f64_f64_f64(&out))
}

/// module.sRGB_to_rgb from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "srgbToRgb")]
pub fn srgb_to_rgb(srgb: i32) -> napi::Result<serde_json::Value> {
    let out = fc::srgb_to_rgb(srgb);
    Ok(tuple_to_json_f64_f64_f64(&out))
}

/// module.set_log from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "setLog")]
pub fn set_log(options: Option<serde_json::Value>) -> napi::Result<()> {
    let op = "setLog";
    opts_key_default(options.as_ref(), "fd", serde_json::Value::Null, "fd")?;
    opts_key_default(
        options.as_ref(),
        "stream",
        serde_json::Value::Null,
        "stream",
    )?;
    opts_key_default(
        options.as_ref(),
        "pylogging",
        serde_json::Value::Null,
        "pylogging",
    )?;
    opts_key_default(
        options.as_ref(),
        "pylogging_logger",
        serde_json::Value::Null,
        "pylogging_logger",
    )?;
    opts_key_default(
        options.as_ref(),
        "pylogging_level",
        serde_json::Value::Null,
        "pylogging_level",
    )?;
    opts_key_default(
        options.as_ref(),
        "pylogging_name",
        serde_json::Value::Null,
        "pylogging_name",
    )?;
    let options_o = log_options_from_json(op, options.as_ref())?;
    fc::set_log(&options_o);
    Ok(())
}

/// module.set_messages from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "setMessages")]
pub fn set_messages(options: Option<serde_json::Value>) -> napi::Result<()> {
    let op = "setMessages";
    opts_key_default(options.as_ref(), "fd", serde_json::Value::Null, "fd")?;
    opts_key_default(
        options.as_ref(),
        "stream",
        serde_json::Value::Null,
        "stream",
    )?;
    opts_key_default(
        options.as_ref(),
        "pylogging",
        serde_json::Value::Null,
        "pylogging",
    )?;
    opts_key_default(
        options.as_ref(),
        "pylogging_logger",
        serde_json::Value::Null,
        "pylogging_logger",
    )?;
    opts_key_default(
        options.as_ref(),
        "pylogging_level",
        serde_json::Value::Null,
        "pylogging_level",
    )?;
    opts_key_default(
        options.as_ref(),
        "pylogging_name",
        serde_json::Value::Null,
        "pylogging_name",
    )?;
    let options_o = log_options_from_json(op, options.as_ref())?;
    fc::set_messages(&options_o);
    Ok(())
}

/// module.string_in_names_list from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "stringInNamesList")]
pub fn string_in_names_list(text: String, names: Vec<String>) -> napi::Result<bool> {
    let out = fc::string_in_names_list(&text, &names);
    Ok(out)
}

/// module.strip_outline from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "stripOutline")]
pub fn strip_outline(
    doc: &Document,
    outlines: serde_json::Value,
    page_count: i32,
    page_object_nums: Vec<i32>,
    names: Vec<String>,
) -> napi::Result<()> {
    let op = "stripOutline";
    doc.shared.check_open(op)?;
    let mut doc_guard = doc.shared.lock(op)?;
    let outlines_v = json_to_compat(&outlines);
    fc::strip_outline(
        &mut doc_guard,
        &outlines_v,
        page_count,
        &page_object_nums,
        &names,
    )
    .map_err(crate::error::facade(op))?;
    Ok(())
}

/// module.strip_outlines from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "stripOutlines")]
pub fn strip_outlines(
    doc: &Document,
    outlines: serde_json::Value,
    page_count: i32,
    page_object_nums: Vec<i32>,
    names: Vec<String>,
) -> napi::Result<()> {
    let op = "stripOutlines";
    doc.shared.check_open(op)?;
    let mut doc_guard = doc.shared.lock(op)?;
    let outlines_v = json_to_compat(&outlines);
    fc::strip_outlines(
        &mut doc_guard,
        &outlines_v,
        page_count,
        &page_object_nums,
        &names,
    )
    .map_err(crate::error::facade(op))?;
    Ok(())
}

/// module.util_concat_matrix from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "utilConcatMatrix")]
pub fn util_concat_matrix(one: MatrixLike, two: MatrixLike) -> napi::Result<Matrix> {
    let out = fc::util_concat_matrix(one.to_compat(), two.to_compat());
    Ok(Matrix { inner: out })
}

/// module.util_ensure_widget_calc from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "utilEnsureWidgetCalc")]
pub fn util_ensure_widget_calc(annot: &Annot) -> napi::Result<()> {
    let op = "utilEnsureWidgetCalc";
    annot.shared.check_open(op)?;
    let mut annot_guard = annot.shared.lock(op)?;
    let mut annot_pm = fc::PageMut::new(&mut annot_guard, annot.page);
    let annot_v = fc::Annot::new(&mut annot_pm, annot.xref);
    fc::util_ensure_widget_calc(annot_v).map_err(crate::error::facade(op))?;
    Ok(())
}

/// module.util_hor_matrix from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "utilHorMatrix")]
pub fn util_hor_matrix(center: PointLike, point: PointLike) -> napi::Result<Matrix> {
    let out = fc::util_hor_matrix(center.to_compat(), point.to_compat());
    Ok(Matrix { inner: out })
}

/// module.util_include_point_in_rect from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "utilIncludePointInRect")]
pub fn util_include_point_in_rect(rect: RectLike, point: PointLike) -> napi::Result<Rect> {
    let out = fc::util_include_point_in_rect(rect.to_compat(), point.to_compat());
    Ok(Rect { inner: out })
}

/// module.util_intersect_rect from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "utilIntersectRect")]
pub fn util_intersect_rect(one: RectLike, two: RectLike) -> napi::Result<Rect> {
    let out = fc::util_intersect_rect(one.to_compat(), two.to_compat());
    Ok(Rect { inner: out })
}

/// module.util_invert_matrix from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "utilInvertMatrix")]
pub fn util_invert_matrix(matrix: MatrixLike) -> napi::Result<Matrix> {
    let out = fc::util_invert_matrix(matrix.to_compat());
    Ok(Matrix { inner: out })
}

/// module.util_is_point_in_rect from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "utilIsPointInRect")]
pub fn util_is_point_in_rect(point: PointLike, rect: RectLike) -> napi::Result<i32> {
    let out = fc::util_is_point_in_rect(point.to_compat(), rect.to_compat());
    Ok(out)
}

/// module.util_make_irect from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "utilMakeIrect")]
pub fn util_make_irect(
    p0: Option<serde_json::Value>,
    p1: Option<serde_json::Value>,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
) -> napi::Result<IRect> {
    let op = "utilMakeIrect";
    accept_default(op, p0.as_ref(), serde_json::Value::Null, "p0")?;
    accept_default(op, p1.as_ref(), serde_json::Value::Null, "p1")?;
    let out = fc::util_make_irect(x0, y0, x1, y1);
    Ok(IRect { inner: out })
}

/// module.util_make_rect from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "utilMakeRect")]
pub fn util_make_rect(
    p0: Option<serde_json::Value>,
    p1: Option<serde_json::Value>,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
) -> napi::Result<Rect> {
    let op = "utilMakeRect";
    accept_default(op, p0.as_ref(), serde_json::Value::Null, "p0")?;
    accept_default(op, p1.as_ref(), serde_json::Value::Null, "p1")?;
    let out = fc::util_make_rect(x0, y0, x1, y1);
    Ok(Rect { inner: out })
}

/// module.util_measure_string from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "utilMeasureString")]
pub fn util_measure_string(
    text: String,
    fontname: String,
    fontsize: f64,
    encoding: i32,
) -> napi::Result<f64> {
    let out = fc::util_measure_string(&text, &fontname, fontsize, encoding);
    Ok(out)
}

/// module.util_point_in_quad from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "utilPointInQuad")]
pub fn util_point_in_quad(point: PointLike, quad: QuadLike) -> napi::Result<bool> {
    let out = fc::util_point_in_quad(point.to_compat(), quad.to_compat());
    Ok(out)
}

/// module.util_round_rect from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "utilRoundRect")]
pub fn util_round_rect(rect: RectLike) -> napi::Result<IRect> {
    let out = fc::util_round_rect(rect.to_compat());
    Ok(IRect { inner: out })
}

/// module.util_sine_between from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "utilSineBetween")]
pub fn util_sine_between(
    center: PointLike,
    point: PointLike,
    other: PointLike,
) -> napi::Result<f64> {
    let out = fc::util_sine_between(center.to_compat(), point.to_compat(), other.to_compat());
    Ok(out)
}

/// module.util_transform_point from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "utilTransformPoint")]
pub fn util_transform_point(point: PointLike, matrix: MatrixLike) -> napi::Result<Point> {
    let out = fc::util_transform_point(point.to_compat(), matrix.to_compat());
    Ok(Point { inner: out })
}

/// module.util_transform_rect from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "utilTransformRect")]
pub fn util_transform_rect(rect: RectLike, matrix: MatrixLike) -> napi::Result<Rect> {
    let out = fc::util_transform_rect(rect.to_compat(), matrix.to_compat());
    Ok(Rect { inner: out })
}

/// module.util_union_rect from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "utilUnionRect")]
pub fn util_union_rect(one: RectLike, two: RectLike) -> napi::Result<Rect> {
    let out = fc::util_union_rect(one.to_compat(), two.to_compat());
    Ok(Rect { inner: out })
}

/// module.vdist from the compatible API.
#[allow(clippy::too_many_arguments)]
#[napi(js_name = "vdist")]
pub fn vdist(
    direction: serde_json::Value,
    a: serde_json::Value,
    b: serde_json::Value,
) -> napi::Result<f64> {
    let direction_v = json_to_compat(&direction);
    let a_v = json_to_compat(&a);
    let b_v = json_to_compat(&b);
    let out = fc::vdist(&direction_v, &a_v, &b_v);
    Ok(out)
}
