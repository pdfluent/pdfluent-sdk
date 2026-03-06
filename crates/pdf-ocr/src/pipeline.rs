//! OCR pipeline: detect scanned pages, run OCR, insert invisible text layers.
//!
//! The pipeline takes a render callback to rasterize pages, avoiding a hard
//! dependency on any particular rendering engine.

use crate::engine::{OcrEngine, OcrPageResult};
use crate::error::{OcrError, Result};
use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, ObjectId, Stream};

/// Configuration for the OCR pipeline.
#[derive(Debug, Clone)]
pub struct OcrConfig {
    /// Resolution for rendering pages (dots per inch).
    pub dpi: u32,
    /// Minimum character count in content stream to consider a page as already containing text.
    pub text_threshold: usize,
    /// Specific pages to process (empty = all pages).
    pub pages: Vec<u32>,
}

impl Default for OcrConfig {
    fn default() -> Self {
        Self {
            dpi: 300,
            text_threshold: 10,
            pages: Vec::new(),
        }
    }
}

/// Report for the entire OCR process.
#[derive(Debug, Clone)]
pub struct OcrReport {
    /// Per-page reports.
    pub pages: Vec<OcrPageReport>,
    /// Total number of pages processed.
    pub pages_processed: usize,
    /// Total number of words recognized.
    pub total_words: usize,
}

/// Report for a single page.
#[derive(Debug, Clone)]
pub struct OcrPageReport {
    /// Page number (1-based).
    pub page: u32,
    /// Whether OCR was needed (page was scanned).
    pub ocr_needed: bool,
    /// Number of words recognized.
    pub words_recognized: usize,
    /// Overall confidence.
    pub confidence: f32,
}

/// Make a PDF searchable by running OCR on scanned pages.
///
/// # Arguments
/// * `doc` - The PDF document to process.
/// * `engine` - The OCR engine to use.
/// * `config` - OCR configuration.
/// * `render_fn` - A callback to render a page to an image: `(doc, page_num, dpi) -> (pixels, width, height)`.
pub fn make_searchable<
    E: OcrEngine,
    R: Fn(&Document, u32, u32) -> std::result::Result<(Vec<u8>, u32, u32), String>,
>(
    doc: &mut Document,
    engine: &E,
    config: &OcrConfig,
    render_fn: R,
) -> Result<OcrReport> {
    let pages = doc.get_pages();
    let total = pages.len() as u32;

    // Determine which pages to process.
    let page_nums: Vec<u32> = if config.pages.is_empty() {
        (1..=total).collect()
    } else {
        // Validate page numbers.
        for &p in &config.pages {
            if p == 0 || p > total {
                return Err(OcrError::PageOutOfRange(p, total));
            }
        }
        config.pages.clone()
    };

    let mut report = OcrReport {
        pages: Vec::new(),
        pages_processed: 0,
        total_words: 0,
    };

    for &page_num in &page_nums {
        let page_id = match pages.get(&page_num) {
            Some(&id) => id,
            None => continue,
        };

        let needs_ocr = page_needs_ocr(doc, page_id, config.text_threshold);

        if !needs_ocr {
            report.pages.push(OcrPageReport {
                page: page_num,
                ocr_needed: false,
                words_recognized: 0,
                confidence: 1.0,
            });
            continue;
        }

        // Render page to image.
        let (image_data, width, height) =
            render_fn(doc, page_num, config.dpi).map_err(OcrError::Render)?;

        // Run OCR.
        let ocr_result = engine
            .recognize(&image_data, width, height, config.dpi)
            .map_err(OcrError::Engine)?;

        let words_count = ocr_result.words.len();
        let confidence = ocr_result.confidence;

        // Insert invisible text layer.
        if !ocr_result.words.is_empty() {
            let media_box = get_media_box(doc, page_id);
            insert_invisible_text_layer(doc, page_id, &ocr_result, &media_box, config.dpi)?;
        }

        report.pages.push(OcrPageReport {
            page: page_num,
            ocr_needed: true,
            words_recognized: words_count,
            confidence,
        });
        report.pages_processed += 1;
        report.total_words += words_count;
    }

    Ok(report)
}

/// Check whether a page needs OCR by counting text characters in the content stream.
fn page_needs_ocr(doc: &Document, page_id: ObjectId, threshold: usize) -> bool {
    let content_bytes = match get_page_content_bytes(doc, page_id) {
        Some(bytes) => bytes,
        None => return true, // No content stream = likely scanned.
    };

    let content = match Content::decode(&content_bytes) {
        Ok(c) => c,
        Err(_) => return true,
    };

    let mut char_count = 0;
    for op in &content.operations {
        match op.operator.as_str() {
            "Tj" => {
                for operand in &op.operands {
                    if let Object::String(bytes, _) = operand {
                        char_count += bytes.len();
                    }
                }
            }
            "TJ" => {
                for operand in &op.operands {
                    if let Object::Array(arr) = operand {
                        for item in arr {
                            if let Object::String(bytes, _) = item {
                                char_count += bytes.len();
                            }
                        }
                    }
                }
            }
            "'" | "\"" => {
                for operand in &op.operands {
                    if let Object::String(bytes, _) = operand {
                        char_count += bytes.len();
                    }
                }
            }
            _ => {}
        }
    }

    char_count < threshold
}

/// Insert an invisible text layer on a page using OCR results.
///
/// Uses rendering mode 3 (invisible) so text is searchable but not visible.
fn insert_invisible_text_layer(
    doc: &mut Document,
    page_id: ObjectId,
    ocr_result: &OcrPageResult,
    media_box: &[f64; 4],
    _dpi: u32,
) -> Result<()> {
    let page_width = media_box[2] - media_box[0];
    let page_height = media_box[3] - media_box[1];
    let img_w = ocr_result.image_width as f64;
    let img_h = ocr_result.image_height as f64;

    let scale_x = page_width / img_w;
    let scale_y = page_height / img_h;

    let mut ops = vec![
        Operation::new("BT", vec![]),
        // Set rendering mode 3 (invisible).
        Operation::new("Tr", vec![Object::Integer(3)]),
        Operation::new(
            "Tf",
            vec![Object::Name(b"Helvetica".to_vec()), Object::Real(10.0)],
        ),
    ];

    for word in &ocr_result.words {
        let [px0, py0, px1, _py1] = word.bbox_px;

        // Convert pixel coordinates to PDF coordinates.
        let pdf_x = media_box[0] + (px0 as f64) * scale_x;
        // PDF y-axis is bottom-up, image y-axis is top-down.
        let pdf_y = media_box[3] - (py0 as f64) * scale_y;

        let word_width_px = (px1 - px0) as f64;
        let word_width_pdf = word_width_px * scale_x;

        // Approximate the natural width of the text at 10pt.
        let natural_width = word.text.len() as f64 * 10.0 * 0.5;
        let h_scale = if natural_width > 0.0 {
            (word_width_pdf / natural_width) * 100.0
        } else {
            100.0
        };

        // Set text position with Tm.
        ops.push(Operation::new(
            "Tm",
            vec![
                Object::Real(1.0),
                Object::Real(0.0),
                Object::Real(0.0),
                Object::Real(1.0),
                Object::Real(pdf_x as f32),
                Object::Real(pdf_y as f32),
            ],
        ));
        // Set horizontal scaling.
        ops.push(Operation::new("Tz", vec![Object::Real(h_scale as f32)]));
        // Show the text.
        ops.push(Operation::new(
            "Tj",
            vec![Object::String(
                word.text.as_bytes().to_vec(),
                lopdf::StringFormat::Literal,
            )],
        ));
    }

    ops.push(Operation::new("ET", vec![]));

    let content = Content { operations: ops };
    let encoded = content
        .encode()
        .map_err(|e| OcrError::Other(format!("failed to encode text layer: {e}")))?;

    let text_stream = Stream::new(dictionary! {}, encoded);
    let text_id = doc.add_object(Object::Stream(text_stream));

    // Append to page contents.
    let existing = {
        let page_obj = match doc.get_object(page_id) {
            Ok(obj) => obj,
            Err(_) => return Ok(()),
        };
        let page_dict = match page_obj {
            Object::Dictionary(ref d) => d,
            _ => return Ok(()),
        };
        page_dict.get(b"Contents").ok().cloned()
    };

    let new_contents = match existing {
        Some(Object::Reference(existing_id)) => Object::Array(vec![
            Object::Reference(existing_id),
            Object::Reference(text_id),
        ]),
        Some(Object::Array(mut arr)) => {
            arr.push(Object::Reference(text_id));
            Object::Array(arr)
        }
        _ => Object::Reference(text_id),
    };

    if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
        d.set("Contents", new_contents);
    }

    Ok(())
}

/// Get the MediaBox for a page.
fn get_media_box(doc: &Document, page_id: ObjectId) -> [f64; 4] {
    let default_box = [0.0, 0.0, 612.0, 792.0];

    let page_obj = match doc.get_object(page_id) {
        Ok(obj) => obj,
        Err(_) => return default_box,
    };

    let page_dict = match page_obj {
        Object::Dictionary(ref d) => d,
        _ => return default_box,
    };

    match page_dict.get(b"MediaBox") {
        Ok(Object::Array(arr)) => {
            if arr.len() >= 4 {
                let vals: Vec<f64> = arr
                    .iter()
                    .filter_map(|v| match v {
                        Object::Integer(i) => Some(*i as f64),
                        Object::Real(f) => Some(*f as f64),
                        _ => None,
                    })
                    .collect();
                if vals.len() >= 4 {
                    [vals[0], vals[1], vals[2], vals[3]]
                } else {
                    default_box
                }
            } else {
                default_box
            }
        }
        _ => default_box,
    }
}

/// Get content stream bytes for a page.
fn get_page_content_bytes(doc: &Document, page_id: ObjectId) -> Option<Vec<u8>> {
    doc.get_page_content(page_id).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{NoOpEngine, OcrPageResult, OcrWord};

    /// A mock OCR engine that returns predetermined results.
    struct MockEngine {
        result: OcrPageResult,
    }

    impl MockEngine {
        fn new(words: Vec<OcrWord>) -> Self {
            let confidence = if words.is_empty() {
                0.0
            } else {
                words.iter().map(|w| w.confidence).sum::<f32>() / words.len() as f32
            };
            Self {
                result: OcrPageResult {
                    words,
                    confidence,
                    image_width: 600,
                    image_height: 800,
                },
            }
        }
    }

    impl OcrEngine for MockEngine {
        fn recognize(
            &self,
            _image_data: &[u8],
            _width: u32,
            _height: u32,
            _dpi: u32,
        ) -> std::result::Result<OcrPageResult, String> {
            Ok(self.result.clone())
        }

        fn supported_languages(&self) -> Vec<String> {
            vec!["eng".to_string()]
        }
    }

    /// Helper: create a doc with a scanned page (no text content).
    fn make_scanned_doc() -> Document {
        let mut doc = Document::with_version("1.7");

        // Page with only graphics (no text operators).
        let content_stream =
            Stream::new(dictionary! {}, b"q 612 0 0 792 0 0 cm /Im0 Do Q".to_vec());
        let content_id = doc.add_object(Object::Stream(content_stream));

        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
        };
        let page_id = doc.add_object(Object::Dictionary(page_dict));

        let pages_dict = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        };
        let pages_id = doc.add_object(Object::Dictionary(pages_dict));

        if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
            d.set("Parent", Object::Reference(pages_id));
        }

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc
    }

    /// Helper: create a doc with a text page.
    fn make_text_doc() -> Document {
        let mut doc = Document::with_version("1.7");

        let content_stream = Stream::new(
            dictionary! {},
            b"BT /F1 12 Tf (This is a text page with enough characters to pass threshold) Tj ET"
                .to_vec(),
        );
        let content_id = doc.add_object(Object::Stream(content_stream));

        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
        };
        let page_id = doc.add_object(Object::Dictionary(page_dict));

        let pages_dict = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        };
        let pages_id = doc.add_object(Object::Dictionary(pages_dict));

        if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
            d.set("Parent", Object::Reference(pages_id));
        }

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc
    }

    /// Helper: create a multi-page doc (mix of scanned and text).
    fn make_mixed_doc() -> Document {
        let mut doc = Document::with_version("1.7");
        let mut page_ids = Vec::new();

        // Page 1: scanned (no text).
        let content1 = Stream::new(dictionary! {}, b"q 612 0 0 792 0 0 cm /Im0 Do Q".to_vec());
        let c1 = doc.add_object(Object::Stream(content1));
        let p1 = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(c1),
        };
        let p1_id = doc.add_object(Object::Dictionary(p1));
        page_ids.push(p1_id);

        // Page 2: text page.
        let content2 = Stream::new(
            dictionary! {},
            b"BT /F1 12 Tf (Enough text content to pass the threshold) Tj ET".to_vec(),
        );
        let c2 = doc.add_object(Object::Stream(content2));
        let p2 = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(c2),
        };
        let p2_id = doc.add_object(Object::Dictionary(p2));
        page_ids.push(p2_id);

        // Page 3: scanned.
        let content3 = Stream::new(dictionary! {}, b"q 612 0 0 792 0 0 cm /Im1 Do Q".to_vec());
        let c3 = doc.add_object(Object::Stream(content3));
        let p3 = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(c3),
        };
        let p3_id = doc.add_object(Object::Dictionary(p3));
        page_ids.push(p3_id);

        let kids: Vec<Object> = page_ids.iter().map(|id| Object::Reference(*id)).collect();
        let pages_dict = dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => Object::Integer(page_ids.len() as i64),
        };
        let pages_id = doc.add_object(Object::Dictionary(pages_dict));

        for &pid in &page_ids {
            if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(pid) {
                d.set("Parent", Object::Reference(pages_id));
            }
        }

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc
    }

    /// Dummy render function for tests.
    fn dummy_render(
        _doc: &Document,
        _page_num: u32,
        _dpi: u32,
    ) -> std::result::Result<(Vec<u8>, u32, u32), String> {
        Ok((vec![0u8; 600 * 800 * 3], 600, 800))
    }

    #[test]
    fn scanned_page_needs_ocr() {
        let doc = make_scanned_doc();
        let pages = doc.get_pages();
        let page_id = *pages.get(&1).unwrap();
        assert!(page_needs_ocr(&doc, page_id, 10));
    }

    #[test]
    fn text_page_does_not_need_ocr() {
        let doc = make_text_doc();
        let pages = doc.get_pages();
        let page_id = *pages.get(&1).unwrap();
        assert!(!page_needs_ocr(&doc, page_id, 10));
    }

    #[test]
    fn noop_engine_processes_scanned() {
        let mut doc = make_scanned_doc();
        let engine = NoOpEngine;
        let config = OcrConfig::default();

        let report = make_searchable(&mut doc, &engine, &config, dummy_render).unwrap();
        assert_eq!(report.pages.len(), 1);
        assert!(report.pages[0].ocr_needed);
        assert_eq!(report.pages[0].words_recognized, 0);
    }

    #[test]
    fn text_page_skipped_by_pipeline() {
        let mut doc = make_text_doc();
        let engine = NoOpEngine;
        let config = OcrConfig::default();

        let report = make_searchable(&mut doc, &engine, &config, dummy_render).unwrap();
        assert_eq!(report.pages.len(), 1);
        assert!(!report.pages[0].ocr_needed);
        assert_eq!(report.pages_processed, 0);
    }

    #[test]
    fn mock_engine_inserts_invisible_text() {
        let mut doc = make_scanned_doc();
        let engine = MockEngine::new(vec![
            OcrWord {
                text: "Hello".to_string(),
                bbox_px: [10, 20, 100, 40],
                confidence: 0.95,
            },
            OcrWord {
                text: "World".to_string(),
                bbox_px: [110, 20, 200, 40],
                confidence: 0.90,
            },
        ]);
        let config = OcrConfig::default();

        let report = make_searchable(&mut doc, &engine, &config, dummy_render).unwrap();
        assert_eq!(report.pages_processed, 1);
        assert_eq!(report.total_words, 2);
        assert!(report.pages[0].ocr_needed);
        assert_eq!(report.pages[0].words_recognized, 2);
    }

    #[test]
    fn ocr_specific_pages() {
        let mut doc = make_mixed_doc();
        let engine = NoOpEngine;
        let config = OcrConfig {
            pages: vec![1],
            ..Default::default()
        };

        let report = make_searchable(&mut doc, &engine, &config, dummy_render).unwrap();
        // Only page 1 should be in the report.
        assert_eq!(report.pages.len(), 1);
        assert_eq!(report.pages[0].page, 1);
    }

    #[test]
    fn ocr_page_out_of_range() {
        let mut doc = make_scanned_doc();
        let engine = NoOpEngine;
        let config = OcrConfig {
            pages: vec![5],
            ..Default::default()
        };

        let result = make_searchable(&mut doc, &engine, &config, dummy_render);
        assert!(result.is_err());
    }
}
