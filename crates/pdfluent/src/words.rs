// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Word-level boxes through the facade.

use crate::document::{Page, PdfDocument};
use crate::error::Result;
use pdf_engine::{ExtractOptions, Word};

impl PdfDocument {
    /// Every word of the requested pages, with the box it was drawn in.
    ///
    /// The same engine pass as [`PdfDocument::extract_blocks`], so the
    /// numbering each word carries -- block, line, word -- names positions
    /// in that answer. Words come in reading order.
    pub fn extract_words(&self, options: &ExtractOptions) -> Result<Vec<Word>> {
        self.engine_words(options)
    }
}

impl Page<'_> {
    /// This page's words, with the box each one was drawn in.
    ///
    /// Blocks are numbered within the page: `blocks[word.block]` names the
    /// block in this page's own [`PdfDocument::extract_blocks`] window.
    pub fn words(&self) -> Result<Vec<Word>> {
        let mut options = ExtractOptions::default();
        options.pages = Some(vec![self.page_index()]);
        self.document().extract_words(&options)
    }
}
