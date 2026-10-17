//! Unified document facade — multi-page rendering, text extraction,
//! metadata, bookmarks, and thumbnails.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::error::{EngineError, Result};
use crate::geometry::{self, PageGeometry};
use crate::limits::{LimitError, ProcessingLimits};
use std::sync::{Arc, Mutex};

/// Shared slot used by the limit-warning collector.
///
/// Stores `Some((observed_bytes, limit_bytes))` when a
/// `StreamTooLarge` warning fires during rendering or extraction.
type LimitSlot = Arc<Mutex<Option<(u64, u64)>>>;
use crate::blocks::{Block, ClipMargins, ExtractOptions, TatrOptions};
use crate::markdown_images::{self, Figure};
use crate::render::{self, ColorMode, RenderConfig, RenderOptions, RenderedPage};
use crate::text::{GeometryMode, TextBlock, TextExtractionDevice};
use crate::thumbnail::ThumbnailOptions;
use crate::words::Word;

use kurbo::Rect;
use pdf_forms::parse::parse_acroform;
use pdf_forms::tree::{FieldType, FieldValue};
use pdf_render::pdf_interpret as InkDeviceTypes;
use pdf_render::pdf_interpret::PageExt;
use pdf_render::pdf_interpret::{
    interpret_page, Cache, Context, InterpreterSettings, InterpreterWarning,
};
use pdf_render::pdf_syntax::object::dict::keys::{
    A, D, DEST, F, FIRST, NEXT, NM, OUTLINES, RECT, S, SUBTYPE, TITLE, URI,
};
use pdf_render::pdf_syntax::object::{Array, Dict, Name, ObjRef, Object, ObjectIdentifier};
use pdf_render::pdf_syntax::page::Page;
use pdf_render::pdf_syntax::{Pdf, PdfLoadLimits};
#[cfg(feature = "parallel")]
use rayon::prelude::*;
use std::collections::BTreeSet;

/// Document metadata extracted from the info dictionary.
#[derive(Debug, Clone, Default)]
pub struct DocumentInfo {
    /// Document title.
    pub title: Option<String>,
    /// Author.
    pub author: Option<String>,
    /// Subject.
    pub subject: Option<String>,
    /// Keywords.
    pub keywords: Option<String>,
    /// Creator application.
    pub creator: Option<String>,
    /// Producer application.
    pub producer: Option<String>,
}

/// A bookmark / outline item.
#[derive(Debug, Clone)]
pub struct BookmarkItem {
    /// Bookmark title.
    pub title: String,
    /// Target page index (0-based), if resolvable.
    pub page: Option<usize>,
    /// Nested child bookmarks.
    pub children: Vec<BookmarkItem>,
}

/// Where a page link points.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LinkTarget {
    /// A `/URI` action: the link opens this address.
    Uri(String),
    /// A same-document jump: the 0-based page it lands on.
    Page(usize),
    /// A named destination, unresolved (catalog `/Names` lookup is a
    /// follow-up, as for outlines): the destination name.
    Named(String),
    /// Anything else: a remote jump, a launch or script action, or a
    /// destination too broken to read.
    Unknown,
}

/// One `/Link` annotation on a page: the region it covers and where it goes.
#[derive(Debug, Clone, PartialEq)]
pub struct PageLink {
    /// Where the link points.
    pub target: LinkTarget,
    /// The `/Rect` in PDF user-space points, bottom-left origin.
    pub rect: [f64; 4],
    /// The annotation's object number, or 0 for a direct (inline) annot.
    pub xref: u32,
    /// The `/NM` entry, or empty when the annot carries none.
    pub id: String,
}

/// High-level PDF document handle.
pub struct PdfDocument {
    pdf: Pdf,
    settings: InterpreterSettings,
    /// Hand back the ligature code points a font declared rather than the
    /// letters they stand for. See [`Self::set_keep_ligatures`].
    keep_ligatures: bool,
}

impl PdfDocument {
    /// Open a PDF from bytes.
    pub fn open(data: impl Into<pdf_render::pdf_syntax::PdfData>) -> Result<Self> {
        let pdf = Pdf::new(data).map_err(|e| match e {
            pdf_render::pdf_syntax::LoadPdfError::Decryption(d) => {
                EngineError::Encrypted(format!("{d:?}"))
            }
            _ => EngineError::InvalidPdf(format!("{e:?}")),
        })?;
        let settings = InterpreterSettings {
            shared_cache: Some(Cache::new()),
            ..InterpreterSettings::default()
        };
        Ok(Self {
            pdf,
            settings,
            keep_ligatures: false,
        })
    }

    /// Open a PDF from bytes with processing limits.
    pub fn open_with_processing_limits(
        data: impl Into<pdf_render::pdf_syntax::PdfData>,
        limits: ProcessingLimits,
    ) -> Result<Self> {
        let syntax_limits = PdfLoadLimits::new()
            .max_object_depth(limits.max_object_depth)
            .max_image_pixels(limits.max_image_pixels)
            .max_stream_bytes(limits.max_stream_bytes);
        let pdf = Pdf::new_with_limits(data, syntax_limits).map_err(|e| match e {
            pdf_render::pdf_syntax::LoadPdfError::Decryption(d) => {
                EngineError::Encrypted(format!("{d:?}"))
            }
            _ => EngineError::InvalidPdf(format!("{e:?}")),
        })?;
        let settings = InterpreterSettings {
            max_operator_count: Some(limits.max_operator_count),
            shared_cache: Some(Cache::new()),
            ..InterpreterSettings::default()
        };
        Ok(Self {
            pdf,
            settings,
            keep_ligatures: false,
        })
    }

    /// Open a password-protected PDF.
    pub fn open_with_password(
        data: impl Into<pdf_render::pdf_syntax::PdfData>,
        password: &str,
    ) -> Result<Self> {
        let pdf = Pdf::new_with_password(data, password).map_err(|e| match e {
            pdf_render::pdf_syntax::LoadPdfError::Decryption(d) => {
                EngineError::Encrypted(format!("{d:?}"))
            }
            _ => EngineError::InvalidPdf(format!("{e:?}")),
        })?;
        let settings = InterpreterSettings {
            shared_cache: Some(Cache::new()),
            ..InterpreterSettings::default()
        };
        Ok(Self {
            pdf,
            settings,
            keep_ligatures: false,
        })
    }

    /// Open a password-protected PDF with processing limits.
    pub fn open_with_password_and_processing_limits(
        data: impl Into<pdf_render::pdf_syntax::PdfData>,
        password: &str,
        limits: ProcessingLimits,
    ) -> Result<Self> {
        let syntax_limits = PdfLoadLimits::new()
            .max_object_depth(limits.max_object_depth)
            .max_image_pixels(limits.max_image_pixels)
            .max_stream_bytes(limits.max_stream_bytes);
        let pdf = Pdf::new_with_password_and_limits(data, password, syntax_limits).map_err(
            |e| match e {
                pdf_render::pdf_syntax::LoadPdfError::Decryption(d) => {
                    EngineError::Encrypted(format!("{d:?}"))
                }
                _ => EngineError::InvalidPdf(format!("{e:?}")),
            },
        )?;
        let settings = InterpreterSettings {
            max_operator_count: Some(limits.max_operator_count),
            shared_cache: Some(Cache::new()),
            ..InterpreterSettings::default()
        };
        Ok(Self {
            pdf,
            settings,
            keep_ligatures: false,
        })
    }

    /// Access the underlying parsed PDF.
    pub fn pdf(&self) -> &Pdf {
        &self.pdf
    }

    /// Structural recovery applied while loading this document (xref / page-tree
    /// rebuild). All-`false` for a document that parsed cleanly.
    pub fn load_recovery(&self) -> pdf_render::pdf_syntax::LoadRecovery {
        self.pdf.load_recovery()
    }

    /// Take another document's interpreter settings and ligature choice.
    ///
    /// For copies opened from derived bytes -- the OCR path's searchable copy
    /// -- which must read the way the document they derive from does.
    #[cfg(feature = "ocr-cli")]
    pub(crate) fn adopt_settings(&mut self, other: &Self) {
        self.settings = other.settings.clone();
        self.keep_ligatures = other.keep_ligatures;
    }

    /// Set interpreter settings (font resolver, cmap resolver, etc.).
    pub fn set_settings(&mut self, settings: InterpreterSettings) {
        self.settings = settings;
    }

    /// Hand back the ligature code points a font declared -- `ﬃ` rather than
    /// `ffi` -- from every TEXT extraction on this document: the pages, the
    /// blocks and their spans, the markdown and JSON over them, and the
    /// AcroForm field values [`Self::extract_acroform_text`] reads.
    ///
    /// Off by default: one glyph for three letters is a typesetting decision,
    /// and a reader that keeps U+FB03 hands back a word no search box will
    /// match (#437). Turn it on when the caller wants the file's own
    /// characters: a typesetting tool, or a comparison against the content
    /// stream.
    ///
    /// Outside it either way is what is not a TEXT output: the training input
    /// [`Self::extract_blocks_before_learning`], which the models read, and
    /// glyph-level readers outside this crate.
    pub fn set_keep_ligatures(&mut self, keep: bool) {
        self.keep_ligatures = keep;
    }

    /// Install a warning sink that receives [`InterpreterWarning`]s raised
    /// during subsequent render / text-extraction operations.
    ///
    /// Replaces any previously installed sink; all other settings (cache,
    /// limits, resolvers) are preserved. The sink is preserved across the
    /// extraction-settings clone, so it covers both rendering and extraction.
    pub fn set_warning_sink(&mut self, sink: pdf_render::pdf_interpret::WarningSinkFn) {
        self.settings.warning_sink = sink;
    }

    /// Number of pages.
    pub fn page_count(&self) -> usize {
        self.pdf.pages().len()
    }

    /// Get the geometry of a page.
    pub fn page_geometry(&self, index: usize) -> Result<PageGeometry> {
        let page = self.get_page(index)?;
        Ok(geometry::extract_geometry(page))
    }

    /// Render a single page.
    ///
    /// If the document contains an XFA template, it is automatically flattened
    /// to static PDF content before rendering.  This prevents the "Please wait"
    /// placeholder page that Adobe Reader would show when rendering an XFA PDF
    /// with a conventional renderer. If flattening fails, rendering falls back
    /// to the original document as a best-effort path.
    pub fn render_page(&self, index: usize, options: &RenderOptions) -> Result<RenderedPage> {
        #[cfg(feature = "xfa")]
        if let Some(flat_doc) = self.open_flattened_xfa_for_render() {
            return flat_doc.render_page(index, options);
        }
        let page = self.get_page(index)?;
        // Pre-flight: reject pathologically small or zero-dimension pages before
        // allocating any pixel buffer. Non-positive dimensions cause panics or
        // zero-sized allocations inside the rasteriser.
        let (w, h) = page.render_dimensions();
        if w <= 0.0 || h <= 0.0 {
            return Err(EngineError::InvalidPageGeometry {
                width: w,
                height: h,
                reason: "page has zero or negative dimensions".into(),
            });
        }
        // Also reject pages so small they produce zero pixels even at the
        // minimum meaningful DPI (1 DPI). Below ~0.72pt at 1 DPI = 0 pixels.
        const MIN_PAGE_PT: f32 = 1.0;
        if w < MIN_PAGE_PT || h < MIN_PAGE_PT {
            return Err(EngineError::InvalidPageGeometry {
                width: w,
                height: h,
                reason: "page too small to render (< 1pt)".into(),
            });
        }
        let (settings, slot) = Self::with_limit_collector(&self.settings);
        let rendered = render::render_page(page, options, &settings);
        Self::check_limit_slot(&slot)?;
        Ok(rendered)
    }

    /// Render a single page using the high-level render config.
    ///
    /// XFA documents are auto-flattened before rendering (same as `render_page`).
    /// If flattening fails, rendering falls back to the original document.
    pub fn render_page_with_config(
        &self,
        index: usize,
        config: &RenderConfig,
    ) -> Result<RenderedPage> {
        #[cfg(feature = "xfa")]
        if let Some(flat_doc) = self.open_flattened_xfa_for_render() {
            return flat_doc.render_page_with_config(index, config);
        }
        let page = self.get_page(index)?;
        let (w, h) = page.render_dimensions();
        if w <= 0.0 || h <= 0.0 {
            return Err(EngineError::InvalidPageGeometry {
                width: w,
                height: h,
                reason: "page has zero or negative dimensions".into(),
            });
        }
        const MIN_PAGE_PT: f32 = 1.0;
        if w < MIN_PAGE_PT || h < MIN_PAGE_PT {
            return Err(EngineError::InvalidPageGeometry {
                width: w,
                height: h,
                reason: "page too small to render (< 1pt)".into(),
            });
        }
        let (settings, slot) = Self::with_limit_collector(&self.settings);
        let rendered = render::render_page_with_config(page, config, &settings);
        Self::check_limit_slot(&slot)?;
        Ok(rendered)
    }

    /// Render a single page to a CMYK buffer.
    pub fn render_page_cmyk(&self, index: usize, dpi: u32) -> Result<RenderedPage> {
        self.render_page_with_config(
            index,
            &RenderConfig {
                color_mode: ColorMode::PreserveCmyk,
                dpi,
            },
        )
    }

    /// Render all pages, in parallel when the `parallel` feature is enabled.
    pub fn render_all(&self, options: &RenderOptions) -> Vec<RenderedPage> {
        let pages = self.pdf.pages();
        #[cfg(feature = "parallel")]
        return (0..pages.len())
            .into_par_iter()
            .map(|i| render::render_page(&pages[i], options, &self.settings))
            .collect();
        #[cfg(not(feature = "parallel"))]
        (0..pages.len())
            .map(|i| render::render_page(&pages[i], options, &self.settings))
            .collect()
    }

    /// Render all pages using the high-level render config.
    pub fn render_all_with_config(&self, config: &RenderConfig) -> Vec<RenderedPage> {
        let pages = self.pdf.pages();
        #[cfg(feature = "parallel")]
        return (0..pages.len())
            .into_par_iter()
            .map(|i| render::render_page_with_config(&pages[i], config, &self.settings))
            .collect();
        #[cfg(not(feature = "parallel"))]
        (0..pages.len())
            .map(|i| render::render_page_with_config(&pages[i], config, &self.settings))
            .collect()
    }

    /// Generate a thumbnail for a single page.
    pub fn thumbnail(&self, index: usize, options: &ThumbnailOptions) -> Result<RenderedPage> {
        let page = self.get_page(index)?;
        Ok(render::render_thumbnail(
            page,
            options.max_dimension,
            &self.settings,
        ))
    }

    /// Generate thumbnails for all pages, in parallel when the `parallel` feature is enabled.
    pub fn thumbnails_all(&self, options: &ThumbnailOptions) -> Vec<RenderedPage> {
        let pages = self.pdf.pages();
        #[cfg(feature = "parallel")]
        return (0..pages.len())
            .into_par_iter()
            .map(|i| render::render_thumbnail(&pages[i], options.max_dimension, &self.settings))
            .collect();
        #[cfg(not(feature = "parallel"))]
        (0..pages.len())
            .map(|i| render::render_thumbnail(&pages[i], options.max_dimension, &self.settings))
            .collect()
    }

    /// Extract text from a page as a single string.
    pub fn extract_text(&self, index: usize) -> Result<String> {
        let page = self.get_page(index)?;
        let (settings, slot) = Self::with_limit_collector(&self.text_extraction_settings());
        let mut device = TextExtractionDevice::new().keep_ligatures(self.keep_ligatures);
        let mut ctx = Self::create_context_with_settings(page, settings);
        interpret_page(page, &mut ctx, &mut device);
        Self::check_limit_slot(&slot)?;
        Ok(device.into_text())
    }

    /// Extract text from a sequence of pages while reusing the same settings object.
    #[doc(hidden)]
    pub fn extract_text_pages_reusing_settings<I>(&self, indices: I) -> Result<Vec<String>>
    where
        I: IntoIterator<Item = usize>,
    {
        let pages = self.pdf.pages();
        let mut settings = self.text_extraction_settings();
        let indices = indices.into_iter();
        let (lower_bound, upper_bound) = indices.size_hint();
        let mut texts = Vec::with_capacity(upper_bound.unwrap_or(lower_bound));

        for index in indices {
            let page = pages.get(index).ok_or(EngineError::PageOutOfRange {
                index,
                count: pages.len(),
            })?;
            let (text, next_settings) =
                Self::extract_text_with_settings(page, settings, self.keep_ligatures);
            settings = next_settings;
            texts.push(text);
        }

        Ok(texts)
    }

    /// Extract structured text blocks from a page.
    pub fn extract_text_blocks(&self, index: usize) -> Result<Vec<TextBlock>> {
        let page = self.get_page(index)?;
        let (settings, slot) = Self::with_limit_collector(&self.text_extraction_settings());
        let mut device = TextExtractionDevice::new().keep_ligatures(self.keep_ligatures);
        let mut ctx = Self::create_context_with_settings(page, settings);
        interpret_page(page, &mut ctx, &mut device);
        Self::check_limit_slot(&slot)?;
        Ok(device.into_blocks())
    }

    /// Extract structured text blocks with tight per-glyph geometry.
    ///
    /// Same blocks as [`Self::extract_text_blocks`], but the device runs in
    /// [`GeometryMode::RichGeometry`] so every span also carries its
    /// outline-tight glyph bounds, advances and bound sources. The door's
    /// span dicts read those; nothing else in the engine consumes them.
    pub fn extract_text_blocks_rich(&self, index: usize) -> Result<Vec<TextBlock>> {
        let page = self.get_page(index)?;
        let (settings, slot) = Self::with_limit_collector(&self.text_extraction_settings());
        let mut device = TextExtractionDevice::with_mode(GeometryMode::RichGeometry)
            .keep_ligatures(self.keep_ligatures);
        let mut ctx = Self::create_context_with_settings(page, settings);
        interpret_page(page, &mut ctx, &mut device);
        Self::check_limit_slot(&slot)?;
        Ok(device.into_blocks())
    }

    /// Extract structured text blocks from all pages, reusing interpreter settings.
    pub fn extract_all_text_blocks(&self) -> Vec<Vec<TextBlock>> {
        let pages = self.pdf.pages();
        let mut settings = self.text_extraction_settings();
        let mut blocks = Vec::with_capacity(pages.len());

        for page in pages.iter() {
            let (page_blocks, next_settings) =
                Self::extract_text_blocks_with_settings(page, settings, self.keep_ligatures);
            settings = next_settings;
            blocks.push(page_blocks);
        }

        blocks
    }

    /// Extract one page's paragraphs (#567).
    ///
    /// Preview: output quality is still improving and the API may change in a minor release.
    ///
    /// Where [`Self::extract_text_blocks`] hands back the page's LINES, this
    /// hands back the units a caller chunks, cites or embeds: the lines merged
    /// into paragraphs, never across a column gap and never across a table.
    ///
    /// `first_index` is where this page's blocks start in the document's
    /// reading order; [`Self::extract_blocks`] counts it for a whole document.
    ///
    /// The heading levels are taken from THIS PAGE's statistics, because a page
    /// asked for on its own is the only document there is to take them from
    /// (#568). A caller that wants the levels the whole document implies asks
    /// for the whole document: [`Self::extract_blocks`] runs the same
    /// classifier over every page at once.
    pub fn extract_page_blocks(&self, index: usize, first_index: usize) -> Result<Vec<Block>> {
        let page = self.get_page(index)?;
        let (settings, slot) = Self::with_limit_collector(&self.text_extraction_settings());
        let mut device = TextExtractionDevice::new();
        let mut ctx = Self::create_context_with_settings(page, settings);
        interpret_page(page, &mut ctx, &mut device);
        Self::check_limit_slot(&slot)?;
        let mut blocks = device.into_paragraph_blocks(index, first_index);
        // No furniture: a running head is what THREE pages repeat, and one
        // page repeats nothing (#571). The reading order still runs -- the
        // columns of a page are a property of that page.
        crate::headings::classify(&mut blocks);
        // AFTER the headings: stacked headings arrive on the same wide pitch
        // as a double-spaced run, and joining first turns two headings into
        // one paragraph (#636).
        let blocks = crate::text::join_double_spaced_runs(blocks, first_index);
        let mut blocks = crate::reading_order::arrange(blocks, first_index);
        // Folded after the decisions, which all read the characters the
        // glyphs declared (see `crate::ligatures`).
        if !self.keep_ligatures {
            for block in &mut blocks {
                crate::ligatures::fold_block(block);
            }
        }
        Ok(blocks)
    }

    /// Every block of the document, in reading order, numbered from zero
    /// (#567, #573).
    ///
    /// Preview: output quality is still improving and the API may change in a minor release.
    ///
    /// `options` says which pages the caller wants back; the document is read
    /// whole whatever it says, because what a block IS is decided against the
    /// rest of the document. See [`ExtractOptions`].
    ///
    /// A `Result`, and the limit collector on every page, because the whole
    /// document is exactly where a caller reading untrusted files leans on
    /// [`ProcessingLimits`]: a convenience method that swallowed a
    /// `StreamTooLarge` would hand back a silently short document where the
    /// caller asked to be told (codex, round 1).
    ///
    /// NOTHING ON THE TEXT PATH RAISES IT TODAY. The only producer of that
    /// warning is the image-XObject decode, and image decode is lazy -- it
    /// happens in the rendering device and not in `interpret_page`, which is
    /// why `crates/pdfluent/tests/processing_limits.rs` has to call `to_images`
    /// to provoke it. So this collector is the same inert guard
    /// [`Self::extract_text`] and [`Self::extract_page_blocks`] already carry,
    /// and it is here for the same reason: the day something on this path does
    /// raise it, the three methods answer alike instead of two of them
    /// answering and the convenience one going quiet.
    ///
    /// [`ProcessingLimits`]: crate::limits::ProcessingLimits
    pub fn extract_blocks(&self, options: &ExtractOptions) -> Result<Vec<Block>> {
        options.validate(self.page_count())?;
        // OCR first, when asked: extraction reads the searchable copy, so
        // recognized words segment exactly like born-digital text. The copy
        // runs with OCR off -- its layer is already laid.
        if let Some(copy) = self.ocr_copy_if_requested(options)? {
            let mut inner = options.clone();
            inner.ocr = crate::OcrMode::Never;
            return copy.extract_blocks(&inner);
        }
        // Raw content-stream order, when asked (#643): the runs the pages
        // draw in the order they draw them, grouped into blocks by
        // content-order proximity -- and nothing else. The furniture, the
        // headings, the reading-order sort and the learned pass never run;
        // the window and the ligature fold still do, so raw blocks page and
        // read like any.
        if options.raw_order {
            return self.extract_blocks_in_raw_order(options);
        }
        let (blocks, ruled, lattices, images, graphics) = self
            .blocks_rules_and_rulings_before_learning(
                options.table_strategy,
                options.document_font_names,
                options.cell_path_words,
                options.actual_text,
                options.clip_margins,
            )?;
        // LAST OF ALL, over the whole document: the learned decisions (#599).
        // They read the finished blocks because that is what they were trained
        // on -- `feature_dump` measures exactly this state -- and the whole
        // document because a heading model reads repetition across pages.
        let sizes: Vec<(f64, f64)> = (0..self.page_count())
            .map(|page| {
                self.page_geometry(page)
                    .map(|g| g.effective_dimensions())
                    .unwrap_or((0.0, 0.0))
            })
            .collect();
        let tatr = self.tatr_context(options.tatr())?;
        let blocks = if options.learned {
            crate::learned::refine_with_tatr(
                blocks,
                &sizes,
                &ruled,
                &lattices,
                &images,
                mask_table_switches(crate::learned::SWITCHES, options.table_strategy),
                tatr.as_ref(),
                options.cell_path_words,
            )
        } else {
            blocks
        };
        let mut blocks = blocks;
        crate::table_row_reclaim::reclaim_table_edge_rows(&mut blocks, &lattices);
        crate::blocks::peel_table_edges(
            &mut blocks,
            crate::learned::SWITCHES.table_runs_on_figures,
        );
        crate::blocks::tables_across_pages(&mut blocks);
        Self::demote_fragment_tables(&mut blocks);
        crate::blocks::join_heading_subtitles(&mut blocks);
        crate::blocks::join_heading_lines(&mut blocks);
        crate::blocks::split_paragraphs_on_steps(&mut blocks);
        if options.use_tags {
            blocks = crate::tagged_evidence::apply(
                &self.pdf,
                &self.text_extraction_settings(),
                blocks,
                &sizes,
                options.cell_path_words,
            );
        }
        let blocks = Self::finish_blocks_with_figures(blocks, &images, &graphics, options);
        let mut blocks = crate::blocks::window(blocks, options);
        // Folded after the decisions, which all read the characters the
        // glyphs declared (see `crate::ligatures`).
        if !self.keep_ligatures {
            for block in &mut blocks {
                crate::ligatures::fold_block(block);
            }
        }
        crate::table_trace::settle_finals(&mut blocks, &lattices, &graphics);
        Ok(blocks)
    }

    /// [`Self::extract_blocks`], with the learned switches given, so a test of
    /// one switch reads its table with that switch on and the shipped tree
    /// reads the shipped switches. The table strategy still masks the table
    /// proposers it sits out; [`TableStrategy::Both`](crate::TableStrategy::Both)
    /// masks nothing.
    ///
    /// Hidden rather than published: a caller gets [`Self::extract_blocks`].
    #[doc(hidden)]
    pub fn extract_blocks_with_switches(
        &self,
        options: &ExtractOptions,
        switches: crate::learned::Switches,
    ) -> Result<Vec<Block>> {
        options.validate(self.page_count())?;
        // OCR first, when asked, like `extract_blocks`.
        if let Some(copy) = self.ocr_copy_if_requested(options)? {
            let mut inner = options.clone();
            inner.ocr = crate::OcrMode::Never;
            return copy.extract_blocks_with_switches(&inner, switches);
        }
        let (blocks, ruled, lattices, images, graphics) = self
            .blocks_rules_and_rulings_before_learning(
                options.table_strategy,
                options.document_font_names,
                options.cell_path_words,
                options.actual_text,
                options.clip_margins,
            )?;
        let sizes: Vec<(f64, f64)> = (0..self.page_count())
            .map(|page| {
                self.page_geometry(page)
                    .map(|g| g.effective_dimensions())
                    .unwrap_or((0.0, 0.0))
            })
            .collect();
        let tatr = self.tatr_context(options.tatr())?;
        let blocks = if options.learned {
            crate::learned::refine_with_tatr(
                blocks,
                &sizes,
                &ruled,
                &lattices,
                &images,
                mask_table_switches(switches, options.table_strategy),
                tatr.as_ref(),
                options.cell_path_words,
            )
        } else {
            blocks
        };
        let mut blocks = blocks;
        crate::table_row_reclaim::reclaim_table_edge_rows(&mut blocks, &lattices);
        crate::blocks::peel_table_edges(&mut blocks, switches.table_runs_on_figures);
        crate::blocks::tables_across_pages(&mut blocks);
        Self::demote_fragment_tables(&mut blocks);
        crate::blocks::join_heading_subtitles(&mut blocks);
        crate::blocks::join_heading_lines(&mut blocks);
        if options.use_tags {
            blocks = crate::tagged_evidence::apply(
                &self.pdf,
                &self.text_extraction_settings(),
                blocks,
                &sizes,
                options.cell_path_words,
            );
        }
        let blocks = Self::finish_blocks_with_figures(blocks, &images, &graphics, options);
        let mut blocks = crate::blocks::window(blocks, options);
        // Folded after the decisions, like `extract_blocks` above.
        if !self.keep_ligatures {
            for block in &mut blocks {
                crate::ligatures::fold_block(block);
            }
        }
        crate::table_trace::settle_finals(&mut blocks, &lattices, &graphics);
        Ok(blocks)
    }

    /// Every word of the document, with the box it was drawn in.
    ///
    /// Preview: output quality is still improving and the API may change in a minor release.
    ///
    /// The `extract_words=True` of pymupdf4llm: what a RAG pipeline cites,
    /// highlights and grounds with. Built from the blocks
    /// [`Self::extract_blocks`] returns -- same options, same window -- by
    /// splitting each span's text on whitespace and taking the union of that
    /// word's `char_bounds`. A span without `char_bounds` gives its words
    /// the span box, split proportionally by character count. Words come in
    /// reading order, block by block and line by line. One function, no
    /// second text pass.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pdf_engine::{ExtractOptions, PdfDocument};
    ///
    /// let document = PdfDocument::open(std::fs::read("report.pdf").unwrap()).unwrap();
    /// let words = document.extract_words(&ExtractOptions::default()).unwrap();
    /// for word in &words {
    ///     println!("p{} {:?} {:?}", word.page, word.text, word.bbox);
    /// }
    /// ```
    pub fn extract_words(&self, options: &ExtractOptions) -> Result<Vec<Word>> {
        let blocks = self.extract_blocks(options)?;
        Ok(crate::words::words_from_blocks(&blocks))
    }

    /// [`Self::extract_blocks`], and the rules each page drew beside them
    /// (#600).
    ///
    /// The second half is one `Vec` per page, in page order, of the horizontal
    /// rules the page drew -- what `crates/pdf-learned` reads to tell a ruled
    /// grid from a column of numbers that merely lines up. The blocks are the
    /// blocks [`Self::extract_blocks`] returns, learned decisions and all, so
    /// there is no second reading of the page to disagree with the first.
    ///
    /// Hidden rather than published, for the reason
    /// [`TextExtractionDevice::into_paragraph_blocks_and_rules`] gives.
    #[doc(hidden)]
    pub fn extract_blocks_and_rules(
        &self,
        options: &ExtractOptions,
    ) -> Result<(Vec<Block>, Vec<Vec<crate::text::HorizontalRule>>)> {
        let (blocks, ruled, _) = self.extract_blocks_rules_and_withdrawn(options)?;
        Ok((blocks, ruled))
    }

    /// [`Self::extract_blocks_and_rules`], and the tables rule A took out of
    /// the blocks (#636), for the balanced tier.
    pub(crate) fn extract_blocks_rules_and_withdrawn(
        &self,
        options: &ExtractOptions,
    ) -> Result<BlocksRulesWithdrawn> {
        options.validate(self.page_count())?;
        // OCR first, when asked, like `extract_blocks`.
        if let Some(copy) = self.ocr_copy_if_requested(options)? {
            let mut inner = options.clone();
            inner.ocr = crate::OcrMode::Never;
            return copy.extract_blocks_rules_and_withdrawn(&inner);
        }
        let (blocks, ruled, lattices, images, graphics) = self
            .blocks_rules_and_rulings_before_learning(
                options.table_strategy,
                options.document_font_names,
                options.cell_path_words,
                options.actual_text,
                options.clip_margins,
            )?;
        let sizes: Vec<(f64, f64)> = (0..self.page_count())
            .map(|page| {
                self.page_geometry(page)
                    .map(|g| g.effective_dimensions())
                    .unwrap_or((0.0, 0.0))
            })
            .collect();
        let tatr = self.tatr_context(options.tatr())?;
        let blocks = if options.learned {
            crate::learned::refine_with_tatr(
                blocks,
                &sizes,
                &ruled,
                &lattices,
                &images,
                mask_table_switches(crate::learned::SWITCHES, options.table_strategy),
                tatr.as_ref(),
                options.cell_path_words,
            )
        } else {
            blocks
        };
        let mut blocks = blocks;
        crate::table_row_reclaim::reclaim_table_edge_rows(&mut blocks, &lattices);
        crate::blocks::peel_table_edges(
            &mut blocks,
            crate::learned::SWITCHES.table_runs_on_figures,
        );
        crate::blocks::tables_across_pages(&mut blocks);
        Self::demote_fragment_tables(&mut blocks);
        crate::blocks::join_heading_subtitles(&mut blocks);
        crate::blocks::join_heading_lines(&mut blocks);
        crate::blocks::split_paragraphs_on_steps(&mut blocks);
        if options.use_tags {
            blocks = crate::tagged_evidence::apply(
                &self.pdf,
                &self.text_extraction_settings(),
                blocks,
                &sizes,
                options.cell_path_words,
            );
        }
        let blocks = Self::finish_blocks_with_figures(blocks, &images, &graphics, options);
        let mut blocks = crate::blocks::window(blocks, options);
        // Folded after the decisions, like `extract_blocks` above.
        if !self.keep_ligatures {
            for block in &mut blocks {
                crate::ligatures::fold_block(block);
            }
        }
        let withdrawn = crate::table_trace::settle_finals(&mut blocks, &lattices, &graphics);
        Ok((blocks, ruled, withdrawn))
    }

    /// The blocks and how sure the table pass is of each page, from one pass.
    ///
    /// The blocks are those [`Self::extract_blocks`] returns with
    /// [`ExtractOptions::default`]; the signals those [`Self::table_signals`]
    /// returns. One computation, so a caller that wants both never runs the
    /// table pass twice.
    #[doc(hidden)]
    pub fn extract_blocks_with_signals(
        &self,
    ) -> Result<(
        Vec<Block>,
        Vec<Option<crate::learned::tables::PageTableSignals>>,
    )> {
        let (blocks, ruled, lattices, images, graphics) = self
            .blocks_rules_and_rulings_before_learning(
                crate::TableStrategy::Both,
                false,
                false,
                false,
                None,
            )?;
        let sizes: Vec<(f64, f64)> = (0..self.page_count())
            .map(|page| {
                self.page_geometry(page)
                    .map(|g| g.effective_dimensions())
                    .unwrap_or((0.0, 0.0))
            })
            .collect();
        let (blocks, signals) = crate::learned::refine_with_images_and_signals_with_cell_path_words(
            blocks,
            &sizes,
            &ruled,
            &lattices,
            &images,
            crate::learned::SWITCHES,
            // What this entry promises: `extract_blocks` with the default.
            false,
        );
        let mut blocks = blocks;
        crate::table_row_reclaim::reclaim_table_edge_rows(&mut blocks, &lattices);
        crate::blocks::peel_table_edges(
            &mut blocks,
            crate::learned::SWITCHES.table_runs_on_figures,
        );
        crate::blocks::tables_across_pages(&mut blocks);
        Self::demote_fragment_tables(&mut blocks);
        crate::blocks::join_heading_subtitles(&mut blocks);
        crate::blocks::join_heading_lines(&mut blocks);
        crate::blocks::split_paragraphs_on_steps(&mut blocks);
        // The blocks this promises are `extract_blocks` under the default,
        // figures and all.
        let mut blocks = Self::finish_blocks_with_figures(
            blocks,
            &images,
            &graphics,
            &ExtractOptions::default(),
        );
        crate::table_trace::settle_finals(&mut blocks, &lattices, &graphics);
        Ok((blocks, signals))
    }

    /// How sure the table pass is of each page: the same pass
    /// [`Self::extract_blocks`] runs, with the numbers beside the decision.
    ///
    /// One entry per page, in page order. `None` where the learned pass does
    /// not run: the switch off, or a scanned page with nothing to ask of --
    /// "not asked" is not "no table".
    #[doc(hidden)]
    pub fn table_signals(&self) -> Result<Vec<Option<crate::learned::tables::PageTableSignals>>> {
        Ok(self.extract_blocks_with_signals()?.1)
    }

    /// The TATR context for one extraction, if asked for (track B, P3).
    ///
    /// The model is resolved up front: where the user asked for TATR and the
    /// model is absent the extraction refuses here with the shared layer's
    /// `MODEL_NOT_AVAILABLE` -- naming the install -- rather than running a
    /// silent fast pass. The fake seam answers without a model, so it skips
    /// the check.
    pub(crate) fn tatr_context(
        &self,
        tatr: Option<&TatrOptions>,
    ) -> Result<Option<crate::learned::tables::TatrContext<'_>>> {
        let Some(options) = tatr else {
            return Ok(None);
        };
        if options.test_hook.is_some() {
            return Ok(Some(crate::learned::tables::TatrContext::new(
                self,
                options.clone(),
            )));
        }
        #[cfg(feature = "ort-tables")]
        {
            let config = options.runtime_config();
            pdf_ocr::runtime::session(&crate::learned::tables_tatr::TATR_SPEC, &config)
                .map_err(std::io::Error::from)?;
            Ok(Some(crate::learned::tables::TatrContext::new(
                self,
                options.clone(),
            )))
        }
        #[cfg(not(feature = "ort-tables"))]
        {
            // Unreachable through the serving entries: `validate` refuses a
            // TATR request without the fake before the pass runs.
            Err(EngineError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                crate::error::OptionsError::TatrNeedsOrtTables,
            )))
        }
    }
}

/// What a run of table cells reads as, for the fragment veto (#636).
struct FragmentGrid {
    /// Over a third of the grid's positions are holes.
    sparse: bool,
    /// The median cell holds at most the fragment median.
    short: bool,
    /// The median cell's alphanumeric count.
    median: usize,
    /// "continu" occurrences across the cells.
    continued_hits: usize,
    /// Every cell carrying one stands in row 0.
    continued_in_first_row: bool,
    /// Some cell holds an alphabetic run of four or more.
    has_word: bool,
    /// The union of the cells' boxes.
    bbox: [f64; 4],
    /// The median cell height, the caption gap's em.
    em: f64,
}

/// The final blocks, the rules each page drew, and the tables rule A2 withdrew.
type BlocksRulesWithdrawn = (
    Vec<Block>,
    Vec<Vec<crate::text::HorizontalRule>>,
    Vec<crate::table_trace::WithdrawnTable>,
);

/// What one box holds for the rebuild: the page's blocks, words and lines
/// plus the rulings and line indices the box claims. Built once by
/// [`PdfDocument::table_in_region_inputs`] for both the region entry and the
/// TATR-only attempt, so the two cannot drift.
struct RegionTableInputs {
    /// The page's blocks before the learned pass, in page order.
    page_blocks: Vec<Block>,
    /// The page's words and geometry.
    features: crate::learned::features::PageFeatures,
    /// The page's judge lines.
    lines: Vec<crate::learned::judge::Line>,
    /// The rulings the region holds, outer edges included.
    held: Vec<crate::text::Ruling>,
    /// The judge lines the padded box holds.
    region_lines: Vec<usize>,
}

impl PdfDocument {
    /// The engine's table inside a given region: the same rebuild the learned
    /// table pass emits for a chosen region, over the lines a box holds.
    ///
    /// `page` counts from zero; `bbox` is `[x0, y0, x1, y1]` in PDF user space,
    /// y up. A line belongs when at least half of its own area lies inside the
    /// box once grown by [`REGION_PAD_LINES`] median line heights on every side
    /// -- the judge's `held_by` rule, so a word a loose teacher box cuts is
    /// neither halved nor dropped -- and the rulings that intersect the grown
    /// box go in with it, so a ruled table decodes by its lines. No stitch
    /// absorb and no is-table refusal: the caller already said "table here".
    ///
    /// `tableCell` blocks as the pass would emit them, in row-major order, or
    /// an empty vector where fewer than two lines belong, the lines hold no
    /// grid, or the grid is a figure's own labels or drawing fragments (the
    /// region veto, [`region_fragment_veto`]).
    ///
    /// [`REGION_PAD_LINES`]: crate::learned::tables::REGION_PAD_LINES
    #[doc(hidden)]
    pub fn table_in_region(&self, page: usize, bbox: [f64; 4]) -> Result<Vec<Block>> {
        self.table_in_region_inner(page, bbox, None)
    }

    /// [`Self::table_in_region`], with TATR table structure inside the box
    /// (track B, P3).
    ///
    /// A teacher box IS a detector box -- the caller said "table here" -- so
    /// TATR is offered it even under `detector_regions_only`. Where TATR
    /// fires its cells are the table, and where it declines, or the region
    /// fails, the same rebuild [`Self::table_in_region`] runs. A missing
    /// model refuses up front, never a silent base run.
    ///
    /// Hidden rather than published: a caller gets [`Self::extract_blocks`].
    #[doc(hidden)]
    pub fn table_in_region_with_tatr(
        &self,
        page: usize,
        bbox: [f64; 4],
        tatr: &TatrOptions,
    ) -> Result<Vec<Block>> {
        let tatr = self.tatr_context(Some(tatr))?;
        self.table_in_region_inner(page, bbox, tatr.as_ref())
    }

    /// The one region entry: the lines a box holds, rebuilt.
    fn table_in_region_inner(
        &self,
        page: usize,
        bbox: [f64; 4],
        tatr: Option<&crate::learned::tables::TatrContext>,
    ) -> Result<Vec<Block>> {
        let Some(inputs) = self.table_in_region_inputs(page, bbox)? else {
            return Ok(Vec::new());
        };
        let refs: Vec<&Block> = inputs.page_blocks.iter().collect();
        // A teacher box IS a detector box: the caller said "table here".
        let region_tatr = tatr.map(|context| crate::learned::tables::TatrRegion::of(context, true));
        let mut out = crate::learned::tables::rebuild_region(
            &refs,
            &inputs.lines,
            &inputs.features.words,
            &inputs.held,
            &inputs.region_lines,
            None,
            crate::learned::SWITCHES,
            region_tatr.as_ref(),
            // The teacher path takes no options: its cells read as before.
            false,
        );
        // A FIGURE'S FRAGMENTS ARE NOT A TABLE HERE EITHER (#636): the
        // region rebuild grids the same shards the `find` veto refuses, and
        // the balanced tier never reaches that veto. The region's lines then
        // stay text, as they did before the spans joined.
        if Self::region_fragment_veto(&out, &inputs.page_blocks) {
            crate::table_trace::emit_run(
                page,
                &out,
                &inputs.page_blocks,
                Some(crate::table_trace::PageCtx {
                    rulings: &inputs.held,
                    graphics: &[],
                }),
            );
            return Ok(Vec::new());
        }
        for block in out.iter_mut() {
            // The rebuild's own stamp, unless TATR fired inside it (the
            // stamp it left is exact already).
            if block.table_origin == crate::table_trace::TableOrigin::Other {
                block.table_origin = crate::table_trace::TableOrigin::Region;
            }
        }
        for (i, block) in out.iter_mut().enumerate() {
            block.index = i;
        }
        if !self.keep_ligatures {
            for block in out.iter_mut() {
                crate::ligatures::fold_block(block);
            }
        }
        Ok(out)
    }

    /// TATR ONLY inside a box: the fired table cells, or `None` where TATR
    /// declines, fires under coverage, or the region fails (track B, P4).
    ///
    /// The balanced tier's R1 step (1): per kept table region, TATR runs
    /// first, and only on a fire are its cells the region's table -- on any
    /// fallback the tier continues exactly as without TATR (the engine grid,
    /// then [`Self::table_in_region`]). The attempt reads the same rebuild
    /// inputs [`Self::table_in_region`] does (one computation, see
    /// [`Self::table_in_region_inputs`]), the same verdict
    /// ([`crate::learned::tables::tatr_for_region`], with its coverage bar,
    /// gap fill and `tatr | fallback(reason)` trace), and a kept box IS a
    /// detector box, so it is offered even under `detector_regions_only`.
    ///
    /// Only the table cells come back: the region lines no cell claims stay
    /// the balanced assembly's to place (R4), exactly as with an engine
    /// grid. `None` where the box holds fewer than two lines, as
    /// [`Self::table_in_region`] is empty there.
    ///
    /// Behind `ort-layout`: the balanced tier is the only caller, and without
    /// it this is dead code.
    #[cfg(feature = "ort-layout")]
    pub(crate) fn tatr_cells_in_region(
        &self,
        page: usize,
        bbox: [f64; 4],
        tatr: &crate::learned::tables::TatrContext,
    ) -> Result<Option<Vec<Block>>> {
        let Some(inputs) = self.table_in_region_inputs(page, bbox)? else {
            return Ok(None);
        };
        let refs: Vec<&Block> = inputs.page_blocks.iter().collect();
        let valid: Vec<usize> = inputs
            .region_lines
            .iter()
            .copied()
            .filter(|&i| i < inputs.lines.len())
            .collect();
        if valid.is_empty() {
            return Ok(None);
        }
        let (pair_words, _) = crate::learned::tables::words_of_region(
            &refs,
            &inputs.lines,
            &inputs.features.words,
            &valid,
        );
        if pair_words.is_empty() {
            return Ok(None);
        }
        // The trace's page, as `rebuild_region` derives it: the first valid
        // line's block's page.
        let page_number = valid
            .iter()
            .filter_map(|&i| inputs.lines.get(i))
            .filter_map(|line| refs.get(line.block))
            .map(|block| block.page)
            .next()
            .unwrap_or(0);
        let region = crate::learned::tables::TatrRegion::of(tatr, true);
        match crate::learned::tables::tatr_for_region_with_cell_path_words(
            Some(&region),
            &refs,
            &inputs.lines,
            &inputs.features.words,
            &inputs.region_lines,
            &pair_words,
            page_number,
            // The teacher path takes no options: its cells read as before.
            false,
        ) {
            crate::learned::tables::TatrVerdict::Cells(blocks) => {
                let mut cells: Vec<Block> = blocks
                    .into_iter()
                    .filter(|b| matches!(b.kind, crate::BlockKind::TableCell { .. }))
                    .collect();
                if cells.is_empty() {
                    return Ok(None);
                }
                for block in cells.iter_mut() {
                    block.table_origin = crate::table_trace::TableOrigin::Tatr;
                }
                // The same veto as the region rebuild: TATR can fire on a
                // figure's labels too, and its cells read the same way.
                if Self::region_fragment_veto(&cells, &inputs.page_blocks) {
                    crate::table_trace::emit_run(
                        page,
                        &cells,
                        &inputs.page_blocks,
                        Some(crate::table_trace::PageCtx {
                            rulings: &inputs.held,
                            graphics: &[],
                        }),
                    );
                    return Ok(None);
                }
                for (i, block) in cells.iter_mut().enumerate() {
                    block.index = i;
                }
                if !self.keep_ligatures {
                    for block in cells.iter_mut() {
                        crate::ligatures::fold_block(block);
                    }
                }
                Ok(Some(cells))
            }
            crate::learned::tables::TatrVerdict::Fallback(_) => Ok(None),
        }
    }

    /// What one box holds for the rebuild: the one computation behind
    /// [`Self::table_in_region`] and [`Self::tatr_cells_in_region`].
    ///
    /// `None` where the page holds no blocks or fewer than two lines belong
    /// to the box -- the two cases the region entry reads as empty.
    fn table_in_region_inputs(
        &self,
        page: usize,
        bbox: [f64; 4],
    ) -> Result<Option<RegionTableInputs>> {
        let count = self.page_count();
        if page >= count {
            return Err(EngineError::PageOutOfRange { index: page, count });
        }
        let (blocks, _, rulings, _, _) = self.blocks_rules_and_rulings_before_learning(
            crate::TableStrategy::Both,
            false,
            false,
            false,
            None,
        )?;

        let page_blocks: Vec<Block> = blocks.into_iter().filter(|b| b.page == page).collect();
        if page_blocks.is_empty() {
            return Ok(None);
        }
        let (width, height) = self.page_geometry(page)?.effective_dimensions();
        let refs: Vec<&Block> = page_blocks.iter().collect();
        let features = crate::learned::features::page_features(&refs, width, height);
        let lines = crate::learned::judge::lines_of_page(&refs, &features.words, &[]);
        let mut heights: Vec<f64> = lines
            .iter()
            .map(|line| line.bbox[3] - line.bbox[1])
            .filter(|h| h.is_finite() && *h > 0.0)
            .collect();
        heights.sort_by(f64::total_cmp);
        let median = if heights.is_empty() {
            0.0
        } else if heights.len() % 2 == 1 {
            heights[heights.len() / 2]
        } else {
            (heights[heights.len() / 2 - 1] + heights[heights.len() / 2]) / 2.0
        };
        let pad = crate::learned::tables::REGION_PAD_LINES * median;
        let left = bbox[0].min(bbox[2]) - pad;
        let bottom = bbox[1].min(bbox[3]) - pad;
        let right = bbox[0].max(bbox[2]) + pad;
        let top = bbox[1].max(bbox[3]) + pad;
        let padded = [left, bottom, right, top];
        let region_lines = crate::learned::judge::held_by(padded, &lines);
        if region_lines.len() < 2 {
            return Ok(None);
        }
        // THE RULINGS THE REGION HOLDS, outer edges included: a drawn box
        // sits outside the ink it holds -- the right edge of a wide column
        // stands a hundred points from the words -- so a lattice the padded box
        // touches goes in whole, and where no lattice touches, the rulings
        // that intersect the box go in for the decoder.
        let page_rulings: &[crate::text::Ruling] =
            rulings.get(page).map(Vec::as_slice).unwrap_or(&[]);
        let held: Vec<crate::text::Ruling> = {
            let lattices = crate::learned::lattice::lattices(page_rulings);
            let touched: Vec<[f64; 4]> = lattices
                .iter()
                .map(|lattice| lattice.bbox())
                .filter(|bbox| {
                    bbox[0] <= padded[2]
                        && bbox[2] >= padded[0]
                        && bbox[1] <= padded[3]
                        && bbox[3] >= padded[1]
                })
                .collect();
            if touched.is_empty() {
                page_rulings
                    .iter()
                    .copied()
                    .filter(|r| {
                        r.x0 <= padded[2]
                            && r.x1 >= padded[0]
                            && r.y0 <= padded[3]
                            && r.y1 >= padded[1]
                    })
                    .collect()
            } else {
                page_rulings
                    .iter()
                    .copied()
                    .filter(|r| {
                        touched.iter().any(|bbox| {
                            // Two points of slack: a ruling that draws the
                            // lattice's own edge snaps inside its bbox by the
                            // lattice tolerance, so the edge sits a quarter
                            // point outside the box it draws.
                            r.x0 <= bbox[2] + 2.0
                                && r.x1 >= bbox[0] - 2.0
                                && r.y0 <= bbox[3] + 2.0
                                && r.y1 >= bbox[1] - 2.0
                        })
                    })
                    .collect()
            }
        };
        Ok(Some(RegionTableInputs {
            page_blocks,
            features,
            lines,
            held,
            region_lines,
        }))
    }

    /// Whether rebuilt region cells are a figure's own labels or drawing
    /// fragments rather than the table the region claims (#636).
    ///
    /// The `find` veto never sees these cells -- the balanced tier rebuilds the
    /// region instead -- so the verdict is read off the cells themselves, with
    /// the page's blocks for the caption half. It fires in two shapes, and a
    /// genuine word or number table fails both:
    ///
    /// - FIGURE LABELS: the median cell holds at most
    ///   [`FRAGMENT_CELL_MEDIAN`](crate::tables::FRAGMENT_CELL_MEDIAN)
    ///   alphanumeric characters, no cell holds a word (an alphabetic run of
    ///   four or more), and a figure-caption marker stands near the grid. A word
    ///   table under a caption keeps its table through its words; emptiness is
    ///   not measured here, because the rebuild merges corridors and a merged
    ///   grid reads dense where the `find` grid reads holes.
    /// - DRAWING SHEET: the grid carries two or more "-continued" panel labels,
    ///   all in its first row, and it is sparse or short (over a third of its
    ///   positions empty, or the median cell at most twice the fragment median).
    ///   A "continued" note in a body's status column is data: it never stands
    ///   twice in the first row.
    fn region_fragment_veto(cells: &[Block], page: &[Block]) -> bool {
        let Some(grid) = Self::fragment_grid_stats(cells) else {
            return false;
        };
        if grid.continued_hits >= 2
            && grid.continued_in_first_row
            && (grid.sparse || grid.median <= 2 * crate::tables::FRAGMENT_CELL_MEDIAN)
        {
            return true;
        }
        if !grid.short || grid.has_word {
            return false;
        }
        Self::caption_near_grid(
            &grid,
            page.iter().map(|block| (block.text.as_str(), block.bbox)),
        )
    }

    /// Read a run of table cells the way the veto sees it: positions against
    /// spans, the median cell length, panel labels, words.
    fn fragment_grid_stats(cells: &[Block]) -> Option<FragmentGrid> {
        struct RegionCell<'a> {
            row: u16,
            column: u16,
            row_span: u16,
            column_span: u16,
            text: &'a str,
            bbox: [f64; 4],
        }
        let grid: Vec<RegionCell<'_>> = cells
            .iter()
            .filter_map(|block| match block.kind {
                crate::BlockKind::TableCell {
                    row,
                    column,
                    row_span,
                    column_span,
                    ..
                } => Some(RegionCell {
                    row,
                    column,
                    row_span,
                    column_span,
                    text: block.text.as_str(),
                    bbox: block.bbox,
                }),
                _ => None,
            })
            .collect();
        if grid.is_empty() {
            return None;
        }
        let rows = grid
            .iter()
            .map(|cell| cell.row as usize + cell.row_span.max(1) as usize)
            .max()
            .unwrap_or(0);
        let columns = grid
            .iter()
            .map(|cell| cell.column as usize + cell.column_span as usize)
            .max()
            .unwrap_or(0);
        if rows == 0 || columns == 0 {
            return None;
        }
        let positions = rows * columns;
        let filled: usize = grid
            .iter()
            .map(|cell| cell.column_span as usize * cell.row_span.max(1) as usize)
            .sum();
        let sparse = positions > 0 && filled < positions && (positions - filled) * 3 > positions;
        let mut lens: Vec<usize> = grid
            .iter()
            .map(|cell| cell.text.chars().filter(|c| c.is_alphanumeric()).count())
            .collect();
        lens.sort_unstable();
        let mid = lens.len() / 2;
        // The same median the `find` veto holds its cells to.
        let short = if lens.len() % 2 == 1 {
            lens[mid] <= crate::tables::FRAGMENT_CELL_MEDIAN
        } else {
            lens[mid - 1] + lens[mid] <= 2 * crate::tables::FRAGMENT_CELL_MEDIAN
        };
        let median = lens[mid];
        let has_word = grid.iter().any(|cell| {
            let mut run = 0usize;
            for c in cell.text.chars() {
                if c.is_alphabetic() {
                    run += 1;
                    if run >= 4 {
                        return true;
                    }
                } else {
                    run = 0;
                }
            }
            false
        });
        let continued_hits: usize = grid
            .iter()
            .map(|cell| cell.text.to_ascii_lowercase().matches("continu").count())
            .sum();
        let continued_in_first_row = grid
            .iter()
            .all(|cell| !cell.text.to_ascii_lowercase().contains("continu") || cell.row == 0);
        let mut bbox = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        let mut heights: Vec<f64> = Vec::with_capacity(grid.len());
        for cell in &grid {
            bbox[0] = bbox[0].min(cell.bbox[0]);
            bbox[2] = bbox[2].max(cell.bbox[2]);
            bbox[1] = bbox[1].min(cell.bbox[1]);
            bbox[3] = bbox[3].max(cell.bbox[3]);
            heights.push(cell.bbox[3] - cell.bbox[1]);
        }
        if !(bbox[0].is_finite() && bbox[2] > bbox[0] && bbox[3] >= bbox[1]) {
            return None;
        }
        heights.sort_by(f64::total_cmp);
        let em = heights[heights.len() / 2];
        if !(em.is_finite() && em > 0.0) {
            return None;
        }
        Some(FragmentGrid {
            sparse,
            short,
            median,
            continued_hits,
            continued_in_first_row,
            has_word,
            bbox,
            em,
        })
    }

    /// Whether a figure-caption marker stands near a vetoed-shape grid: overlapping
    /// it sideways within twelve ems above or below it.
    fn caption_near_grid<'a>(
        grid: &FragmentGrid,
        mut page: impl Iterator<Item = (&'a str, [f64; 4])>,
    ) -> bool {
        let near = crate::tables::FIGURE_CAPTION_NEAR_EM * grid.em;
        page.any(|(text, bbox)| {
            if !crate::tables::is_figure_caption(text) {
                return false;
            }
            let [left, lo, right, hi] = bbox;
            if !(left.is_finite() && right > left && hi >= lo) {
                return false;
            }
            if right <= grid.bbox[0] || left >= grid.bbox[2] {
                return false;
            }
            let gap = if hi < grid.bbox[1] {
                grid.bbox[1] - hi
            } else if lo > grid.bbox[3] {
                lo - grid.bbox[3]
            } else {
                0.0
            };
            gap <= near
        })
    }

    /// Release vetoed tables' cells back to paragraphs (#636).
    ///
    /// The builder veto refuses fragment grids, but the learned pass rebuilds
    /// forced regions from the same lines, so a vetoed shape can still come out
    /// as `tableCell`s. Every finished table is therefore held against the region
    /// veto once more here, page by page; a vetoed run's cells become paragraphs
    /// -- kind, record and confidence cleared the way `peel_table_edges` leaves
    /// them -- and the text stays on the page, as in base.
    fn demote_fragment_tables(blocks: &mut [Block]) {
        let mut start: Option<usize> = None;
        let mut runs: Vec<(usize, usize)> = Vec::new();
        for (i, block) in blocks.iter().enumerate() {
            if matches!(block.kind, crate::BlockKind::TableCell { .. }) {
                start.get_or_insert(i);
            } else if let Some(first) = start.take() {
                runs.push((first, i));
            }
        }
        if let Some(first) = start.take() {
            runs.push((first, blocks.len()));
        }
        for (first, end) in runs {
            let mut page_first = first;
            while page_first < end {
                let page = blocks[page_first].page;
                let mut page_end = page_first + 1;
                while page_end < end && blocks[page_end].page == page {
                    page_end += 1;
                }
                let veto = match Self::fragment_grid_stats(&blocks[page_first..page_end]) {
                    Some(grid) => {
                        (grid.continued_hits >= 2
                            && grid.continued_in_first_row
                            && (grid.sparse
                                || grid.median <= 2 * crate::tables::FRAGMENT_CELL_MEDIAN))
                            || (grid.sparse
                                && grid.short
                                && !grid.has_word
                                && Self::caption_near_grid(
                                    &grid,
                                    blocks
                                        .iter()
                                        .filter(|block| block.page == page)
                                        .map(|block| (block.text.as_str(), block.bbox)),
                                ))
                    }
                    None => false,
                };
                if veto {
                    // The trace's line for the removed candidate, before its
                    // cells go back to paragraphs. No page graphics in this
                    // scope: the ruling and drawing fields print `-`.
                    crate::table_trace::emit_run(page, &blocks[page_first..page_end], blocks, None);
                    for block in &mut blocks[page_first..page_end] {
                        block.kind = crate::BlockKind::Paragraph;
                        block.grid = None;
                        block.confidence = None;
                        block.continues_previous = false;
                    }
                }
                page_first = page_end;
            }
        }
    }

    /// How much OCR one page needs: born-digital text, a scan, or text around
    /// image regions that carry no text of their own.
    ///
    /// See [`crate::ocr_need::OcrNeed`] for what each verdict means and the
    /// thresholds behind it. A page index that does not exist reports
    /// [`crate::ocr_need::OcrNeed::None`].
    pub fn page_needs_ocr(&self, index: usize) -> crate::ocr_need::OcrNeed {
        let pages = self.pdf.pages();
        let page = match pages.get(index) {
            Some(page) => page,
            None => return crate::ocr_need::OcrNeed::None,
        };
        let (width, height) = page.render_dimensions();
        let (settings, slot) = Self::with_limit_collector(&self.text_extraction_settings());
        let mut device = crate::ocr_need::OcrSurveyDevice::new(f64::from(width), f64::from(height));
        let mut ctx = Self::create_context_with_settings(page, settings);
        interpret_page(page, &mut ctx, &mut device);
        if Self::check_limit_slot(&slot).is_err() {
            return crate::ocr_need::OcrNeed::None;
        }
        device.decide()
    }

    /// Which pages are worth sending to a vision model, and why (#624).
    ///
    /// One entry per page, in page order. What sets a flag is documented on
    /// [`VisionAdvice`]; today that is the scanned page and nothing else, and the
    /// two reasons that are not wired say so in their own documentation rather
    /// than quietly reading false.
    ///
    /// A PAGE THAT DRAWS NOTHING IS NOT A SCAN. An empty page yields no text
    /// either, and sending it to a vision model costs a call and returns
    /// nothing; so the flag asks for ink as well as for silence. Ink is any
    /// image, glyph or path the page draws, counted by the interpreter rather
    /// than guessed from the resources: a page whose `/XObject` names an image
    /// nothing ever draws is not a scan of anything.
    pub fn vision_advice(&self) -> Result<Vec<VisionAdvice>> {
        let pages = self.pdf.pages();
        let mut out: Vec<VisionAdvice> = Vec::with_capacity(pages.len());
        for (index, page) in pages.iter().enumerate() {
            let (settings, slot) = Self::with_limit_collector(&self.text_extraction_settings());
            let mut device = InkDevice::default();
            let mut ctx = Self::create_context_with_settings(page, settings);
            interpret_page(page, &mut ctx, &mut device);
            Self::check_limit_slot(&slot)?;
            let text = self.extract_page_blocks(index, 0).map(|blocks| {
                blocks
                    .iter()
                    .map(|b| b.text.trim().chars().count())
                    .sum::<usize>()
            })?;
            out.push(VisionAdvice {
                no_text_layer: text < VISION_MIN_CHARACTERS && device.drew_anything(),
                ..VisionAdvice::default()
            });
        }
        Ok(out)
    }

    /// The blocks exactly as the heuristics leave them, before any learned
    /// decision: every page, every kind, in reading order.
    ///
    /// For `feature_dump` and nothing else. A model trained on blocks the
    /// models had already touched would learn its own output and read it as
    /// progress, so the training records are made from this state, and each
    /// record says so (`refine: "off"`). Every table stands here, whatever
    /// strategy a later call asks for: the dump reads the heuristics whole.
    #[doc(hidden)]
    pub fn extract_blocks_before_learning(&self) -> Result<Vec<Block>> {
        let (blocks, _, _, _, _) = self.blocks_rules_and_rulings_before_learning(
            crate::TableStrategy::Both,
            false,
            false,
            false,
            None,
        )?;
        Ok(blocks)
    }

    /// [`Self::extract_blocks_before_learning`], and the rules each page drew,
    /// from the same single pass (#598, `pdfluent.features/4`).
    ///
    /// For `feature_dump`: its records are made from the blocks before any
    /// learned decision, and a ruled table's lines belong beside the very
    /// blocks they were read with -- a second interpretation of the page for
    /// the lines alone is a second chance for the two readings to disagree
    /// (T4, #600). One `Vec` of [`crate::text::HorizontalRule`] per page, in
    /// page order.
    #[doc(hidden)]
    pub fn extract_blocks_and_rules_before_learning(
        &self,
    ) -> Result<(Vec<Block>, Vec<Vec<crate::text::HorizontalRule>>)> {
        self.blocks_and_rules_before_learning()
    }

    /// One page's lattice rulings and drawn-cluster boxes, in the frame of
    /// its blocks: what rule A reads to count the drawings inside a table
    /// (#636). A pass over that one page.
    #[cfg(feature = "ort-layout")]
    pub(crate) fn page_rulings_and_graphics(
        &self,
        page: usize,
    ) -> Result<(Vec<crate::text::Ruling>, Vec<[f64; 4]>)> {
        let count = self.page_count();
        let pages = self.pdf.pages();
        let Some(source) = pages.get(page) else {
            return Err(EngineError::PageOutOfRange { index: page, count });
        };
        let (settings, slot) = Self::with_limit_collector(&self.text_extraction_settings());
        let mut device = TextExtractionDevice::new();
        let mut ctx = Self::create_context_with_settings(source, settings);
        interpret_page(source, &mut ctx, &mut device);
        Self::check_limit_slot(&slot)?;
        let rulings = device.take_rulings();
        let (width, height) = source.render_dimensions();
        let graphics = device.take_graphics(f64::from(width), f64::from(height));
        Ok((rulings, graphics))
    }

    /// Every page's figure boxes, `[left, bottom, right, top]` in the frame of
    /// the blocks (#636): the images it places, in drawing order, then the
    /// figures it draws as paths (see `TextExtractionDevice::take_graphics`).
    ///
    /// A pass of its own over the pages, and only the feature dump asks for
    /// it: a caption model has to see where the pictures are, and nothing on
    /// the extraction path reads them yet. Capped at
    /// [`MAX_GRAPHICS_SCAN_PAGES`](crate::markdown_images::MAX_GRAPHICS_SCAN_PAGES)
    /// pages: one entry per page either way, and pages past the cap report
    /// no figures.
    #[doc(hidden)]
    pub fn extract_image_boxes(&self) -> Result<Vec<Vec<[f64; 4]>>> {
        let mut out = Vec::new();
        for (number, page) in self.pdf.pages().iter().enumerate() {
            if number >= markdown_images::MAX_GRAPHICS_SCAN_PAGES {
                out.push(Vec::new());
                continue;
            }
            let (settings, slot) = Self::with_limit_collector(&self.text_extraction_settings());
            let mut device = TextExtractionDevice::new();
            let mut ctx = Self::create_context_with_settings(page, settings);
            interpret_page(page, &mut ctx, &mut device);
            Self::check_limit_slot(&slot)?;
            let (w, h) = page.render_dimensions();
            let mut boxes = device.take_images();
            boxes.extend(device.take_graphics(f64::from(w), f64::from(h)));
            out.push(boxes);
        }
        Ok(out)
    }

    /// The boxes one page places its images into, in drawing order (#658).
    ///
    /// The single-page half of [`Self::extract_image_boxes`], without the
    /// drawn clusters and without the page cap: bindings list a page's own
    /// pictures through this. A page past the last one reports none.
    #[doc(hidden)]
    pub fn page_image_boxes(&self, index: usize) -> Result<Vec<[f64; 4]>> {
        let pages = self.pdf.pages();
        let Some(page) = pages.get(index) else {
            return Ok(Vec::new());
        };
        let (settings, slot) = Self::with_limit_collector(&self.text_extraction_settings());
        let mut device = TextExtractionDevice::new();
        let mut ctx = Self::create_context_with_settings(page, settings);
        interpret_page(page, &mut ctx, &mut device);
        Self::check_limit_slot(&slot)?;
        Ok(device.take_images())
    }

    /// Every figure of the window, in reading order, with rendered pixels
    /// (#658).
    ///
    /// Preview: output quality is still improving and the API may change in a minor release.
    ///
    /// The [`BlockKind::Figure`](crate::BlockKind::Figure) blocks
    /// [`Self::extract_blocks`] lists under these options, each with its box
    /// rendered at 150 dots per inch and encoded as PNG. One render per page
    /// that carries figures. Names are
    /// [`figure_name`](crate::markdown_images::figure_name)s, the same ones
    /// [`Self::extract_markdown`] references.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pdf_engine::{ExtractOptions, PdfDocument};
    ///
    /// let document = PdfDocument::open(std::fs::read("report.pdf").unwrap()).unwrap();
    /// let options = ExtractOptions::default();
    /// for figure in document.extract_figures(&options).unwrap() {
    ///     std::fs::write(&figure.name, &figure.png).unwrap();
    /// }
    /// ```
    pub fn extract_figures(&self, options: &ExtractOptions) -> Result<Vec<Figure>> {
        use std::collections::BTreeMap;
        let blocks = self.extract_blocks(options)?;
        let mut pages: BTreeMap<usize, Vec<[f64; 4]>> = BTreeMap::new();
        for block in &blocks {
            if matches!(block.kind, crate::BlockKind::Figure) {
                pages.entry(block.page).or_default().push(block.bbox);
            }
        }
        let mut out = Vec::new();
        for (page, boxes) in &pages {
            let renders = markdown_images::render_crops(self, *page, boxes)?;
            for (ordinal, (bbox, png)) in boxes.iter().zip(renders).enumerate() {
                out.push(Figure {
                    page: *page,
                    bbox: *bbox,
                    name: markdown_images::figure_name(*page, ordinal),
                    png,
                });
            }
        }
        Ok(out)
    }

    /// The same pass, keeping the rules each page drew (#600).
    fn blocks_and_rules_before_learning(
        &self,
    ) -> Result<(Vec<Block>, Vec<Vec<crate::text::HorizontalRule>>)> {
        let (blocks, ruled, _, _, _) = self.blocks_rules_and_rulings_before_learning(
            crate::TableStrategy::Both,
            false,
            false,
            false,
            None,
        )?;
        Ok((blocks, ruled))
    }

    /// The thin lines and rectangle edges page `index` draws, as the table
    /// lattice reads them (#574). For the table proposers and their dumps.
    #[doc(hidden)]
    pub fn page_rulings(&self, index: usize) -> Result<Vec<crate::text::Ruling>> {
        let pages = self.pdf.pages();
        let Some(page) = pages.get(index) else {
            return Ok(Vec::new());
        };
        let (settings, slot) = Self::with_limit_collector(&self.text_extraction_settings());
        let mut device = TextExtractionDevice::new();
        let mut ctx = Self::create_context_with_settings(page, settings);
        interpret_page(page, &mut ctx, &mut device);
        Self::check_limit_slot(&slot)?;
        Ok(device.take_rulings())
    }

    /// The rulings and the solid fills page `index` draws, in one reading
    /// (#574): the line network's vector inputs -- a rule over a line, an
    /// underline, a box around it, shading behind it -- for the dump.
    #[doc(hidden)]
    #[allow(clippy::type_complexity)]
    pub fn page_rulings_and_fills(
        &self,
        index: usize,
    ) -> Result<(Vec<crate::text::Ruling>, Vec<[f64; 4]>)> {
        let pages = self.pdf.pages();
        let Some(page) = pages.get(index) else {
            return Ok((Vec::new(), Vec::new()));
        };
        let (settings, slot) = Self::with_limit_collector(&self.text_extraction_settings());
        let mut device = TextExtractionDevice::new();
        let mut ctx = Self::create_context_with_settings(page, settings);
        interpret_page(page, &mut ctx, &mut device);
        Self::check_limit_slot(&slot)?;
        Ok((device.take_rulings(), device.take_fills()))
    }

    /// The rectangle `margins` leave on a page `width` by `height` points:
    /// `[left, bottom, width - right, height - top]`, y up like the blocks.
    /// Read per page, because every page brings its own size.
    fn clip_rect(margins: &ClipMargins, width: f64, height: f64) -> [f64; 4] {
        let [left, top, right, bottom] = margins.as_array();
        [left, bottom, width - right, height - top]
    }

    /// The blocks in raw content-stream order: every page's runs grouped by
    /// content-order proximity, in the order the pages draw them (#643).
    fn extract_blocks_in_raw_order(&self, options: &ExtractOptions) -> Result<Vec<Block>> {
        let pages = self.pdf.pages();
        let mut blocks: Vec<Block> = Vec::new();
        for (index, page) in pages.iter().enumerate() {
            let (settings, slot) = Self::with_limit_collector(&self.text_extraction_settings());
            let mut device = TextExtractionDevice::new()
                .raw_runs(true)
                .actual_text(options.actual_text);
            let mut ctx = Self::create_context_with_settings(page, settings);
            interpret_page(page, &mut ctx, &mut device);
            Self::check_limit_slot(&slot)?;
            if let Some(margins) = &options.clip_margins {
                let (width, height) = page.render_dimensions();
                device.clip_spans(Self::clip_rect(
                    margins,
                    f64::from(width),
                    f64::from(height),
                ));
            }
            blocks.extend(device.into_raw_blocks(index, blocks.len()));
        }
        let mut blocks = crate::blocks::window(blocks, options);
        // Folded after the grouping, like `extract_blocks` above.
        if !self.keep_ligatures {
            for block in &mut blocks {
                crate::ligatures::fold_block(block);
            }
        }
        Ok(blocks)
    }

    /// The same pass, keeping every page's rules, its lattice rulings --
    /// the thin lines and rectangle edges the table lattice reads (#574) --
    /// and its figure boxes: the placed images, then the drawn clusters.
    #[allow(clippy::type_complexity)]
    fn blocks_rules_and_rulings_before_learning(
        &self,
        table_strategy: crate::TableStrategy,
        // Whether spans carry the document's font names: ExtractOptions
        // says, and the paths without options answer false (#643).
        document_font_names: bool,
        // Whether cell words join on the cell path: ExtractOptions says,
        // and the paths without options answer false (#659).
        cell_path_words: bool,
        // Whether `/ActualText` replacements apply: ExtractOptions says,
        // and the paths without options answer false (#663).
        actual_text: bool,
        // Clip margins in points, or `None` for the whole page:
        // ExtractOptions says, and the paths without options answer None.
        clip_margins: Option<ClipMargins>,
    ) -> Result<(
        Vec<Block>,
        Vec<Vec<crate::text::HorizontalRule>>,
        Vec<Vec<crate::text::Ruling>>,
        Vec<Vec<[f64; 4]>>,
        Vec<Vec<[f64; 4]>>,
    )> {
        let pages = self.pdf.pages();
        let mut images: Vec<Vec<[f64; 4]>> = Vec::new();
        let mut graphics: Vec<Vec<[f64; 4]>> = Vec::new();
        let mut blocks: Vec<Block> = Vec::new();
        let mut ruled: Vec<Vec<crate::text::HorizontalRule>> = Vec::new();
        let mut lattices: Vec<Vec<crate::text::Ruling>> = Vec::new();

        for (index, page) in pages.iter().enumerate() {
            let (settings, slot) = Self::with_limit_collector(&self.text_extraction_settings());
            let mut device = TextExtractionDevice::new()
                .document_font_names(document_font_names)
                .actual_text(actual_text);
            let mut ctx = Self::create_context_with_settings(page, settings);
            interpret_page(page, &mut ctx, &mut device);
            Self::check_limit_slot(&slot)?;
            lattices.push(device.take_rulings());
            let figures = device.take_images();
            images.push(figures);
            let (width, height) = page.render_dimensions();
            graphics.push(device.take_graphics(f64::from(width), f64::from(height)));
            // BEFORE segmentation: content the margins cover is never cut
            // into lines, so a clipped line cannot pull its neighbours into
            // a block of its own.
            if let Some(margins) = &clip_margins {
                device.clip_spans(Self::clip_rect(
                    margins,
                    f64::from(width),
                    f64::from(height),
                ));
            }
            let (page_blocks, page_rules) = device.into_paragraph_blocks_and_rules_with_strategy(
                index,
                blocks.len(),
                table_strategy,
                cell_path_words,
            );
            blocks.extend(page_blocks);
            ruled.push(page_rules);
        }

        // BEFORE the headings, because a running head is set in a face of its
        // own as often as not: a classifier that saw it as body first would
        // spend a heading level on the line every page repeats (#571).
        crate::furniture::mark(&mut blocks);
        // AFTER every page, because the statistics are the DOCUMENT's: a size
        // is a heading against what the rest of the document is set in, and a
        // page measured on its own would call the one large line on a table of
        // contents a title and the body of page two a heading (#568).
        crate::headings::classify(&mut blocks);
        // AFTER the headings: stacked headings arrive on the same wide pitch
        // as a double-spaced run, and joining first turns two headings into
        // one paragraph (#636).
        let blocks = crate::text::join_double_spaced_runs(blocks, 0);
        // LAST, because it is the order the rest of them are read in: the
        // columns of each page, the pages in turn, the paragraph the page
        // break cut in two, and the furniture after the body (#571).
        let blocks = crate::reading_order::arrange(blocks, 0);
        Ok((blocks, ruled, lattices, images, graphics))
    }

    /// The last step before the window: the over-image drop where asked,
    /// then the figures beside the text they stand with.
    ///
    /// Which pages carry text is read BEFORE the drop: a page whose only
    /// line stood over its picture still holds its figure after the line
    /// goes. The merge renumbers every block in place.
    fn finish_blocks_with_figures(
        blocks: Vec<Block>,
        images: &[Vec<[f64; 4]>],
        graphics: &[Vec<[f64; 4]>],
        options: &ExtractOptions,
    ) -> Vec<Block> {
        let pages = images.len().max(graphics.len());
        let mut page_has_text = vec![false; pages];
        for block in &blocks {
            if let Some(slot) = page_has_text.get_mut(block.page) {
                *slot = true;
            }
        }
        let blocks = if options.drop_text_over_images {
            markdown_images::drop_text_over_images(blocks, images)
        } else {
            blocks
        };
        markdown_images::merge_figures(blocks, images, graphics, &page_has_text)
    }

    /// The document as markdown, in reading order (#573).
    ///
    /// Preview: output quality is still improving and the API may change in a minor release.
    ///
    /// [`Self::extract_blocks`] and [`crate::to_markdown`] in one call, which
    /// is the whole of it: headings become `#` to `###`, list items `-` and
    /// `1.`, a table a pipe table or an HTML one where markdown cannot hold
    /// its shape, and a change of page an HTML comment. The running heads are
    /// not in it -- they are in the blocks, where a caller that wants them
    /// reads them, or the caller sets [`ExtractOptions::include_headers`]
    /// and [`ExtractOptions::include_footers`] and they are written as plain
    /// paragraphs at their reading-order position.
    /// [`ExtractOptions::page_separators`] drops the page comment instead.
    /// `docs/STRUCTURED_EXTRACTION.md` is the reader's account of every one
    /// of those rules.
    ///
    /// Monospace runs become code unless [`ExtractOptions::code`] is false: an
    /// inline span in a paragraph or a list item, a fenced block for a
    ///
    /// A `Result` and not a bare `String` for the reason
    /// [`Self::extract_blocks`] is one: the limits a caller set on an
    /// untrusted file are the whole point of setting them, and a convenience
    /// method that swallowed a [`crate::limits::LimitError`] would hand back a
    /// silently short document to the caller who asked to be told.
    ///
    /// With [`Quality::Balanced`](crate::Quality) this runs the balanced tier
    /// instead of the above (#641): the layout model regions each page and the
    /// engine's own text fills the regions. It needs the `ort-layout` cargo
    /// feature and the weights from an explicit `pdfluent models install
    /// layout`; without the feature it fails with `MODEL_NOT_AVAILABLE`
    /// naming the feature and the install command, and with the feature but
    /// without the files, with `MODEL_NOT_AVAILABLE` naming the path looked
    /// at and the install command. [`ExtractOptions::model_dir`] points the
    /// tier at a scratch directory, [`ExtractOptions::keep_furniture`] keeps
    /// running heads as paragraphs.
    pub fn extract_markdown(&self, options: &ExtractOptions) -> Result<String> {
        if matches!(options.quality, crate::Quality::Balanced) {
            return crate::model::balanced_markdown_for(self, options);
        }
        let mut blocks = self.extract_blocks(options)?;
        // Embeds and written files resolve each figure's reference text
        // against this document before the serialiser runs; markers need no
        // document and are rendered as listed.
        if options.include_figures && (options.embed_figures || options.figure_image_dir.is_some())
        {
            markdown_images::resolve_figure_texts(self, &mut blocks, options)?;
        }
        let markdown = crate::markdown::MarkdownOptions::from(options);
        Ok(crate::markdown::to_markdown_with_options(
            &blocks, &markdown,
        ))
    }

    /// The document as a Docling `DoclingDocument`, in reading order (#629).
    ///
    /// Preview: output quality is still improving and the API may change in a minor release.
    ///
    /// [`Self::extract_blocks`] and [`crate::to_docling_json`] in one call, with
    /// every page of the window listed under `pages` at its size. `name` is
    /// `DoclingDocument.name`, which Docling fills with the file's stem. What
    /// maps onto Docling's schema and what does not is written out in
    /// [`crate::docling`].
    pub fn extract_docling(&self, name: &str, options: &ExtractOptions) -> Result<String> {
        let blocks = self.extract_blocks(options)?;
        let mut pages = Vec::new();
        for page in (0..self.page_count()).filter(|page| options.holds(*page)) {
            // The frame the blocks are measured in: the crop box, turned by
            // the page's `/Rotate` (`initial_transform`).
            let (width, height) = self.page_geometry(page)?.effective_dimensions();
            pages.push(crate::docling::DoclingPage {
                page,
                width,
                height,
            });
        }
        Ok(crate::docling::to_docling_json(name, &pages, &blocks))
    }

    /// Extract text values from AcroForm fields, including push-button captions.
    ///
    /// Returns a single string concatenating all non-empty field values separated
    /// by newlines. Useful when the document stores its readable content in form
    /// field values rather than (or in addition to) page content streams.
    pub fn extract_acroform_text(&self) -> String {
        let Some(tree) = parse_acroform(&self.pdf) else {
            return String::new();
        };
        let mut parts: Vec<String> = Vec::new();
        for id in tree.all_ids() {
            let node = tree.get(id);
            if node.children.is_empty() {
                // Terminal (widget) — collect text-like values.
                let value_str = match &node.value {
                    Some(FieldValue::Text(s)) if !s.is_empty() => Some(s.clone()),
                    Some(FieldValue::StringArray(arr)) => {
                        let joined = arr
                            .iter()
                            .filter(|s| !s.is_empty())
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ");
                        if joined.is_empty() {
                            None
                        } else {
                            Some(joined)
                        }
                    }
                    _ => None,
                };
                let button_caption =
                    value_str.is_none() && tree.effective_field_type(id) == Some(FieldType::Button);
                let extracted = value_str.or_else(|| {
                    button_caption.then(|| {
                        node.mk
                            .as_ref()
                            .and_then(|mk| mk.caption.as_ref())
                            .filter(|caption| !caption.is_empty())
                            .cloned()
                    })?
                });
                if let Some(s) = extracted {
                    parts.push(s);
                }
            }
        }
        let joined = parts.join("\n");
        if self.keep_ligatures {
            return joined;
        }
        // A field value is a string the author stored rather than glyphs
        // anybody drew, but a searcher cannot tell the two apart: it is the
        // same `oﬃce` and the same failed search (codex, #437). Folding here
        // rather than in `extract_all_text` covers this method's own callers
        // and the zero-page document, whose whole text is this string.
        crate::ligatures::fold(&joined).into_owned()
    }

    /// Extract all text from the document: page content streams plus AcroForm
    /// field values.  Mirrors pdftotext behaviour.
    pub fn extract_all_text(&self) -> String {
        let pages = self.pdf.pages();
        let mut settings = self.text_extraction_settings();
        let mut page_texts = Vec::with_capacity(pages.len());
        for page in pages.iter() {
            let (page_text, next_settings) =
                Self::extract_text_with_settings(page, settings, self.keep_ligatures);
            settings = next_settings;
            page_texts.push(page_text);
        }

        // AcroForm values ride on the last page rather than after the last
        // page separator: they belong to no page of their own, and a trailing
        // block behind the final form feed would read as one more page to
        // every consumer that counts them.
        let acroform = self.extract_acroform_text();
        if !acroform.is_empty() {
            match page_texts.last_mut() {
                Some(last) => {
                    if !last.is_empty() && !last.ends_with('\n') {
                        last.push('\n');
                    }
                    last.push_str(&acroform);
                }
                None => return acroform,
            }
        }

        join_page_texts(page_texts.iter().map(String::as_str))
    }

    /// Simple text search: returns page indices containing the query string.
    ///
    /// The query is folded the same way the pages are, so a word carrying a
    /// ligature code point -- pasted out of a content stream, or typed from a
    /// file that maps its glyphs to U+FB00-FB06 -- still finds the page it
    /// came from.
    pub fn search_text(&self, query: &str) -> Vec<usize> {
        let pages = self.pdf.pages();
        let query_lower = query.to_lowercase();
        // Fold the needle exactly when the haystack is folded, or the one
        // query that most needs the fold -- `oﬃce`, carrying the code point
        // the file declared -- is the one query that can never match
        // (codex, #437).
        let query_lower = if self.keep_ligatures {
            query_lower
        } else {
            crate::ligatures::fold(&query_lower).into_owned()
        };
        #[cfg(feature = "parallel")]
        let page_contains = |i: usize| -> Option<usize> {
            let page = &pages[i];
            let (text, _) = Self::extract_text_with_settings(
                page,
                self.text_extraction_settings(),
                self.keep_ligatures,
            );
            if text.to_lowercase().contains(&query_lower) {
                Some(i)
            } else {
                None
            }
        };
        #[cfg(feature = "parallel")]
        return (0..pages.len())
            .into_par_iter()
            .filter_map(page_contains)
            .collect();
        #[cfg(not(feature = "parallel"))]
        {
            let mut settings = self.text_extraction_settings();
            let mut hits = Vec::new();
            for (i, page) in pages.iter().enumerate() {
                let (text, next_settings) =
                    Self::extract_text_with_settings(page, settings, self.keep_ligatures);
                settings = next_settings;
                if text.to_lowercase().contains(&query_lower) {
                    hits.push(i);
                }
            }
            hits
        }
    }

    /// Extract document metadata.
    pub fn info(&self) -> DocumentInfo {
        let meta = self.pdf.metadata();
        DocumentInfo {
            title: meta.title.as_ref().map(|b| bytes_to_string(b)),
            author: meta.author.as_ref().map(|b| bytes_to_string(b)),
            subject: meta.subject.as_ref().map(|b| bytes_to_string(b)),
            keywords: meta.keywords.as_ref().map(|b| bytes_to_string(b)),
            creator: meta.creator.as_ref().map(|b| bytes_to_string(b)),
            producer: meta.producer.as_ref().map(|b| bytes_to_string(b)),
        }
    }

    /// Extract document outline / bookmarks.
    pub fn bookmarks(&self) -> Vec<BookmarkItem> {
        let xref = self.pdf.xref();
        let root_id = xref.root_id();
        let catalog: Dict<'_> = match xref.get(root_id) {
            Some(d) => d,
            None => return Vec::new(),
        };

        let outlines: Dict<'_> = match catalog.get(OUTLINES) {
            Some(d) => d,
            None => return Vec::new(),
        };

        let first: Dict<'_> = match outlines.get(FIRST) {
            Some(d) => d,
            None => return Vec::new(),
        };

        // `visited` breaks cyclic /Next loops and /First back-references;
        // `depth` bounds deep nesting so a malformed outline cannot hang or
        // overflow the stack.
        let mut visited = BTreeSet::new();
        let pages = self.page_object_ids();
        parse_outline_items(&first, 0, &mut visited, &pages)
    }

    /// Every `/Link` annotation on one page, in `/Annots` order.
    ///
    /// A link without a readable `/Rect` is skipped: with no region it
    /// cannot be rendered or cited. All other annots on the page are
    /// skipped too; only `/Link` is a link.
    pub fn page_links(&self, index: usize) -> Result<Vec<PageLink>> {
        let page = self.get_page(index)?;
        let pages = self.page_object_ids();
        let mut links = Vec::new();
        for annot in page.annots() {
            if annot.get::<Name>(SUBTYPE).as_deref() != Some(b"Link".as_slice()) {
                continue;
            }
            let rect = match annot.get::<pdf_render::pdf_syntax::object::Rect>(RECT) {
                Some(rect) => [rect.x0, rect.y0, rect.x1, rect.y1],
                None => continue,
            };
            let id = annot
                .get::<pdf_render::pdf_syntax::object::String>(NM)
                .map(|s| bytes_to_string(s.as_bytes()))
                .unwrap_or_default();
            let xref = annot
                .obj_id()
                .and_then(|id| u32::try_from(id.obj_number).ok())
                .unwrap_or(0);
            links.push(PageLink {
                target: link_target(&annot, &pages),
                rect,
                xref,
                id,
            });
        }
        Ok(links)
    }

    /// The object id of each page's dictionary, in page order.
    ///
    /// An outline destination names its page by reference, so resolving it is
    /// a lookup in this list. A page without an id (an inline page dict, which
    /// only brute-force recovery builds) keeps its place as a hole no
    /// reference can match.
    fn page_object_ids(&self) -> Vec<Option<ObjectIdentifier>> {
        self.pdf.pages().iter().map(|p| p.raw().obj_id()).collect()
    }

    /// Run OCR on a page and return the recognized text and word positions.
    ///
    /// The page is rendered at `dpi` (default 150) before recognition.
    /// Pass any [`OcrBackend`] implementation; use [`OcrsBackend::try_default`]
    /// to load the pure-Rust `ocrs` engine from the standard model paths.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # #[cfg(feature = "ocr")] {
    /// use pdf_engine::{PdfDocument, OcrsBackend, RenderOptions};
    ///
    /// let doc = PdfDocument::open(std::fs::read("scan.pdf").unwrap()).unwrap();
    /// let backend = OcrsBackend::try_default().unwrap();
    /// let result = doc.ocr_page(0, &backend, 150.0_f64).unwrap();
    /// println!("{}", result.text);
    /// # }
    /// ```
    pub fn ocr_page(
        &self,
        index: usize,
        backend: &dyn crate::ocr::OcrBackend,
        dpi: f64,
    ) -> crate::error::Result<crate::ocr::OcrResult> {
        let opts = crate::render::RenderOptions {
            dpi,
            ..Default::default()
        };
        let rendered = self.render_page(index, &opts)?;

        // Convert RGBA → RGB (ocrs expects RGB input).
        let mut rgb = Vec::with_capacity((rendered.width * rendered.height * 3) as usize);
        for chunk in rendered.pixels.chunks(4) {
            rgb.push(chunk[0]);
            rgb.push(chunk[1]);
            rgb.push(chunk[2]);
        }

        backend
            .recognize(&rgb, rendered.width, rendered.height)
            .map_err(|e| crate::error::EngineError::RenderError(e.to_string()))
    }

    /// Wrap `settings` with a warning sink that captures the first
    /// `InterpreterWarning::StreamTooLarge` into a shared slot.
    ///
    /// The returned slot is checked by [`Self::check_limit_slot`] after
    /// the operation completes. Any previously installed sink is still
    /// called so no warnings are silently dropped.
    fn with_limit_collector(settings: &InterpreterSettings) -> (InterpreterSettings, LimitSlot) {
        let slot: LimitSlot = Arc::new(Mutex::new(None));
        let slot_clone = Arc::clone(&slot);
        let prev_sink = settings.warning_sink.clone();
        let mut new_settings = settings.clone();
        new_settings.warning_sink = Arc::new(move |w: InterpreterWarning| {
            if let InterpreterWarning::StreamTooLarge { observed, limit } = w {
                let mut guard = slot_clone.lock().unwrap_or_else(|e| e.into_inner());
                if guard.is_none() {
                    *guard = Some((observed, limit));
                }
            }
            prev_sink(w);
        });
        (new_settings, slot)
    }

    /// Check the slot populated by [`Self::with_limit_collector`].
    ///
    /// Returns `Err(EngineError::LimitExceeded(...))` if a
    /// `StreamTooLarge` warning was captured, `Ok(())` otherwise.
    fn check_limit_slot(slot: &LimitSlot) -> Result<()> {
        if let Some((observed, limit)) = *slot.lock().unwrap_or_else(|e| e.into_inner()) {
            return Err(EngineError::LimitExceeded(LimitError::StreamTooLarge {
                actual_bytes: observed,
                limit_bytes: limit,
            }));
        }
        Ok(())
    }

    fn get_page(&self, index: usize) -> Result<&Page<'_>> {
        let pages = self.pdf.pages();
        if index >= pages.len() {
            return Err(EngineError::PageOutOfRange {
                index,
                count: pages.len(),
            });
        }
        Ok(&pages[index])
    }

    fn text_extraction_settings(&self) -> InterpreterSettings {
        let mut settings = self.settings.clone();
        // Text extraction should include signature widget appearance streams
        // that rendering skips to match MuPDF visual output.
        settings.skip_signature_widgets = false;
        settings
    }

    fn create_context_with_settings<'a>(
        page: &Page<'a>,
        settings: InterpreterSettings,
    ) -> Context<'a> {
        let (w, h) = page.render_dimensions();
        Context::new(
            page.initial_transform(false),
            Rect::new(0.0, 0.0, w as f64, h as f64),
            page.xref(),
            settings,
        )
    }

    fn extract_text_with_settings<'a>(
        page: &Page<'a>,
        settings: InterpreterSettings,
        keep_ligatures: bool,
    ) -> (String, InterpreterSettings) {
        let mut device = TextExtractionDevice::new().keep_ligatures(keep_ligatures);
        let mut ctx = Self::create_context_with_settings(page, settings);
        interpret_page(page, &mut ctx, &mut device);
        let settings = ctx.into_settings();
        (device.into_text(), settings)
    }

    fn extract_text_blocks_with_settings<'a>(
        page: &Page<'a>,
        settings: InterpreterSettings,
        keep_ligatures: bool,
    ) -> (Vec<TextBlock>, InterpreterSettings) {
        let mut device = TextExtractionDevice::new().keep_ligatures(keep_ligatures);
        let mut ctx = Self::create_context_with_settings(page, settings);
        interpret_page(page, &mut ctx, &mut device);
        let settings = ctx.into_settings();
        (device.into_blocks(), settings)
    }

    #[cfg(feature = "xfa")]
    fn open_flattened_xfa_for_render(&self) -> Option<Self> {
        if !crate::xfa::has_xfa(self) {
            return None;
        }

        let flat_bytes = crate::xfa::flatten(self).ok()?;
        let mut flat_doc = Self::open(flat_bytes).ok()?;
        flat_doc.settings = self.settings.clone();
        Some(flat_doc)
    }
}

/// Render a page to Gray8 bytes at the given dpi: `(width, height, data)`,
/// row-major, top-left origin, 0 is black.
///
/// The RGBA output of [`PdfDocument::render_page`] composited over white and
/// converted to ITU-R BT.601 luma. The ONE conversion: the vision layer's Gray8
/// path calls this, and the dump's image arm reads its bytes without the
/// `vision` feature (#574).
pub fn gray_page(doc: &PdfDocument, page: usize, dpi: u32) -> Result<(u32, u32, Vec<u8>)> {
    let options = RenderOptions {
        dpi: f64::from(dpi),
        ..Default::default()
    };
    let rendered = doc.render_page(page, &options)?;
    let mut data = Vec::with_capacity(rendered.width as usize * rendered.height as usize);
    for px in rendered.pixels.chunks_exact(4) {
        let (r, g, b, a) = (px[0], px[1], px[2], px[3]);
        // Composite over white; the default background is opaque white so
        // alpha is 255 in practice, but never trust it.
        let alpha = f32::from(a) / 255.0;
        let blend = |c: u8| (f32::from(c) * alpha + 255.0 * (1.0 - alpha)).round() as u8;
        let (r, g, b) = (blend(r), blend(g), blend(b));
        let luma = 0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b);
        data.push(luma.round().clamp(0.0, 255.0) as u8);
    }
    Ok((rendered.width, rendered.height, data))
}

/// Join per-page text the way `pdftotext` and `mutool` have for twenty years:
/// a form feed (U+000C) **after every page**, preceded by a blank line so the
/// last paragraph of a page is not glued to the separator.
///
/// After every page and not between them, which is the convention every reader
/// in the #390 comparison emits: a fourteen-page document carries fourteen form
/// feeds in `pdftotext`, `mutool`, PyMuPDF and pdfminer.six alike. Splitting on
/// U+000C therefore yields `n + 1` fields with an empty one at the end, and
/// every consumer of a form-feed stream already drops it. Separating instead of
/// terminating would save that one field and cost the property the convention
/// exists for: that our text is substitutable for theirs (#449).
pub fn join_page_texts<I>(page_texts: I) -> String
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    let mut text = String::new();

    for page_text in page_texts {
        text.push_str(page_text.as_ref());
        while !text.is_empty() && !text.ends_with("\n\n") {
            text.push('\n');
        }
        text.push('\u{000C}');
    }

    text
}

/// The learned switches a table strategy allows (#659).
///
/// [`TableStrategy`](crate::TableStrategy) is enforced before learning by
/// keeping grids back, and here by sitting out the proposers that would
/// put them back: the word-reading region, structure and judge models
/// under [`Lines`](crate::TableStrategy::Lines), the drawn-grid lattice
/// under [`Text`](crate::TableStrategy::Text).
/// [`Both`](crate::TableStrategy::Both) masks nothing.
fn mask_table_switches(
    switches: crate::learned::Switches,
    strategy: crate::TableStrategy,
) -> crate::learned::Switches {
    let mut switches = switches;
    match strategy {
        crate::TableStrategy::Lines => {
            switches.table_region = false;
            switches.table_structure = false;
            switches.table_judge = false;
        }
        crate::TableStrategy::Text => {
            switches.table_lattice = false;
        }
        crate::TableStrategy::Both => {}
    }
    switches
}

#[cfg(test)]
mod extract_all_text_tests {
    use super::join_page_texts;

    #[test]
    fn terminates_every_page_like_pdftotext() {
        assert_eq!(
            join_page_texts(["Page 1", "Page 2"]),
            "Page 1\n\n\u{000C}Page 2\n\n\u{000C}"
        );
    }

    #[test]
    fn a_page_carries_a_separator_even_when_it_has_no_text() {
        // The page exists; a reader counting form feeds must still find it.
        assert_eq!(
            join_page_texts(["", "Page 2"]),
            "\u{000C}Page 2\n\n\u{000C}"
        );
    }

    #[test]
    fn reuses_existing_blank_line_before_form_feed() {
        assert_eq!(
            join_page_texts(["Page 1\n\n", "Page 2"]),
            "Page 1\n\n\u{000C}Page 2\n\n\u{000C}"
        );
    }

    #[test]
    fn as_many_separators_as_pages() {
        let text = join_page_texts(["a", "b", "c"]);
        assert_eq!(text.matches('\u{000C}').count(), 3);
        // The trailing field every form-feed consumer already drops.
        assert_eq!(text.split('\u{000C}').count(), 4);
    }
}

/// Maximum outline (bookmark) nesting depth. Bounds recursion on adversarial
/// `/First` chains; real outlines are far shallower.
const MAX_OUTLINE_DEPTH: usize = 100;

/// Walk the outline linked list (FIRST → NEXT chain).
///
/// `visited` (object ids of already-seen items) breaks cyclic `/Next` loops and
/// `/First` back-references; `depth` bounds deeply nested `/First` chains. Both
/// protect against malformed/adversarial outlines hanging or overflowing.
/// `pages` is the page order [`PdfDocument::page_object_ids`] measured, which
fn parse_outline_items(
    item_dict: &Dict<'_>,
    depth: usize,
    visited: &mut BTreeSet<ObjectIdentifier>,
    pages: &[Option<ObjectIdentifier>],
) -> Vec<BookmarkItem> {
    let mut items = Vec::new();
    if depth >= MAX_OUTLINE_DEPTH {
        return items;
    }
    let mut current: Option<Dict<'_>> = Some(item_dict.clone());

    while let Some(dict) = current {
        // Stop if we re-enter an item: covers a /Next chain that loops back and
        // a /First child that references an ancestor.
        if let Some(id) = dict.obj_id() {
            if !visited.insert(id) {
                break;
            }
        }

        let title = dict
            .get::<pdf_render::pdf_syntax::object::String>(TITLE)
            .map(|s| bytes_to_string(s.as_bytes()))
            .unwrap_or_default();

        let children = match dict.get::<Dict<'_>>(FIRST) {
            Some(child_dict) => parse_outline_items(&child_dict, depth + 1, visited, pages),
            None => Vec::new(),
        };

        items.push(BookmarkItem {
            title,
            page: outline_destination_page(&dict, pages),
            children,
        });

        current = dict.get::<Dict<'_>>(NEXT);
    }

    items
}

/// The 0-based page an outline item points at, or `None`.
///
/// An item carries its destination as `/Dest`, or as an explicit go-to action
/// (`/A << /S /GoTo /D ... >>`); both hold an explicit destination array whose
/// first element is the page's own dictionary by reference. A named
/// destination (a string or name, resolved through the catalog's `/Names` or
/// `/Dests`) stays `None`: that lookup is a follow-up, and a guessed page
/// would be worse than none.
fn outline_destination_page(item: &Dict<'_>, pages: &[Option<ObjectIdentifier>]) -> Option<usize> {
    if let Some(page) = explicit_dest_page(item.get::<Array<'_>>(DEST), pages) {
        return Some(page);
    }
    let action = item.get::<Dict<'_>>(A)?;
    if action.get::<Name>(S).as_deref() != Some(b"GoTo".as_slice()) {
        return None;
    }
    if action.contains_key(F) {
        // A remote go-to: /D names a page in another file.
        return None;
    }
    explicit_dest_page(action.get::<Array<'_>>(D), pages)
}

/// The page an explicit destination array points at: its first element is the
/// page dictionary by reference, looked up in the page order.
fn explicit_dest_page(
    dest: Option<Array<'_>>,
    pages: &[Option<ObjectIdentifier>],
) -> Option<usize> {
    let first = dest?.raw_iter().next()?;
    let target = ObjectIdentifier::from(ObjRef::try_from(first).ok()?);
    pages.iter().position(|id| *id == Some(target))
}

/// Where one link annotation points.
///
/// An `/A` action wins over a direct `/Dest`, and a `/URI` action over
/// everything: only `/URI` and same-document `/GoTo` resolve further.
/// Anything else -- a remote jump, a launch or script action, a missing
/// address -- is [`LinkTarget::Unknown`] rather than a guess.
fn link_target(annot: &Dict<'_>, pages: &[Option<ObjectIdentifier>]) -> LinkTarget {
    if let Some(action) = annot.get::<Dict<'_>>(A) {
        if action.get::<Name>(S).as_deref() == Some(b"URI".as_slice()) {
            let uri = action
                .get::<pdf_render::pdf_syntax::object::String>(URI)
                .map(|s| bytes_to_string(s.as_bytes()))
                .unwrap_or_default();
            return LinkTarget::Uri(uri);
        }
        if action.get::<Name>(S).as_deref() == Some(b"GoTo".as_slice()) && !action.contains_key(F) {
            return link_destination(action.get::<Object<'_>>(D), pages);
        }
        return LinkTarget::Unknown;
    }
    link_destination(annot.get::<Object<'_>>(DEST), pages)
}

/// Where one destination object points: an explicit array resolves to
/// its page, a name or string stays a named destination, and anything
/// else (including a missing destination) is unknown.
fn link_destination(dest: Option<Object<'_>>, pages: &[Option<ObjectIdentifier>]) -> LinkTarget {
    match dest {
        Some(Object::Array(array)) => explicit_dest_page(Some(array), pages)
            .map(LinkTarget::Page)
            .unwrap_or(LinkTarget::Unknown),
        Some(Object::Name(name)) => LinkTarget::Named(name.as_str().to_string()),
        Some(Object::String(text)) => LinkTarget::Named(bytes_to_string(text.as_bytes())),
        _ => LinkTarget::Unknown,
    }
}

/// Convert PDF string bytes to a Rust String (UTF-8 with Latin-1 fallback).
fn bytes_to_string(bytes: &[u8]) -> String {
    // Check for UTF-16 BOM
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let chars: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter_map(|c| {
                if c.len() == 2 {
                    Some(u16::from_be_bytes([c[0], c[1]]))
                } else {
                    None
                }
            })
            .collect();
        return String::from_utf16_lossy(&chars);
    }

    // Try UTF-8, fall back to Latin-1.
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{ColorMode, PixelFormat, RenderConfig, RenderOptions};
    use lopdf::{Document as LoDocument, Object};
    use std::path::PathBuf;

    fn corpus_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus")
            .join(name)
    }

    /// A cyclic outline (`/Next` chain that loops back) must not hang the
    /// bookmark walker; the cycle guard breaks it into a finite list.
    #[test]
    fn cyclic_outline_terminates_and_is_bounded() {
        fn cyclic_outline_pdf() -> Vec<u8> {
            let objs: [&[u8]; 6] = [
                b"<< /Type /Catalog /Pages 2 0 R /Outlines 4 0 R >>",
                b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
                b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
                b"<< /Type /Outlines /First 5 0 R >>",
                b"<< /Title (A) /Next 6 0 R >>",
                b"<< /Title (B) /Next 5 0 R >>", // /Next back to A -> cycle
            ];
            let mut buf = Vec::new();
            let mut offsets = [0usize; 7];
            buf.extend_from_slice(b"%PDF-1.7\n");
            for (i, body) in objs.iter().enumerate() {
                offsets[i + 1] = buf.len();
                buf.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
                buf.extend_from_slice(body);
                buf.extend_from_slice(b"\nendobj\n");
            }
            let xref_off = buf.len();
            buf.extend_from_slice(b"xref\n0 7\n0000000000 65535 f \n");
            for o in &offsets[1..7] {
                buf.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
            }
            buf.extend_from_slice(
                format!("trailer\n<< /Size 7 /Root 1 0 R >>\nstartxref\n{xref_off}\n%%EOF")
                    .as_bytes(),
            );
            buf
        }

        fn count(b: &BookmarkItem) -> usize {
            1 + b.children.iter().map(count).sum::<usize>()
        }

        let doc = PdfDocument::open(cyclic_outline_pdf()).expect("open cyclic-outline PDF");
        let bookmarks = doc.bookmarks();
        let total: usize = bookmarks.iter().map(count).sum();
        assert!(
            total <= 2,
            "cyclic /Next outline must not loop forever; got {total} items"
        );
    }

    /// An outline item's destination resolves to its 0-based page: `/Dest` and
    /// an explicit `/A << /S /GoTo /D ... >>` both name the page by reference.
    #[test]
    fn outline_destinations_resolve_to_their_pages() {
        fn outlined_pdf() -> Vec<u8> {
            let objs: [&[u8]; 9] = [
                b"<< /Type /Catalog /Pages 2 0 R /Outlines 5 0 R >>",
                b"<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>",
                b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
                b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
                b"<< /Type /Outlines /First 6 0 R /Count 3 >>",
                b"<< /Title (A) /Parent 5 0 R /Next 7 0 R /Dest [3 0 R /Fit] >>",
                b"<< /Title (B) /Parent 5 0 R /Prev 6 0 R /Next 8 0 R /First 9 0 R /A << /S /GoTo /D [4 0 R /Fit] >> >>",
                b"<< /Title (C) /Parent 5 0 R /Prev 7 0 R /Dest (chapter-three) >>",
                b"<< /Title (B.1) /Parent 7 0 R /Dest [4 0 R /Fit] >>",
            ];
            let mut buf = Vec::new();
            let mut offsets = [0usize; 10];
            buf.extend_from_slice(b"%PDF-1.7\n");
            for (i, body) in objs.iter().enumerate() {
                offsets[i + 1] = buf.len();
                buf.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
                buf.extend_from_slice(body);
                buf.extend_from_slice(b"\nendobj\n");
            }
            let xref_off = buf.len();
            buf.extend_from_slice(b"xref\n0 10\n0000000000 65535 f \n");
            for o in &offsets[1..10] {
                buf.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
            }
            buf.extend_from_slice(
                format!("trailer\n<< /Size 10 /Root 1 0 R >>\nstartxref\n{xref_off}\n%%EOF")
                    .as_bytes(),
            );
            buf
        }

        let doc = PdfDocument::open(outlined_pdf()).expect("open outlined PDF");
        let bookmarks = doc.bookmarks();
        assert_eq!(bookmarks.len(), 3);
        assert_eq!(
            (bookmarks[0].title.as_str(), bookmarks[0].page),
            ("A", Some(0))
        );
        assert_eq!(
            (bookmarks[1].title.as_str(), bookmarks[1].page),
            ("B", Some(1))
        );
        assert_eq!(bookmarks[1].children.len(), 1);
        assert_eq!(
            (
                bookmarks[1].children[0].title.as_str(),
                bookmarks[1].children[0].page
            ),
            ("B.1", Some(1))
        );
        assert_eq!(
            (bookmarks[2].title.as_str(), bookmarks[2].page),
            ("C", None)
        );
    }

    /// Cache-on vs cache-off: the shared decoded-image cache is a pure
    /// performance optimisation, so rendering with it enabled must produce
    /// pixel-identical output to rendering with it disabled. Exercised against a
    /// committed image-bearing fixture.
    /// Locate a repository test fixture. Published crates do not carry the
    /// repository's fixture tree, so callers skip when this returns `None`
    /// rather than failing a consumer's `cargo test`.
    fn repo_fixture(name: &str) -> Option<PathBuf> {
        let dir = std::env::var("PDFLUENT_TEST_FIXTURES").ok().map_or_else(
            || {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("..")
                    .join("..")
                    .join("tests")
            },
            PathBuf::from,
        );
        let path = dir.join(name);
        path.exists().then_some(path)
    }

    #[test]
    fn shared_image_cache_is_render_neutral() {
        let Some(path) = repo_fixture("corpus-mini/scanned.pdf") else {
            eprintln!("skipping: repository fixture tree not present");
            return;
        };
        let data = std::fs::read(&path).expect("read scanned.pdf fixture");
        let cfg = RenderConfig::default();

        // Cache ON: `PdfDocument::open` installs a shared cache, so the second
        // render of the same page is a cache hit.
        let doc_on = PdfDocument::open(data.clone()).expect("open cache-on");
        let miss = doc_on
            .render_page_with_config(0, &cfg)
            .expect("cold render (cache miss)");
        let hit = doc_on
            .render_page_with_config(0, &cfg)
            .expect("warm render (cache hit)");

        // Cache OFF: default settings carry `shared_cache: None`, so every render
        // decodes images from scratch.
        let mut doc_off = PdfDocument::open(data).expect("open cache-off");
        doc_off.set_settings(InterpreterSettings::default());
        let uncached = doc_off
            .render_page_with_config(0, &cfg)
            .expect("render with cache disabled");

        assert_eq!(
            (miss.width, miss.height),
            (uncached.width, uncached.height),
            "render dimensions must match"
        );
        assert_eq!(
            miss.pixels, hit.pixels,
            "a cache hit must return exactly the freshly-decoded render"
        );
        assert_eq!(
            miss.pixels, uncached.pixels,
            "cache-enabled render must equal cache-disabled render"
        );
    }

    fn normalize_text(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn strip_type0_tounicode(data: &[u8]) -> (Vec<u8>, usize) {
        fn get_name(dict: &lopdf::Dictionary, key: &[u8]) -> Option<Vec<u8>> {
            match dict.get(key).ok()? {
                Object::Name(name) => Some(name.clone()),
                _ => None,
            }
        }

        fn descendant_is_cidfont_type2(doc: &LoDocument, type0: &lopdf::Dictionary) -> bool {
            let Some(Object::Array(descendants)) = type0.get(b"DescendantFonts").ok() else {
                return false;
            };
            let Some(Object::Reference(desc_id)) = descendants.first() else {
                return false;
            };
            let Ok(Object::Dictionary(descendant)) = doc.get_object(*desc_id) else {
                return false;
            };
            matches!(
                descendant.get(b"Subtype").ok(),
                Some(Object::Name(name)) if name.as_slice() == b"CIDFontType2"
            )
        }

        let mut doc = LoDocument::load_mem(data).expect("load stripped-to-unicode fixture");
        let ids: Vec<_> = doc.objects.keys().copied().collect();
        let mut removed = 0usize;

        for id in ids {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&id) else {
                continue;
            };
            if !matches!(
                dict.get(b"Subtype").ok(),
                Some(Object::Name(name)) if name.as_slice() == b"Type0"
            ) {
                continue;
            }
            if !matches!(
                get_name(dict, b"Encoding").as_deref(),
                Some(b"Identity-H") | Some(b"Identity-V")
            ) {
                continue;
            }
            if !descendant_is_cidfont_type2(&doc, dict) {
                continue;
            }

            if let Some(Object::Dictionary(type0)) = doc.objects.get_mut(&id) {
                if type0.has(b"ToUnicode") {
                    type0.remove(b"ToUnicode");
                    removed += 1;
                }
            }
        }

        let mut out = Vec::new();
        doc.save_to(&mut out)
            .expect("save stripped-to-unicode fixture");
        (out, removed)
    }

    fn solid_fill_pdf_bytes(color_operator: &str) -> Vec<u8> {
        use lopdf::{dictionary, Document, Object, Stream};

        let mut doc = Document::with_version("1.4");

        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        let content = format!("{color_operator}\n0 0 72 72 re f\n");
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));

        doc.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => Object::Name(b"Page".to_vec()),
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => Object::Array(vec![
                    Object::Integer(0),
                    Object::Integer(0),
                    Object::Integer(72),
                    Object::Integer(72),
                ]),
                "Contents" => Object::Reference(content_id),
            }),
        );

        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => Object::Name(b"Pages".to_vec()),
                "Kids" => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => Object::Integer(1),
            }),
        );

        let catalog_id = doc.new_object_id();
        doc.objects.insert(
            catalog_id,
            Object::Dictionary(dictionary! {
                "Type" => Object::Name(b"Catalog".to_vec()),
                "Pages" => Object::Reference(pages_id),
            }),
        );

        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).expect("save solid fill fixture");
        bytes
    }

    fn mixed_rgb_cmyk_pdf_bytes() -> Vec<u8> {
        use lopdf::{dictionary, Document, Object, Stream};

        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        let content = b"1 0 0 rg\n0 0 36 72 re f\n1 0 0 0 k\n36 0 36 72 re f\n".to_vec();
        let content_id = doc.add_object(Stream::new(dictionary! {}, content));

        doc.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => Object::Array(vec![0.into(), 0.into(), 72.into(), 72.into()]),
                "Contents" => Object::Reference(content_id),
            }),
        );
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => Object::Integer(1),
            }),
        );
        let catalog_id = doc.new_object_id();
        doc.objects.insert(
            catalog_id,
            Object::Dictionary(dictionary! {
                "Type" => "Catalog",
                "Pages" => Object::Reference(pages_id),
            }),
        );
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes)
            .expect("save mixed rgb/cmyk fixture");
        bytes
    }

    fn transparent_cmyk_pdf_bytes() -> Vec<u8> {
        use lopdf::{dictionary, Document, Object, Stream};

        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        let gs_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "ExtGState",
            "ca" => Object::Real(0.5),
        }));
        let content = b"/GS1 gs\n1 0 0 0 k\n0 0 72 72 re f\n".to_vec();
        let content_id = doc.add_object(Stream::new(dictionary! {}, content));

        doc.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => Object::Array(vec![0.into(), 0.into(), 72.into(), 72.into()]),
                "Resources" => dictionary! {
                    "ExtGState" => dictionary! {
                        "GS1" => Object::Reference(gs_id),
                    },
                },
                "Contents" => Object::Reference(content_id),
            }),
        );
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => Object::Integer(1),
            }),
        );
        let catalog_id = doc.new_object_id();
        doc.objects.insert(
            catalog_id,
            Object::Dictionary(dictionary! {
                "Type" => "Catalog",
                "Pages" => Object::Reference(pages_id),
            }),
        );
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes)
            .expect("save transparent cmyk fixture");
        bytes
    }

    fn cmyk_image_pdf_bytes() -> Vec<u8> {
        use lopdf::{dictionary, Document, Object, Stream};

        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        let image_id = doc.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => Object::Integer(2),
                "Height" => Object::Integer(1),
                "BitsPerComponent" => Object::Integer(8),
                "ColorSpace" => "DeviceCMYK",
            },
            vec![255, 0, 0, 0, 0, 255, 0, 0],
        ));
        let content = b"q\n2 0 0 1 0 0 cm\n/Im1 Do\nQ\n".to_vec();
        let content_id = doc.add_object(Stream::new(dictionary! {}, content));

        doc.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => Object::Array(vec![0.into(), 0.into(), 2.into(), 1.into()]),
                "Resources" => dictionary! {
                    "XObject" => dictionary! {
                        "Im1" => Object::Reference(image_id),
                    },
                },
                "Contents" => Object::Reference(content_id),
            }),
        );
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => Object::Integer(1),
            }),
        );
        let catalog_id = doc.new_object_id();
        doc.objects.insert(
            catalog_id,
            Object::Dictionary(dictionary! {
                "Type" => "Catalog",
                "Pages" => Object::Reference(pages_id),
            }),
        );
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).expect("save cmyk image fixture");
        bytes
    }

    fn pixel_at(rendered: &RenderedPage, x: u32, y: u32) -> [u8; 4] {
        let idx = ((y * rendered.width + x) * 4) as usize;
        [
            rendered.pixels[idx],
            rendered.pixels[idx + 1],
            rendered.pixels[idx + 2],
            rendered.pixels[idx + 3],
        ]
    }

    /// Build a minimal one-page PDF whose only font is a non-embedded TrueType
    /// reference (no `FontFile2`). The character codes in the content stream
    /// resolve through the declared `/Encoding`, exercising the same code path
    /// as corpus PDFs like `171_171940.pdf`.
    fn non_embedded_truetype_pdf_bytes(
        base_font: &[u8],
        encoding: &[u8],
        text_bytes: &[u8],
    ) -> Vec<u8> {
        use lopdf::{dictionary, Document, Object, Stream};

        let mut doc = Document::with_version("1.4");

        let font_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Font",
            "Subtype" => "TrueType",
            "Name" => Object::Name(b"F0".to_vec()),
            "BaseFont" => Object::Name(base_font.to_vec()),
            "Encoding" => Object::Name(encoding.to_vec()),
        }));

        let resources_id = doc.add_object(Object::Dictionary(dictionary! {
            "Font" => dictionary! { "F0" => Object::Reference(font_id) },
        }));

        let mut content = Vec::new();
        content.extend_from_slice(b"BT\n/F0 12 Tf\n100 700 Td\n(");
        for &b in text_bytes {
            match b {
                b'(' | b')' | b'\\' => {
                    content.push(b'\\');
                    content.push(b);
                }
                _ => content.push(b),
            }
        }
        content.extend_from_slice(b") Tj\nET\n");
        let content_id = doc.add_object(Stream::new(dictionary! {}, content));

        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
            "Resources" => Object::Reference(resources_id),
            "Contents" => Object::Reference(content_id),
        }));
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => Object::Integer(1),
            }),
        );
        let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        }));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).expect("save non-embedded fixture");
        bytes
    }

    /// Build a minimal AcroForm push button whose only human-readable text
    /// lives in the widget `/MK /CA` caption entry.
    fn push_button_caption_pdf_bytes(caption: &[u8]) -> Vec<u8> {
        use lopdf::{dictionary, Document, Object, Stream, StringFormat};

        let mut doc = Document::with_version("1.4");

        let catalog_id = doc.new_object_id();
        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        let acroform_id = doc.new_object_id();
        let content_id = doc.new_object_id();
        let widget_id = doc.new_object_id();

        doc.objects.insert(
            content_id,
            Object::Stream(Stream::new(dictionary! {}, Vec::new())),
        );
        doc.objects.insert(
            widget_id,
            Object::Dictionary(dictionary! {
                "Type" => "Annot",
                "Subtype" => "Widget",
                "FT" => "Btn",
                "Ff" => Object::Integer(1 << 16),
                "T" => Object::String(b"button".to_vec(), StringFormat::Literal),
                "MK" => dictionary! {
                    "CA" => Object::String(caption.to_vec(), StringFormat::Literal),
                },
                "Rect" => Object::Array(vec![100.into(), 700.into(), 260.into(), 730.into()]),
                "P" => Object::Reference(page_id),
            }),
        );
        doc.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => Object::Reference(pages_id),
                "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
                "Annots" => Object::Array(vec![Object::Reference(widget_id)]),
                "Contents" => Object::Reference(content_id),
            }),
        );
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => Object::Integer(1),
            }),
        );
        doc.objects.insert(
            acroform_id,
            Object::Dictionary(dictionary! {
                "Fields" => Object::Array(vec![Object::Reference(widget_id)]),
            }),
        );
        doc.objects.insert(
            catalog_id,
            Object::Dictionary(dictionary! {
                "Type" => "Catalog",
                "Pages" => Object::Reference(pages_id),
                "AcroForm" => Object::Reference(acroform_id),
            }),
        );
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut bytes = Vec::new();
        doc.save_to(&mut bytes)
            .expect("save push-button caption fixture");
        bytes
    }

    #[test]
    fn extract_text_non_embedded_truetype_alias_resolves_via_winansi() {
        // Mirrors corpus PDF `171_171940.pdf`: TrueType font references
        // `TimesNewRoman` (resolves through the standard-font alias table)
        // with `WinAnsiEncoding` and no embedded font program. Extraction must
        // recover the text from the declared encoding even though no glyph
        // outlines are available.
        let bytes = non_embedded_truetype_pdf_bytes(
            b"TimesNewRoman",
            b"WinAnsiEncoding",
            b"UNITED STATES DISTRICT COURT",
        );
        let text = PdfDocument::open(bytes)
            .expect("open non-embedded TrueType fixture")
            .extract_text(0)
            .expect("extract non-embedded TrueType text");
        let norm = normalize_text(&text);
        assert!(
            norm.contains("UNITED STATES DISTRICT COURT"),
            "expected WinAnsi-decoded text, got: {norm:?}"
        );
    }

    #[test]
    fn extract_text_non_embedded_truetype_unknown_name_still_decodes() {
        // Custom BaseFont that does not match any standard alias and lacks the
        // keywords used by the heuristic. The standard-font fallback (via
        // FallbackFontQuery) still picks Helvetica, but on hosts without the
        // embedded font assets that path returns None — the new TextOnly
        // branch is what keeps extraction non-empty in that case. Either way,
        // the WinAnsi-driven char map must produce the original prose.
        let bytes = non_embedded_truetype_pdf_bytes(
            b"OpaqueCustomXYZ",
            b"WinAnsiEncoding",
            b"Hello, world!",
        );
        let text = PdfDocument::open(bytes)
            .expect("open custom non-embedded fixture")
            .extract_text(0)
            .expect("extract custom non-embedded text");
        let norm = normalize_text(&text);
        assert!(
            norm.contains("Hello, world!"),
            "expected WinAnsi-decoded text, got: {norm:?}"
        );
    }

    #[test]
    fn extract_acroform_text_includes_push_button_mk_caption() {
        let bytes = push_button_caption_pdf_bytes(b"Don't cry over spilt milk");
        let doc = PdfDocument::open(bytes).expect("open push-button caption fixture");

        let page_text = doc.extract_text(0).expect("extract page text");
        assert!(
            normalize_text(&page_text).is_empty(),
            "expected empty page content stream, got: {page_text:?}"
        );

        let acroform_text = doc.extract_acroform_text();
        assert_eq!(normalize_text(&acroform_text), "Don't cry over spilt milk");

        let all_text = doc.extract_all_text();
        assert_eq!(normalize_text(&all_text), "Don't cry over spilt milk");
    }

    /// A form field value is text a searcher reads, and a ligature in one
    /// fails the same search a ligature on the page fails. The option covers
    /// it too, which is what makes `extract_all_text` one answer rather than
    /// a folded half and an unfolded half (codex, #437).
    #[test]
    fn acroform_values_fold_their_ligatures() {
        // A PDF text string carrying a non-Latin-1 character is UTF-16BE
        // behind a byte-order mark; the literal bytes would be read as
        // Latin-1 and never reach the fold as one character.
        let mut caption = vec![0xFE_u8, 0xFF_u8];
        for unit in "o\u{FB03}ce hours".encode_utf16() {
            caption.extend_from_slice(&unit.to_be_bytes());
        }
        let bytes = push_button_caption_pdf_bytes(&caption);
        let mut doc = PdfDocument::open(bytes).expect("open push-button caption fixture");

        assert_eq!(normalize_text(&doc.extract_acroform_text()), "office hours");
        assert_eq!(normalize_text(&doc.extract_all_text()), "office hours");

        doc.set_keep_ligatures(true);
        assert_eq!(
            normalize_text(&doc.extract_acroform_text()),
            "o\u{FB03}ce hours",
            "keep_ligatures did not reach the field values"
        );
    }

    #[test]
    fn bytes_to_string_utf8() {
        assert_eq!(bytes_to_string(b"hello"), "hello");
    }

    #[test]
    fn bytes_to_string_latin1() {
        let bytes = &[0xC4, 0xD6, 0xDC]; // ÄÖÜ in Latin-1
        let s = bytes_to_string(bytes);
        assert_eq!(s, "ÄÖÜ");
    }

    #[test]
    fn bytes_to_string_utf16() {
        let bytes = &[0xFE, 0xFF, 0x00, 0x48, 0x00, 0x69]; // UTF-16 "Hi"
        assert_eq!(bytes_to_string(bytes), "Hi");
    }

    #[test]
    fn document_info_default() {
        let info = DocumentInfo::default();
        assert!(info.title.is_none());
        assert!(info.author.is_none());
    }

    #[test]
    fn bookmark_item_children() {
        let item = BookmarkItem {
            title: "Root".into(),
            page: None,
            children: vec![BookmarkItem {
                title: "Child".into(),
                page: Some(0),
                children: Vec::new(),
            }],
        };
        assert_eq!(item.children.len(), 1);
        assert_eq!(item.children[0].title, "Child");
    }

    #[test]
    fn extract_text_type0_without_tounicode_uses_font_program_fallback() {
        let original = std::fs::read(corpus_path("sf181.pdf")).expect("read sf181 fixture");
        let expected = PdfDocument::open(original.clone())
            .expect("open original sf181")
            .extract_text(0)
            .expect("extract original sf181 text");
        assert!(
            expected.contains("Guide to Personnel Data Standards"),
            "unexpected baseline extraction: {expected}"
        );

        let (stripped, removed) = strip_type0_tounicode(&original);
        assert!(
            removed > 0,
            "expected to strip at least one Type0 ToUnicode"
        );

        let actual = PdfDocument::open(stripped)
            .expect("open stripped sf181")
            .extract_text(0)
            .expect("extract stripped sf181 text");

        let actual_norm = normalize_text(&actual);
        let expected_norm = normalize_text(&expected);

        assert!(
            actual_norm.contains("Guide to Personnel Data Standards"),
            "missing main heading after stripping ToUnicode: {actual_norm}"
        );
        assert!(
            actual_norm.contains("Privacy Act Statement"),
            "missing body text after stripping ToUnicode: {actual_norm}"
        );
        assert!(
            actual_norm.len() + 32 >= expected_norm.len(),
            "too much text lost after stripping ToUnicode: expected {} chars, got {}",
            expected_norm.len(),
            actual_norm.len()
        );
    }

    #[test]
    fn extract_text_identity_h_bogus_tounicode_recovers_via_identity_fallback() {
        // PDFBOX-4322-3.pdf ships an Identity-H Type0 font whose `/ToUnicode`
        // stream is actually an Identity-H *encoding* CMap (only
        // `begincidrange <0000> <FFFF> 0`, no bf-mappings). The embedded
        // TrueType subset also has no `cmap` table, so both the ToUnicode
        // lookup and the reverse-cmap fallback fail. Previously this yielded
        // a 0-byte extraction because the character codes — which are Unicode
        // code points under Identity-H — were silently discarded.
        let bytes =
            std::fs::read(corpus_path("PDFBOX-4322-3.pdf")).expect("read PDFBOX-4322-3 fixture");
        let doc = PdfDocument::open(bytes).expect("open PDFBOX-4322-3");
        let text = doc.extract_all_text();

        let norm = normalize_text(&text);
        assert!(
            norm.contains("Transatlantic Council"),
            "expected Identity-H codes to resolve as Unicode: {norm}"
        );
        assert!(
            norm.contains("Boy Scouts of America"),
            "expected body text to be recovered: {norm}"
        );
    }

    #[test]
    fn render_max_pixels_none_is_unchanged_default_behavior() {
        // Default (max_pixels = None) must be byte-identical to an explicit no-budget render.
        let doc = PdfDocument::open(solid_fill_pdf_bytes("1 0 0 rg")).expect("open fixture");
        let baseline = doc
            .render_page(
                0,
                &RenderOptions {
                    dpi: 144.0,
                    ..Default::default()
                },
            )
            .expect("baseline render");
        let explicit_none = doc
            .render_page(
                0,
                &RenderOptions {
                    dpi: 144.0,
                    max_pixels: None,
                    ..Default::default()
                },
            )
            .expect("explicit-none render");
        assert_eq!(baseline.width, explicit_none.width);
        assert_eq!(baseline.height, explicit_none.height);
        assert_eq!(baseline.pixels, explicit_none.pixels);
    }

    #[test]
    fn render_max_pixels_budget_clamps_resolution() {
        let doc = PdfDocument::open(solid_fill_pdf_bytes("1 0 0 rg")).expect("open fixture");
        let full = doc
            .render_page(
                0,
                &RenderOptions {
                    dpi: 288.0,
                    ..Default::default()
                },
            )
            .expect("full render");
        let full_px = full.width * full.height;
        // Budget well below the full pixel count must reduce output dimensions.
        let budget = full_px / 4;
        let capped = doc
            .render_page(
                0,
                &RenderOptions {
                    dpi: 288.0,
                    max_pixels: Some(budget),
                    ..Default::default()
                },
            )
            .expect("capped render");
        assert!(
            capped.width * capped.height <= full_px,
            "capped output must not exceed full output"
        );
        assert!(
            capped.width < full.width || capped.height < full.height,
            "budget below full pixel count must shrink at least one dimension"
        );
    }

    #[test]
    fn render_max_pixels_large_budget_no_clamp() {
        // A budget larger than the rendered size must not change the output.
        let doc = PdfDocument::open(solid_fill_pdf_bytes("1 0 0 rg")).expect("open fixture");
        let baseline = doc
            .render_page(
                0,
                &RenderOptions {
                    dpi: 72.0,
                    ..Default::default()
                },
            )
            .expect("baseline");
        let huge = doc
            .render_page(
                0,
                &RenderOptions {
                    dpi: 72.0,
                    max_pixels: Some(100_000_000),
                    ..Default::default()
                },
            )
            .expect("huge-budget render");
        assert_eq!(baseline.width, huge.width);
        assert_eq!(baseline.height, huge.height);
        assert_eq!(baseline.pixels, huge.pixels);
    }

    #[test]
    fn render_page_with_config_srgb_matches_legacy_render_page() {
        let doc = PdfDocument::open(solid_fill_pdf_bytes("1 0 0 rg")).expect("open rgb fixture");
        let legacy = doc
            .render_page(
                0,
                &RenderOptions {
                    dpi: 72.0,
                    ..Default::default()
                },
            )
            .expect("legacy render succeeds");
        let configured = doc
            .render_page_with_config(
                0,
                &RenderConfig {
                    color_mode: ColorMode::Srgb,
                    dpi: 72,
                },
            )
            .expect("configured render succeeds");

        assert_eq!(legacy.width, configured.width);
        assert_eq!(legacy.height, configured.height);
        assert_eq!(legacy.pixel_format, PixelFormat::Rgba8);
        assert_eq!(configured.pixel_format, PixelFormat::Rgba8);
        assert_eq!(legacy.pixels, configured.pixels);
    }

    #[test]
    fn render_page_with_config_preserve_cmyk_returns_cmyk_buffer() {
        let doc = PdfDocument::open(solid_fill_pdf_bytes("1 0 0 0 k")).expect("open cmyk fixture");
        let rendered = doc
            .render_page_with_config(
                0,
                &RenderConfig {
                    color_mode: ColorMode::PreserveCmyk,
                    dpi: 72,
                },
            )
            .expect("cmyk render succeeds");

        assert_eq!(rendered.pixel_format, PixelFormat::Cmyk8);
        assert_eq!(
            rendered.pixels.len(),
            rendered.width as usize * rendered.height as usize * 4
        );
        assert_eq!(
            pixel_at(&rendered, rendered.width / 2, rendered.height / 2),
            crate::color::preserve_device_cmyk(1.0, 0.0, 0.0, 0.0)
        );
    }

    #[test]
    fn render_page_with_config_simulate_cmyk_does_not_panic_on_cmyk_pdf() {
        let doc = PdfDocument::open(solid_fill_pdf_bytes("1 0 0 0 k")).expect("open cmyk fixture");
        let rendered = doc
            .render_page_with_config(
                0,
                &RenderConfig {
                    color_mode: ColorMode::SimulateCmyk,
                    dpi: 72,
                },
            )
            .expect("simulate cmyk render succeeds");

        assert_eq!(rendered.pixel_format, PixelFormat::Rgba8);
        assert!(!rendered.pixels.is_empty());
    }

    #[test]
    fn render_page_with_config_preserve_cmyk_mixed_page_preserves_only_cmyk_region() {
        let doc = PdfDocument::open(mixed_rgb_cmyk_pdf_bytes()).expect("open mixed fixture");
        let rendered = doc
            .render_page_with_config(
                0,
                &RenderConfig {
                    color_mode: ColorMode::PreserveCmyk,
                    dpi: 72,
                },
            )
            .expect("mixed render succeeds");

        assert_eq!(
            pixel_at(&rendered, 54, 36),
            crate::color::preserve_device_cmyk(1.0, 0.0, 0.0, 0.0)
        );
        assert_ne!(
            pixel_at(&rendered, 18, 36),
            crate::color::preserve_device_cmyk(1.0, 0.0, 0.0, 0.0)
        );
    }

    #[test]
    fn render_page_with_config_preserve_cmyk_transparent_page_does_not_crash() {
        let doc =
            PdfDocument::open(transparent_cmyk_pdf_bytes()).expect("open transparent cmyk fixture");
        let rendered = doc
            .render_page_with_config(
                0,
                &RenderConfig {
                    color_mode: ColorMode::PreserveCmyk,
                    dpi: 72,
                },
            )
            .expect("transparent cmyk render succeeds");

        assert_eq!(rendered.pixel_format, PixelFormat::Cmyk8);
        assert_eq!(
            rendered.pixels.len(),
            rendered.width as usize * rendered.height as usize * 4
        );
    }

    #[test]
    fn render_page_with_config_preserve_cmyk_keeps_device_cmyk_image_bytes() {
        let doc = PdfDocument::open(cmyk_image_pdf_bytes()).expect("open cmyk image fixture");
        let rendered = doc
            .render_page_with_config(
                0,
                &RenderConfig {
                    color_mode: ColorMode::PreserveCmyk,
                    dpi: 72,
                },
            )
            .expect("cmyk image render succeeds");

        assert_eq!(rendered.width, 2);
        assert_eq!(rendered.height, 1);
        assert_eq!(pixel_at(&rendered, 0, 0), [255, 0, 0, 0]);
        assert_eq!(pixel_at(&rendered, 1, 0), [0, 255, 0, 0]);
    }
}

#[cfg(test)]
mod load_recovery_tests {
    use super::PdfDocument;

    #[test]
    fn broken_xref_sets_xref_rebuilt() {
        // Objects + trailer present, but startxref points nowhere: pd-syntax
        // rebuilds the xref by scanning, and the signal must reflect that.
        let body: &[u8] = b"%PDF-1.7\n\
1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n\
3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>\nendobj\n\
trailer\n<< /Root 1 0 R /Size 4 >>\nstartxref\n999999\n%%EOF";
        let doc = PdfDocument::open(body.to_vec()).expect("recovers via xref rebuild");
        assert!(doc.load_recovery().xref_rebuilt, "xref_rebuilt must be set");
    }

    #[test]
    fn clean_document_reports_no_recovery() {
        // A well-formed PDF with a correct xref must report no recovery.
        let objs: [&[u8]; 3] = [
            b"<< /Type /Catalog /Pages 2 0 R >>",
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>",
        ];
        let mut buf = Vec::new();
        let mut off = [0usize; 4];
        buf.extend_from_slice(b"%PDF-1.7\n");
        for (i, body) in objs.iter().enumerate() {
            off[i + 1] = buf.len();
            buf.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
            buf.extend_from_slice(body);
            buf.extend_from_slice(b"\nendobj\n");
        }
        let xref_off = buf.len();
        buf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
        for o in &off[1..4] {
            buf.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
        }
        buf.extend_from_slice(
            format!("trailer\n<< /Root 1 0 R /Size 4 >>\nstartxref\n{xref_off}\n%%EOF").as_bytes(),
        );
        let doc = PdfDocument::open(buf).expect("clean doc opens");
        let r = doc.load_recovery();
        assert!(
            !r.xref_rebuilt && !r.page_tree_rebuilt,
            "a clean document must report no recovery; got {r:?}"
        );
    }
}

/// Why a page may be worth sending to a vision model (#624).
///
/// A flag with its reasons rather than a bare boolean, because an agent that
/// routes a page somewhere expensive wants to know what it is routing: a scan
/// needs OCR, a chart needs a picture read, and a table we are unsure of needs
/// neither -- it needs a second opinion on a structure we already have.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VisionAdvice {
    /// The page draws ink but yields no text: a scan, or text drawn as curves.
    ///
    /// This is the one signal of the three that is measured rather than
    /// promised. On the DocLayNet sample of the round of 24-09-2026 it holds for
    /// 25 of 200 pages -- twelve per cent -- and on those pages nothing is
    /// extracted at all: the page carries no usable text layer, which is what
    /// makes them worth a vision model rather than a better parser.
    pub no_text_layer: bool,
    /// The page holds a table the table model is unsure of.
    ///
    /// ALWAYS FALSE TODAY, and deliberately so rather than guessed at: the
    /// learned table structure of #600 is not on master, and the heuristics that
    /// find a table there now produce a decision and not a probability. A
    /// confidence invented from a heuristic would be a number with nothing
    /// behind it, which is the opposite of what this whole flag is for.
    pub low_confidence_table: bool,
    /// The page holds a chart or a figure whose content is not text.
    ///
    /// ALWAYS FALSE TODAY. [`crate::BlockKind`] has no figure variant -- the
    /// engine does not detect one -- so there is nothing to report. The
    /// measurement that says it is worth having is on #574: a figure's data
    /// labels are one of the three shapes we turn into a table, and on those
    /// pages our text score is 0.433 against the reference's 0.526.
    pub figure: bool,
}

impl VisionAdvice {
    /// Whether any reason holds.
    pub fn needs_vision(&self) -> bool {
        self.no_text_layer || self.low_confidence_table || self.figure
    }
}

/// How few characters a page may yield before it counts as having no text layer
/// (#624).
///
/// Not zero, because a scan regularly carries a stamped page number or a
/// scanner's own header in real text while the body is pixels: `CN100339781C`
/// shows some thirty characters of such stray text on four of its
/// twenty-three pages, and all twenty-three are scans. Forty is above that
/// stray text and far below what any page of prose yields.
const VISION_MIN_CHARACTERS: usize = 40;

/// Counts whether a page draws anything at all (#624).
#[derive(Default)]
struct InkDevice {
    glyphs: usize,
    images: usize,
    paths: usize,
}

impl InkDevice {
    fn drew_anything(&self) -> bool {
        self.glyphs > 0 || self.images > 0 || self.paths > 0
    }
}

impl<'a> InkDeviceTypes::Device<'a> for InkDevice {
    fn set_soft_mask(&mut self, _: Option<InkDeviceTypes::SoftMask<'a>>) {}
    fn set_blend_mode(&mut self, _: InkDeviceTypes::BlendMode) {}
    fn draw_path(
        &mut self,
        _: &kurbo::BezPath,
        _: kurbo::Affine,
        _: &InkDeviceTypes::Paint<'_>,
        _: &InkDeviceTypes::PathDrawMode,
    ) {
        self.paths += 1;
    }
    fn push_clip_path(&mut self, _: &InkDeviceTypes::ClipPath) {}
    fn push_transparency_group(
        &mut self,
        _: f32,
        _: Option<InkDeviceTypes::SoftMask<'_>>,
        _: InkDeviceTypes::BlendMode,
    ) {
    }
    fn draw_glyph(
        &mut self,
        _: &pdf_render::pdf_interpret::font::Glyph<'_>,
        _: kurbo::Affine,
        _: kurbo::Affine,
        _: &InkDeviceTypes::Paint<'_>,
        mode: &InkDeviceTypes::GlyphDrawMode,
    ) {
        // INVISIBLE TEXT IS NOT INK (codex on #1977). Text rendering mode 3 paints
        // nothing, and counting it had two consequences, one of them the opposite
        // of what this device is for. A visually blank page carrying a few
        // invisible characters would have read as "draws ink, yields no text" and
        // been sent to a vision model that has nothing to look at -- the very
        // blank-page case `a_blank_page_does_not_either` exists to prevent.
        //
        // And the other way round matters more for the flag's purpose: an OCR'd
        // scan draws its text layer invisibly OVER the image. Those pages have a
        // text layer, are read perfectly well, and must not be flagged -- they are
        // not flagged, because the character count decides that and the image is
        // what supplies the ink. Not counting invisible glyphs leaves that case
        // exactly as it was and fixes the blank one.
        if !matches!(mode, InkDeviceTypes::GlyphDrawMode::Invisible) {
            self.glyphs += 1;
        }
    }
    fn draw_image(&mut self, _: InkDeviceTypes::Image<'a, '_>, _: kurbo::Affine) {
        self.images += 1;
    }
    fn pop_clip_path(&mut self) {}
    fn pop_transparency_group(&mut self) {}
}

#[cfg(test)]
mod last_glyph_record_tests {
    //! Spec `specs/text/circlecopyrt.md`, review 14-10 02:00 (i): the
    //! thread-local that names the text device holding a last glyph's place
    //! is empty once a page has been extracted, and once an extraction has
    //! panicked.

    use super::*;
    use crate::text::last_glyph_is_held;
    use lopdf::{dictionary, Document as LoDocument, Object, Stream};
    use pdf_render::pdf_interpret::font::{Glyph, GlyphPlace};
    use pdf_render::pdf_interpret::{
        BlendMode, ClipPath, Device, GlyphDrawMode, Image, Paint, PathDrawMode, SoftMask,
    };

    /// One page drawing `abc` in Helvetica.
    fn one_line() -> Vec<u8> {
        let mut doc = LoDocument::with_version("1.7");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let content = b"BT /F1 12 Tf 72 700 Td (abc) Tj ET".to_vec();
        let content_id = doc.add_object(Stream::new(dictionary! {}, content));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page_id)],
                "Count" => 1_i64,
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).expect("fixture saves");
        bytes
    }

    /// The text device, made to panic at its second glyph, once the first
    /// has been remembered.
    struct PanicsAtTheSecondGlyph {
        inner: TextExtractionDevice,
        glyphs: usize,
    }

    impl<'a> Device<'a> for PanicsAtTheSecondGlyph {
        fn set_soft_mask(&mut self, mask: Option<SoftMask<'a>>) {
            self.inner.set_soft_mask(mask);
        }
        fn set_blend_mode(&mut self, blend_mode: BlendMode) {
            self.inner.set_blend_mode(blend_mode);
        }
        fn draw_path(
            &mut self,
            path: &kurbo::BezPath,
            transform: kurbo::Affine,
            paint: &Paint<'a>,
            draw_mode: &PathDrawMode,
        ) {
            self.inner.draw_path(path, transform, paint, draw_mode);
        }
        fn push_clip_path(&mut self, clip_path: &ClipPath) {
            self.inner.push_clip_path(clip_path);
        }
        fn push_transparency_group(
            &mut self,
            opacity: f32,
            mask: Option<SoftMask<'a>>,
            blend_mode: BlendMode,
        ) {
            self.inner
                .push_transparency_group(opacity, mask, blend_mode);
        }
        fn draw_glyph(
            &mut self,
            glyph: &Glyph<'a>,
            transform: kurbo::Affine,
            glyph_transform: kurbo::Affine,
            paint: &Paint<'a>,
            draw_mode: &GlyphDrawMode,
        ) {
            self.glyphs += 1;
            if self.glyphs == 2 {
                assert!(last_glyph_is_held(), "the first glyph is remembered");
                panic!("inside extraction");
            }
            self.inner
                .draw_glyph(glyph, transform, glyph_transform, paint, draw_mode);
        }
        fn draw_image(&mut self, image: Image<'a, '_>, transform: kurbo::Affine) {
            self.inner.draw_image(image, transform);
        }
        fn pop_clip_path(&mut self) {
            self.inner.pop_clip_path();
        }
        fn pop_transparency_group(&mut self) {
            self.inner.pop_transparency_group();
        }
    }

    #[test]
    fn the_last_glyph_record_is_empty_after_a_page_is_extracted() {
        let doc = PdfDocument::open(one_line()).expect("fixture opens");
        let text = doc.extract_text(0).expect("text");
        assert!(text.contains("abc"), "{text:?}");
        assert!(!last_glyph_is_held());
    }

    #[test]
    fn the_last_glyph_record_is_empty_after_a_panic_inside_extraction() {
        let doc = PdfDocument::open(one_line()).expect("fixture opens");
        let page = doc.get_page(0).expect("page");
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut device = PanicsAtTheSecondGlyph {
                inner: TextExtractionDevice::new(),
                glyphs: 0,
            };
            let mut ctx =
                PdfDocument::create_context_with_settings(page, doc.text_extraction_settings());
            interpret_page(page, &mut ctx, &mut device);
        }));
        assert!(outcome.is_err(), "the device panicked");
        assert!(!last_glyph_is_held());
    }

    /// One page drawing a Helvetica `c` at 102.8 and then, at 100, a one-em
    /// glyph named `circlecopyrt` whose box holds the `c`'s centre: drawing
    /// the circle offers it to the `c`.
    fn c_then_circle() -> Vec<u8> {
        let mut doc = LoDocument::with_version("1.7");
        let pages_id = doc.new_object_id();
        let helvetica = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let circle = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "CMSY10",
            "FirstChar" => 65_i64,
            "LastChar" => 65_i64,
            "Widths" => vec![Object::Integer(1000)],
            "Encoding" => dictionary! {
                "Type" => "Encoding",
                "Differences" => vec![Object::Integer(65), Object::Name(b"circlecopyrt".to_vec())],
            },
        });
        let content = b"BT /F1 10 Tf 1 0 0 1 102.8 700 Tm (c) Tj \
                        /F2 10 Tf 1 0 0 1 100 700 Tm (A) Tj ET"
            .to_vec();
        let content_id = doc.add_object(Stream::new(dictionary! {}, content));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! {
                "Font" => dictionary! { "F1" => helvetica, "F2" => circle },
            },
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(page_id)],
                "Count" => 1_i64,
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).expect("fixture saves");
        bytes
    }

    /// A device that keeps each glyph's place and, at the second glyph, asks
    /// it what the glyph at each of a few places reads as now.
    #[derive(Default)]
    struct AsksAtTheSecondGlyph {
        places: Vec<Option<GlyphPlace>>,
        /// A place from another page interpretation, asked first.
        foreign: Option<GlyphPlace>,
        /// For each question, whether the second glyph answered it.
        answered: Vec<(&'static str, bool)>,
    }

    impl<'a> Device<'a> for AsksAtTheSecondGlyph {
        fn set_soft_mask(&mut self, _: Option<SoftMask<'a>>) {}
        fn set_blend_mode(&mut self, _: BlendMode) {}
        fn draw_path(
            &mut self,
            _: &kurbo::BezPath,
            _: kurbo::Affine,
            _: &Paint<'a>,
            _: &PathDrawMode,
        ) {
        }
        fn push_clip_path(&mut self, _: &ClipPath) {}
        fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
        fn draw_glyph(
            &mut self,
            glyph: &Glyph<'a>,
            _: kurbo::Affine,
            _: kurbo::Affine,
            _: &Paint<'a>,
            _: &GlyphDrawMode,
        ) {
            let Glyph::Outline(outline) = glyph else {
                return;
            };
            if let [Some(before)] = self.places[..] {
                if let Some(foreign) = self.foreign {
                    let read = outline.reread_previous(foreign).is_some();
                    self.answered
                        .push(("a place from another interpretation", read));
                }
                let own = outline.place().expect("drawn in an interpretation");
                let read = outline.reread_previous(own).is_some();
                self.answered.push(("its own place", read));
                // Last: this read takes the offer.
                let read = outline.reread_previous(before).is_some();
                self.answered.push(("the glyph directly before", read));
            }
            self.places.push(outline.place());
        }
        fn draw_image(&mut self, _: Image<'a, '_>, _: kurbo::Affine) {}
        fn pop_clip_path(&mut self) {}
        fn pop_transparency_group(&mut self) {}
    }

    /// [`c_then_circle`] interpreted on a thread of its own, the first one
    /// that thread runs, with `foreign` as the place asked first.
    fn asked_on_a_new_thread(foreign: Option<GlyphPlace>) -> AsksAtTheSecondGlyph {
        std::thread::spawn(move || {
            let doc = PdfDocument::open(c_then_circle()).expect("fixture opens");
            let page = doc.get_page(0).expect("page");
            let mut device = AsksAtTheSecondGlyph {
                foreign,
                ..Default::default()
            };
            let mut ctx =
                PdfDocument::create_context_with_settings(page, doc.text_extraction_settings());
            interpret_page(page, &mut ctx, &mut device);
            device
        })
        .join()
        .expect("the thread finishes")
    }

    /// Spec review 14-10 07:18: a place answers only for the glyph directly
    /// before, in the same interpretation. The foreign place stands at the
    /// same position in the same page, drawn by another thread's first
    /// interpretation, so only the record's number can tell the two apart.
    #[test]
    fn a_place_from_elsewhere_than_the_last_glyph_gives_no_reread() {
        let first = asked_on_a_new_thread(None);
        let foreign = first.places[0].expect("the c has a place");
        let second = asked_on_a_new_thread(Some(foreign));
        assert_eq!(
            second.answered,
            [
                ("a place from another interpretation", false),
                ("its own place", false),
                ("the glyph directly before", true),
            ]
        );
    }
}
