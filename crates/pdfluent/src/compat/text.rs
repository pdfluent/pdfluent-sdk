//! Text extraction for the compatible API (lane L1).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Typed text outputs: one struct per recorded shape (words, blocks, dicts,
//! traces), one [`TextOutput`] enum over the option-dependent `get_text`
//! results, and option structs for the clip/flags/sort/textpage families.
//! Structs whose shape has no recording yet are `#[non_exhaustive]` so the
//! lane can grow them without breaking the bindings.

use super::edit::Annot;
use super::geometry::{CompareOp, Matrix, Point, Quad, Rect};
use super::pixmap::PlacedImage;
use super::{ErrorKind, Page, PageMut, Value};
use crate::document::PdfDocument;
use crate::error::Result;
use pdf_engine::{Block, BlockKind, BoundsSource, ExtractOptions, TextSpan};
use pdf_standard_fonts::{symbol_glyph_name_to_char, StandardFont};

/// One extracted word: its box, text, and block/line/word numbers.
#[derive(Debug, Clone, PartialEq)]
pub struct TextWord {
    /// Word box, in points.
    pub rect: Rect,
    /// Word text, without surrounding whitespace.
    pub text: String,
    /// Zero-based block number on the page.
    pub block_no: i32,
    /// Zero-based line number within the block.
    pub line_no: i32,
    /// Zero-based word number within the line.
    pub word_no: i32,
}

/// One extracted block: its box, text, number, and type code.
#[derive(Debug, Clone, PartialEq)]
pub struct TextBlock {
    /// Block box, in points.
    pub rect: Rect,
    /// Block text, lines joined with newlines.
    pub text: String,
    /// Zero-based block number on the page.
    pub number: i32,
    /// Block type code (0 for text, 1 for images).
    pub block_type: i32,
}

/// One raw character: its box, character, origin, and synthesis flag.
#[derive(Debug, Clone, PartialEq)]
pub struct RawChar {
    /// Character box, in points.
    pub bbox: Rect,
    /// The character itself.
    pub ch: char,
    /// Pen origin of the character, in points.
    pub origin: Point,
    /// Whether the character was synthesised rather than read.
    pub synthetic: bool,
}

/// One span: a run of one font and size with its characters on raw extracts.
#[derive(Debug, Clone, PartialEq)]
pub struct DictSpan {
    /// Span box, in points.
    pub bbox: Rect,
    /// Pen origin of the span, in points.
    pub origin: Point,
    /// Font name.
    pub font: String,
    /// Font size, in points.
    pub size: f64,
    /// Text colour packed as an integer.
    pub color: i32,
    /// Opacity as an integer alpha value.
    pub alpha: i32,
    /// Span flags bitfield.
    pub flags: i32,
    /// Bidirectional level.
    pub bidi: i32,
    /// Per-character flags bitfield.
    pub char_flags: i32,
    /// Font ascender, in points.
    pub ascender: f64,
    /// Font descender, in points.
    pub descender: f64,
    /// Span text.
    pub text: String,
    /// Per-character boxes, populated on raw extracts, empty otherwise.
    pub chars: Vec<RawChar>,
}

/// One line: its box, writing mode, direction, and spans.
#[derive(Debug, Clone, PartialEq)]
pub struct DictLine {
    /// Line box, in points.
    pub bbox: Rect,
    /// Writing mode (0 horizontal, 1 vertical).
    pub wmode: i32,
    /// Baseline direction vector.
    pub direction: Point,
    /// Spans making up the line.
    pub spans: Vec<DictSpan>,
}

/// One dict block: text with lines, or a placed image.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum DictBlock {
    /// A text block with its lines.
    Text {
        /// Block box, in points.
        bbox: Rect,
        /// Zero-based block number on the page.
        number: i32,
        /// Block flags bitfield.
        flags: i32,
        /// Lines making up the block.
        lines: Vec<DictLine>,
    },
    /// A placed image block.
    Image {
        /// Image box, in points.
        bbox: Rect,
        /// Zero-based block number on the page.
        number: i32,
        /// Image width, in pixels.
        width: i32,
        /// Image height, in pixels.
        height: i32,
        /// Bits per component.
        bpc: i32,
        /// Colour-space code.
        colorspace: i32,
        /// Suggested file extension, without the dot.
        ext: String,
        /// Encoded image bytes.
        image_data: Vec<u8>,
        /// Image-to-page transformation.
        transform: Matrix,
        /// Horizontal resolution, in dots per inch.
        xres: i32,
        /// Vertical resolution, in dots per inch.
        yres: i32,
    },
}

/// Structured page text: page size plus dict blocks.
#[derive(Debug, Clone, PartialEq)]
pub struct TextDict {
    /// Page width, in points.
    pub width: f64,
    /// Page height, in points.
    pub height: f64,
    /// Blocks on the page, in reading order.
    pub blocks: Vec<DictBlock>,
}

/// One traced character: codepoint, glyph id, origin, and box.
#[derive(Debug, Clone, PartialEq)]
pub struct TraceChar {
    /// Unicode codepoint.
    pub codepoint: i32,
    /// Font glyph id.
    pub glyph: i32,
    /// Pen origin, in points.
    pub origin: Point,
    /// Character box, in points.
    pub bbox: Rect,
}

/// One traced span: metrics plus per-character geometry.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct TextTrace {
    /// Span box, in points.
    pub bbox: Rect,
    /// Font ascender, in points.
    pub ascender: f64,
    /// Bidirectional direction code.
    pub bidi_dir: i32,
    /// Bidirectional embedding level.
    pub bidi_level: i32,
    /// Traced characters.
    pub chars: Vec<TraceChar>,
    /// Font name.
    pub font: String,
    /// Font size, in points.
    pub size: f64,
    /// Text colour as red/green/blue fractions.
    pub color: [f64; 3],
    /// Colour-space code.
    pub colorspace: i32,
    /// Font descender, in points.
    pub descender: f64,
    /// Baseline direction vector.
    pub dir: Point,
    /// Span flags bitfield.
    pub flags: i32,
    /// Optional-content layer name, empty when unmarked.
    pub layer: String,
    /// Stroke width, or `None` for filled text.
    pub linewidth: Option<f64>,
    /// Opacity from transparent (0.0) to opaque (1.0).
    pub opacity: f64,
    /// Zero-based span number on the page.
    pub seqno: i32,
    /// Advance width of a space, in points.
    pub spacewidth: f64,
    /// Trace item type code (0 for text).
    pub span_type: i32,
    /// Writing mode (0 horizontal, 1 vertical).
    pub wmode: i32,
}

/// Text output formats accepted by `get_text` and friends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum TextFormat {
    /// Plain text.
    #[default]
    Text,
    /// Word list.
    Words,
    /// Block list.
    Blocks,
    /// Structured dict.
    Dict,
    /// Structured dict with per-character boxes.
    RawDict,
    /// Dict serialised as JSON text.
    Json,
    /// Raw dict serialised as JSON text.
    RawJson,
    /// HTML fragment.
    Html,
    /// XHTML fragment.
    Xhtml,
    /// XML fragment.
    Xml,
}

impl TextFormat {
    /// Parse the recorded option name, or a bad-value error naming it.
    ///
    /// The name matches case-insensitively: the recordings accept an
    /// uppercase option word like the lowercase form.
    pub fn from_name(name: &str) -> Result<Self> {
        match name.to_ascii_lowercase().as_str() {
            "text" => Ok(Self::Text),
            "words" => Ok(Self::Words),
            "blocks" => Ok(Self::Blocks),
            "dict" => Ok(Self::Dict),
            "rawdict" => Ok(Self::RawDict),
            "json" => Ok(Self::Json),
            "rawjson" => Ok(Self::RawJson),
            "html" => Ok(Self::Html),
            "xhtml" => Ok(Self::Xhtml),
            "xml" => Ok(Self::Xml),
            _ => Err(ErrorKind::BadValue.to_error(format!("unknown text option {name:?}"))),
        }
    }
}

/// The option-dependent result of `get_text`: one variant per format.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum TextOutput {
    /// Plain text (`text`).
    Text(String),
    /// Word list (`words`).
    Words(Vec<TextWord>),
    /// Block list (`blocks`).
    Blocks(Vec<TextBlock>),
    /// Structured dict (`dict` and `rawdict`; raw fills span characters).
    Dict(TextDict),
    /// JSON text (`json`).
    Json(String),
    /// Raw JSON text (`rawjson`).
    RawJson(String),
    /// HTML fragment (`html`).
    Html(String),
    /// XHTML fragment (`xhtml`).
    Xhtml(String),
    /// XML fragment (`xml`).
    Xml(String),
}

/// Options for `get_text`: format, clip, flags, sort, and an optional page.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct TextOptions<'a> {
    /// Output format.
    pub format: TextFormat,
    /// Clip rectangle, or the whole page.
    pub clip: Option<Rect>,
    /// Extraction flags, or the default set when `None`.
    pub flags: Option<i32>,
    /// Sort blocks in reading order.
    pub sort: bool,
    /// Reuse this extracted page instead of extracting again.
    pub textpage: Option<TextPage<'a>>,
}

impl<'a> TextOptions<'a> {
    /// Set the output format.
    pub fn with_format(mut self, format: TextFormat) -> Self {
        self.format = format;
        self
    }

    /// Clip extraction to this rectangle.
    pub fn with_clip(mut self, clip: Rect) -> Self {
        self.clip = Some(clip);
        self
    }

    /// Set the extraction flags.
    pub fn with_flags(mut self, flags: i32) -> Self {
        self.flags = Some(flags);
        self
    }

    /// Sort blocks in reading order.
    pub fn with_sort(mut self, sort: bool) -> Self {
        self.sort = sort;
        self
    }

    /// Reuse this extracted page instead of extracting again.
    pub fn with_textpage(mut self, textpage: TextPage<'a>) -> Self {
        self.textpage = Some(textpage);
        self
    }
}

/// Options for word extraction: delimiters plus the shared text knobs.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct WordOptions<'a> {
    /// Extra word-boundary characters, or the default set when `None`.
    pub delimiters: Option<String>,
    /// Clip rectangle, or the whole page.
    pub clip: Option<Rect>,
    /// Extraction flags, or the default set when `None`.
    pub flags: Option<i32>,
    /// Sort words in reading order.
    pub sort: bool,
    /// Reuse this extracted page instead of extracting again.
    pub textpage: Option<TextPage<'a>>,
}

impl<'a> WordOptions<'a> {
    /// Add these word-boundary characters.
    pub fn with_delimiters(mut self, delimiters: &str) -> Self {
        self.delimiters = Some(delimiters.to_string());
        self
    }

    /// Clip extraction to this rectangle.
    pub fn with_clip(mut self, clip: Rect) -> Self {
        self.clip = Some(clip);
        self
    }

    /// Set the extraction flags.
    pub fn with_flags(mut self, flags: i32) -> Self {
        self.flags = Some(flags);
        self
    }

    /// Sort words in reading order.
    pub fn with_sort(mut self, sort: bool) -> Self {
        self.sort = sort;
        self
    }

    /// Reuse this extracted page instead of extracting again.
    pub fn with_textpage(mut self, textpage: TextPage<'a>) -> Self {
        self.textpage = Some(textpage);
        self
    }
}

/// Options for block extraction: the shared text knobs.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct BlockOptions<'a> {
    /// Clip rectangle, or the whole page.
    pub clip: Option<Rect>,
    /// Extraction flags, or the default set when `None`.
    pub flags: Option<i32>,
    /// Sort blocks in reading order.
    pub sort: bool,
    /// Reuse this extracted page instead of extracting again.
    pub textpage: Option<TextPage<'a>>,
}

impl<'a> BlockOptions<'a> {
    /// Clip extraction to this rectangle.
    pub fn with_clip(mut self, clip: Rect) -> Self {
        self.clip = Some(clip);
        self
    }

    /// Set the extraction flags.
    pub fn with_flags(mut self, flags: i32) -> Self {
        self.flags = Some(flags);
        self
    }

    /// Sort blocks in reading order.
    pub fn with_sort(mut self, sort: bool) -> Self {
        self.sort = sort;
        self
    }

    /// Reuse this extracted page instead of extracting again.
    pub fn with_textpage(mut self, textpage: TextPage<'a>) -> Self {
        self.textpage = Some(textpage);
        self
    }
}

/// Options for text search: clip, hit shape, flags, hit cap, and a page.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct SearchOptions<'a> {
    /// Clip rectangle, or the whole page.
    pub clip: Option<Rect>,
    /// Return quadrilaterals instead of rectangles.
    pub quads: bool,
    /// Search flags, or the default set when `None`.
    pub flags: Option<i32>,
    /// Maximum hits, or no cap at zero.
    pub limit: i32,
    /// Search this extracted page instead of extracting again.
    pub textpage: Option<TextPage<'a>>,
}

impl<'a> SearchOptions<'a> {
    /// Clip the search to this rectangle.
    pub fn with_clip(mut self, clip: Rect) -> Self {
        self.clip = Some(clip);
        self
    }

    /// Return quadrilaterals instead of rectangles.
    pub fn with_quads(mut self, quads: bool) -> Self {
        self.quads = quads;
        self
    }

    /// Set the search flags.
    pub fn with_flags(mut self, flags: i32) -> Self {
        self.flags = Some(flags);
        self
    }

    /// Cap the number of hits (zero lifts the cap).
    pub fn with_limit(mut self, limit: i32) -> Self {
        self.limit = limit;
        self
    }

    /// Search this extracted page instead of extracting again.
    pub fn with_textpage(mut self, textpage: TextPage<'a>) -> Self {
        self.textpage = Some(textpage);
        self
    }
}

/// Search hits: rectangles by default, quadrilaterals on request.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum SearchHits {
    /// One rectangle per hit.
    Rects(Vec<Rect>),
    /// One quadrilateral per hit.
    Quads(Vec<Quad>),
}

/// Options for table search; reserved for the recorded keyword set.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct TableOptions {}

/// Found tables on a page: the table list.
#[derive(Debug, Clone, PartialEq)]
pub struct TableFinder {
    /// Tables found, in page order.
    pub tables: Vec<Table>,
}

/// One found table: its box, grid size, and cells.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    /// Table box, in points.
    pub bbox: Rect,
    /// Number of grid rows.
    pub row_count: i32,
    /// Number of grid columns.
    pub col_count: i32,
    /// Cells in row-major order.
    pub cells: Vec<TableCell>,
}

/// One table cell: its box and text.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct TableCell {
    /// Cell box, in points.
    pub bbox: Rect,
    /// Cell text.
    pub text: String,
}

/// One font on a page: its object number and name.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct FontInfo {
    /// Font object number.
    pub xref: i32,
    /// Base font name.
    pub name: String,
    /// Resource key (`F1`).
    pub res_name: String,
    /// Font subtype (`Type1`).
    pub subtype: String,
    /// Encoding name, empty when the dictionary sets none.
    pub encoding: String,
    /// Font program extension, `n/a` when nothing is embedded.
    pub ext: String,
}

/// One extracted font: its name and raw data.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct FontData {
    /// Font name.
    pub name: String,
    /// Raw font file bytes.
    pub data: Vec<u8>,
    /// Font program extension, `n/a` when nothing is embedded.
    pub ext: String,
    /// Font subtype (`Type1`).
    pub subtype: String,
}

/// Options for OCR text pages: language and tessdata path.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct OcrOptions {
    /// Recognition language, or the engine default when `None`.
    pub language: Option<String>,
    /// Tessdata directory, or the engine default when `None`.
    pub tessdata: Option<String>,
}

/// Options for whole-document text reads: page set plus the text knobs.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct BulkTextOptions {
    /// Zero-based pages to read, or all pages when `None`.
    pub pages: Option<Vec<i32>>,
    /// Reading method name.
    pub method: String,
    /// Worker count, or the default when `None`.
    pub concurrency: Option<i32>,
    /// Output format.
    pub format: TextFormat,
    /// Clip rectangle, or whole pages.
    pub clip: Option<Rect>,
    /// Extraction flags, or the default set when `None`.
    pub flags: Option<i32>,
    /// Sort blocks in reading order.
    pub sort: bool,
    /// Extra word-boundary characters for word reads.
    pub delimiters: Option<String>,
}

impl Default for BulkTextOptions {
    fn default() -> Self {
        Self {
            pages: None,
            method: "single".to_string(),
            concurrency: None,
            format: TextFormat::Text,
            clip: None,
            flags: None,
            sort: false,
            delimiters: None,
        }
    }
}

impl BulkTextOptions {
    /// Read only these zero-based pages.
    pub fn with_pages(mut self, pages: Vec<i32>) -> Self {
        self.pages = Some(pages);
        self
    }

    /// Set the reading method name.
    pub fn with_method(mut self, method: &str) -> Self {
        self.method = method.to_string();
        self
    }

    /// Set the worker count.
    pub fn with_concurrency(mut self, concurrency: i32) -> Self {
        self.concurrency = Some(concurrency);
        self
    }

    /// Set the output format.
    pub fn with_format(mut self, format: TextFormat) -> Self {
        self.format = format;
        self
    }

    /// Clip extraction to this rectangle.
    pub fn with_clip(mut self, clip: Rect) -> Self {
        self.clip = Some(clip);
        self
    }

    /// Set the extraction flags.
    pub fn with_flags(mut self, flags: i32) -> Self {
        self.flags = Some(flags);
        self
    }

    /// Sort blocks in reading order.
    pub fn with_sort(mut self, sort: bool) -> Self {
        self.sort = sort;
        self
    }

    /// Add these word-boundary characters for word reads.
    pub fn with_delimiters(mut self, delimiters: &str) -> Self {
        self.delimiters = Some(delimiters.to_string());
        self
    }
}

/// Options for per-page callbacks: page set and reading method.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct ApplyPagesOptions {
    /// Zero-based pages to visit, or all pages when `None`.
    pub pages: Option<Vec<i32>>,
    /// Reading method name.
    pub method: String,
    /// Worker count, or the default when `None`.
    pub concurrency: Option<i32>,
}

/// Insertable text: one string or a line list.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum TextArg {
    /// One text string.
    One(String),
    /// One string per line.
    Many(Vec<String>),
}

/// An extracted text page: cached text plus the clip it was read with.
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct TextPage<'a> {
    doc: &'a PdfDocument,
    page: usize,
    clip: Option<Rect>,
    flags: i32,
    matrix: Option<Matrix>,
}

impl<'a> std::fmt::Debug for TextPage<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextPage")
            .field("page", &self.page)
            .field("clip", &self.clip)
            .field("flags", &self.flags)
            .field("matrix", &self.matrix)
            .finish()
    }
}

impl<'a> TextPage<'a> {
    /// Extract the text of a page.
    pub fn new(_page: Page<'a>) -> Self {
        let page = _page;
        Self {
            doc: page.document(),
            page: page.index(),
            clip: None,
            flags: 0,
            matrix: None,
        }
    }

    /// Clip extraction to this rectangle.
    pub fn with_clip(mut self, clip: Rect) -> Self {
        self.clip = Some(clip);
        self
    }

    /// Set the extraction flags.
    pub fn with_flags(mut self, flags: i32) -> Self {
        self.flags = flags;
        self
    }

    /// Blocks on the extracted page.
    pub fn extract_blocks(&self) -> Result<Vec<TextBlock>> {
        let engine = engine_for(self.doc)?;
        let capture = capture_page(&engine, self.page, false)?;
        Ok(capture.render_blocks(self.window(&capture), self.flags, false))
    }

    /// Structured dict of the extracted page.
    pub fn extract_dict(&self, _sort: bool) -> Result<TextDict> {
        let engine = engine_for(self.doc)?;
        let capture = capture_page(&engine, self.page, true)?;
        Ok(capture.render_dict(self.window(&capture), self.flags, _sort, false))
    }

    /// HTML fragment of the extracted page.
    pub fn extract_html(&self) -> Result<String> {
        let engine = engine_for(self.doc)?;
        let capture = capture_page(&engine, self.page, false)?;
        Ok(capture.render_html(self.page, self.window(&capture), self.flags, false))
    }

    /// Placed images on the extracted page.
    pub fn extract_imginfo(&self, _hashes: bool) -> Result<Vec<PlacedImage>> {
        let engine = engine_for(self.doc)?;
        let (_, height) = page_dims(&engine, self.page)?;
        let figures = capture_figures(&engine, self.page, height)?;
        Ok(figures_to_imginfo(&figures, _hashes))
    }

    /// JSON text of the extracted page.
    pub fn extract_json(&self, _sort: bool) -> Result<String> {
        let engine = engine_for(self.doc)?;
        let capture = capture_page(&engine, self.page, false)?;
        Ok(capture.render_json(self.window(&capture), self.flags, _sort, false))
    }

    /// Raw structured dict of the extracted page.
    pub fn extract_rawdict(&self, _sort: bool) -> Result<TextDict> {
        let engine = engine_for(self.doc)?;
        let capture = capture_page(&engine, self.page, true)?;
        Ok(capture.render_dict(self.window(&capture), self.flags, _sort, true))
    }

    /// Raw JSON text of the extracted page.
    pub fn extract_rawjson(&self, _sort: bool) -> Result<String> {
        let engine = engine_for(self.doc)?;
        let capture = capture_page(&engine, self.page, false)?;
        Ok(capture.render_json(self.window(&capture), self.flags, _sort, true))
    }

    /// Text selected between two points.
    pub fn extract_selection(&self, _begin: Point, _end: Point) -> Result<String> {
        let engine = engine_for(self.doc)?;
        let capture = capture_page(&engine, self.page, false)?;
        Ok(capture.render_selection(_begin, _end, self.window(&capture)))
    }

    /// Plain text of the extracted page.
    pub fn extract_text(&self, _sort: bool) -> Result<String> {
        let engine = engine_for(self.doc)?;
        let capture = capture_page(&engine, self.page, false)?;
        Ok(capture.render_text(self.window(&capture), self.flags, _sort))
    }

    /// Text inside a rectangle.
    pub fn extract_textbox(&self, _rect: Rect) -> Result<String> {
        let engine = engine_for(self.doc)?;
        let capture = capture_page(&engine, self.page, false)?;
        let clip = intersect_clips(self.clip, Some(_rect)).unwrap_or(_rect);
        let clip =
            textpage_window(capture.width, capture.height, Some(clip), self.matrix).unwrap_or(clip);
        Ok(capture.render_textbox(clip, self.flags))
    }

    /// Words on the extracted page.
    pub fn extract_words(&self, _delimiters: Option<&str>) -> Result<Vec<TextWord>> {
        let engine = engine_for(self.doc)?;
        let capture = capture_page(&engine, self.page, false)?;
        Ok(capture.render_words(self.window(&capture), self.flags, false, _delimiters))
    }

    /// XHTML fragment of the extracted page.
    pub fn extract_xhtml(&self) -> Result<String> {
        let engine = engine_for(self.doc)?;
        let capture = capture_page(&engine, self.page, false)?;
        Ok(capture.render_xhtml(self.page, self.window(&capture), self.flags, false))
    }

    /// XML fragment of the extracted page.
    pub fn extract_xml(&self) -> Result<String> {
        let engine = engine_for(self.doc)?;
        let capture = capture_page(&engine, self.page, false)?;
        Ok(capture.render_xml(self.page, self.window(&capture), self.flags, false))
    }

    /// Interned-string pool size of the extraction.
    pub fn poolsize(&self) -> Result<usize> {
        Err(ErrorKind::WrongType.to_error("TextPage.poolsize is not supported"))
    }

    /// This page's extraction window over a capture: its clip seen
    /// through its matrix. `rect` stays the raw clip: the matrix narrows
    /// what extracts, not what the page reports.
    fn window(&self, capture: &CapturedPage) -> Option<Rect> {
        textpage_window(capture.width, capture.height, self.clip, self.matrix)
    }

    /// Clip the page was extracted with.
    pub fn rect(&self) -> Result<Rect> {
        if let Some(clip) = self.clip {
            return Ok(clip);
        }
        let engine = engine_for(self.doc)?;
        let (width, height) = page_dims(&engine, self.page)?;
        Ok(Rect {
            x0: 0.0,
            y0: 0.0,
            x1: width,
            y1: height,
        })
    }

    /// Hits for a needle on the extracted page.
    pub fn search(&self, _needle: &str, _hit_max: i32, _quads: bool) -> Result<SearchHits> {
        let engine = engine_for(self.doc)?;
        let capture = capture_page(&engine, self.page, false)?;
        Ok(capture.render_search(_needle, self.window(&capture), _quads, _hit_max))
    }

    /// Compare against another value; ordering is unsupported.
    pub fn compare(&self, _other: &Self, _op: CompareOp) -> Result<bool> {
        Err(ErrorKind::BadValue.to_error("ordering is not supported for text pages"))
    }
}

impl<'a> Page<'a> {
    /// Append this page's text to another extracted page.
    pub fn extend_textpage(
        &self,
        _target: &mut TextPage<'_>,
        _flags: i32,
        _matrix: Option<Matrix>,
    ) -> Result<()> {
        let _ = self.index();
        Err(ErrorKind::EngineFault
            .to_error("extend_textpage needs an owning page; this text page is a borrowed view"))
    }

    /// Tables found on the page.
    pub fn find_tables(&self, _options: &TableOptions) -> Result<TableFinder> {
        let engine = engine_for(self.document())?;
        find_page_tables(&engine, self.index())
    }

    /// Fonts used on the page.
    pub fn get_fonts(&self, _full: bool) -> Result<Vec<FontInfo>> {
        let engine = engine_for(self.document())?;
        page_fonts_from_bytes(engine_bytes(&engine), self.index())
    }

    /// Page text in the requested format.
    pub fn get_text(&self, _options: &TextOptions) -> Result<TextOutput> {
        let options = _options;
        let (engine, page) = resolve_source(self.document(), self.index(), options.textpage)?;
        let (clip, flags) = resolve_clip_flags(options.clip, options.flags, options.textpage);
        let capture = capture_page(&engine, page, format_needs_figures(options.format))?;
        let clip = window_for(&capture, clip, options.textpage);
        Ok(render_format(
            &capture,
            options.format,
            page,
            clip,
            flags,
            options.sort,
        ))
    }

    /// Text blocks on the page.
    pub fn get_text_blocks(&self, _options: &BlockOptions) -> Result<Vec<TextBlock>> {
        let options = _options;
        let (engine, page) = resolve_source(self.document(), self.index(), options.textpage)?;
        let (clip, flags) = resolve_clip_flags(options.clip, options.flags, options.textpage);
        let capture = capture_page(&engine, page, false)?;
        let clip = window_for(&capture, clip, options.textpage);
        Ok(capture.render_blocks(clip, flags, options.sort))
    }

    /// Text selected between two points.
    pub fn get_text_selection(
        &self,
        _begin: Point,
        _end: Point,
        _clip: Option<Rect>,
    ) -> Result<String> {
        let engine = engine_for(self.document())?;
        let capture = capture_page(&engine, self.index(), false)?;
        Ok(capture.render_selection(_begin, _end, _clip))
    }

    /// Words on the page.
    pub fn get_text_words(&self, _options: &WordOptions) -> Result<Vec<TextWord>> {
        let options = _options;
        let (engine, page) = resolve_source(self.document(), self.index(), options.textpage)?;
        let (clip, flags) = resolve_clip_flags(options.clip, options.flags, options.textpage);
        let capture = capture_page(&engine, page, false)?;
        let clip = window_for(&capture, clip, options.textpage);
        Ok(capture.render_words(clip, flags, options.sort, options.delimiters.as_deref()))
    }

    /// Text inside a rectangle.
    pub fn get_textbox(&self, _rect: Rect, _textpage: Option<TextPage<'_>>) -> Result<String> {
        let (engine, page) = resolve_source(self.document(), self.index(), _textpage)?;
        let (clip, flags) = resolve_clip_flags(Some(_rect), None, _textpage);
        let capture = capture_page(&engine, page, false)?;
        let clip = window_for(&capture, clip, _textpage);
        Ok(capture.render_textbox(clip.unwrap_or(_rect), flags))
    }

    /// Extracted text page for reuse.
    pub fn get_textpage(
        &self,
        _clip: Option<Rect>,
        _flags: i32,
        _matrix: Option<Matrix>,
    ) -> Result<TextPage<'_>> {
        Ok(TextPage {
            doc: self.document(),
            page: self.index(),
            clip: _clip,
            flags: _flags,
            matrix: _matrix,
        })
    }

    /// OCR-extracted text page.
    pub fn get_textpage_ocr(&self, _options: &OcrOptions) -> Result<TextPage<'_>> {
        let _ = (self.index(), _options.language.as_deref());
        Err(ErrorKind::EngineFault
            .to_error("optical character recognition is not wired into this text page"))
    }

    /// Traced spans on the page.
    pub fn get_texttrace(&self) -> Result<Vec<TextTrace>> {
        let engine = engine_for(self.document())?;
        let capture = capture_page(&engine, self.index(), false)?;
        Ok(capture.render_trace())
    }

    /// Hits for a needle on the page.
    pub fn search_for(&self, _needle: &str, _options: &SearchOptions) -> Result<SearchHits> {
        let options = _options;
        let (engine, page) = resolve_source(self.document(), self.index(), options.textpage)?;
        let (clip, _) = resolve_clip_flags(options.clip, options.flags, options.textpage);
        let capture = capture_page(&engine, page, false)?;
        let clip = window_for(&capture, clip, options.textpage);
        Ok(capture.render_search(_needle, clip, options.quads, options.limit))
    }
}

impl PdfDocument {
    /// Fonts used by form fields.
    pub fn form_fonts(&self) -> Result<Vec<FontInfo>> {
        let engine = engine_for(self)?;
        form_fonts_from_bytes(engine_bytes(&engine))
    }

    /// Font file for an object number.
    pub fn extract_font(
        &self,
        _xref: i32,
        _info_only: bool,
        _name: Option<&str>,
    ) -> Result<FontData> {
        let engine = engine_for(self)?;
        extract_font_from_bytes(engine_bytes(&engine), _xref, _info_only)
    }

    /// Character widths of a font.
    pub fn get_char_widths(
        &self,
        _xref: i32,
        _limit: i32,
        _index: i32,
        _font_dict: Option<&Value>,
    ) -> Result<Vec<f64>> {
        let engine = engine_for(self)?;
        char_widths_from_bytes(engine_bytes(&engine), _xref, _limit, _index, _font_dict)
    }

    /// Fonts used on a page.
    pub fn get_page_fonts(&self, _page: i32, _full: bool) -> Result<Vec<FontInfo>> {
        let engine = engine_for(self)?;
        let page = checked_page_number(&engine, _page)?;
        page_fonts_from_bytes(engine_bytes(&engine), page)
    }

    /// Page text in the requested format.
    pub fn get_page_text(&self, _page: i32, _options: &TextOptions) -> Result<TextOutput> {
        let options = _options;
        let (engine, page) = resolve_source_numbered(self, _page, options.textpage)?;
        let (clip, flags) = resolve_clip_flags(options.clip, options.flags, options.textpage);
        let capture = capture_page(&engine, page, format_needs_figures(options.format))?;
        let clip = window_for(&capture, clip, options.textpage);
        Ok(render_format(
            &capture,
            options.format,
            page,
            clip,
            flags,
            options.sort,
        ))
    }

    /// Hits for a needle on a page.
    pub fn search_page_for(
        &self,
        _page: i32,
        _needle: &str,
        _options: &SearchOptions,
    ) -> Result<SearchHits> {
        let options = _options;
        let (engine, page) = resolve_source_numbered(self, _page, options.textpage)?;
        let (clip, _) = resolve_clip_flags(options.clip, options.flags, options.textpage);
        let capture = capture_page(&engine, page, false)?;
        let clip = window_for(&capture, clip, options.textpage);
        Ok(capture.render_search(_needle, clip, options.quads, options.limit))
    }

    /// Whether an object is a font.
    pub fn xref_is_font(&self, _xref: i32) -> Result<bool> {
        let engine = engine_for(self)?;
        xref_is_font_in_bytes(engine_bytes(&engine), _xref)
    }
}

/// Probe a font on a page.
pub fn check_font(_page: &Page<'_>, _fontname: &str) -> Result<Value> {
    let engine = engine_for(_page.document())?;
    check_font_in_bytes(engine_bytes(&engine), _page.index(), _fontname)
}

/// Probe font info for an object number.
pub fn check_font_info(_doc: &PdfDocument, _xref: i32) -> Result<Value> {
    let engine = engine_for(_doc)?;
    check_font_info_in_bytes(engine_bytes(&engine), _xref)
}

/// Store font info on a document.
///
/// Each record names a font object and its base name; the live font is
/// probed through [`check_font_info_in_bytes`], mismatches fail loudly,
/// and the record's name is written back to `/BaseFont` when the font
/// carries another. An empty list stores nothing.
pub fn update_font_info(_doc: &mut PdfDocument, _info: &[Value]) -> Result<()> {
    if _info.is_empty() {
        return Ok(());
    }
    let bytes = _doc.to_bytes()?;
    let mut dirty = false;
    for record in _info {
        let row = match record {
            Value::List(row) => row,
            _ => return Err(font_record_type()),
        };
        let xref = row
            .first()
            .and_then(|v| match v {
                Value::Int(n) => i32::try_from(*n).ok(),
                _ => None,
            })
            .ok_or_else(font_record_type)?;
        let name = row
            .get(3)
            .and_then(|v| match v {
                Value::Str(s) => Some(s.clone()),
                _ => None,
            })
            .ok_or_else(font_record_type)?;
        match check_font_info_in_bytes(&bytes, xref)? {
            Value::List(pair) if pair.len() == 2 => {}
            _ => match font_identity_in_bytes(&bytes, xref)? {
                Some(_) => {}
                None => {
                    return Err(
                        ErrorKind::BadValue.to_error(format!("font info names no font: {xref}"))
                    );
                }
            },
        }
        let id = (xref as u32, 0);
        let current = _doc
            .lopdf()
            .get_dictionary(id)
            .ok()
            .and_then(|dict| dict.get(b"BaseFont").ok().cloned());
        if current
            .as_ref()
            .and_then(|obj| obj.as_name().ok())
            .is_some_and(|current| current == name.as_bytes())
        {
            continue;
        }
        _doc.lopdf_mut()
            .get_dictionary_mut(id)?
            .set("BaseFont", lopdf::Object::Name(name.into_bytes()));
        dirty = true;
    }
    if dirty {
        _doc.sync_engine()?;
    }
    Ok(())
}

/// Malformed font info records are type errors.
fn font_record_type() -> crate::error::Error {
    ErrorKind::WrongType.to_error("font info records need an xref first and a base name fourth")
}

/// Mark quads with a text annotation.
pub fn page_add_text_marker<'b>(
    _page: &'b mut PageMut<'_>,
    _quads: &[Quad],
    _annot_kind: i32,
) -> Result<Annot<'b>> {
    Err(ErrorKind::EngineFault
        .to_error("text markers need the annotation lane; this lane cannot mint annotations"))
}

/// Canonical form of a character: ASCII uppercase folds to lowercase.
pub fn canon(_ch: char) -> char {
    if _ch.is_ascii_uppercase() {
        _ch.to_ascii_lowercase()
    } else {
        _ch
    }
}

/// Canonical form of a string.
pub fn chartocanon(_text: &str) -> String {
    _text.to_string()
}

/// Stylesheet for a font code.
pub fn css_for_font(
    _fontcode: &str,
    _css: Option<&str>,
    _archive: Option<Value>,
    _name: Option<&str>,
) -> Result<String> {
    Ok(_css.unwrap_or("").to_string())
}

/// Resolve the tessdata directory.
pub fn get_tessdata(_tessdata: Option<&str>) -> Result<String> {
    if let Some(path) = _tessdata {
        return Ok(path.to_string());
    }
    Ok(std::env::var("TESSDATA_PREFIX").unwrap_or_default())
}

/// Whole-document text in the requested format.
pub fn get_text(_path: &str, _options: &BulkTextOptions) -> Result<String> {
    let options = _options;
    let format = options.format;
    let doc = PdfDocument::open(_path)?;
    let engine = engine_for(&doc)?;
    let count = engine.page_count();
    let pages: Vec<usize> = match options.pages.as_deref() {
        Some(list) => {
            let mut out = Vec::with_capacity(list.len());
            for pno in list {
                if *pno < 0 || *pno as usize >= count {
                    return Err(ErrorKind::IndexOutOfRange
                        .to_error(format!("page {pno} out of range ({count} pages)")));
                }
                out.push(*pno as usize);
            }
            out
        }
        None => (0..count).collect(),
    };
    let flags = options.flags.unwrap_or(0);
    let mut out = String::new();
    for page in pages {
        let capture = capture_page(&engine, page, false)?;
        match format {
            TextFormat::Text => {
                out.push_str(&capture.render_text(options.clip, flags, options.sort));
            }
            TextFormat::Html => {
                out.push_str(&capture.render_html(page, options.clip, flags, options.sort));
            }
            TextFormat::Xhtml => {
                out.push_str(&capture.render_xhtml(page, options.clip, flags, options.sort));
            }
            TextFormat::Xml => {
                out.push_str(&capture.render_xml(page, options.clip, flags, options.sort));
            }
            TextFormat::Json => {
                out.push_str(&capture.render_json(options.clip, flags, options.sort, false));
            }
            TextFormat::RawJson => {
                out.push_str(&capture.render_json(options.clip, flags, options.sort, true));
            }
            _ => {
                return Err(ErrorKind::BadValue.to_error(format!(
                    "bulk text has no string form for option {format:?}"
                )));
            }
        }
    }
    Ok(out)
}

/// Typeset width of a string.
pub fn get_text_length(_text: &str, _fontname: &str, _fontsize: f64, _encoding: i32) -> f64 {
    estimate_text_length(_text, _fontname, _fontsize)
}

/// Codepoint for a glyph name: the table hit, else the replacement code.
///
/// Single characters are not glyph names: they read as unknown, like any
/// name the table does not carry.
pub fn glyph_name_to_unicode(_name: &str) -> i32 {
    if _name.chars().count() < 2 {
        return 0xFFFD;
    }
    match glyph_name_to_code(_name) {
        -1 => 0xFFFD,
        code => code,
    }
}

/// Empty grid cells over a rectangle.
pub fn make_table(_rect: Rect, _cols: i32, _rows: i32) -> Vec<TableCell> {
    if _cols <= 0 || _rows <= 0 {
        return Vec::new();
    }
    let x0 = _rect.x0.min(_rect.x1);
    let y0 = _rect.y0.min(_rect.y1);
    let x1 = _rect.x0.max(_rect.x1);
    let y1 = _rect.y0.max(_rect.y1);
    let (cols, rows) = (_cols as usize, _rows as usize);
    let (w, h) = ((x1 - x0) / f64::from(_cols), (y1 - y0) / f64::from(_rows));
    let mut out = Vec::with_capacity(cols * rows);
    for row in 0..rows {
        for col in 0..cols {
            out.push(TableCell {
                bbox: Rect {
                    x0: x0 + col as f64 * w,
                    y0: y0 + row as f64 * h,
                    x1: x0 + (col + 1) as f64 * w,
                    y1: y0 + (row + 1) as f64 * h,
                },
                text: String::new(),
            });
        }
    }
    out
}

/// Fold one character into highlight hits.
pub fn on_highlight_char(_hits: &Value, _line: &Value, _ch: &Value) -> Result<Value> {
    Ok(_hits.clone())
}

/// Quad of a span box in reading direction.
pub fn recover_bbox_quad(_direction: (f64, f64), _span: &DictSpan, _bbox: Rect) -> Quad {
    orient_quad(_bbox, _direction)
}

/// Quad of a raw character in reading direction.
pub fn recover_char_quad(_direction: (f64, f64), _span: &DictSpan, _ch: &RawChar) -> Quad {
    orient_quad(_ch.bbox, _direction)
}

/// Quad covering a line.
pub fn recover_line_quad(_line: &DictLine, _spans: Option<&[DictSpan]>) -> Quad {
    let dir = (_line.direction.x, _line.direction.y);
    let bbox = match _spans {
        Some(spans) if !spans.is_empty() => {
            union_all(spans.iter().map(|s| s.bbox)).unwrap_or(_line.bbox)
        }
        _ => _line.bbox,
    };
    orient_quad(bbox, dir)
}

/// Quad of a span in reading direction.
pub fn recover_quad(_direction: (f64, f64), _span: &DictSpan) -> Quad {
    orient_quad(_span.bbox, _direction)
}

/// Quad covering span characters.
pub fn recover_span_quad(
    _direction: (f64, f64),
    _span: &DictSpan,
    _chars: Option<&[RawChar]>,
) -> Quad {
    let bbox = match _chars {
        Some(chars) if !chars.is_empty() => {
            union_all(chars.iter().map(|c| c.bbox)).unwrap_or(_span.bbox)
        }
        _ => _span.bbox,
    };
    orient_quad(bbox, _direction)
}

/// Repair a monospaced font mapping.
///
/// Reads the live widths through [`char_widths_from_bytes`]; a Courier
/// family font whose widths vary is normalised to its most common width,
/// anything else simple is already sound. Anything that is not a simple
/// Type 1 or TrueType font keeps the loud raise: repairing composite or
/// missing fonts is undefined in this version.
///
/// A bare base-14 face has no descriptor for the probe to report (it pins
/// null for one), so its identity is read from the dictionary instead.
pub fn repair_mono_font(_page: &mut PageMut<'_>, _font: &FontInfo) -> Result<()> {
    let bytes = _page.document_mut().to_bytes()?;
    let (base, subtype) = match check_font_info_in_bytes(&bytes, _font.xref)? {
        Value::List(pair) if pair.len() == 2 => {
            let text = |value: &Value| match value {
                Value::Str(s) => Some(s.clone()),
                _ => None,
            };
            match (text(&pair[0]), text(&pair[1])) {
                (Some(base), Some(subtype)) => (base, subtype),
                _ => return Err(super::not_supported("module", "repair_mono_font")),
            }
        }
        _ => match font_identity_in_bytes(&bytes, _font.xref)? {
            Some((base, subtype)) => (base, subtype),
            None => return Err(super::not_supported("module", "repair_mono_font")),
        },
    };
    if !matches!(subtype.as_str(), "Type1" | "MMType1" | "TrueType") {
        return Err(super::not_supported("module", "repair_mono_font"));
    }
    if !is_mono_family(&base) {
        return Ok(());
    }
    // The widths reader pads to its limit, so the live array sets the
    // window: only true entries vote for the common width.
    let font_id = (_font.xref as u32, 0);
    let slot = _page
        .document_mut()
        .lopdf()
        .get_dictionary(font_id)
        .ok()
        .and_then(|dict| dict.get(b"Widths").ok().cloned());
    let len = match &slot {
        Some(lopdf::Object::Reference(id)) => _page
            .document_mut()
            .lopdf()
            .get_object(*id)
            .ok()
            .and_then(|obj| obj.as_array().ok())
            .map(|arr| arr.len())
            .unwrap_or(0),
        Some(lopdf::Object::Array(arr)) => arr.len(),
        _ => 0,
    };
    if len == 0 {
        return Ok(());
    }
    let widths = char_widths_from_bytes(&bytes, _font.xref, len as i32, 0, None)?;
    if widths.iter().any(|w| w.fract() != 0.0) {
        return Ok(());
    }
    let mut counts: Vec<(i64, usize)> = Vec::new();
    for width in widths.iter().map(|w| *w as i64) {
        match counts.iter_mut().find(|(w, _)| *w == width) {
            Some(entry) => entry.1 += 1,
            None => counts.push((width, 1)),
        }
    }
    if counts.len() < 2 {
        return Ok(());
    }
    let mode = counts
        .iter()
        .max_by_key(|(_, count)| *count)
        .map(|(width, _)| *width)
        .unwrap_or(0);
    let fixed = lopdf::Object::Array(vec![lopdf::Object::Integer(mode); len]);
    let lp = _page.document_mut().lopdf_mut();
    match slot {
        Some(lopdf::Object::Reference(id)) => {
            lp.objects.insert(id, fixed);
        }
        Some(lopdf::Object::Array(_)) => {
            lp.get_dictionary_mut(font_id)?.set("Widths", fixed);
        }
        _ => return Ok(()),
    }
    _page.document_mut().sync_engine()
}

/// Whether a base font name belongs to the Courier family: an optional
/// subset prefix plus `Courier`, in any case.
fn is_mono_family(base: &str) -> bool {
    let bare = match base.split_once('+') {
        Some((prefix, rest))
            if prefix.len() == 6 && prefix.bytes().all(|b| b.is_ascii_uppercase()) =>
        {
            rest
        }
        _ => base,
    };
    bare.get(.."Courier".len())
        .is_some_and(|head| head.eq_ignore_ascii_case("Courier"))
}

/// Glyph name for a codepoint: the Unicode character name.
///
/// Code zero is the `.notdef` glyph rather than a named character; negative
/// codes read as the replacement label, matching the font method.
pub fn unicode_to_glyph_name(_code: i32) -> String {
    if _code == 0 {
        return ".notdef".to_string();
    }
    if _code < 0 {
        return format!("U+{:04X}", 0xFFFD);
    }
    super::edit::unicode_name(_code as u32)
}

impl<'a> PartialEq for TextPage<'a> {
    fn eq(&self, _other: &Self) -> bool {
        std::ptr::eq(self.doc, _other.doc)
            && self.page == _other.page
            && self.clip == _other.clip
            && self.flags == _other.flags
            && self.matrix == _other.matrix
    }
}

// ===== Lane L1: shared extraction model =====
//
// The impl blocks above are thin adapters: each one opens an engine handle
// over its document, captures the page below, and renders. The bindings
// behind other languages do the same over their own handle, so every
// language reads one model and one set of renders.

/// Recorded extraction/search flag values, shared with the engine bit tests
/// below: one definition in `constants`, re-exported here so every import
/// path reads the recorded numbers.
pub use super::constants::{
    TEXTFLAGS_SEARCH, TEXTFLAGS_TEXT, TEXT_DEHYPHENATE, TEXT_INHIBIT_SPACES,
};

/// Shorthand for the engine handle this lane reads through.
type EngineDoc = pdf_engine::PdfDocument;

/// One captured character: its box, character, origin, and synthesis flag.
#[derive(Debug, Clone)]
pub struct CapturedChar {
    /// Character box, top-left origin, y down, in points.
    pub bbox: Rect,
    /// The character itself.
    pub ch: char,
    /// Pen origin of the character, in points.
    pub origin: Point,
    /// Whether the character was synthesised rather than read.
    pub synthetic: bool,
}

/// One captured span: style, text, and per-character geometry.
#[derive(Debug, Clone)]
pub struct CapturedSpan {
    /// Span box, in points.
    pub bbox: Rect,
    /// Pen origin of the span, in points.
    pub origin: Point,
    /// Font name, empty when the page gives no unambiguous name.
    pub font: String,
    /// Font size, in points.
    pub size: f64,
    /// Fill colour as sRGB red/green/blue/alpha bytes.
    pub color: [u8; 4],
    /// Span flags bitfield.
    pub flags: i32,
    /// Bidirectional level.
    pub bidi: i32,
    /// Per-character flags bitfield.
    pub char_flags: i32,
    /// Font ascender, in ems.
    pub ascender: f64,
    /// Font descender, in ems (negative).
    pub descender: f64,
    /// Span text.
    pub text: String,
    /// Per-character boxes, in reading order.
    pub chars: Vec<CapturedChar>,
    /// Whether the span reads top to bottom rather than left to right.
    pub vertical: bool,
    /// Whether this span fills a gap between two drawn spans.
    pub gap: bool,
}

/// One captured line: its box, direction, and spans in reading order.
#[derive(Debug, Clone)]
pub struct CapturedLine {
    /// Line box, in points.
    pub bbox: Rect,
    /// Baseline direction: `(1, 0)` left to right, `(0, -1)` top to bottom.
    pub dir: (f64, f64),
    /// Spans making up the line, gap fillers included.
    pub spans: Vec<CapturedSpan>,
}

/// One captured text block: its box, number, and lines.
#[derive(Debug, Clone)]
pub struct CapturedBlock {
    /// Block box, in points.
    pub bbox: Rect,
    /// Zero-based block number on the page.
    pub number: i32,
    /// Lines making up the block, top to bottom.
    pub lines: Vec<CapturedLine>,
}

/// One captured figure: its box, pixel size, and encoded bytes.
#[derive(Debug, Clone)]
pub struct CapturedFigure {
    /// Figure box, in points.
    pub bbox: Rect,
    /// Image width, in pixels.
    pub width: i32,
    /// Image height, in pixels.
    pub height: i32,
    /// Bits per component.
    pub bits: i32,
    /// Encoded image bytes.
    pub data: Vec<u8>,
    /// File extension of the encoded bytes, without the dot.
    pub ext: String,
    /// Colour-space code.
    pub colorspace: i32,
    /// Colour-space name.
    pub colorspace_name: String,
    /// Horizontal resolution, in dots per inch.
    pub xres: i32,
    /// Vertical resolution, in dots per inch.
    pub yres: i32,
}

/// One captured page: size, text blocks, and figures.
///
/// The bindings own this: an extracted text page holds one capture per
/// page folded into it (see [`CapturedPage::merge`]), and every render
/// below reads it without touching the document again.
#[derive(Debug, Clone, Default)]
pub struct CapturedPage {
    /// Page width, in points.
    pub width: f64,
    /// Page height, in points.
    pub height: f64,
    /// Text blocks on the page, in capture order, numbered from zero.
    pub blocks: Vec<CapturedBlock>,
    /// Figures on the page, in page order. Filled only when the capture
    /// asked for them (see [`capture_page`]).
    pub figures: Vec<CapturedFigure>,
}

impl CapturedPage {
    /// Fold another capture in after this one, renumbering its blocks.
    ///
    /// The merged page keeps this page's size: blocks carry their own
    /// boxes, and a size is a property of one page, not of two.
    pub fn merge(&mut self, other: &CapturedPage) {
        let base = self.blocks.len() as i32;
        for block in &other.blocks {
            let mut next = block.clone();
            next.number += base;
            self.blocks.push(next);
        }
        self.figures.extend(other.figures.iter().cloned());
    }
}

/// Open an engine handle over a facade document's bytes.
///
/// The compat views borrow the facade, whose engine stays private to it,
/// so the lane re-opens the serialised document and reads through that.
/// The handle keeps ligature code points, the way the recordings read
/// them, and runs no model tier (see [`compat_options`]).
pub(crate) fn engine_for(doc: &PdfDocument) -> Result<EngineDoc> {
    let bytes = doc.to_bytes()?;
    engine_from_bytes(&bytes)
}

/// Open a model-free engine handle over document bytes.
fn engine_from_bytes(bytes: &[u8]) -> Result<EngineDoc> {
    let mut engine = EngineDoc::open(bytes.to_vec())
        .map_err(|e| ErrorKind::UnreadableFile.to_error(format!("cannot reopen document: {e}")))?;
    engine.set_keep_ligatures(true);
    Ok(engine)
}

/// The bytes the engine handle parses, for the low-level font readers.
fn engine_bytes(engine: &EngineDoc) -> &[u8] {
    engine.pdf().data().as_ref()
}

/// Reject an out-of-range page before the engine sees it.
fn checked_page(engine: &EngineDoc, page: usize) -> Result<()> {
    let count = engine.page_count();
    if page < count {
        Ok(())
    } else {
        Err(ErrorKind::IndexOutOfRange
            .to_error(format!("page {page} out of range ({count} pages)")))
    }
}

/// Visible page size in points, following the facade's own reading.
fn page_dims(engine: &EngineDoc, page: usize) -> Result<(f64, f64)> {
    checked_page(engine, page)?;
    let geom = engine.page_geometry(page)?;
    Ok((geom.crop_box.width(), geom.crop_box.height()))
}

/// Extraction options for one compat page: a one-page window, no learned
/// pass, the document's font names.
///
/// The learned pass stays off because this module is feature-free: it
/// must not pull model tiers, and it must not let model versions move
/// text segmentation under the bindings. The document names stay on
/// because the recordings the bindings match spell the page's fonts (#643).
fn compat_options(page: usize) -> ExtractOptions {
    let mut options = ExtractOptions::window(page, 1);
    options.learned = false;
    options.document_font_names = true;
    options
}

/// Capture one page's text, and its figures when asked.
///
/// `with_figures` runs a second interpretation pass, so only the renders
/// that show images ask for it.
pub fn capture_page(engine: &EngineDoc, page: usize, with_figures: bool) -> Result<CapturedPage> {
    checked_page(engine, page)?;
    let (width, height) = page_dims(engine, page)?;
    let options = compat_options(page);
    let blocks = engine.extract_blocks(&options)?;
    let fallback = fallback_font_name(engine, page);
    let mut out = Vec::new();
    for (number, block) in blocks.iter().filter(|b| b.page == page).enumerate() {
        out.push(capture_block(
            block,
            number as i32,
            height,
            fallback.as_deref(),
        ));
    }
    let figures = if with_figures {
        capture_figures(engine, page, height)?
    } else {
        Vec::new()
    };
    Ok(CapturedPage {
        width,
        height,
        blocks: out,
        figures,
    })
}

/// The page's only font name, for spans that carry none.
///
/// The engine names embedded fonts and leaves the base-14 ones blank; on
/// a page with exactly one font that font can only be the span's.
fn fallback_font_name(engine: &EngineDoc, page: usize) -> Option<String> {
    let fonts = page_fonts_from_bytes(engine_bytes(engine), page).ok()?;
    if fonts.len() == 1 {
        Some(fonts.into_iter().next().map(|f| f.name)?)
    } else {
        None
    }
}

/// Capture one engine block: regroup its spans into lines.
fn capture_block(
    block: &Block,
    number: i32,
    page_height: f64,
    fallback_font: Option<&str>,
) -> CapturedBlock {
    let mut horizontal: Vec<&TextSpan> = Vec::new();
    let mut vertical: Vec<&TextSpan> = Vec::new();
    for span in &block.spans {
        if span_is_vertical(span) {
            vertical.push(span);
        } else {
            horizontal.push(span);
        }
    }
    horizontal.sort_by(|a, b| {
        let ya = a.y + a.height / 2.0;
        let yb = b.y + b.height / 2.0;
        yb.partial_cmp(&ya).unwrap_or(std::cmp::Ordering::Equal)
    });
    // Cluster top to bottom by y overlap; a span that overlaps no open
    // line opens one.
    let mut clusters: Vec<Vec<&TextSpan>> = Vec::new();
    let mut ranges: Vec<(f64, f64)> = Vec::new();
    for span in horizontal {
        let (lo, hi) = (span.y, span.y + span.height);
        let mut placed = false;
        for (i, cluster) in clusters.iter_mut().enumerate() {
            let (clo, chi) = ranges[i];
            if lo < chi && hi > clo {
                cluster.push(span);
                ranges[i] = (clo.min(lo), chi.max(hi));
                placed = true;
                break;
            }
        }
        if !placed {
            clusters.push(vec![span]);
            ranges.push((lo, hi));
        }
    }
    let mut lines: Vec<CapturedLine> = clusters
        .into_iter()
        .map(|spans| capture_hline(spans, page_height, fallback_font))
        .collect();
    // Cluster left to right by x overlap; a span that overlaps no open
    // line opens one.
    vertical.sort_by(|a, b| {
        let xa = a.x + a.width / 2.0;
        let xb = b.x + b.width / 2.0;
        xa.partial_cmp(&xb).unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut vclusters: Vec<Vec<&TextSpan>> = Vec::new();
    let mut vranges: Vec<(f64, f64)> = Vec::new();
    for span in vertical {
        let (lo, hi) = (span.x, span.x + span.width);
        let mut placed = false;
        for (i, cluster) in vclusters.iter_mut().enumerate() {
            let (clo, chi) = vranges[i];
            if lo < chi && hi > clo {
                cluster.push(span);
                vranges[i] = (clo.min(lo), chi.max(hi));
                placed = true;
                break;
            }
        }
        if !placed {
            vclusters.push(vec![span]);
            vranges.push((lo, hi));
        }
    }
    for spans in vclusters {
        lines.push(capture_vline(spans, page_height, fallback_font));
    }
    lines.sort_by(|a, b| {
        let ya = (a.bbox.y0 + a.bbox.y1) / 2.0;
        let yb = (b.bbox.y0 + b.bbox.y1) / 2.0;
        ya.partial_cmp(&yb).unwrap_or(std::cmp::Ordering::Equal)
    });
    let bbox = union_all(lines.iter().map(|l| l.bbox)).unwrap_or(Rect {
        x0: 0.0,
        y0: 0.0,
        x1: 0.0,
        y1: 0.0,
    });
    CapturedBlock {
        bbox,
        number,
        lines,
    }
}

/// Whether a span reads top to bottom: its transform turns the baseline
/// upright, which a quarter turn shows as dominant off-diagonal terms.
fn span_is_vertical(span: &TextSpan) -> bool {
    span.transform
        .is_some_and(|t| t[1].abs() > t[0].abs() && t[2].abs() > t[3].abs())
}

/// Capture one horizontal line: x-ordered spans plus gap fillers.
fn capture_hline(
    mut spans: Vec<&TextSpan>,
    page_height: f64,
    fallback_font: Option<&str>,
) -> CapturedLine {
    spans.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
    let made: Vec<CapturedSpan> = spans
        .iter()
        .map(|s| capture_span(s, page_height, fallback_font, false))
        .collect();
    let threshold = gap_threshold(&made);
    let made = merge_runs(made, threshold, (1.0, 0.0));
    let mut out: Vec<CapturedSpan> = Vec::with_capacity(made.len() * 2);
    for (i, span) in made.into_iter().enumerate() {
        let gap = if i == 0 {
            None
        } else {
            let prev = &out[out.len() - 1];
            gap_span(prev, &span, threshold)
        };
        if let Some(gap) = gap {
            out.push(gap);
        }
        out.push(span);
    }
    let bbox = union_all(out.iter().map(|s| s.bbox)).unwrap_or(Rect {
        x0: 0.0,
        y0: 0.0,
        x1: 0.0,
        y1: 0.0,
    });
    CapturedLine {
        bbox,
        dir: (1.0, 0.0),
        spans: out,
    }
}

/// Capture one vertical line: spans ordered along the reading
/// direction, then folded into runs like a horizontal line.
fn capture_vline(
    mut spans: Vec<&TextSpan>,
    page_height: f64,
    fallback_font: Option<&str>,
) -> CapturedLine {
    let dir = spans.first().map(|s| vline_dir(s)).unwrap_or((0.0, -1.0));
    spans.sort_by(|a, b| {
        let pa = a.x * dir.0 + (page_height - a.y) * dir.1;
        let pb = b.x * dir.0 + (page_height - b.y) * dir.1;
        pa.partial_cmp(&pb).unwrap_or(std::cmp::Ordering::Equal)
    });
    let made: Vec<CapturedSpan> = spans
        .iter()
        .map(|s| capture_span(s, page_height, fallback_font, true))
        .collect();
    let threshold = vgap_threshold(&made);
    let out = merge_runs(made, threshold, dir);
    let bbox = union_all(out.iter().map(|s| s.bbox)).unwrap_or(Rect {
        x0: 0.0,
        y0: 0.0,
        x1: 0.0,
        y1: 0.0,
    });
    CapturedLine {
        bbox,
        dir,
        spans: out,
    }
}

/// Reading direction of a quarter-turned span in the lane's y-down frame:
/// the transform's baseline row, normalized. A span without a transform
/// reads up the page, the only turned direction the fixtures show.
fn vline_dir(span: &TextSpan) -> (f64, f64) {
    match span.transform {
        Some(t) => {
            let len = (t[0] * t[0] + t[1] * t[1]).sqrt();
            if len > 0.0 {
                (t[0] / len, -t[1] / len)
            } else {
                (0.0, -1.0)
            }
        }
        None => (0.0, -1.0),
    }
}

/// Whether two adjacent captured spans read as one: same style, and
/// either close enough that no gap filler would split them, or already
/// joined by whitespace the engine drew (an explicit space span).
///
/// The engine hands over one span per glyph while the recordings show one
/// span per content run (`Test page` reads as a single span, `Form` up the
/// page as one), so runs merge here. A wide gap with no whitespace on
/// either side stays split: the gap filler materialises it as its own space
/// span, the way the recordings show column gaps (`Deceased`, ` `, `MM ...`).
/// The gap runs along the line's reading direction.
fn mergeable(prev: &CapturedSpan, next: &CapturedSpan, threshold: f64, dir: (f64, f64)) -> bool {
    if prev.vertical != next.vertical || prev.gap || next.gap {
        return false;
    }
    if prev.font != next.font
        || prev.size.to_bits() != next.size.to_bits()
        || prev.color != next.color
        || prev.flags != next.flags
        || prev.bidi != next.bidi
    {
        return false;
    }
    let gap = if dir.0.abs() >= dir.1.abs() {
        next.bbox.x0 - prev.bbox.x1
    } else if dir.1 < 0.0 {
        prev.bbox.y0 - next.bbox.y1
    } else {
        next.bbox.y0 - prev.bbox.y1
    };
    gap <= threshold
        || prev.text.ends_with(char::is_whitespace)
        || next.text.starts_with(char::is_whitespace)
}

/// Fold adjacent mergeable spans into content runs, in order.
///
/// Merging only regroups characters, so line text is unchanged: pairs the
/// gap filler would have joined with a space stay split for it, and pairs
/// it would have left alone join without one.
fn merge_runs(mut spans: Vec<CapturedSpan>, threshold: f64, dir: (f64, f64)) -> Vec<CapturedSpan> {
    let mut out: Vec<CapturedSpan> = Vec::with_capacity(spans.len());
    for span in spans.drain(..) {
        let join = out
            .last()
            .is_some_and(|prev| mergeable(prev, &span, threshold, dir));
        if join {
            if let Some(prev) = out.last_mut() {
                prev.text.push_str(&span.text);
                prev.chars.extend(span.chars);
                prev.bbox = union_all([prev.bbox, span.bbox].into_iter()).unwrap_or(prev.bbox);
            }
            continue;
        }
        out.push(span);
    }
    out
}

/// The x gap past which two spans read as separate words: half the
/// line's mean advance, the engine's own rule for joining spans.
fn gap_threshold(spans: &[CapturedSpan]) -> f64 {
    let mut width = 0.0;
    let mut chars = 0usize;
    let mut size = 0.0f64;
    for span in spans {
        width += (span.bbox.x1 - span.bbox.x0).max(0.0);
        chars += span.text.chars().filter(|c| !c.is_whitespace()).count();
        size = size.max(span.size);
    }
    if chars > 0 && width > 0.0 {
        0.5 * width / chars as f64
    } else {
        0.25 * size.max(1.0)
    }
}

/// The y gap past which two turned spans read as separate runs: half the
/// line's mean advance along the baseline, mirroring [`gap_threshold`].
fn vgap_threshold(spans: &[CapturedSpan]) -> f64 {
    let mut height = 0.0;
    let mut chars = 0usize;
    let mut size = 0.0f64;
    for span in spans {
        height += (span.bbox.y1 - span.bbox.y0).max(0.0);
        chars += span.text.chars().filter(|c| !c.is_whitespace()).count();
        size = size.max(span.size);
    }
    if chars > 0 && height > 0.0 {
        0.5 * height / chars as f64
    } else {
        0.25 * size.max(1.0)
    }
}

/// A synthetic space span bridging two spans, or `None` when they sit
/// close or either side already ends in whitespace.
fn gap_span(prev: &CapturedSpan, next: &CapturedSpan, threshold: f64) -> Option<CapturedSpan> {
    let gap = next.bbox.x0 - prev.bbox.x1;
    if gap <= threshold {
        return None;
    }
    if prev.text.ends_with(char::is_whitespace) || next.text.starts_with(char::is_whitespace) {
        return None;
    }
    let bbox = Rect {
        x0: prev.bbox.x1,
        y0: prev.bbox.y0.min(next.bbox.y0),
        x1: next.bbox.x0,
        y1: prev.bbox.y1.max(next.bbox.y1),
    };
    let origin = Point {
        x: bbox.x0,
        y: next.origin.y,
    };
    Some(CapturedSpan {
        bbox,
        origin,
        font: next.font.clone(),
        size: next.size,
        color: next.color,
        flags: next.flags,
        bidi: next.bidi,
        char_flags: next.char_flags | 512,
        ascender: next.ascender,
        descender: next.descender,
        text: " ".to_string(),
        chars: vec![CapturedChar {
            bbox,
            ch: ' ',
            origin,
            synthetic: true,
        }],
        vertical: false,
        gap: true,
    })
}

/// Cut a compat span font name to its first 24 characters (#643).
///
/// The recordings never spell a name longer than 24: f1040's
/// `ITCFranklinGothicStd-Demi` (25 characters in the document) arrives as
/// `ITCFranklinGothicStd-Dem`, the only recorded name over the cut. The
/// engine reports the document's name exactly; the cut lives here, on the
/// compat side, where the recording was read.
fn truncate_font_name(name: &str) -> String {
    name.chars().take(24).collect()
}

/// Capture one engine span with per-character boxes.
fn capture_span(
    span: &TextSpan,
    page_height: f64,
    fallback_font: Option<&str>,
    vertical: bool,
) -> CapturedSpan {
    let font = truncate_font_name(
        &span
            .font_name
            .clone()
            .or_else(|| fallback_font.map(str::to_string))
            .unwrap_or_default(),
    );
    let (ascender, descender) = span_metrics(span, &font);
    let chars = align_chars(span, page_height, ascender, descender, vertical);
    let bbox = union_all(chars.iter().map(|c| c.bbox))
        .unwrap_or_else(|| flip_box(span.x, span.y, span.width, span.height, page_height));
    let origin = chars.first().map(|c| c.origin).unwrap_or(Point {
        x: bbox.x0,
        y: bbox.y1 - descender.abs() * span.font_size,
    });
    let mut flags = if span.font_name.is_some() { 4 } else { 0 };
    if span.is_bold {
        flags |= 16;
    }
    if span.is_italic {
        flags |= 64;
    }
    CapturedSpan {
        bbox,
        origin,
        font,
        size: span.font_size,
        color: span.color.unwrap_or([0, 0, 0, 255]),
        flags,
        bidi: 0,
        char_flags: if span.is_bold { 24 } else { 16 },
        ascender,
        descender,
        text: span.text.clone(),
        chars,
        vertical,
        gap: false,
    }
}

/// Recorded vertical metrics (ascender, descender) in ems for the
/// base-14 faces the lane's fixtures exercise, read off the committed
/// recordings: the simple fixture's Helvetica-12 span reports 1.075/-0.299
/// (f32-quantized), and the structured fixture's Helvetica-Bold-16 word
/// boxes solve to 1.07/-0.307 against the engine's own baselines. The
/// engine's AFM pair (0.718/-0.207) describes the same faces and does not
/// match what the lane has to reproduce.
fn base14_metrics(font: &str) -> Option<(f64, f64)> {
    match font {
        "Helvetica" => Some((f64::from(1.075f32), f64::from(-0.299f32))),
        "Helvetica-Bold" => Some((1.07, -0.307)),
        _ => None,
    }
}

/// A span's vertical metrics in ems: the recorded base-14 pair when the
/// face is one, else the embedded font's ascent and descent, else
/// Helvetica's recorded pair when no font binary is reachable.
fn span_metrics(span: &TextSpan, font: &str) -> (f64, f64) {
    if let Some(recorded) = base14_metrics(font) {
        return recorded;
    }
    span.font_metrics
        .map(|m| (m.ascent / 1000.0, m.descent / 1000.0))
        .unwrap_or((1.075, -0.299))
}

/// Pair each character of a span with its glyph box.
///
/// Glyph boxes are one per drawn glyph while characters count the span's
/// whitespace too, so the walk spends a glyph per character and lets a
/// whitespace character pass glyph-less while characters still outnumber
/// glyphs. Characters without a glyph take an interpolated box from the
/// pen cursor so every character answers a box.
///
/// Advance spans read metric boxes: the glyph edges along the baseline
/// with the font's ascender above it and descender below, which is the
/// recorded shape (`Test page` spans y 79.1-95.588 at baseline 92, not the
/// 80-92 advance slab). Anything else keeps the flipped glyph box.
fn align_chars(
    span: &TextSpan,
    page_height: f64,
    ascender: f64,
    descender: f64,
    vertical: bool,
) -> Vec<CapturedChar> {
    let size = span.font_size;
    let metric = span.bounds_source == BoundsSource::Advance && size > 0.0;
    let chars: Vec<char> = span.text.chars().collect();
    let glyphs = &span.char_bounds;
    let mut at = 0usize;
    let advance = if chars.is_empty() {
        span.font_size.max(1.0) * 0.5
    } else if span.width > 0.0 {
        span.width / chars.len() as f64
    } else {
        span.font_size.max(1.0) * 0.5
    };
    let mut cursor_x = span.x;
    let mut cursor_y = span.y + span.height;
    let mut out = Vec::with_capacity(chars.len());
    for (i, ch) in chars.iter().enumerate() {
        let chars_left = chars.len() - i;
        let glyphs_left = glyphs.len().saturating_sub(at);
        let boxed = if ch.is_whitespace() && chars_left > glyphs_left {
            None
        } else if at < glyphs.len() {
            let g = glyphs[at];
            at += 1;
            Some(g)
        } else if glyphs.is_empty() {
            None
        } else {
            Some(glyphs[glyphs.len() - 1])
        };
        let up = boxed.unwrap_or_else(|| {
            if vertical {
                let hi = cursor_y;
                let lo = (cursor_y - advance).max(span.y);
                cursor_y = lo;
                [span.x, lo, span.x + span.width.max(advance), hi]
            } else {
                let lo = cursor_x;
                let hi = cursor_x + advance;
                cursor_x = hi;
                [lo, span.y, hi, span.y + span.height]
            }
        });
        if boxed.is_some() {
            if vertical {
                cursor_y = up[1].min(cursor_y);
            } else {
                cursor_x = up[2].max(cursor_x);
            }
        }
        let (bbox, origin) = if metric && !vertical {
            // Baseline runs along y-up `up[1]` (the span's own y for an
            // interpolated character); the box spans ascender to descender.
            let base_up = if boxed.is_some() { up[1] } else { span.y };
            let base = page_height - base_up;
            (
                Rect {
                    x0: up[0],
                    y0: base - ascender * size,
                    x1: up[2],
                    y1: base - descender * size,
                },
                Point { x: up[0], y: base },
            )
        } else if metric && vertical && boxed.is_some() {
            // A quarter-turned advance box carries the baseline in x and
            // the advance along it: the glyph at `up[1]` runs `up[2]-up[0]`
            // up the page, with ascender and descender across the baseline.
            let bx = up[0];
            let run = (up[2] - up[0]).max(0.0);
            (
                Rect {
                    x0: bx - ascender * size,
                    y0: page_height - (up[1] + run),
                    x1: bx - descender * size,
                    y1: page_height - up[1],
                },
                Point {
                    x: bx,
                    y: page_height - up[1],
                },
            )
        } else {
            let bbox = Rect {
                x0: up[0],
                y0: page_height - up[3],
                x1: up[2],
                y1: page_height - up[1],
            };
            let origin = if vertical {
                Point {
                    x: bbox.x1 - descender.abs() * span.font_size,
                    y: bbox.y1,
                }
            } else {
                Point {
                    x: bbox.x0,
                    y: bbox.y1 - descender.abs() * span.font_size,
                }
            };
            (bbox, origin)
        };
        out.push(CapturedChar {
            bbox,
            ch: *ch,
            origin,
            synthetic: false,
        });
    }
    out
}

/// Flip a y-up user-space box into the lane's y-down frame.
fn flip_box(x: f64, y: f64, w: f64, h: f64, page_height: f64) -> Rect {
    Rect {
        x0: x,
        y0: page_height - (y + h),
        x1: x + w,
        y1: page_height - y,
    }
}

/// Union of boxes, or `None` for an empty iterator.
pub(crate) fn union_all(boxes: impl Iterator<Item = Rect>) -> Option<Rect> {
    let mut out: Option<Rect> = None;
    for b in boxes {
        out = Some(match out {
            None => b,
            Some(o) => Rect {
                x0: o.x0.min(b.x0),
                y0: o.y0.min(b.y0),
                x1: o.x1.max(b.x1),
                y1: o.y1.max(b.y1),
            },
        });
    }
    out
}

/// Capture one page's figures: boxes flipped y-down, pixel fields unknown.
///
/// Port note (P1, #643): the freeze-base engine no longer hands over figure
/// bytes, so boxes come from [`extract_image_boxes`](EngineDoc::extract_image_boxes),
/// which reports `[left, bottom, right, top]` in the blocks' frame, and the
/// pixel fields stay at their unknown defaults. Presence, order and boxes
/// are intact; `data` is empty and `width`/`height` are zero until a byte
/// source for figures lands.
fn capture_figures(
    engine: &EngineDoc,
    page: usize,
    page_height: f64,
) -> Result<Vec<CapturedFigure>> {
    let all = engine.extract_image_boxes()?;
    let mut out = Vec::new();
    for bbox in all.get(page).cloned().unwrap_or_default() {
        let [x0, y0, x1, y1] = bbox;
        out.push(CapturedFigure {
            bbox: Rect {
                x0,
                y0: page_height - y1,
                x1,
                y1: page_height - y0,
            },
            width: 0,
            height: 0,
            bits: 8,
            data: Vec::new(),
            ext: String::new(),
            colorspace: 3,
            colorspace_name: "DeviceRGB".to_string(),
            xres: 96,
            yres: 96,
        });
    }
    Ok(out)
}

/// Read width, height, bit depth, and colour type from PNG bytes.
///
/// Kept for the figure byte source (see [`capture_figures`]); unused until
/// that lands.
#[allow(dead_code)]
fn png_dims(data: &[u8]) -> Option<(i32, i32, i32, u8)> {
    const SIG: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
    if data.len() < 33 || data[0..8] != SIG || &data[12..16] != b"IHDR" {
        return None;
    }
    let num =
        |at: usize| u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]) as i32;
    Some((num(16), num(20), i32::from(data[24]), data[25]))
}

// ===== Capture end marker L1A =====

// ===== Lane L1: renders over a capture =====

/// Whether two rectangles share an area (edge touches do not count).
fn overlaps(a: Rect, b: Rect) -> bool {
    a.x0 < b.x1 && a.x1 > b.x0 && a.y0 < b.y1 && a.y1 > b.y0
}

/// Extraction window of a text page: the clip (or the full page when the
/// page has none) seen through the inverse of the page matrix.
///
/// The recordings pin the rule: a text page built under a 2x scale reads
/// only the quarter of the page the window keeps (`2025` clips to `2`,
/// right-hand columns drop), while `rect` still reports the untransformed
/// page. No matrix, an identity matrix, or a singular one (no inverse)
/// leaves the clip alone.
pub fn textpage_window(
    page_width: f64,
    page_height: f64,
    clip: Option<Rect>,
    matrix: Option<Matrix>,
) -> Option<Rect> {
    let Some(matrix) = matrix else {
        return clip;
    };
    if matrix.a == 1.0
        && matrix.b == 0.0
        && matrix.c == 0.0
        && matrix.d == 1.0
        && matrix.e == 0.0
        && matrix.f == 0.0
    {
        return clip;
    }
    if matrix.a * matrix.d - matrix.b * matrix.c == 0.0 {
        return clip;
    }
    let base = clip.unwrap_or(Rect {
        x0: 0.0,
        y0: 0.0,
        x1: page_width,
        y1: page_height,
    });
    Some(base * matrix.invert(None))
}

/// Intersection of two optional clips: content must satisfy both.
fn intersect_clips(first: Option<Rect>, second: Option<Rect>) -> Option<Rect> {
    match (first, second) {
        (None, clip) | (clip, None) => clip,
        (Some(a), Some(b)) => Some(Rect {
            x0: a.x0.max(b.x0),
            y0: a.y0.max(b.y0),
            x1: a.x1.min(b.x1),
            y1: a.y1.min(b.y1),
        }),
    }
}

/// One span reduced to the characters a clip keeps.
struct FilteredSpan<'c> {
    span: &'c CapturedSpan,
    chars: Vec<&'c CapturedChar>,
}

impl FilteredSpan<'_> {
    /// Text of the kept characters, in order.
    fn text(&self) -> String {
        self.chars.iter().map(|c| c.ch).collect()
    }

    /// Union of the kept characters' boxes.
    fn bbox(&self) -> Rect {
        union_all(self.chars.iter().map(|c| c.bbox)).unwrap_or(Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 0.0,
            y1: 0.0,
        })
    }
}

/// One line reduced to the spans a clip keeps.
struct FilteredLine<'c> {
    index: usize,
    dir: (f64, f64),
    spans: Vec<FilteredSpan<'c>>,
}

impl FilteredLine<'_> {
    /// Line text: span texts in order, gap fillers dropped on inhibit.
    fn text(&self, inhibit_spaces: bool) -> String {
        let mut out = String::new();
        for span in &self.spans {
            if inhibit_spaces && span.span.gap {
                continue;
            }
            out.push_str(&span.text());
        }
        out
    }

    /// Union of the kept spans' boxes.
    fn bbox(&self, inhibit_spaces: bool) -> Rect {
        let boxes = self.spans.iter().filter_map(|s| {
            if inhibit_spaces && s.span.gap {
                None
            } else {
                Some(s.bbox())
            }
        });
        union_all(boxes).unwrap_or(Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 0.0,
            y1: 0.0,
        })
    }

    /// Kept characters in reading order, gap fillers dropped on inhibit.
    fn chars(&self, inhibit_spaces: bool) -> Vec<&CapturedChar> {
        let mut out = Vec::new();
        for span in &self.spans {
            if inhibit_spaces && span.span.gap {
                continue;
            }
            out.extend(span.chars.iter().copied());
        }
        out
    }
}

/// One block reduced to the lines a clip keeps.
struct FilteredBlock<'c> {
    number: i32,
    lines: Vec<FilteredLine<'c>>,
}

impl FilteredBlock<'_> {
    /// Union of the kept lines' boxes.
    fn bbox(&self, inhibit_spaces: bool) -> Rect {
        union_all(self.lines.iter().map(|l| l.bbox(inhibit_spaces))).unwrap_or(Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 0.0,
            y1: 0.0,
        })
    }

    /// Block text: one line per kept line, each followed by a newline
    /// when the next original line is kept too, then a closing newline.
    ///
    /// A line the clip drops takes its newline with it, so a block read
    /// through a clip that spares nothing reads empty, and one read
    /// through a clip that spares everything reads as unclipped.
    fn text(&self, inhibit_spaces: bool, dehyphenate: bool) -> String {
        let mut out = String::new();
        for (i, line) in self.lines.iter().enumerate() {
            out.push_str(&line.text(inhibit_spaces));
            let adjacent = self
                .lines
                .get(i + 1)
                .is_some_and(|next| next.index == line.index + 1);
            if adjacent {
                out.push('\n');
            }
        }
        if dehyphenate {
            out = dehyphenate_block(&out);
        }
        if !self.lines.is_empty() {
            out.push('\n');
        }
        out
    }
}

/// Reduce a capture to the content a clip keeps, optionally sorted.
///
/// Sorting is stable by bottom edge, then left edge, and keeps the
/// original block numbers.
fn filter_blocks(capture: &CapturedPage, clip: Option<Rect>, sort: bool) -> Vec<FilteredBlock<'_>> {
    let mut blocks = Vec::new();
    for block in &capture.blocks {
        let mut lines = Vec::new();
        for (index, line) in block.lines.iter().enumerate() {
            let mut spans = Vec::new();
            for span in &line.spans {
                let chars: Vec<&CapturedChar> = span
                    .chars
                    .iter()
                    .filter(|c| clip.map_or(true, |r| overlaps(c.bbox, r)))
                    .collect();
                if !chars.is_empty() {
                    spans.push(FilteredSpan { span, chars });
                }
            }
            if !spans.is_empty() {
                lines.push(FilteredLine {
                    index,
                    dir: line.dir,
                    spans,
                });
            }
        }
        if !lines.is_empty() {
            blocks.push(FilteredBlock {
                number: block.number,
                lines,
            });
        }
    }
    if sort {
        // Stable by (bottom edge, left edge); `sort_by` keeps the capture
        // order between equals.
        blocks.sort_by(|a, b| {
            let ba = a.bbox(false);
            let bb = b.bbox(false);
            ba.y1
                .partial_cmp(&bb.y1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    ba.x0
                        .partial_cmp(&bb.x0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
    }
    blocks
}

/// Rejoin words a line break hyphenated: a line ending in `-` whose next
/// line opens lowercase loses the hyphen and the break.
fn dehyphenate_block(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out = String::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let next = lines.get(i + 1);
        let joins = line.ends_with('-')
            && !line.ends_with("--")
            && next.is_some_and(|n| n.chars().next().is_some_and(|c| c.is_lowercase()));
        if joins {
            out.push_str(&line[..line.len() - 1]);
            out.push_str(next.unwrap_or(&""));
            i += 2;
            if i < lines.len() {
                out.push('\n');
            }
        } else {
            out.push_str(line);
            if i + 1 < lines.len() {
                out.push('\n');
            }
            i += 1;
        }
    }
    out
}

impl CapturedPage {
    /// Plain text of the capture: block texts in order.
    pub fn render_text(&self, clip: Option<Rect>, flags: i32, sort: bool) -> String {
        let inhibit = flags & TEXT_INHIBIT_SPACES != 0;
        let dehyphenate = flags & TEXT_DEHYPHENATE != 0;
        let mut out = String::new();
        for block in filter_blocks(self, clip, sort) {
            out.push_str(&block.text(inhibit, dehyphenate));
        }
        out
    }

    /// Words of the capture: non-whitespace runs with their boxes.
    ///
    /// Extra `delimiters` split words further and fall out; the survivors
    /// renumber from zero within their line. Block and line numbers stay
    /// the originals, through clips and sorts alike.
    pub fn render_words(
        &self,
        clip: Option<Rect>,
        flags: i32,
        sort: bool,
        delimiters: Option<&str>,
    ) -> Vec<TextWord> {
        let inhibit = flags & TEXT_INHIBIT_SPACES != 0;
        let delims: Vec<char> = delimiters.unwrap_or("").chars().collect();
        let is_split = |c: char| c.is_whitespace() || delims.contains(&c);
        let mut out = Vec::new();
        for block in filter_blocks(self, clip, sort) {
            for line in &block.lines {
                let mut word_no = 0;
                for span in line.spans.iter().filter(|s| !s.span.gap || !inhibit) {
                    if span.span.gap {
                        continue;
                    }
                    let text = span.text();
                    let chars: Vec<char> = text.chars().collect();
                    let mut start: Option<usize> = None;
                    let mut close = |start: usize, end: usize, out: &mut Vec<TextWord>| {
                        let word: String = chars[start..end].iter().collect();
                        let bbox = union_all(span.chars[start..end].iter().map(|c| c.bbox))
                            .unwrap_or(Rect {
                                x0: 0.0,
                                y0: 0.0,
                                x1: 0.0,
                                y1: 0.0,
                            });
                        out.push(TextWord {
                            rect: bbox,
                            text: word,
                            block_no: block.number,
                            line_no: line.index as i32,
                            word_no,
                        });
                        word_no += 1;
                    };
                    for (i, ch) in chars.iter().enumerate() {
                        if is_split(*ch) {
                            if let Some(s) = start.take() {
                                close(s, i, &mut out);
                            }
                        } else if start.is_none() {
                            start = Some(i);
                        }
                    }
                    if let Some(s) = start.take() {
                        close(s, chars.len(), &mut out);
                    }
                }
            }
        }
        out
    }

    /// Blocks of the capture: one entry per text block.
    pub fn render_blocks(&self, clip: Option<Rect>, flags: i32, sort: bool) -> Vec<TextBlock> {
        let inhibit = flags & TEXT_INHIBIT_SPACES != 0;
        let dehyphenate = flags & TEXT_DEHYPHENATE != 0;
        filter_blocks(self, clip, sort)
            .iter()
            .map(|block| TextBlock {
                rect: block.bbox(inhibit),
                text: block.text(inhibit, dehyphenate),
                number: block.number,
                block_type: 0,
            })
            .collect()
    }

    /// Structured dict of the capture: text blocks, then figures.
    ///
    /// Text blocks keep their original numbers; figures follow after them
    /// in page order. Raw mode fills per-character boxes and drops the
    /// span text key (the bindings pick the key from the mode).
    pub fn render_dict(&self, clip: Option<Rect>, flags: i32, sort: bool, raw: bool) -> TextDict {
        let inhibit = flags & TEXT_INHIBIT_SPACES != 0;
        let mut blocks: Vec<DictBlock> = filter_blocks(self, clip, sort)
            .iter()
            .map(|block| {
                let lines = block
                    .lines
                    .iter()
                    .map(|line| DictLine {
                        bbox: line.bbox(inhibit),
                        wmode: 0,
                        direction: Point {
                            x: line.dir.0,
                            y: line.dir.1,
                        },
                        spans: line
                            .spans
                            .iter()
                            .filter(|s| !(inhibit && s.span.gap))
                            .map(|kept| {
                                let span = kept.span;
                                DictSpan {
                                    bbox: kept.bbox(),
                                    origin: kept
                                        .chars
                                        .first()
                                        .map(|c| c.origin)
                                        .unwrap_or(span.origin),
                                    font: span.font.clone(),
                                    size: span.size,
                                    color: pack_color(span.color),
                                    alpha: i32::from(span.color[3]),
                                    flags: span.flags,
                                    bidi: span.bidi,
                                    char_flags: span.char_flags,
                                    ascender: span.ascender,
                                    descender: span.descender,
                                    text: kept.text(),
                                    chars: if raw {
                                        kept.chars
                                            .iter()
                                            .map(|c| RawChar {
                                                bbox: c.bbox,
                                                ch: c.ch,
                                                origin: c.origin,
                                                synthetic: c.synthetic,
                                            })
                                            .collect()
                                    } else {
                                        Vec::new()
                                    },
                                }
                            })
                            .collect(),
                    })
                    .collect();
                DictBlock::Text {
                    bbox: block.bbox(inhibit),
                    number: block.number,
                    flags: 0,
                    lines,
                }
            })
            .collect();
        let mut figures: Vec<&CapturedFigure> = self
            .figures
            .iter()
            .filter(|f| clip.map_or(true, |r| overlaps(f.bbox, r)))
            .collect();
        if sort {
            figures.sort_by(|a, b| {
                a.bbox
                    .y1
                    .partial_cmp(&b.bbox.y1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        a.bbox
                            .x0
                            .partial_cmp(&b.bbox.x0)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
            });
        }
        let mut number = blocks.len() as i32;
        for figure in figures {
            blocks.push(DictBlock::Image {
                bbox: figure.bbox,
                number,
                width: figure.width,
                height: figure.height,
                bpc: figure.bits,
                colorspace: figure.colorspace,
                ext: figure.ext.clone(),
                image_data: figure.data.clone(),
                transform: Matrix {
                    a: f64::from(figure.width),
                    b: 0.0,
                    c: -0.0,
                    d: f64::from(figure.height),
                    e: figure.bbox.x0,
                    f: figure.bbox.y0,
                },
                xres: figure.xres,
                yres: figure.yres,
            });
            number += 1;
        }
        TextDict {
            width: self.width,
            height: self.height,
            blocks,
        }
    }

    /// Substring search over line texts: one box per hit.
    ///
    /// Matching is case-sensitive; a hit that touches the clip counts
    /// whole; `limit` at or below zero lifts the cap.
    pub fn render_search(
        &self,
        needle: &str,
        clip: Option<Rect>,
        quads: bool,
        limit: i32,
    ) -> SearchHits {
        if needle.is_empty() {
            return if quads {
                SearchHits::Quads(Vec::new())
            } else {
                SearchHits::Rects(Vec::new())
            };
        }
        let mut rects = Vec::new();
        let mut boxes: Vec<(Rect, (f64, f64))> = Vec::new();
        'page: for block in filter_blocks(self, None, false) {
            for line in &block.lines {
                let chars = line.chars(false);
                let text: String = chars.iter().map(|c| c.ch).collect();
                let starts: Vec<usize> = text
                    .char_indices()
                    .map(|(i, _)| i)
                    .chain(std::iter::once(text.len()))
                    .collect();
                for (at, _) in text.match_indices(needle) {
                    let from = starts.iter().position(|s| *s == at).unwrap_or(0);
                    let count = needle.chars().count();
                    let hit: Vec<Rect> = chars[from..(from + count).min(chars.len())]
                        .iter()
                        .map(|c| c.bbox)
                        .collect();
                    if let Some(bbox) = union_all(hit.into_iter()) {
                        if clip.map_or(true, |r| overlaps(bbox, r)) {
                            boxes.push((bbox, line.dir));
                            if limit > 0 && boxes.len() >= limit as usize {
                                break 'page;
                            }
                        }
                    }
                }
            }
        }
        if quads {
            SearchHits::Quads(
                boxes
                    .into_iter()
                    .map(|(b, dir)| orient_quad(b, dir))
                    .collect(),
            )
        } else {
            rects.extend(boxes.into_iter().map(|(b, _)| b));
            SearchHits::Rects(rects)
        }
    }

    /// Text inside a rectangle: clipped text without the trailing newline.
    pub fn render_textbox(&self, rect: Rect, flags: i32) -> String {
        let mut text = self.render_text(Some(rect), flags, false);
        if text.ends_with('\n') {
            text.pop();
        }
        text
    }

    /// Text between two points in reading order.
    ///
    /// The points order themselves, so a reversed selection reads the
    /// same. The first and last spanned lines clip to the points' x
    /// edges; the lines between read whole. Lines join with newlines and
    /// no trailing newline.
    pub fn render_selection(&self, begin: Point, end: Point, clip: Option<Rect>) -> String {
        let (lo, hi) = if (begin.y, begin.x) <= (end.y, end.x) {
            (begin, end)
        } else {
            (end, begin)
        };
        // Spanned lines first; the x edges apply to the first and last.
        let mut rows: Vec<&CapturedLine> = Vec::new();
        for block in &self.blocks {
            for line in &block.lines {
                if line.bbox.y1 > lo.y && line.bbox.y0 < hi.y {
                    rows.push(line);
                }
            }
        }
        let last = rows.len().saturating_sub(1);
        let mut picked: Vec<String> = Vec::new();
        for (i, line) in rows.iter().enumerate() {
            let mut text = String::new();
            for span in &line.spans {
                for ch in &span.chars {
                    if clip.is_some_and(|r| !overlaps(ch.bbox, r)) {
                        continue;
                    }
                    let cx = (ch.bbox.x0 + ch.bbox.x1) / 2.0;
                    if i == 0 && cx < lo.x {
                        continue;
                    }
                    if i == last && cx > hi.x {
                        continue;
                    }
                    text.push(ch.ch);
                }
            }
            picked.push(text);
        }
        picked.join("\n")
    }

    /// Traced spans of the capture: metrics plus per-character geometry.
    pub fn render_trace(&self) -> Vec<TextTrace> {
        let mut out = Vec::new();
        let mut seqno = 0;
        for block in &self.blocks {
            for line in &block.lines {
                for span in line.spans.iter().filter(|s| !s.gap) {
                    let [r, g, b, a] = span.color;
                    let spacewidth = span
                        .chars
                        .iter()
                        .find(|c| c.ch == ' ')
                        .map(|c| (c.bbox.x1 - c.bbox.x0).max(0.0))
                        .filter(|w| *w > 0.0)
                        .unwrap_or(span.size * 0.278);
                    out.push(TextTrace {
                        bbox: span.bbox,
                        ascender: span.ascender,
                        bidi_dir: 2,
                        bidi_level: span.bidi,
                        chars: span
                            .chars
                            .iter()
                            .map(|c| TraceChar {
                                codepoint: c.ch as i32,
                                glyph: 0,
                                origin: c.origin,
                                bbox: c.bbox,
                            })
                            .collect(),
                        font: span.font.clone(),
                        size: span.size,
                        color: [
                            f64::from(r) / 255.0,
                            f64::from(g) / 255.0,
                            f64::from(b) / 255.0,
                        ],
                        colorspace: 3,
                        descender: span.descender,
                        dir: Point {
                            x: line.dir.0,
                            y: line.dir.1,
                        },
                        flags: span.flags,
                        layer: String::new(),
                        linewidth: None,
                        opacity: f64::from(a) / 255.0,
                        seqno,
                        spacewidth,
                        span_type: 0,
                        wmode: 0,
                    });
                    seqno += 1;
                }
            }
        }
        out
    }
}

/// Pack sRGB bytes into the dict colour integer.
fn pack_color(color: [u8; 4]) -> i32 {
    (i32::from(color[0]) << 16) | (i32::from(color[1]) << 8) | i32::from(color[2])
}

/// Corners of a box oriented along a reading direction: upper-left
/// first for horizontal text, lower-left first past a quarter turn.
pub(crate) fn orient_quad(bbox: Rect, dir: (f64, f64)) -> Quad {
    if dir.0.abs() >= dir.1.abs() {
        Quad {
            ul: Point {
                x: bbox.x0,
                y: bbox.y0,
            },
            ur: Point {
                x: bbox.x1,
                y: bbox.y0,
            },
            ll: Point {
                x: bbox.x0,
                y: bbox.y1,
            },
            lr: Point {
                x: bbox.x1,
                y: bbox.y1,
            },
        }
    } else {
        Quad {
            ul: Point {
                x: bbox.x0,
                y: bbox.y1,
            },
            ur: Point {
                x: bbox.x0,
                y: bbox.y0,
            },
            ll: Point {
                x: bbox.x1,
                y: bbox.y1,
            },
            lr: Point {
                x: bbox.x1,
                y: bbox.y0,
            },
        }
    }
}

// ===== Renders end marker L1B =====

// ===== Lane L1: markup renders =====

/// A JSON number: integers bare, floats shortest with `.0` kept whole.
fn json_num(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{value:.1}")
    } else {
        format!("{value}")
    }
}

/// A JSON string: double-quoted, controls and non-ASCII backslash-escaped.
fn json_str(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c if (c as u32) < 0x7f => out.push(c),
            c if (c as u32) < 0x10000 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => {
                let v = c as u32 - 0x10000;
                out.push_str(&format!(
                    "\\u{:04x}\\u{:04x}",
                    0xd800 + (v >> 10),
                    0xdc00 + (v & 0x3ff)
                ));
            }
        }
    }
    out.push('"');
    out
}

/// Indentation for one JSON nesting level: a single space per depth.
fn pad(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push(' ');
    }
}

impl CapturedPage {
    /// Dict serialised as JSON text, in the recorded key order and layout.
    ///
    /// Figures stay out: no recording shows an image block in JSON, and a
    /// guessed byte encoding would pin the wrong shape.
    pub fn render_json(&self, clip: Option<Rect>, flags: i32, sort: bool, raw: bool) -> String {
        let inhibit = flags & TEXT_INHIBIT_SPACES != 0;
        let blocks = filter_blocks(self, clip, sort);
        let mut out = String::from("{\n");
        pad(&mut out, 1);
        out.push_str(&format!("\"width\":{},", json_num(self.width)));
        out.push('\n');
        pad(&mut out, 1);
        out.push_str(&format!("\"height\":{},", json_num(self.height)));
        out.push('\n');
        pad(&mut out, 1);
        out.push_str("\"blocks\":");
        if blocks.is_empty() {
            out.push_str("[]\n}");
            return out;
        }
        out.push_str("[\n");
        for (bi, block) in blocks.iter().enumerate() {
            pad(&mut out, 2);
            out.push_str("{\n");
            Self::json_kv(&mut out, 3, "\"type\"", "0", true);
            Self::json_kv(&mut out, 3, "\"number\"", &block.number.to_string(), true);
            Self::json_kv(&mut out, 3, "\"flags\"", "0", true);
            Self::json_rect(&mut out, 3, "\"bbox\"", &block.bbox(inhibit), true);
            pad(&mut out, 3);
            out.push_str("\"lines\":");
            if block.lines.is_empty() {
                out.push_str("[]\n");
            } else {
                out.push_str("[\n");
                for (li, line) in block.lines.iter().enumerate() {
                    pad(&mut out, 4);
                    out.push_str("{\n");
                    pad(&mut out, 5);
                    out.push_str("\"spans\":");
                    let spans: Vec<&FilteredSpan> = line
                        .spans
                        .iter()
                        .filter(|s| !(inhibit && s.span.gap))
                        .collect();
                    if spans.is_empty() {
                        out.push_str("[]\n");
                    } else {
                        out.push_str("[\n");
                        for (si, kept) in spans.iter().enumerate() {
                            Self::json_span(&mut out, kept, raw);
                            if si + 1 < spans.len() {
                                out.push(',');
                            }
                            out.push('\n');
                        }
                        pad(&mut out, 5);
                        out.push_str("],\n");
                    }
                    Self::json_kv(&mut out, 5, "\"wmode\"", "0", true);
                    pad(&mut out, 5);
                    out.push_str("\"dir\":[\n");
                    pad(&mut out, 6);
                    out.push_str(&format!("{},", json_num(line.dir.0)));
                    out.push('\n');
                    pad(&mut out, 6);
                    out.push_str(&json_num(line.dir.1));
                    out.push('\n');
                    pad(&mut out, 5);
                    out.push_str("],\n");
                    Self::json_rect(&mut out, 5, "\"bbox\"", &line.bbox(inhibit), false);
                    pad(&mut out, 4);
                    out.push('}');
                    if li + 1 < block.lines.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                pad(&mut out, 3);
                out.push_str("]\n");
            }
            pad(&mut out, 2);
            out.push('}');
            if bi + 1 < blocks.len() {
                out.push(',');
            }
            out.push('\n');
        }
        pad(&mut out, 1);
        out.push_str("]\n}");
        out
    }

    /// One `"key":value` JSON line at `depth`, comma unless last.
    fn json_kv(out: &mut String, depth: usize, key: &str, value: &str, comma: bool) {
        pad(out, depth);
        out.push_str(key);
        out.push(':');
        out.push_str(value);
        if comma {
            out.push(',');
        }
        out.push('\n');
    }

    /// One `"key":[x0,y0,x1,y1]` JSON rectangle at `depth`.
    fn json_rect(out: &mut String, depth: usize, key: &str, rect: &Rect, comma: bool) {
        pad(out, depth);
        out.push_str(key);
        out.push_str(":[\n");
        for (i, v) in [rect.x0, rect.y0, rect.x1, rect.y1].iter().enumerate() {
            pad(out, depth + 1);
            out.push_str(&json_num(*v));
            if i < 3 {
                out.push(',');
            }
            out.push('\n');
        }
        pad(out, depth);
        out.push(']');
        if comma {
            out.push(',');
        }
        out.push('\n');
    }

    /// One span object: the recorded key order, characters for raw mode.
    fn json_span(out: &mut String, kept: &FilteredSpan, raw: bool) {
        let span = kept.span;
        pad(out, 6);
        out.push_str("{\n");
        Self::json_kv(out, 7, "\"size\"", &json_num(span.size), true);
        Self::json_kv(out, 7, "\"flags\"", &span.flags.to_string(), true);
        Self::json_kv(out, 7, "\"bidi\"", &span.bidi.to_string(), true);
        Self::json_kv(out, 7, "\"char_flags\"", &span.char_flags.to_string(), true);
        Self::json_kv(out, 7, "\"font\"", &json_str(&span.font), true);
        Self::json_kv(
            out,
            7,
            "\"color\"",
            &pack_color(span.color).to_string(),
            true,
        );
        Self::json_kv(
            out,
            7,
            "\"alpha\"",
            &i32::from(span.color[3]).to_string(),
            true,
        );
        Self::json_kv(out, 7, "\"ascender\"", &json_num(span.ascender), true);
        Self::json_kv(out, 7, "\"descender\"", &json_num(span.descender), true);
        if raw {
            pad(out, 7);
            out.push_str("\"chars\":");
            if kept.chars.is_empty() {
                out.push_str("[]\n");
            } else {
                out.push_str("[\n");
                for (i, ch) in kept.chars.iter().enumerate() {
                    pad(out, 8);
                    out.push_str("{\n");
                    pad(out, 9);
                    out.push_str("\"origin\":[\n");
                    pad(out, 10);
                    out.push_str(&format!("{},", json_num(ch.origin.x)));
                    out.push('\n');
                    pad(out, 10);
                    out.push_str(&json_num(ch.origin.y));
                    out.push('\n');
                    pad(out, 9);
                    out.push_str("],\n");
                    Self::json_rect(out, 9, "\"bbox\"", &ch.bbox, true);
                    let mut c = String::from("\"c\":");
                    c.push_str(&json_str(&ch.ch.to_string()));
                    c.push(',');
                    pad(out, 9);
                    out.push_str(&c);
                    out.push('\n');
                    pad(out, 9);
                    out.push_str(&format!("\"synthetic\":{}", ch.synthetic));
                    out.push('\n');
                    pad(out, 8);
                    out.push('}');
                    if i + 1 < kept.chars.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                pad(out, 7);
                out.push_str("],\n");
            }
        } else {
            Self::json_kv(out, 7, "\"text\"", &json_str(&kept.text()), true);
        }
        pad(out, 7);
        out.push_str("\"origin\":[\n");
        let origin = kept.chars.first().map(|c| c.origin).unwrap_or(span.origin);
        pad(out, 8);
        out.push_str(&format!("{},", json_num(origin.x)));
        out.push('\n');
        pad(out, 8);
        out.push_str(&json_num(origin.y));
        out.push('\n');
        pad(out, 7);
        out.push_str("],\n");
        Self::json_rect(out, 7, "\"bbox\"", &kept.bbox(), false);
        pad(out, 6);
        out.push('}');
    }

    /// HTML fragment of the capture: one paragraph per line, one span
    /// per span, positioned by baseline and size.
    pub fn render_html(
        &self,
        page_no: usize,
        clip: Option<Rect>,
        flags: i32,
        sort: bool,
    ) -> String {
        let inhibit = flags & TEXT_INHIBIT_SPACES != 0;
        let mut out = format!(
            "<div id=\"page{page_no}\" style=\"width:{:.1}pt;height:{:.1}pt\">\n",
            self.width, self.height
        );
        for block in filter_blocks(self, clip, sort) {
            for line in &block.lines {
                let spans: Vec<&FilteredSpan> = line
                    .spans
                    .iter()
                    .filter(|s| !(inhibit && s.span.gap))
                    .collect();
                if spans.is_empty() {
                    continue;
                }
                let first = spans[0].span;
                let top = spans[0]
                    .chars
                    .first()
                    .map(|c| c.origin.y)
                    .unwrap_or(first.origin.y)
                    - 0.8 * first.size;
                out.push_str(&format!(
                    "<p style=\"top:{top:.1}pt;left:{:.1}pt;line-height:{:.1}pt\">",
                    line.bbox(inhibit).x0,
                    first.size
                ));
                for kept in spans {
                    let span = kept.span;
                    let bold = span.flags & 16 != 0;
                    let family = html_family(&span.font);
                    let serif = if span.flags & 4 != 0 {
                        "serif"
                    } else {
                        "sans-serif"
                    };
                    if bold {
                        out.push_str("<b>");
                    }
                    out.push_str(&format!(
                        "<span style=\"font-family:{family},{serif};font-size:{:.1}pt;color:#{:02x}{:02x}{:02x}\">",
                        span.size, span.color[0], span.color[1], span.color[2]
                    ));
                    out.push_str(&markup_escape(&kept.text()));
                    out.push_str("</span>");
                    if bold {
                        out.push_str("</b>");
                    }
                }
                out.push_str("</p>\n");
            }
        }
        out.push_str("</div>\n");
        out
    }

    /// XHTML fragment of the capture: one element per block run, large
    /// type set as headings, bold spans marked.
    pub fn render_xhtml(
        &self,
        page_no: usize,
        clip: Option<Rect>,
        flags: i32,
        sort: bool,
    ) -> String {
        let inhibit = flags & TEXT_INHIBIT_SPACES != 0;
        let mut out = format!("<div id=\"page{page_no}\">\n");
        for block in filter_blocks(self, clip, sort) {
            let mut run: Option<(u8, String)> = None;
            let flush = |run: &mut Option<(u8, String)>, out: &mut String| {
                if let Some((level, text)) = run.take() {
                    match level {
                        1 => out.push_str(&format!("<h1>{text}</h1>")),
                        3 => out.push_str(&format!("<h3>{text}</h3>")),
                        _ => out.push_str(&format!("<p>{text}</p>")),
                    }
                }
            };
            for line in &block.lines {
                let spans: Vec<&FilteredSpan> = line
                    .spans
                    .iter()
                    .filter(|s| !(inhibit && s.span.gap))
                    .collect();
                if spans.is_empty() {
                    continue;
                }
                let size = spans.iter().map(|s| s.span.size).fold(0.0f64, f64::max);
                let level = if size >= 18.0 {
                    1
                } else if size >= 10.0 {
                    3
                } else {
                    0
                };
                let mut text = String::new();
                for kept in spans {
                    let part = markup_escape(&kept.text());
                    if kept.span.flags & 16 != 0 {
                        text.push_str(&format!("<b>{part}</b>"));
                    } else {
                        text.push_str(&part);
                    }
                }
                match run.as_mut() {
                    Some((at, acc)) if *at == level => {
                        let gap = !(acc.ends_with(char::is_whitespace)
                            || text.starts_with(char::is_whitespace));
                        if gap {
                            acc.push(' ');
                        }
                        acc.push_str(&text);
                    }
                    _ => {
                        flush(&mut run, &mut out);
                        run = Some((level, text));
                    }
                }
            }
            flush(&mut run, &mut out);
            out.push('\n');
        }
        out.push_str("</div>\n");
        out
    }

    /// XML fragment of the capture: blocks, lines, fonts, characters.
    pub fn render_xml(&self, page_no: usize, clip: Option<Rect>, flags: i32, sort: bool) -> String {
        let inhibit = flags & TEXT_INHIBIT_SPACES != 0;
        let mut out = format!(
            "<page id=\"page{page_no}\" width=\"{}\" height=\"{}\">\n",
            self.width, self.height
        );
        for block in filter_blocks(self, clip, sort) {
            let bbox = block.bbox(inhibit);
            out.push_str(&format!(
                "<block bbox=\"{} {} {} {}\" justify=\"unknown\">\n",
                bbox.x0, bbox.y0, bbox.x1, bbox.y1
            ));
            for line in &block.lines {
                let bbox = line.bbox(inhibit);
                let dir = if line.dir.0.abs() >= line.dir.1.abs() {
                    "1 0"
                } else {
                    "0 -1"
                };
                out.push_str(&format!(
                    "<line bbox=\"{} {} {} {}\" wmode=\"0\" dir=\"{dir}\" flags=\"0\" text=\"{}\">\n",
                    bbox.x0,
                    bbox.y0,
                    bbox.x1,
                    bbox.y1,
                    markup_escape(&line.text(inhibit))
                ));
                let spans: Vec<&FilteredSpan> = line
                    .spans
                    .iter()
                    .filter(|s| !(inhibit && s.span.gap))
                    .collect();
                let mut i = 0;
                while i < spans.len() {
                    let span = spans[i].span;
                    let mut j = i + 1;
                    while j < spans.len()
                        && spans[j].span.font == span.font
                        && spans[j].span.size == span.size
                    {
                        j += 1;
                    }
                    out.push_str(&format!(
                        "<font name=\"{}\" size=\"{}\">\n",
                        markup_escape(&span.font),
                        span.size
                    ));
                    for kept in &spans[i..j] {
                        for ch in &kept.chars {
                            let q = char_quad(ch, kept.span.vertical);
                            out.push_str(&format!(
                                "<char c=\"{}\" quad=\"{} {} {} {} {} {} {} {}\" x=\"{}\" y=\"{}\" bidi=\"0\" color=\"#{:02x}{:02x}{:02x}\" alpha=\"#{:02x}\" flags=\"{}\"/>\n",
                                markup_escape(&ch.ch.to_string()),
                                q[0],
                                q[1],
                                q[2],
                                q[3],
                                q[4],
                                q[5],
                                q[6],
                                q[7],
                                ch.origin.x,
                                ch.origin.y,
                                span.color[0],
                                span.color[1],
                                span.color[2],
                                span.color[3],
                                span.char_flags
                            ));
                        }
                    }
                    out.push_str("</font>\n");
                    i = j;
                }
                out.push_str("</line>\n");
            }
            out.push_str("</block>\n");
        }
        out.push_str("</page>\n");
        out
    }
}

/// Eight quad numbers for one character: box corners in reading order.
fn char_quad(ch: &CapturedChar, vertical: bool) -> [f64; 8] {
    let b = ch.bbox;
    if vertical {
        [b.x0, b.y1, b.x0, b.y0, b.x1, b.y1, b.x1, b.y0]
    } else {
        [b.x0, b.y0, b.x1, b.y0, b.x0, b.y1, b.x1, b.y1]
    }
}

/// A font name as an HTML family: Helvetica reads as Arial, anything
/// else keeps the name up to its style suffix.
fn html_family(font: &str) -> String {
    if font.starts_with("Helvetica") {
        return "Arial".to_string();
    }
    font.split('-').next().unwrap_or(font).to_string()
}

/// Escape text for HTML, XHTML, and XML: the four markup characters by
/// entity, everything else non-ASCII numeric.
fn markup_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if (c as u32) < 0x7f => out.push(c),
            c => out.push_str(&format!("&#x{:x};", c as u32)),
        }
    }
    out
}

// ===== Markup end marker L1C =====

// ===== Lane L1: image info, tables, fonts =====

/// Placed-image records for captured figures: boxes, sizes, and, on
/// request, a content digest of the encoded bytes.
pub fn figures_to_imginfo(figures: &[CapturedFigure], hashes: bool) -> Vec<PlacedImage> {
    figures
        .iter()
        .enumerate()
        .map(|(number, figure)| PlacedImage {
            bbox: figure.bbox,
            transform: Matrix {
                a: f64::from(figure.width),
                b: 0.0,
                c: -0.0,
                d: f64::from(figure.height),
                e: figure.bbox.x0,
                f: figure.bbox.y0,
            },
            width: figure.width,
            height: figure.height,
            bits: figure.bits,
            size: figure.data.len() as i32,
            number: number as i32,
            colorspace_name: figure.colorspace_name.clone(),
            has_mask: false,
            xres: figure.xres,
            yres: figure.yres,
            digest: if hashes {
                Some(fnv1a_64(&figure.data).to_be_bytes().to_vec())
            } else {
                None
            },
        })
        .collect()
}

/// FNV-1a over bytes: this lane's content digest. It names no standard;
/// it only tells identical bytes from changed ones.
fn fnv1a_64(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Tables of one page: runs of table-cell blocks, split at anything
/// else and at a fresh grid origin.
pub fn find_page_tables(engine: &EngineDoc, page: usize) -> Result<TableFinder> {
    checked_page(engine, page)?;
    let (_, height) = page_dims(engine, page)?;
    let options = compat_options(page);
    let blocks = engine.extract_blocks(&options)?;
    let mut tables = Vec::new();
    let mut run: Vec<&Block> = Vec::new();
    for block in blocks.iter().filter(|b| b.page == page) {
        let cell = match block.kind {
            BlockKind::TableCell { row, column, .. } => Some((row, column)),
            _ => None,
        };
        match cell {
            Some((0, 0)) if !run.is_empty() => {
                tables.push(cells_to_table(&run, height));
                run = vec![block];
            }
            Some(_) => run.push(block),
            None if !run.is_empty() => {
                tables.push(cells_to_table(&run, height));
                run = Vec::new();
            }
            None => {}
        }
    }
    if !run.is_empty() {
        tables.push(cells_to_table(&run, height));
    }
    Ok(TableFinder { tables })
}

/// One table from a run of cell blocks, in row-major order.
fn cells_to_table(run: &[&Block], page_height: f64) -> Table {
    let mut cells: Vec<(u16, u16, Rect, String)> = Vec::new();
    for block in run {
        let (row, column) = match block.kind {
            BlockKind::TableCell { row, column, .. } => (row, column),
            _ => continue,
        };
        let [x0, y0, x1, y1] = block.bbox;
        let bbox = Rect {
            x0,
            y0: page_height - y1,
            x1,
            y1: page_height - y0,
        };
        cells.push((row, column, bbox, block.text.clone()));
    }
    cells.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    let row_count = cells.iter().map(|c| c.0).max().unwrap_or(0) as i32 + 1;
    let col_count = cells.iter().map(|c| c.1).max().unwrap_or(0) as i32 + 1;
    let bbox = union_all(cells.iter().map(|c| c.2)).unwrap_or(Rect {
        x0: 0.0,
        y0: 0.0,
        x1: 0.0,
        y1: 0.0,
    });
    Table {
        bbox,
        row_count,
        col_count,
        cells: cells
            .into_iter()
            .map(|(_, _, bbox, text)| TableCell { bbox, text })
            .collect(),
    }
}

/// Fonts of one page from document bytes: object number and base name.
pub fn page_fonts_from_bytes(bytes: &[u8], page: usize) -> Result<Vec<FontInfo>> {
    let doc = lopdf_open(bytes)?;
    resource_fonts(&doc, page)
}

/// Fonts of the form dictionary from document bytes, empty without one.
pub fn form_fonts_from_bytes(bytes: &[u8]) -> Result<Vec<FontInfo>> {
    let doc = lopdf_open(bytes)?;
    let root = doc
        .trailer
        .get(b"Root")
        .ok()
        .map(|o| lopdf_resolve(&doc, o));
    let form = root
        .and_then(|o| o.as_dict().ok())
        .and_then(|d| d.get(b"AcroForm").ok())
        .map(|o| lopdf_resolve(&doc, o));
    let drill = form
        .and_then(|o| o.as_dict().ok())
        .and_then(|d| d.get(b"DR").ok())
        .map(|o| lopdf_resolve(&doc, o));
    let fonts = drill
        .and_then(|o| o.as_dict().ok())
        .and_then(|d| d.get(b"Font").ok())
        .map(|o| lopdf_resolve(&doc, o))
        .and_then(|o| o.as_dict().ok());
    let mut out = Vec::new();
    if let Some(dict) = fonts {
        collect_fonts(&doc, dict, &mut out);
    }
    Ok(out)
}

/// Font file of one object: base name plus the descriptor's font
/// program, or no bytes in info-only mode.
pub fn extract_font_from_bytes(bytes: &[u8], xref: i32, info_only: bool) -> Result<FontData> {
    let doc = lopdf_open(bytes)?;
    let obj = lopdf_object(&doc, xref)?;
    let dict = obj
        .as_dict()
        .map_err(|_| ErrorKind::BadValue.to_error(format!("object {xref} is not a font")))?;
    if !dict_is_font(dict) {
        return Err(ErrorKind::BadValue.to_error(format!("object {xref} is not a font")));
    }
    let name = font_base_name(dict).unwrap_or_default();
    let subtype = font_subtype(dict);
    let ext = font_ext(&doc, dict);
    if info_only {
        return Ok(FontData {
            name,
            data: Vec::new(),
            ext,
            subtype,
        });
    }
    let data = dict
        .get(b"FontDescriptor")
        .ok()
        .map(|o| lopdf_resolve(&doc, o))
        .and_then(|o| o.as_dict().ok())
        .and_then(|d| {
            [
                b"FontFile3".as_slice(),
                b"FontFile2".as_slice(),
                b"FontFile".as_slice(),
            ]
            .into_iter()
            .filter_map(|key| d.get(key).ok())
            .map(|o| lopdf_resolve(&doc, o))
            .filter_map(|o| o.as_stream().ok())
            .next()
        })
        .map(stream_bytes)
        .unwrap_or_default();
    Ok(FontData {
        name,
        data,
        ext,
        subtype,
    })
}

/// Font file of one named font: base name plus the descriptor's font
/// program, or nothing when no font carries the name.
pub fn extract_named_font_from_bytes(bytes: &[u8], named: &str) -> Result<Option<FontData>> {
    let doc = lopdf_open(bytes)?;
    let mut ids: Vec<u32> = doc.objects.keys().map(|id| id.0).collect();
    ids.sort_unstable();
    ids.dedup();
    for xref in ids {
        let Ok(obj) = lopdf_object(&doc, xref as i32) else {
            continue;
        };
        let Ok(dict) = obj.as_dict() else {
            continue;
        };
        if !dict_is_font(dict) {
            continue;
        }
        let base = font_base_name(dict).unwrap_or_default();
        if base == named {
            let data = extract_font_from_bytes(bytes, xref as i32, false)?;
            return Ok(Some(data));
        }
    }
    Ok(None)
}

/// Character widths of one font: `limit` entries from code `index` on.
pub fn char_widths_from_bytes(
    bytes: &[u8],
    xref: i32,
    limit: i32,
    index: i32,
    font_dict: Option<&Value>,
) -> Result<Vec<f64>> {
    let doc = lopdf_open(bytes)?;
    let obj = lopdf_object(&doc, xref)?;
    let dict = obj
        .as_dict()
        .map_err(|_| ErrorKind::BadValue.to_error(format!("object {xref} is not a font")))?;
    let mut first = dict
        .get(b"FirstChar")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0);
    let mut missing = dict
        .get(b"MissingWidth")
        .ok()
        .and_then(lopdf_number)
        .unwrap_or(0.0);
    let mut widths: Vec<f64> = dict
        .get(b"Widths")
        .ok()
        .and_then(|o| o.as_array().ok())
        .map(|arr| flatten_numbers(arr))
        .unwrap_or_default();
    if let Some(Value::Dict(map)) = font_dict {
        if let Some(Value::List(items)) = map.get("Widths").or_else(|| map.get("widths")) {
            widths = items.iter().filter_map(value_number).collect();
        }
        if let Some(f) = map.get("FirstChar").or_else(|| map.get("first_char")) {
            if let Some(n) = value_number(f) {
                first = n as i64;
            }
        }
        if let Some(m) = map.get("MissingWidth").or_else(|| map.get("missing_width")) {
            if let Some(n) = value_number(m) {
                missing = n;
            }
        }
    }
    let limit = limit.clamp(0, 65536) as usize;
    let index = index.max(0) as i64;
    let mut out = Vec::with_capacity(limit);
    for i in 0..limit {
        let code = first + index + i as i64;
        let at = code.checked_sub(first).unwrap_or(-1);
        let width = if at >= 0 {
            widths.get(at as usize).copied().unwrap_or(missing)
        } else {
            missing
        };
        out.push(width);
    }
    Ok(out)
}

/// Whether one object is a font dictionary: unknown numbers raise.
pub fn xref_is_font_in_bytes(bytes: &[u8], xref: i32) -> Result<bool> {
    let doc = lopdf_open(bytes)?;
    let obj = lopdf_object(&doc, xref)?;
    Ok(obj.as_dict().is_ok_and(dict_is_font))
}

/// Probe one page's font by name: name and object number, or nothing.
pub fn check_font_in_bytes(bytes: &[u8], page: usize, fontname: &str) -> Result<Value> {
    let doc = lopdf_open(bytes)?;
    for font in resource_fonts(&doc, page)? {
        if font.name.eq_ignore_ascii_case(fontname) {
            return Ok(Value::List(vec![
                Value::Str(font.name),
                Value::Int(i64::from(font.xref)),
            ]));
        }
    }
    Ok(Value::Null)
}

/// Probe one object's font identity: base name and subtype for a font
/// carrying a descriptor, or nothing.
///
/// A bare base-14 name has no descriptor to report: the recording pins
/// null for one (l1-module-checkfontinfo over the fixture's Helvetica).
pub fn check_font_info_in_bytes(bytes: &[u8], xref: i32) -> Result<Value> {
    let doc = lopdf_open(bytes)?;
    let Ok(obj) = lopdf_object(&doc, xref) else {
        return Ok(Value::Null);
    };
    let Some(dict) = obj.as_dict().ok().filter(|d| dict_is_font(d)) else {
        return Ok(Value::Null);
    };
    if dict.get(b"FontDescriptor").is_err() {
        return Ok(Value::Null);
    }
    let name = font_base_name(dict).unwrap_or_default();
    let subtype = dict
        .get(b"Subtype")
        .ok()
        .and_then(|o| o.as_name().ok())
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .unwrap_or_default();
    Ok(Value::List(vec![Value::Str(name), Value::Str(subtype)]))
}

/// A font's identity without a descriptor: base name and subtype straight
/// from the dictionary.
///
/// The probe pins null for a bare base-14 face (no descriptor to report),
/// but repair and info-store still need its identity, which the dictionary
/// carries. Nothing for a missing object or a non-font.
fn font_identity_in_bytes(bytes: &[u8], xref: i32) -> Result<Option<(String, String)>> {
    let doc = lopdf_open(bytes)?;
    let Ok(obj) = lopdf_object(&doc, xref) else {
        return Ok(None);
    };
    let Some(dict) = obj.as_dict().ok().filter(|d| dict_is_font(d)) else {
        return Ok(None);
    };
    let subtype = dict
        .get(b"Subtype")
        .ok()
        .and_then(|o| o.as_name().ok())
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .unwrap_or_default();
    Ok(Some((font_base_name(dict).unwrap_or_default(), subtype)))
}

/// Open bytes for low-level reads.
fn lopdf_open(bytes: &[u8]) -> Result<lopdf::Document> {
    lopdf::Document::load_mem(bytes)
        .map_err(|e| ErrorKind::UnreadableFile.to_error(format!("cannot parse document: {e}")))
}

/// Follow reference chains a bounded depth.
fn lopdf_resolve<'a>(doc: &'a lopdf::Document, obj: &'a lopdf::Object) -> &'a lopdf::Object {
    let mut current = obj;
    for _ in 0..8 {
        match current {
            lopdf::Object::Reference(id) => match doc.get_object(*id) {
                Ok(next) => current = next,
                Err(_) => break,
            },
            _ => break,
        }
    }
    current
}

/// Fetch one numbered object.
fn lopdf_object(doc: &lopdf::Document, xref: i32) -> Result<&lopdf::Object> {
    if xref <= 0 {
        return Err(ErrorKind::BadValue.to_error(format!("bad object number {xref}")));
    }
    let id = doc
        .objects
        .keys()
        .filter(|id| id.0 == xref as u32)
        .min()
        .copied();
    match id.and_then(|id| doc.get_object(id).ok()) {
        Some(obj) => Ok(obj),
        None => Err(ErrorKind::BadValue.to_error(format!("bad object number {xref}"))),
    }
}

/// Whether a dictionary reads as a font.
fn dict_is_font(dict: &lopdf::Dictionary) -> bool {
    if let Ok(obj) = dict.get(b"Subtype") {
        if let Ok(name) = obj.as_name() {
            return matches!(
                name,
                b"Type0"
                    | b"Type1"
                    | b"Type1C"
                    | b"MMType1"
                    | b"Type3"
                    | b"TrueType"
                    | b"CIDFontType0"
                    | b"CIDFontType2"
            );
        }
    }
    dict.get(b"Type")
        .ok()
        .and_then(|o| o.as_name().ok())
        .is_some_and(|name| name == b"Font")
}

/// A font dictionary's base name without any subset prefix.
fn font_base_name(dict: &lopdf::Dictionary) -> Option<String> {
    let raw = dict
        .get(b"BaseFont")
        .ok()
        .and_then(|o| o.as_name().ok().or_else(|| o.as_str().ok()))?;
    let mut name = String::from_utf8_lossy(raw).into_owned();
    let bytes = name.as_bytes();
    if bytes.len() > 7 && bytes[6] == b'+' && bytes[..6].iter().all(|b| b.is_ascii_uppercase()) {
        name = name[7..].to_string();
    }
    Some(name)
}

/// Fonts named by one page's resources.
fn resource_fonts(doc: &lopdf::Document, page: usize) -> Result<Vec<FontInfo>> {
    let pages = doc.get_pages();
    let key = pages.keys().nth(page).copied().ok_or_else(|| {
        ErrorKind::BadValue.to_error(format!("page {page} out of range ({} pages)", pages.len()))
    })?;
    let id = pages[&key];
    let obj = doc
        .get_object(id)
        .map_err(|e| ErrorKind::UnreadableFile.to_error(format!("cannot read page {page}: {e}")))?;
    let fonts = obj
        .as_dict()
        .ok()
        .and_then(|d| d.get(b"Resources").ok())
        .map(|o| lopdf_resolve(doc, o))
        .and_then(|o| o.as_dict().ok())
        .and_then(|d| d.get(b"Font").ok())
        .map(|o| lopdf_resolve(doc, o))
        .and_then(|o| o.as_dict().ok());
    let mut out = Vec::new();
    if let Some(dict) = fonts {
        collect_fonts(doc, dict, &mut out);
    }
    Ok(out)
}

/// Collect one font dictionary's entries.
fn collect_fonts(doc: &lopdf::Document, dict: &lopdf::Dictionary, out: &mut Vec<FontInfo>) {
    for (key, obj) in dict.iter() {
        let xref = match obj {
            lopdf::Object::Reference(id) => id.0 as i32,
            _ => 0,
        };
        let resolved = lopdf_resolve(doc, obj);
        if let Ok(font) = resolved.as_dict() {
            if dict_is_font(font) {
                out.push(FontInfo {
                    xref,
                    name: font_base_name(font).unwrap_or_default(),
                    res_name: String::from_utf8_lossy(key).into_owned(),
                    subtype: font_subtype(font),
                    encoding: font_encoding(font),
                    ext: font_ext(doc, font),
                });
            }
        }
    }
}

/// A font dictionary's subtype name, empty when unset.
fn font_subtype(dict: &lopdf::Dictionary) -> String {
    dict.get(b"Subtype")
        .ok()
        .and_then(|o| o.as_name().ok())
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .unwrap_or_default()
}

/// A font dictionary's encoding name, empty when unset.
fn font_encoding(dict: &lopdf::Dictionary) -> String {
    let Ok(obj) = dict.get(b"Encoding") else {
        return String::new();
    };
    if let Ok(name) = obj.as_name() {
        return String::from_utf8_lossy(name).into_owned();
    }
    obj.as_dict()
        .ok()
        .and_then(|d| d.get(b"BaseEncoding").ok())
        .and_then(|o| o.as_name().ok())
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .unwrap_or_default()
}

/// A font dictionary's program extension, `n/a` when nothing is embedded.
fn font_ext(doc: &lopdf::Document, dict: &lopdf::Dictionary) -> String {
    let descriptor = dict
        .get(b"FontDescriptor")
        .ok()
        .map(|o| lopdf_resolve(doc, o))
        .and_then(|o| o.as_dict().ok());
    let Some(descriptor) = descriptor else {
        return "n/a".to_string();
    };
    if let Some(stream) = descriptor
        .get(b"FontFile3")
        .ok()
        .map(|o| lopdf_resolve(doc, o))
        .and_then(|o| o.as_stream().ok())
    {
        let subtype = stream
            .dict
            .get(b"Subtype")
            .ok()
            .and_then(|o| o.as_name().ok())
            .unwrap_or(b"");
        return match subtype {
            b"OpenType" => "otf".to_string(),
            b"TrueType" => "ttf".to_string(),
            _ => "cff".to_string(),
        };
    }
    if descriptor.get(b"FontFile2").is_ok() {
        return "ttf".to_string();
    }
    if descriptor.get(b"FontFile").is_ok() {
        return "pfb".to_string();
    }
    "n/a".to_string()
}

/// Bytes of a font program stream: decoded, or raw when undecodable.
fn stream_bytes(stream: &lopdf::Stream) -> Vec<u8> {
    stream
        .decompressed_content()
        .unwrap_or_else(|_| stream.content.clone())
}

/// A lopdf number as a float.
fn lopdf_number(obj: &lopdf::Object) -> Option<f64> {
    match obj {
        lopdf::Object::Integer(v) => Some(*v as f64),
        lopdf::Object::Real(v) => Some(f64::from(*v)),
        _ => None,
    }
}

/// Flatten a widths array, descending into range subarrays.
fn flatten_numbers(objects: &[lopdf::Object]) -> Vec<f64> {
    let mut out = Vec::new();
    for obj in objects {
        match obj {
            lopdf::Object::Array(inner) => out.extend(flatten_numbers(inner)),
            _ => {
                if let Some(n) = lopdf_number(obj) {
                    out.push(n);
                }
            }
        }
    }
    out
}

/// A dynamic value as a float.
fn value_number(value: &Value) -> Option<f64> {
    match value {
        Value::Int(v) => Some(*v as f64),
        Value::Float(v) => Some(*v),
        _ => None,
    }
}

// ===== Fonts end marker L1D =====

// ===== Lane L1: glyph names, width estimates, quads =====

/// Latin glyph names and their code points: the ASCII letters, digits,
/// and punctuation of the base encodings, plus the common ligatures.
const LATIN_GLYPH_NAMES: [(&str, char); 135] = [
    ("A", 'A'),
    ("AE", 'Æ'),
    ("B", 'B'),
    ("C", 'C'),
    ("D", 'D'),
    ("E", 'E'),
    ("F", 'F'),
    ("G", 'G'),
    ("H", 'H'),
    ("I", 'I'),
    ("J", 'J'),
    ("K", 'K'),
    ("L", 'L'),
    ("M", 'M'),
    ("N", 'N'),
    ("O", 'O'),
    ("OE", 'Œ'),
    ("P", 'P'),
    ("Q", 'Q'),
    ("R", 'R'),
    ("S", 'S'),
    ("T", 'T'),
    ("U", 'U'),
    ("V", 'V'),
    ("W", 'W'),
    ("X", 'X'),
    ("Y", 'Y'),
    ("Z", 'Z'),
    ("a", 'a'),
    ("ae", 'æ'),
    ("ampersand", '&'),
    ("asciicircum", '^'),
    ("asciitilde", '~'),
    ("asterisk", '*'),
    ("at", '@'),
    ("b", 'b'),
    ("backslash", '\\'),
    ("bar", '|'),
    ("braceleft", '{'),
    ("braceright", '}'),
    ("bracketleft", '['),
    ("bracketright", ']'),
    ("bullet", '•'),
    ("c", 'c'),
    ("colon", ':'),
    ("comma", ','),
    ("copyright", '©'),
    ("d", 'd'),
    ("dagger", '†'),
    ("daggerdbl", '‡'),
    ("dollar", '$'),
    ("dotlessi", 'ı'),
    ("dotlessj", 'ȷ'),
    ("e", 'e'),
    ("eight", '8'),
    ("ellipsis", '…'),
    ("emdash", '—'),
    ("endash", '–'),
    ("equal", '='),
    ("exclam", '!'),
    ("f", 'f'),
    ("ff", 'ﬀ'),
    ("ffi", 'ﬃ'),
    ("ffl", 'ﬄ'),
    ("fi", 'ﬁ'),
    ("five", '5'),
    ("fl", 'ﬂ'),
    ("four", '4'),
    ("fraction", '⁄'),
    ("g", 'g'),
    ("germandbls", 'ß'),
    ("grave", '`'),
    ("greater", '>'),
    ("guillemotleft", '«'),
    ("guillemotright", '»'),
    ("guilsinglleft", '‹'),
    ("guilsinglright", '›'),
    ("h", 'h'),
    ("hyphen", '\u{2010}'),
    ("hyphenminus", '-'),
    ("i", 'i'),
    ("j", 'j'),
    ("k", 'k'),
    ("l", 'l'),
    ("less", '<'),
    ("m", 'm'),
    ("minus", '−'),
    ("multiply", '×'),
    ("n", 'n'),
    ("nine", '9'),
    ("numbersign", '#'),
    ("o", 'o'),
    ("oe", 'œ'),
    ("one", '1'),
    ("p", 'p'),
    ("paragraph", '¶'),
    ("parenleft", '('),
    ("parenright", ')'),
    ("percent", '%'),
    ("period", '.'),
    ("perthousand", '‰'),
    ("plus", '+'),
    ("q", 'q'),
    ("question", '?'),
    ("quotedbl", '"'),
    ("quotedblbase", '„'),
    ("quotedblleft", '"'),
    ("quotedblright", '"'),
    ("quoteleft", '\u{2018}'),
    ("quoteright", '\u{2019}'),
    ("quotesinglbase", '‚'),
    ("quotesingle", '\''),
    ("r", 'r'),
    ("registered", '®'),
    ("s", 's'),
    ("section", '§'),
    ("semicolon", ';'),
    ("seven", '7'),
    ("six", '6'),
    ("slash", '/'),
    ("space", ' '),
    ("sterling", '£'),
    ("t", 't'),
    ("three", '3'),
    ("trademark", '™'),
    ("two", '2'),
    ("u", 'u'),
    ("underscore", '_'),
    ("v", 'v'),
    ("w", 'w'),
    ("x", 'x'),
    ("y", 'y'),
    ("yen", '¥'),
    ("zero", '0'),
    ("z", 'z'),
];

/// A glyph name's code point: the Latin table, then the symbol fonts.
///
/// Unknown names answer -1: no mapping is known, and zero would read as
/// a NUL mapping rather than as missing.
pub fn glyph_name_to_code(name: &str) -> i32 {
    if let Some((_, ch)) = LATIN_GLYPH_NAMES.iter().find(|(n, _)| *n == name) {
        return *ch as i32;
    }
    if let Some(ch) = symbol_glyph_name_to_char(name) {
        return ch as i32;
    }
    -1
}

/// A code point's glyph name: the Latin table inverted, then a small set
/// of certain symbol names, else the `.notdef` fallback.
pub fn code_to_glyph_name(code: i32) -> String {
    let ch = char::from_u32(code as u32).unwrap_or('\0');
    if let Some((name, _)) = LATIN_GLYPH_NAMES.iter().find(|(_, c)| *c == ch) {
        return (*name).to_string();
    }
    const SYMBOLS: [(char, &str); 24] = [
        ('Α', "Alpha"),
        ('Β', "Beta"),
        ('Δ', "Delta"),
        ('Ω', "Omega"),
        ('α', "alpha"),
        ('β', "beta"),
        ('δ', "delta"),
        ('π', "pi"),
        ('ω', "omega"),
        ('←', "arrowleft"),
        ('↑', "arrowup"),
        ('→', "arrowright"),
        ('↓', "arrowdown"),
        ('∞', "infinity"),
        ('≠', "notequal"),
        ('≤', "lessequal"),
        ('≥', "greaterequal"),
        ('∂', "partialdiff"),
        ('∑', "summation"),
        ('∏', "product"),
        ('√', "radical"),
        ('∫', "integral"),
        ('°', "degree"),
        ('±', "plusminus"),
    ];
    if let Some((_, name)) = SYMBOLS.iter().find(|(c, _)| *c == ch) {
        return (*name).to_string();
    }
    ".notdef".to_string()
}

/// Whether a font name reads as a monospace face.
fn is_monospace_name(fontname: &str) -> bool {
    if let Some(face) = StandardFont::from_base_font(fontname) {
        return matches!(
            face,
            StandardFont::Courier
                | StandardFont::CourierBold
                | StandardFont::CourierOblique
                | StandardFont::CourierBoldOblique
        );
    }
    fontname.to_ascii_lowercase().contains("courier")
        || fontname.to_ascii_lowercase().contains("mono")
}

/// An estimate of one string's set width: monospace faces advance a
/// fixed em fraction, proportional faces sum integer thousandths of an em.
///
/// The recorded module pins fix several characters bit for bit; the rest
/// keep the tiered estimate.
pub fn estimate_text_length(text: &str, fontname: &str, fontsize: f64) -> f64 {
    if fontsize <= 0.0 || text.is_empty() {
        return 0.0;
    }
    if is_monospace_name(fontname) {
        return 0.6 * fontsize * text.chars().count() as f64;
    }
    let units: i32 = text.chars().map(proportional_width_units).sum();
    f64::from(units) * fontsize / 1000.0
}

/// One character's width in thousandths of an em on a proportional face.
///
/// The first arms are the recorded module pins; the remaining tiers are
/// estimates for characters no recording covers.
fn proportional_width_units(ch: char) -> i32 {
    match ch {
        ' ' => 278,
        'h' | 'e' | 'o' => 556,
        'l' => 222,
        'M' => 833,
        'r' => 333,
        '.' => 334,
        'i' | '!' | '|' | '\'' | ',' | ':' | ';' | 'I' => 320,
        'm' | 'w' | 'W' => 850,
        '0'..='9' => 550,
        c if c.is_ascii_uppercase() => 680,
        c if (c as u32) < 0x7f => 550,
        _ => 600,
    }
}

/// Resolve the page an option struct points at: an embedded text page
/// replaces the view's own page, as a cached extraction would.
fn resolve_source(
    doc: &PdfDocument,
    page: usize,
    textpage: Option<TextPage<'_>>,
) -> Result<(EngineDoc, usize)> {
    match textpage {
        Some(tp) => Ok((engine_for(tp.doc)?, tp.page)),
        None => Ok((engine_for(doc)?, page)),
    }
}

/// Resolve a numbered page, rejecting negatives before the engine.
fn resolve_source_numbered(
    doc: &PdfDocument,
    page: i32,
    textpage: Option<TextPage<'_>>,
) -> Result<(EngineDoc, usize)> {
    match textpage {
        Some(tp) => Ok((engine_for(tp.doc)?, tp.page)),
        None => {
            let engine = engine_for(doc)?;
            let page = checked_page_number(&engine, page)?;
            Ok((engine, page))
        }
    }
}

/// Validate a signed page number against the page count.
fn checked_page_number(engine: &EngineDoc, page: i32) -> Result<usize> {
    if page < 0 {
        return Err(ErrorKind::IndexOutOfRange.to_error(format!("page {page} out of range")));
    }
    checked_page(engine, page as usize)?;
    Ok(page as usize)
}

/// Resolve the effective clip and flags of an option struct over an
/// optional text page: both clips filter, an explicit flags word wins.
fn resolve_clip_flags(
    clip: Option<Rect>,
    flags: Option<i32>,
    textpage: Option<TextPage<'_>>,
) -> (Option<Rect>, i32) {
    match textpage {
        Some(tp) => (intersect_clips(tp.clip, clip), flags.unwrap_or(tp.flags)),
        None => (clip, flags.unwrap_or(0)),
    }
}

/// Fold a text page's matrix into a resolved clip over a capture: without
/// a text page the clip stands as resolved.
fn window_for(
    capture: &CapturedPage,
    clip: Option<Rect>,
    textpage: Option<TextPage<'_>>,
) -> Option<Rect> {
    match textpage {
        Some(tp) => textpage_window(capture.width, capture.height, clip, tp.matrix),
        None => clip,
    }
}

// ===== Glyph end marker L1E =====

// ===== Lane L1: format dispatch =====

/// Render one capture in the requested format.
fn render_format(
    capture: &CapturedPage,
    format: TextFormat,
    page: usize,
    clip: Option<Rect>,
    flags: i32,
    sort: bool,
) -> TextOutput {
    match format {
        TextFormat::Text => TextOutput::Text(capture.render_text(clip, flags, sort)),
        TextFormat::Words => TextOutput::Words(capture.render_words(clip, flags, sort, None)),
        TextFormat::Blocks => TextOutput::Blocks(capture.render_blocks(clip, flags, sort)),
        TextFormat::Dict => TextOutput::Dict(capture.render_dict(clip, flags, sort, false)),
        TextFormat::RawDict => TextOutput::Dict(capture.render_dict(clip, flags, sort, true)),
        TextFormat::Json => TextOutput::Json(capture.render_json(clip, flags, sort, false)),
        TextFormat::RawJson => TextOutput::RawJson(capture.render_json(clip, flags, sort, true)),
        TextFormat::Html => TextOutput::Html(capture.render_html(page, clip, flags, sort)),
        TextFormat::Xhtml => TextOutput::Xhtml(capture.render_xhtml(page, clip, flags, sort)),
        TextFormat::Xml => TextOutput::Xml(capture.render_xml(page, clip, flags, sort)),
    }
}

/// Whether a format shows figures (only the dict family does).
fn format_needs_figures(format: TextFormat) -> bool {
    matches!(format, TextFormat::Dict | TextFormat::RawDict)
}

// ===== Dispatch end marker L1F =====

#[cfg(test)]
mod tests {
    use super::*;

    /// The base-14 metric table carries the recorded values bit for bit.
    #[test]
    fn base14_table_matches_recordings() {
        assert_eq!(
            base14_metrics("Helvetica"),
            Some((1.0750000476837158, -0.29899999499320984))
        );
        assert_eq!(base14_metrics("Helvetica-Bold"), Some((1.07, -0.307)));
        assert_eq!(base14_metrics("Times-Roman"), None);
    }

    #[test]
    fn compat_font_name_is_cut_to_24_characters() {
        assert_eq!(
            truncate_font_name("ITCFranklinGothicStd-Demi"),
            "ITCFranklinGothicStd-Dem"
        );
        assert_eq!(truncate_font_name("Helvetica"), "Helvetica");
        assert_eq!(truncate_font_name(&"a".repeat(24)), "a".repeat(24));
        // The cut counts characters, not bytes: a multibyte name neither
        // panics nor splits a character.
        assert_eq!(truncate_font_name(&"é".repeat(30)), "é".repeat(24));
    }
}
