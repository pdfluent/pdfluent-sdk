//! A Python callable as an OCR engine (lane L6).
//!
//! [`OcrCallback`] implements [`pdf_ocr::OcrEngine`] by calling a Python
//! function once per page. The function takes the page raster --
//! `function(image, width, height, dpi)` with `image` as RGB row-major
//! bytes -- and returns the words it reads as a list of dicts with `text`,
//! `bbox` (`[x0, y0, x1, y1]` in pixels) and an optional `confidence`
//! between 0 and 1 (1 when absent). A call that raises, or a return value
//! that is not shaped like that, is an OCR failure naming what was wrong.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_ocr::{OcrEngine, OcrPageResult, OcrWord};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

/// An [`OcrEngine`] that asks a Python callable to read each page.
#[derive(Debug)]
pub(crate) struct OcrCallback {
    function: Py<PyAny>,
}

impl OcrCallback {
    /// Call `function` for every page this engine is asked to read.
    pub(crate) fn new(function: Py<PyAny>) -> Self {
        Self { function }
    }

    /// The callable's name for error messages, or `ocr_function`.
    fn describe(&self, py: Python<'_>) -> String {
        self.function
            .getattr(py, "__name__")
            .and_then(|name| name.extract::<String>(py))
            .unwrap_or_else(|_| "ocr_function".to_string())
    }
}

impl OcrEngine for OcrCallback {
    fn recognize(
        &self,
        image_data: &[u8],
        width: u32,
        height: u32,
        dpi: u32,
    ) -> Result<OcrPageResult, String> {
        Python::with_gil(|py| {
            let image = PyBytes::new(py, image_data);
            let name = self.describe(py);
            let result = self
                .function
                .call1(py, (image, width, height, dpi))
                .map_err(|e| format!("ocr_function {name} failed: {e}"))?;
            let words = parse_words(&name, result.bind(py))?;
            let confidence = if words.is_empty() {
                0.0
            } else {
                words.iter().map(|w| w.confidence).sum::<f32>() / words.len() as f32
            };
            Ok(OcrPageResult {
                words,
                confidence,
                image_width: width,
                image_height: height,
            })
        })
    }

    fn supported_languages(&self) -> Vec<String> {
        Vec::new()
    }
}

/// The callback's return value as words: a list of `text`/`bbox`/
/// `confidence` dicts, in reading order. Empty texts are skipped the way
/// the TSV reader skips them; anything misshapen names the word index.
fn parse_words(name: &str, result: &Bound<'_, PyAny>) -> Result<Vec<OcrWord>, String> {
    let items: Vec<Bound<'_, PyAny>> = result.extract().map_err(|_| {
        format!(
            "ocr_function {name} must return a list of word dicts, not {}",
            typename(result)
        )
    })?;
    let mut words = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let word = item.downcast::<PyDict>().map_err(|_| {
            format!(
                "ocr_function {name} word {index} must be a dict with text, bbox and confidence"
            )
        })?;
        let text: String = get(word, name, index, "text")?;
        if text.is_empty() {
            continue;
        }
        let bbox = bbox_of(word, name, index)?;
        let confidence = confidence_of(word, name, index)?;
        words.push(OcrWord {
            text,
            bbox_px: bbox,
            confidence,
        });
    }
    Ok(words)
}

/// The type name of a value, for the message that names the returned type.
fn typename(value: &Bound<'_, PyAny>) -> String {
    value
        .get_type()
        .name()
        .map(|name| name.to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

/// One required string field of a word dict.
fn get(dict: &Bound<'_, PyDict>, name: &str, index: usize, key: &str) -> Result<String, String> {
    dict.get_item(key)
        .map_err(|e| format!("ocr_function {name} word {index} has no {key}: {e}"))?
        .ok_or_else(|| format!("ocr_function {name} word {index} has no {key}"))?
        .extract()
        .map_err(|_| format!("ocr_function {name} word {index} {key} must be a string"))
}

/// One word's `bbox` as `[x0, y0, x1, y1]` pixels: four finite numbers with
/// the corners ordered.
fn bbox_of(dict: &Bound<'_, PyDict>, name: &str, index: usize) -> Result<[u32; 4], String> {
    let invalid =
        || format!("ocr_function {name} word {index} bbox must be [x0, y0, x1, y1] pixels");
    let raw: Vec<f64> = dict
        .get_item("bbox")
        .map_err(|_| invalid())?
        .ok_or_else(invalid)?
        .extract()
        .map_err(|_| invalid())?;
    if raw.len() != 4
        || raw.iter().any(|v| !v.is_finite() || *v < 0.0)
        || raw[0] > raw[2]
        || raw[1] > raw[3]
    {
        return Err(invalid());
    }
    Ok([raw[0] as u32, raw[1] as u32, raw[2] as u32, raw[3] as u32])
}

/// One word's `confidence`: 1 when absent, clamped into 0 to 1.
fn confidence_of(dict: &Bound<'_, PyDict>, name: &str, index: usize) -> Result<f32, String> {
    let Some(value) = dict
        .get_item("confidence")
        .map_err(|e| format!("ocr_function {name} word {index} confidence unreadable: {e}"))?
    else {
        return Ok(1.0);
    };
    let confidence: f64 = value
        .extract()
        .map_err(|_| format!("ocr_function {name} word {index} confidence must be a number"))?;
    if !confidence.is_finite() {
        return Err(format!(
            "ocr_function {name} word {index} confidence must be a number"
        ));
    }
    Ok(confidence.clamp(0.0, 1.0) as f32)
}
