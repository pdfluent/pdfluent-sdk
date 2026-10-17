//! Layout detection on the shared ONNX Runtime layer (#641).
//!
//! Behind the `ort-layout` cargo feature (off by default). Turns one PDF page
//! into the layout detector's boxes, exactly as the Python reference does
//! (Heron-ft ep03, RT-DETRv2, HF transformers 4.57.6 post-processing):
//!
//! * [`LAYOUT_EP03`] is the model, loaded through
//!   [`pdf_ocr::runtime::session`] only.
//! * [`render_page_rgb`] renders the page at 96 dpi, RGB on white, through the
//!   same calls as the `render_page_png` example. No new renderer.
//! * [`preprocess_to_nchw`] resizes to 640x640 non-uniform with PIL BILINEAR
//!   (triangle support widened on downscale, as PIL does), rescales by 1/255
//!   with no mean/std normalisation, and lays out NCHW `f32` 1x3x640x640 --
//!   the flags in the model's `preprocessor_config.json`
//!   (`RTDetrImageProcessor`, `do_resize`, `do_rescale`, no `do_normalize`,
//!   no `do_pad`).
//! * [`layout_boxes`] runs the graph (input/output names read from the ONNX
//!   session metadata, as exported: `pixel_values` in, `logits` and
//!   `pred_boxes` out) and post-processes exactly as HF
//!   `post_process_object_detection` for RT-DETR: sigmoid over the logits,
//!   top-300 over the flattened scores, `label = index % 17`,
//!   `query = index / 17`, boxes from normalised cxcywh to xyxy scaled to the
//!   render size, keeping `score >= 0.05`. Label names are the model's
//!   `config.json` `id2label`.
//!
//! Model failures come back as [`EngineError::Io`](crate::EngineError)
//! carrying the typed [`pdf_ocr::runtime::ModelError`]
//! -- no new `EngineError` variants. Callers match that variant and find the
//! cause with
//! [`pdf_ocr::runtime::model_error`].

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_ocr::runtime::session;

pub use pdf_ocr::runtime::RuntimeConfig;

use crate::{EngineError, PdfDocument, PixelFormat, RenderOptions, Result};

/// The layout model: Heron-ft ep03 (RT-DETRv2), ONNX export. Defined in
/// [`crate::model`] so builds without the tier name the same spec; re-exported
/// here so the session path keeps its name.
pub use crate::model::LAYOUT_EP03;

/// Detector input width in pixels (HF `size.width`).
const INPUT_WIDTH: u32 = 640;

/// Detector input height in pixels (HF `size.height`).
const INPUT_HEIGHT: u32 = 640;

/// Render resolution in dots per inch, the `render_page_png` default.
const RENDER_DPI: f64 = 96.0;

/// Score threshold: HF `post_process_object_detection(threshold=0.05)`.
///
/// HF filters with `score > threshold` and this keeps `score >= 0.05`; the
/// two differ only for a score landing exactly on the boundary.
const SCORE_THRESHOLD: f32 = 0.05;

/// Queries kept: HF `num_top_queries`, the top-k over the flattened scores.
const TOP_K: usize = 300;

/// Label names by class id: the `id2label` of the model's `config.json`
/// (Heron-ft ep03, transformers 4.57.6). The post-processing maps
/// `label = index % 17` through this table.
const LABELS: [&str; 17] = [
    "caption",
    "footnote",
    "formula",
    "list_item",
    "page_footer",
    "page_header",
    "picture",
    "section_header",
    "table",
    "text",
    "title",
    "document_index",
    "code",
    "checkbox_selected",
    "checkbox_unselected",
    "form",
    "key_value_region",
];

/// One layout detection: what the region is, where it is on the render.
#[derive(Debug, Clone)]
pub struct LayoutBox {
    /// Label name from the model's `id2label` (e.g. `"text"`, `"table"`).
    pub label: String,
    /// Sigmoid score of the winning (query, class) pair, in 0.05..=1.0.
    pub score: f32,
    /// Box corners `[x1, y1, x2, y2]` in render pixels at 96 dpi.
    pub bbox_px: [f32; 4],
    /// Render width in pixels the box is addressed against.
    pub render_w: u32,
    /// Render height in pixels the box is addressed against.
    pub render_h: u32,
}

/// An 8-bit RGB page render at 96 dpi, row-major, three bytes per pixel.
#[derive(Debug, Clone)]
pub struct RgbPage {
    /// Interleaved RGB bytes, row-major.
    pub data: Vec<u8>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Render one page at 96 dpi, RGB on white.
///
/// Exactly the path of the `render_page_png` example: the same
/// [`RenderOptions`] (96 dpi, opaque white background), the same
/// [`PdfDocument::render_page`] call, and the same compositing of the
/// premultiplied RGBA over white. Page `page_index` counts from zero.
pub fn render_page_rgb(doc: &PdfDocument, page_index: usize) -> Result<RgbPage> {
    let opts = RenderOptions {
        dpi: RENDER_DPI,
        background: [1.0, 1.0, 1.0, 1.0],
        ..Default::default()
    };
    let rendered = doc.render_page(page_index, &opts)?;
    if rendered.pixel_format != PixelFormat::Rgba8 {
        return Err(EngineError::RenderError(
            "layout render: expected RGBA pixels".to_string(),
        ));
    }
    // Premultiplied RGBA composited over the white background: with an opaque
    // page this is the channels as they stand; anywhere the alpha is short
    // the white shows through by exactly the missing share.
    let mut rgb = Vec::with_capacity(rendered.pixels.len() / 4 * 3);
    for px in rendered.pixels.chunks_exact(4) {
        let missing = 255 - px[3];
        rgb.push(px[0].saturating_add(missing));
        rgb.push(px[1].saturating_add(missing));
        rgb.push(px[2].saturating_add(missing));
    }
    Ok(RgbPage {
        data: rgb,
        width: rendered.width,
        height: rendered.height,
    })
}

/// Preprocess a render into the detector's NCHW `f32` input (1x3x640x640).
///
/// Resize to 640x640 non-uniform with PIL BILINEAR (triangle support widened
/// on downscale, as PIL does), then rescale by 1/255 with no mean/std
/// normalisation, laid out channel-planar.
pub fn preprocess_to_nchw(page: &RgbPage) -> Vec<f32> {
    let resized = resize_rgb_bilinear_pil(
        &page.data,
        page.width,
        page.height,
        INPUT_WIDTH,
        INPUT_HEIGHT,
    );
    let plane = INPUT_WIDTH as usize * INPUT_HEIGHT as usize;
    let mut nchw = vec![0.0f32; 3 * plane];
    for (i, px) in resized.chunks_exact(3).enumerate() {
        nchw[i] = f32::from(px[0]) / 255.0;
        nchw[plane + i] = f32::from(px[1]) / 255.0;
        nchw[2 * plane + i] = f32::from(px[2]) / 255.0;
    }
    nchw
}

/// The layout detector's boxes for one page.
///
/// Renders ([`render_page_rgb`]), preprocesses ([`preprocess_to_nchw`]), runs
/// [`LAYOUT_EP03`] through the shared session, and post-processes as HF does.
/// Page `page_index` counts from zero. Boxes come back in descending score
/// order; an empty page reads back as an empty vector, not an error.
///
/// Model failures (missing file, checksum, runtime, inference) come back as
/// [`EngineError::Io`](crate::EngineError) carrying the typed
/// [`pdf_ocr::runtime::ModelError`].
pub fn layout_boxes(
    doc: &PdfDocument,
    page_index: usize,
    config: &RuntimeConfig,
) -> Result<Vec<LayoutBox>> {
    let page = render_page_rgb(doc, page_index)?;
    let nchw = preprocess_to_nchw(&page);
    let shared = session(&LAYOUT_EP03, config).map_err(model_io_error)?;
    let (logits, boxes_cxcywh) = shared
        .run(|ort_session| {
            // Names from the graph, not from the export script: the input key
            // is whatever the single graph input is called, and the outputs
            // are selected by name so their order cannot matter.
            let input_name = {
                let graph_inputs = ort_session.inputs();
                if graph_inputs.len() != 1 {
                    return Err(ort::Error::new(format!(
                        "layout model has {} inputs, want the single pixel_values input",
                        graph_inputs.len()
                    )));
                }
                graph_inputs[0].name().to_string()
            };
            {
                let graph_outputs = ort_session.outputs();
                for want in ["logits", "pred_boxes"] {
                    if !graph_outputs.iter().any(|o| o.name() == want) {
                        return Err(ort::Error::new(format!(
                            "layout model has no {want} output"
                        )));
                    }
                }
            }
            let input = ort::value::Tensor::from_array((
                [1usize, 3, INPUT_HEIGHT as usize, INPUT_WIDTH as usize],
                nchw,
            ))?;
            let outputs = ort_session.run(ort::inputs![input_name => input])?;
            let logits = outputs
                .get("logits")
                .ok_or_else(|| ort::Error::new("layout model returned no logits output"))?;
            let pred_boxes = outputs
                .get("pred_boxes")
                .ok_or_else(|| ort::Error::new("layout model returned no pred_boxes output"))?;
            let (logits_shape, logits_data) = logits.try_extract_tensor::<f32>()?;
            let (boxes_shape, boxes_data) = pred_boxes.try_extract_tensor::<f32>()?;
            if logits_shape.len() != 3
                || logits_shape[0] != 1
                || logits_shape[2] != LABELS.len() as i64
            {
                return Err(ort::Error::new(format!(
                    "layout logits have shape {logits_shape:?}, want [1, queries, 17]"
                )));
            }
            let queries = logits_shape[1];
            if boxes_shape.len() != 3
                || boxes_shape[0] != 1
                || boxes_shape[1] != queries
                || boxes_shape[2] != 4
            {
                return Err(ort::Error::new(format!(
                    "layout pred_boxes have shape {boxes_shape:?}, want [1, {queries}, 4]"
                )));
            }
            Ok((logits_data.to_vec(), boxes_data.to_vec()))
        })
        .map_err(model_io_error)?;
    Ok(postprocess(&logits, &boxes_cxcywh, page.width, page.height))
}

/// A shared-layer failure as the engine carries it: `EngineError::Io` with
/// the typed cause, downcastable through
/// [`pdf_ocr::runtime::model_error`].
fn model_io_error(e: pdf_ocr::runtime::ModelError) -> EngineError {
    EngineError::Io(std::io::Error::from(e))
}

/// HF `post_process_object_detection` for RT-DETR (focal-loss branch).
///
/// Sigmoid over the `[queries x 17]` logits, top-[`TOP_K`] over the flattened
/// scores, `label = index % 17`, `query = index / 17`, boxes from normalised
/// cxcywh to xyxy scaled to the render size, keeping `score >= 0.05`.
/// Boxes come back in descending score order. Generic over the query count so
/// a hand-made tensor tests it; production runs it with 300.
fn postprocess(
    logits: &[f32],
    boxes_cxcywh: &[f32],
    render_w: u32,
    render_h: u32,
) -> Vec<LayoutBox> {
    let classes = LABELS.len();
    let queries = (boxes_cxcywh.len() / 4).min(logits.len() / classes);
    let total = queries * classes;
    let scores: Vec<f32> = logits[..total]
        .iter()
        .map(|&logit| 1.0 / (1.0 + (-logit).exp()))
        .collect();
    let mut order: Vec<usize> = (0..total).collect();
    order.sort_by(|&a, &b| scores[b].total_cmp(&scores[a]));
    let w = render_w as f32;
    let h = render_h as f32;
    let mut out = Vec::new();
    for index in order.into_iter().take(TOP_K.min(total)) {
        let score = scores[index];
        if score < SCORE_THRESHOLD {
            continue;
        }
        let label_id = index % classes;
        let query = index / classes;
        let base = query * 4;
        let (cx, cy, bw, bh) = (
            boxes_cxcywh[base],
            boxes_cxcywh[base + 1],
            boxes_cxcywh[base + 2],
            boxes_cxcywh[base + 3],
        );
        out.push(LayoutBox {
            label: LABELS[label_id].to_string(),
            score,
            bbox_px: [
                (cx - 0.5 * bw) * w,
                (cy - 0.5 * bh) * h,
                (cx + 0.5 * bw) * w,
                (cy + 0.5 * bh) * h,
            ],
            render_w,
            render_h,
        });
    }
    out
}

/// One destination sample's contributing source indices and weights.
type AxisTaps = Vec<(usize, f32)>;

/// PIL BILINEAR's per-axis filter: destination sample `d` reads source
/// centre `(d + 0.5) * scale - 0.5` with triangle support 1, widened to
/// `scale` on downscale (PIL antialiases), weights renormalised over the
/// clipped range so edges do not darken.
fn axis_taps(src_len: u32, dst_len: u32) -> Vec<AxisTaps> {
    let src_len = src_len as usize;
    let scale = f64::from(src_len as u32) / f64::from(dst_len);
    let support = if scale > 1.0 { scale } else { 1.0 };
    (0..dst_len as usize)
        .map(|d| {
            let centre = (d as f64 + 0.5) * scale - 0.5;
            let lo = (centre - support).floor() as i64 + 1;
            let hi = (centre + support).ceil() as i64 - 1;
            let mut taps = Vec::new();
            let mut sum = 0.0f64;
            for i in lo.max(0)..=hi.min(src_len as i64 - 1) {
                let t = ((i as f64 - centre) / support).abs();
                let weight = if t < 1.0 { 1.0 - t } else { 0.0 };
                if weight > 0.0 {
                    taps.push((i as usize, weight));
                    sum += weight;
                }
            }
            if sum > 0.0 {
                taps.into_iter()
                    .map(|(i, weight)| (i, (weight / sum) as f32))
                    .collect()
            } else {
                // Unreachable: the centre always lands within support of a
                // source sample. Nearest rather than empty, so no caller can
                // read a black pixel from a filter that found nothing.
                let nearest = centre.round().clamp(0.0, f64::from(src_len as u32 - 1)) as usize;
                vec![(nearest, 1.0)]
            }
        })
        .collect()
}

/// Resize interleaved RGB with PIL BILINEAR semantics, separable.
///
/// Horizontal pass rounded and clipped to bytes, then vertical, as PIL runs
/// its horizontal resample into a byte image the vertical pass reads. Both
/// round half up. Matches Pillow 10.2.0's `Image.resize(..., BILINEAR)`
/// exactly on the reference cases in
/// `tests/fixtures/ort_layout/pil_resize_reference.py`; a plain 2x2 bilinear
/// (support 1 on downscale) reads 0/255 high-frequency content instead of its
/// mean.
fn resize_rgb_bilinear_pil(src: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
    let src_w = src_w as usize;
    let src_h = src_h as usize;
    let dst_w = dst_w as usize;
    let dst_h = dst_h as usize;
    let horizontal = axis_taps(src_w as u32, dst_w as u32);
    let mut mid = vec![0u8; dst_w * src_h * 3];
    for y in 0..src_h {
        for (x, taps) in horizontal.iter().enumerate() {
            let mut acc = [0.0f32; 3];
            for &(i, weight) in taps {
                let base = (y * src_w + i) * 3;
                acc[0] += f32::from(src[base]) * weight;
                acc[1] += f32::from(src[base + 1]) * weight;
                acc[2] += f32::from(src[base + 2]) * weight;
            }
            let base = (y * dst_w + x) * 3;
            mid[base] = acc[0].round().clamp(0.0, 255.0) as u8;
            mid[base + 1] = acc[1].round().clamp(0.0, 255.0) as u8;
            mid[base + 2] = acc[2].round().clamp(0.0, 255.0) as u8;
        }
    }
    let vertical = axis_taps(src_h as u32, dst_h as u32);
    let mut dst = vec![0u8; dst_w * dst_h * 3];
    for (y, taps) in vertical.iter().enumerate() {
        for x in 0..dst_w {
            let mut acc = [0.0f32; 3];
            for &(j, weight) in taps {
                let base = (j * dst_w + x) * 3;
                acc[0] += f32::from(mid[base]) * weight;
                acc[1] += f32::from(mid[base + 1]) * weight;
                acc[2] += f32::from(mid[base + 2]) * weight;
            }
            let base = (y * dst_w + x) * 3;
            dst[base] = acc[0].round().clamp(0.0, 255.0) as u8;
            dst[base + 1] = acc[1].round().clamp(0.0, 255.0) as u8;
            dst[base + 2] = acc[2].round().clamp(0.0, 255.0) as u8;
        }
    }
    dst
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sigmoid(x: f32) -> f32 {
        1.0 / (1.0 + (-x).exp())
    }

    #[test]
    fn the_layout_spec_names_the_ep03_package() {
        assert_eq!(LAYOUT_EP03.name, "layout-heron-ep03");
        assert_eq!(LAYOUT_EP03.file, "heron_ep03.onnx");
        assert_eq!(
            LAYOUT_EP03.sha256,
            "b7cc5d31f3223c74cc477a1386b894793618aeb398ca59cf5eb7264ec61d8afa"
        );
        assert!(
            LAYOUT_EP03.install_command.contains("layout"),
            "the install hint names the model package: {}",
            LAYOUT_EP03.install_command
        );
        assert_eq!(LABELS.len(), 17);
        assert_eq!(LABELS[9], "text");
        assert_eq!(LABELS[8], "table");
    }

    #[test]
    fn postprocess_top_k_runs_across_classes_with_modulo_labels() {
        // Two queries, seventeen classes: everything at -10 (sigmoid ~0)
        // except two classes on query 0 and one class on query 1, plus one
        // class just under the threshold that must not survive.
        let mut logits = vec![-10.0f32; 2 * 17];
        logits[9] = 3.0; // query 0, class 9 ("text")
        logits[3] = 2.0; // query 0, class 3 ("list_item")
        logits[17] = 0.0; // query 1, class 0 ("caption")
        logits[22] = -3.0; // query 1, class 5: sigmoid 0.047 < 0.05
        let boxes_cxcywh = vec![
            0.5, 0.5, 0.2, 0.4, // query 0
            0.25, 0.75, 0.5, 0.5, // query 1
        ];
        let boxes = postprocess(&logits, &boxes_cxcywh, 1000, 800);
        assert_eq!(boxes.len(), 3, "the under-threshold class must not survive");
        // Descending score: the same query twice, then query 1. Index 17 is
        // the modulo case: 17 % 17 == 0 ("caption"), 17 / 17 == 1 (query 1).
        assert_eq!(boxes[0].label, "text");
        assert_eq!(boxes[1].label, "list_item");
        assert_eq!(boxes[2].label, "caption");
        assert!((boxes[0].score - sigmoid(3.0)).abs() < 1e-6);
        assert!((boxes[1].score - sigmoid(2.0)).abs() < 1e-6);
        assert!((boxes[2].score - sigmoid(0.0)).abs() < 1e-6);
        // The top two share query 0's box; the third reads query 1's. The
        // corners compare with a tolerance: 0.4 is not exact in f32, so
        // (0.5 - 0.5 * 0.4) * 800 reads 240.00002, as HF's f32 math does.
        assert_eq!(boxes[0].bbox_px, boxes[1].bbox_px);
        for (got, want) in boxes[0].bbox_px.iter().zip([400.0, 240.0, 600.0, 560.0]) {
            assert!((got - want).abs() < 1e-3, "got {got}, want {want}");
        }
        for (got, want) in boxes[2].bbox_px.iter().zip([0.0, 400.0, 500.0, 800.0]) {
            assert!((got - want).abs() < 1e-3, "got {got}, want {want}");
        }
        for b in &boxes {
            assert_eq!((b.render_w, b.render_h), (1000, 800));
        }
    }

    #[test]
    fn postprocess_keeps_the_top_300_queries() {
        // 300 queries would pass the threshold; the 301st must not survive
        // even though it too is above it.
        let mut logits = vec![-10.0f32; 301 * 17];
        for q in 0..301 {
            logits[q * 17] = 0.0; // sigmoid 0.5, class 0
        }
        let boxes_cxcywh = vec![0.5f32, 0.5, 0.1, 0.1]
            .into_iter()
            .cycle()
            .take(301 * 4)
            .collect::<Vec<_>>();
        let boxes = postprocess(&logits, &boxes_cxcywh, 640, 640);
        assert_eq!(boxes.len(), TOP_K);
    }

    #[test]
    fn postprocess_reads_nothing_below_threshold() {
        let logits = vec![-10.0f32; 4 * 17];
        let boxes_cxcywh = vec![0.5f32; 4 * 4];
        assert!(postprocess(&logits, &boxes_cxcywh, 640, 640).is_empty());
    }

    /// Reference outputs from
    /// `tests/fixtures/ort_layout/pil_resize_reference.py` (Pillow 10.2.0).
    /// The resampler matches PIL exactly on these; a plain 2x2 bilinear
    /// without the widened downscale support fails the checker and edge
    /// cases (0/255 instead of ~128, 0 instead of 36).
    #[test]
    fn resize_matches_pil_on_a_downscaled_ramp() {
        let mut src = Vec::new();
        for y in 0..4u8 {
            for x in 0..4u8 {
                src.extend_from_slice(&[x * 64, y * 64, 128]);
            }
        }
        assert_eq!(
            resize_rgb_bilinear_pil(&src, 4, 4, 2, 2),
            vec![46, 46, 128, 146, 46, 128, 46, 146, 128, 146, 146, 128]
        );
    }

    #[test]
    fn resize_averages_a_downscaled_checkerboard_like_pil() {
        let mut src = Vec::new();
        for y in 0..4 {
            for x in 0..4 {
                let v = if (x + y) % 2 == 0 { 255 } else { 0 };
                src.extend_from_slice(&[v, v, v]);
            }
        }
        assert_eq!(
            resize_rgb_bilinear_pil(&src, 4, 4, 2, 2),
            vec![130, 130, 130, 125, 125, 125, 125, 125, 125, 130, 130, 130]
        );
    }

    #[test]
    fn resize_matches_pil_on_a_downscaled_edge() {
        let src = [0u8, 0, 0, 0, 0, 0]
            .into_iter()
            .chain([255u8; 18])
            .collect::<Vec<_>>();
        assert_eq!(
            resize_rgb_bilinear_pil(&src, 8, 1, 4, 1),
            vec![36, 36, 36, 223, 223, 223, 255, 255, 255, 255, 255, 255]
        );
    }

    #[test]
    fn resize_matches_pil_on_an_upscaled_ramp() {
        let mut src = Vec::new();
        for y in 0..3u8 {
            for x in 0..3u8 {
                src.extend_from_slice(&[x * 100, y * 100, 50]);
            }
        }
        let mut expected = Vec::new();
        for g in [0, 40, 100, 160, 200] {
            for r in [0, 40, 100, 160, 200] {
                expected.extend_from_slice(&[r, g, 50]);
            }
        }
        assert_eq!(resize_rgb_bilinear_pil(&src, 3, 3, 5, 5), expected);
    }

    #[test]
    fn preprocess_lays_out_nchw_f32_in_unit_range() {
        let page = RgbPage {
            data: vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255],
            width: 2,
            height: 2,
        };
        let nchw = preprocess_to_nchw(&page);
        assert_eq!(nchw.len(), 3 * 640 * 640);
        assert!(nchw.iter().all(|&v| (0.0..=1.0).contains(&v)));
        // The mean of a half-white image resized to 640x640 stays ~0.5 per
        // channel; a channel swap or a 0..255 leak would read far from it.
        for c in 0..3 {
            let plane = &nchw[c * 640 * 640..(c + 1) * 640 * 640];
            let mean = plane.iter().sum::<f32>() / plane.len() as f32;
            assert!(
                (mean - 0.5).abs() < 0.02,
                "channel {c} mean {mean} is not ~0.5"
            );
        }
    }
}
