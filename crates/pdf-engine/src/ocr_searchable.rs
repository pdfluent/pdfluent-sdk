//! OCR before extraction: read a searchable copy instead of the document.
//!
//! [`PdfDocument::searchable_copy`](crate::PdfDocument::searchable_copy) runs
//! `pdf-ocr`'s hidden-text-layer path over an in-memory copy of the document:
//! each page the window asks for is rendered at
//! [`ExtractOptions::ocr_dpi`](crate::ExtractOptions::ocr_dpi) with this
//! crate's renderer, the engine recognizes the raster, and the words land on
//! the copy as an invisible text layer. Extraction then reads the copy, so
//! recognized words go through the same segmentation as born-digital text --
//! there is no second text path for OCR to disagree with.
//!
//! The copy is an incremental update over the original bytes, not a rewrite:
//! only the layer's objects are appended, and everything the pipeline did
//! not touch is read from the untouched original.
//!
//! [`OcrMode::Auto`](crate::OcrMode::Auto) recognizes the pages under
//! `pdf-ocr`'s text threshold; [`OcrMode::Force`](crate::OcrMode::Force)
//! recognizes every page in the window after dropping its native text, so the
//! OCR layer replaces born-digital text rather than doubling it. Where the
//! pipeline changes nothing -- `Auto` over born-digital pages, or no words
//! recognized anywhere -- extraction reads the document itself: no roundtrip,
//! no behavior change.
//!

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::blocks::{ExtractOptions, OcrMode};
use crate::document::PdfDocument;
use crate::error::{EngineError, Result};
#[cfg(feature = "ocr-cli")]
use crate::render::RenderOptions;

impl PdfDocument {
    /// A copy of this document with an OCR text layer over the window's
    /// pages, recognized by `engine` (feature `ocr-cli`).
    ///
    /// `options` steers which pages and how: [`OcrMode::Auto`] recognizes
    /// the pages under `pdf-ocr`'s text threshold,
    /// [`OcrMode::Force`] recognizes every page in the window after
    /// dropping its native text, at [`ExtractOptions::ocr_dpi`] and in
    /// [`ExtractOptions::ocr_language`] -- the language only steers the
    /// engine this method is handed when that engine reads it. Pages
    /// outside the window are untouched, and an empty window reads back an
    /// unchanged copy.
    ///
    /// [`OcrMode::Never`] is a caller error here: nothing would be
    /// recognized and the copy would be a roundtrip for nothing. Callers
    /// that take "no OCR" as a normal case want
    /// [`extract_blocks`](Self::extract_blocks), which branches before this.
    #[cfg(feature = "ocr-cli")]
    pub fn searchable_copy<E: pdf_ocr::OcrEngine>(
        &self,
        options: &ExtractOptions,
        engine: &E,
    ) -> Result<PdfDocument> {
        options.validate(self.page_count())?;
        if options.ocr == OcrMode::Never {
            return Err(EngineError::ocr_failure(
                "searchable_copy with ocr=Never: nothing would be recognized",
            ));
        }
        Ok(self.searchable_copy_inner(options, engine)?.0)
    }

    /// The document extraction reads when OCR was asked for: `None` for the
    pub(crate) fn ocr_copy_if_requested(
        &self,
        options: &ExtractOptions,
    ) -> Result<Option<PdfDocument>> {
        #[cfg(feature = "ocr-cli")]
        {
            if options.ocr == OcrMode::Never {
                return Ok(None);
            }
            // A zero dpi never reaches here: `validate` rejects it before
            // extraction branches into OCR, on every entry path.
            if self.ocr_window_pages(options).is_empty() {
                return Ok(None);
            }
            let engine = match pdf_ocr::TesseractCliEngine::new(&options.ocr_language) {
                Ok(engine) => engine,
                Err(e) if e.is_no_binary() && options.ocr == OcrMode::Auto => {
                    return Ok(None);
                }
                Err(e) => return Err(EngineError::ocr_engine_failure(e)),
            };
            let (copy, changed) = self.searchable_copy_inner(options, &engine)?;
            Ok(changed.then_some(copy))
        }
        #[cfg(not(feature = "ocr-cli"))]
        {
            match options.ocr {
                // No engine compiled in is no engine found: Auto reads on.
                OcrMode::Never | OcrMode::Auto => Ok(None),
                OcrMode::Force => Err(EngineError::ocr_failure(
                    "OCR was forced but this build has no OCR engine: it looks \
                     for the tesseract binary (enable the `ocr-cli` feature)",
                )),
            }
        }
    }

    /// The window's pages, 0-based, in document order.
    #[cfg(feature = "ocr-cli")]
    fn ocr_window_pages(&self, options: &ExtractOptions) -> Vec<usize> {
        (0..self.page_count())
            .filter(|page| options.holds(*page))
            .collect()
    }

    /// The searchable copy and whether the pipeline changed anything: no
    /// words recognized and nothing stripped means the copy is the document
    /// again, and the caller reads the document itself instead.
    #[cfg(feature = "ocr-cli")]
    fn searchable_copy_inner<E: pdf_ocr::OcrEngine>(
        &self,
        options: &ExtractOptions,
        engine: &E,
    ) -> Result<(PdfDocument, bool)> {
        // `validate` owns the dpi check: both callers (`searchable_copy`
        // and `ocr_copy_if_requested` behind extraction) validate first.
        let window = self.ocr_window_pages(options);
        let source: &[u8] = self.pdf().data().as_ref();
        if window.is_empty() {
            let copy = PdfDocument::open(source.to_vec())
                .map_err(|e| EngineError::ocr_failure(format!("reopen OCR copy: {e}")))?;
            return Ok((copy, false));
        }
        let original = lopdf::Document::load_mem(source)
            .map_err(|e| EngineError::ocr_failure(format!("read document for OCR: {e}")))?;
        let mut working = original.clone();

        let mut changed = false;
        if options.ocr == OcrMode::Force {
            changed |= strip_window_text(&mut working, &window)?;
        }

        let pages: Vec<u32> = window.iter().map(|page| *page as u32 + 1).collect();
        let config = pdf_ocr::OcrConfig {
            dpi: options.ocr_dpi,
            pages,
            ..pdf_ocr::OcrConfig::default()
        };
        // The pipeline hands its working copy to the render callback; the
        // renderer reads this document instead. The copy only ever gains an
        // invisible text layer, which renders nothing, so both read the same
        // pixels -- and for Force the raster keeps the native text, which is
        // what the recognizer should see.
        let report = pdf_ocr::make_searchable(&mut working, engine, &config, |_, page, dpi| {
            self.render_page_rgb(page - 1, dpi)
        })
        .map_err(|e| EngineError::ocr_failure(e.to_string()))?;
        changed |= report.total_words > 0;

        let bytes = incremental_bytes(source, &original, &working)?;
        let mut copy = PdfDocument::open(bytes)
            .map_err(|e| EngineError::ocr_failure(format!("reopen OCR copy: {e}")))?;
        copy.adopt_settings(self);
        Ok((copy, changed))
    }

    /// Page `index` (0-based) at `dpi` as RGB row-major pixels for OCR.
    #[cfg(feature = "ocr-cli")]
    fn render_page_rgb(
        &self,
        index: u32,
        dpi: u32,
    ) -> std::result::Result<(Vec<u8>, u32, u32), String> {
        let options = RenderOptions {
            dpi: f64::from(dpi),
            ..RenderOptions::default()
        };
        let rendered = self
            .render_page(index as usize, &options)
            .map_err(|e| e.to_string())?;
        let mut rgb = Vec::with_capacity(rendered.width as usize * rendered.height as usize * 3);
        for px in rendered.pixels.chunks_exact(4) {
            rgb.extend_from_slice(&px[..3]);
        }
        Ok((rgb, rendered.width, rendered.height))
    }
}

/// The working copy as an incremental update over the original bytes.
///
/// Only objects the pipeline added or changed are appended; everything else
/// is read from the original bytes untouched. A full rewrite would
/// re-serialize streams this parser misreads -- a quirk the lenient reader
/// tolerates, like the scan-source fixture's `//Length`, would come back
/// empty and the copy would lose native text. The facade's incremental save
/// is the same shape.
#[cfg(feature = "ocr-cli")]
fn incremental_bytes(
    source: &[u8],
    original: &lopdf::Document,
    working: &lopdf::Document,
) -> Result<Vec<u8>> {
    let mut incremental =
        lopdf::IncrementalDocument::create_from(source.to_vec(), original.clone());
    for (object_id, object) in &working.objects {
        let changed = match original.objects.get(object_id) {
            Some(previous) => previous != object,
            None => true,
        };
        if changed {
            incremental
                .new_document
                .set_object(*object_id, object.clone());
        }
    }
    incremental.new_document.version = working.version.clone();
    for (key, value) in &working.trailer {
        if key != b"Prev" && key != b"Size" {
            incremental
                .new_document
                .trailer
                .set(key.clone(), value.clone());
        }
    }
    incremental.new_document.max_id = working.max_id;
    let mut bytes = Vec::with_capacity(source.len() + 64 * 1024);
    incremental
        .save_to(&mut bytes)
        .map_err(|e| EngineError::ocr_failure(format!("write OCR copy: {e}")))?;
    Ok(bytes)
}

/// Drop the text-showing operators from the window's pages, for Force.
///
/// The OCR layer replaces the native text rather than doubling it; graphics,
/// positioning and marked content stay. Returns whether any page was
/// rewritten -- a page with no text-showing operators is left untouched, so
/// a force pass over scanned pages roundtrips nothing. `pages` is 0-based.
#[cfg(feature = "ocr-cli")]
fn strip_window_text(doc: &mut lopdf::Document, pages: &[usize]) -> Result<bool> {
    let ids = doc.get_pages();
    let mut changed = false;
    for &page in pages {
        let number = page as u32 + 1;
        let Some(&page_id) = ids.get(&number) else {
            continue;
        };
        changed |= strip_page_text(doc, page_id)?;
    }
    Ok(changed)
}

/// Drop the text-showing operators from one page's content.
///
/// The full content -- every stream, decompressed -- decodes, loses `Tj`,
/// `TJ`, `'` and `"`, and is written back as one plain stream when anything
/// was lost. Concatenated streams read as one, so one stream reads the same;
/// the orphaned originals stay in the file, which is valid and only costs
/// bytes in memory. Returns whether the page was rewritten.
///
/// The strip sees what the working parser sees: on the rare page whose text
/// it cannot parse (while the lenient reader still can), nothing is stripped
/// and Force degrades to appending the layer, like Auto.
#[cfg(feature = "ocr-cli")]
fn strip_page_text(doc: &mut lopdf::Document, page_id: lopdf::ObjectId) -> Result<bool> {
    let content = doc.get_page_content(page_id);
    let mut decoded = match lopdf::content::Content::decode(&content) {
        Ok(decoded) => decoded,
        // Unparsable to the working parser while the lenient reader still
        // reads it: nothing is stripped and Force degrades to appending the
        // layer, like Auto, rather than failing the extraction.
        Err(_) => return Ok(false),
    };
    let before = decoded.operations.len();
    decoded
        .operations
        .retain(|op| !matches!(op.operator.as_str(), "Tj" | "TJ" | "'" | "\""));
    if decoded.operations.len() == before {
        return Ok(false);
    }
    let encoded = decoded
        .encode()
        .map_err(|e| EngineError::ocr_failure(format!("write stripped page for OCR: {e}")))?;
    let stream_id = doc.add_object(lopdf::Object::Stream(lopdf::Stream::new(
        lopdf::dictionary! {},
        encoded,
    )));
    if let Ok(lopdf::Object::Dictionary(d)) = doc.get_object_mut(page_id) {
        d.set("Contents", lopdf::Object::Reference(stream_id));
    }
    Ok(true)
}
