//! Unified page-decoration surface (Epic 3 #1225).
//!
//! The website promises several decoration APIs that semantically
//! overlap — watermark, header/footer, page numbers, stamp — and each
//! would otherwise grow its own per-type method on `PdfDocument`.
//! `PageDecoration` is the shared entry point.
//!
//! # Shape
//!
//! A [`PageDecoration`] is a non-exhaustive enum tagged by decoration
//! family. The one variant with wired options today is
//! [`PageDecoration::Watermark`]. Additional variants — header/footer,
//! page numbers, stamp — are reserved for 1.1 follow-ups; they are
//! intentionally left out of this enum rather than stubbed with empty
//! option structs so the 1.0 surface honestly reflects what's
//! implementable.
//!
//! # Usage
//!
//! ```no_run
//! # fn main() -> pdfluent::Result<()> {
//! use pdfluent::prelude::*;
//!
//! let mut doc = PdfDocument::open("in.pdf")?;
//!
//! // Single call, builder-friendly:
//! doc.add_decoration(PageDecoration::watermark(
//!     "DRAFT",
//!     WatermarkOptions::centered().rotated(45.0).opacity(0.3),
//! ))?;
//! # Ok(()) }
//! ```
//!
//! The existing per-family methods (`add_watermark`) are preserved for
//! backwards compatibility with website snippets; they delegate to
//! [`PdfDocument::add_decoration`].
//!
//! [`PdfDocument::add_decoration`]: crate::PdfDocument::add_decoration

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::watermark::WatermarkOptions;

/// A page decoration — the unified input type for
/// [`PdfDocument::add_decoration`].
///
/// [`PdfDocument::add_decoration`]: crate::PdfDocument::add_decoration
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum PageDecoration {
    /// A text watermark applied to each page.
    ///
    /// The string is the watermark text; styling / position / opacity
    /// come from [`WatermarkOptions`].
    Watermark {
        /// Watermark text.
        text: String,
        /// Watermark styling options.
        options: WatermarkOptions,
    },
}

impl PageDecoration {
    /// Convenience constructor for a watermark decoration.
    pub fn watermark(text: impl Into<String>, options: WatermarkOptions) -> Self {
        Self::Watermark {
            text: text.into(),
            options,
        }
    }
}
