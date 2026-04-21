//! Options and reports for Epic 3 #1224 parity methods.
//!
//! Types that `PdfDocument::{to_images,compress,subset_fonts,embed_font,
//! insert_image}` consume or produce, separated out of `document.rs` to
//! keep the facade module scannable.

use std::path::PathBuf;

/// Output image format for [`PdfDocument::to_images`].
///
/// [`PdfDocument::to_images`]: crate::PdfDocument::to_images
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImageFormat {
    /// Portable Network Graphics (lossless).
    Png,
    /// JPEG (lossy, smaller).
    Jpeg,
}

impl ImageFormat {
    /// File extension, no leading dot.
    pub fn extension(self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpg",
        }
    }
}

/// Options for [`PdfDocument::to_images`].
///
/// [`PdfDocument::to_images`]: crate::PdfDocument::to_images
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ToImagesOptions {
    /// Render resolution in DPI. Default: 150.
    pub dpi: u32,
    /// Output format. Default: [`ImageFormat::Png`].
    pub format: ImageFormat,
    /// Optional page range (1-based, inclusive). `None` = all pages.
    pub pages: Option<(usize, usize)>,
}

impl Default for ToImagesOptions {
    fn default() -> Self {
        Self {
            dpi: 150,
            format: ImageFormat::Png,
            pages: None,
        }
    }
}

impl ToImagesOptions {
    /// New default options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the DPI.
    pub fn with_dpi(mut self, dpi: u32) -> Self {
        self.dpi = dpi;
        self
    }

    /// Override the image format.
    pub fn with_format(mut self, format: ImageFormat) -> Self {
        self.format = format;
        self
    }

    /// Limit rendering to a 1-based inclusive page range.
    pub fn with_pages(mut self, from: usize, to: usize) -> Self {
        self.pages = Some((from, to));
        self
    }
}

/// Options for [`PdfDocument::compress`].
///
/// The default preset runs the full optimisation stack (font subsetting,
/// stream compression, duplicate-object deduplication, unused-object
/// removal). Individual passes can be toggled.
///
/// [`PdfDocument::compress`]: crate::PdfDocument::compress
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct CompressOptions {
    /// Subset embedded fonts to only their used glyphs.
    pub subset_fonts: bool,
    /// Compress uncompressed streams.
    pub compress_streams: bool,
    /// Deduplicate content streams with identical bytes.
    pub deduplicate_streams: bool,
    /// Remove objects unreachable from the catalog.
    pub remove_unused: bool,
}

impl Default for CompressOptions {
    fn default() -> Self {
        Self {
            subset_fonts: true,
            compress_streams: true,
            deduplicate_streams: true,
            remove_unused: true,
        }
    }
}

impl CompressOptions {
    /// New default options (full stack enabled).
    pub fn new() -> Self {
        Self::default()
    }

    /// Strict preset — full stack, no lossy passes.
    ///
    /// Identical to [`Self::default`] today; the explicit preset exists
    /// so calling code reads intent-first and is stable against future
    /// additions to lossy-only passes.
    pub fn strict() -> Self {
        Self {
            subset_fonts: true,
            compress_streams: true,
            deduplicate_streams: true,
            remove_unused: true,
        }
    }

    /// Lossy preset — enables every pass the strict preset enables.
    ///
    /// Today this is the same pass set as `strict()`. Lossy image
    /// downsampling lands in the 1.1 compress stack; reserving the
    /// preset now keeps callers forward-compatible.
    pub fn lossy() -> Self {
        Self::strict()
    }

    /// Archival preset — maximally safe: skip lossy / non-reversible
    /// passes, keep unused objects that might be referenced by future
    /// incremental updates.
    pub fn archival() -> Self {
        Self {
            subset_fonts: true,
            compress_streams: true,
            deduplicate_streams: true,
            // Archival preserves unused objects that might be referenced
            // by future incremental updates or signed appearance streams.
            remove_unused: false,
        }
    }
}

/// Report returned by [`PdfDocument::subset_fonts`] and included in the
/// combined [`CompressReport`].
///
/// [`PdfDocument::subset_fonts`]: crate::PdfDocument::subset_fonts
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct FontSubsetReport {
    /// Embedded fonts visited.
    pub fonts_processed: usize,
    /// Embedded fonts whose `FontFile*` stream was replaced with a
    /// subset.
    pub fonts_subsetted: usize,
    /// Total bytes saved across all subsetted font streams.
    pub bytes_saved: usize,
}

/// Report returned by [`PdfDocument::compress`].
///
/// [`PdfDocument::compress`]: crate::PdfDocument::compress
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct CompressReport {
    /// Result of font subsetting, or `None` if the pass was disabled.
    pub font_subset: Option<FontSubsetReport>,
    /// Number of streams compressed.
    pub streams_compressed: usize,
    /// Number of duplicate streams merged.
    pub streams_deduplicated: usize,
    /// Number of unused objects removed.
    pub unused_removed: usize,
}

/// Supported [`PdfDocument::insert_image`] formats.
///
/// [`PdfDocument::insert_image`]: crate::PdfDocument::insert_image
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum InsertImageFormat {
    /// JPEG — inserted as raw DCTDecode stream (no re-encoding).
    Jpeg,
    /// PNG — decoded to raw pixels, FlateDecode with optional alpha SMask.
    Png,
}

/// Description of an image to be inserted into a PDF page.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ImageInsert {
    /// Image bytes (JPEG or PNG).
    pub bytes: Vec<u8>,
    /// Format of `bytes`.
    pub format: InsertImageFormat,
    /// Target page (1-based).
    pub page: usize,
    /// X position in PDF points from the page's bottom-left.
    pub x: f64,
    /// Y position in PDF points from the page's bottom-left.
    pub y: f64,
    /// Display width in PDF points.
    pub width: f64,
    /// Display height in PDF points.
    pub height: f64,
    /// Optional opacity, `0.0..=1.0`. `None` = fully opaque.
    pub opacity: Option<f64>,
}

impl ImageInsert {
    /// Construct from raw bytes + format + destination.
    pub fn new(
        bytes: impl Into<Vec<u8>>,
        format: InsertImageFormat,
        page: usize,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> Self {
        Self {
            bytes: bytes.into(),
            format,
            page,
            x,
            y,
            width,
            height,
            opacity: None,
        }
    }

    /// Set the image opacity, clamped to `[0.0, 1.0]`.
    pub fn with_opacity(mut self, opacity: f64) -> Self {
        self.opacity = Some(opacity.clamp(0.0, 1.0));
        self
    }
}

/// Result of a successful [`PdfDocument::insert_image`] call.
///
/// [`PdfDocument::insert_image`]: crate::PdfDocument::insert_image
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ImageInsertReport {
    /// Pixel width of the decoded image.
    pub pixel_width: u32,
    /// Pixel height of the decoded image.
    pub pixel_height: u32,
    /// Resource name used in the page's content stream (e.g. `"Im1"`).
    pub resource_name: String,
}

/// Result of a successful [`PdfDocument::to_images`] call.
///
/// [`PdfDocument::to_images`]: crate::PdfDocument::to_images
#[derive(Debug, Clone)]
pub struct ToImagesReport {
    /// Paths the images were written to, in page order.
    pub paths: Vec<PathBuf>,
}
