//! Paragraph styles: `word/styles.xml` plus the `basedOn` chains.
//!
//! A paragraph's own `w:pPr` only carries direct formatting. Whether it is a
//! heading or a list item often lives in its style instead: the style's
//! `pPr/outlineLvl` and `pPr/numPr`, inherited along the `w:basedOn` chain
//! with the nearest setting winning. This table is read once per document
//! and answers both resolutions.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::collections::HashMap;
use std::io::BufRead;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::document::attr_value;
use crate::DocxError;

/// One paragraph style: its display name, its parent, and the two inherited
/// paragraph properties this crate resolves.
#[derive(Debug, Clone, Default)]
struct StyleInfo {
    /// The `w:name` display value (`Heading 4`, `Title`, ...).
    name: String,
    /// The `w:basedOn` parent style id, if the style states one.
    based_on: Option<String>,
    /// The `pPr/outlineLvl` value (0-based; the heading level is one more).
    outline: Option<u8>,
    /// The `pPr/numPr` numbering (`numId`, `ilvl`).
    num_pr: Option<(u32, u8)>,
}

/// The paragraph styles of one document, keyed by style id.
#[derive(Debug, Default)]
pub(crate) struct Styles {
    styles: HashMap<String, StyleInfo>,
}

/// The half-open elements of `styles.xml` while the event loop runs.
#[derive(Debug, Default)]
struct Open {
    id: Option<String>,
    name: String,
    based_on: Option<String>,
    outline: Option<u8>,
    num_id: Option<u32>,
    ilvl: u8,
}

impl Styles {
    /// Read `word/styles.xml`. Character and table styles are skipped: only
    /// paragraph properties resolve here.
    pub(crate) fn parse(xml: &[u8]) -> Result<Self, DocxError> {
        let mut styles = Self::default();
        let mut open = Open::default();

        let mut reader = Reader::from_reader(xml);
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Eof) => break,
                Ok(Event::Start(ref e)) => Self::start(&mut open, e, &reader),
                Ok(Event::Empty(ref e)) => {
                    Self::start(&mut open, e, &reader);
                    Self::end(&mut styles, &mut open, e.local_name().as_ref());
                }
                Ok(Event::End(ref e)) => {
                    Self::end(&mut styles, &mut open, e.local_name().as_ref());
                }
                Err(error) => return Err(DocxError::Xml(error)),
                _ => {}
            }
            buf.clear();
        }
        Ok(styles)
    }

    /// Handle an opening (or self-closing) tag of `styles.xml`.
    fn start<R: BufRead>(open: &mut Open, e: &BytesStart, reader: &Reader<R>) {
        match e.local_name().as_ref() {
            b"style" => {
                open.id = attr_value(e, reader, "styleId");
                open.name = String::new();
                open.based_on = None;
                open.outline = None;
                open.num_id = None;
                open.ilvl = 0;
            }
            b"name" => {
                if open.id.is_some() {
                    open.name = attr_value(e, reader, "val").unwrap_or_default();
                }
            }
            b"basedOn" => {
                if open.id.is_some() {
                    open.based_on = attr_value(e, reader, "val");
                }
            }
            b"outlineLvl" => {
                if open.id.is_some() && open.outline.is_none() {
                    open.outline = attr_value(e, reader, "val").and_then(|v| v.parse().ok());
                }
            }
            b"ilvl" => {
                if open.id.is_some() {
                    open.ilvl = attr_value(e, reader, "val")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                }
            }
            b"numId" => {
                if open.id.is_some() {
                    open.num_id = attr_value(e, reader, "val").and_then(|v| v.parse().ok());
                }
            }
            _ => {}
        }
    }

    /// Handle a closing (or self-closing) tag of `styles.xml`.
    fn end(styles: &mut Self, open: &mut Open, name: &[u8]) {
        if name == b"style" {
            if let Some(id) = open.id.take() {
                styles.styles.insert(
                    id,
                    StyleInfo {
                        name: std::mem::take(&mut open.name),
                        based_on: open.based_on.take(),
                        outline: open.outline.take(),
                        num_pr: open.num_id.take().map(|num| (num, open.ilvl)),
                    },
                );
            }
            open.based_on = None;
            open.outline = None;
            open.num_id = None;
            open.ilvl = 0;
        }
    }

    /// The style and its `basedOn` ancestors, nearest first.
    ///
    /// A missing style or a cycle ends the walk; what was found on the way
    /// still counts.
    fn chain(&self, style_id: &str) -> Vec<(&str, &StyleInfo)> {
        let mut out = Vec::new();
        let mut seen: Vec<&str> = Vec::new();
        let mut next = self.styles.get_key_value(style_id);
        while let Some((id, info)) = next {
            if seen.contains(&id.as_str()) {
                break;
            }
            seen.push(id.as_str());
            out.push((id.as_str(), info));
            next = info
                .based_on
                .as_deref()
                .and_then(|parent| self.styles.get_key_value(parent));
        }
        out
    }

    /// The nearest `outlineLvl` on the style's chain, if any states one.
    pub(crate) fn outline(&self, style_id: &str) -> Option<u8> {
        self.chain(style_id)
            .iter()
            .find_map(|(_, info)| info.outline)
    }

    /// The nearest `numPr` on the style's chain, if any states one.
    pub(crate) fn num_pr(&self, style_id: &str) -> Option<(u32, u8)> {
        self.chain(style_id)
            .iter()
            .find_map(|(_, info)| info.num_pr)
    }

    /// The heading level of the nearest style on the chain whose id or name
    /// reads `Heading1`..`Heading9` or `Title`.
    pub(crate) fn heading_name_level(
        &self,
        style_id: &str,
        mut level_of: impl FnMut(&str) -> Option<u8>,
    ) -> Option<u8> {
        for (id, info) in self.chain(style_id) {
            if let Some(level) = level_of(id).or_else(|| level_of(&info.name)) {
                return Some(level);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

    /// Three styles: `C2` derives from `C1` derives from `Heading2`, and a
    /// `CycA`/`CycB` pair whose `basedOn` points at each other.
    fn styles() -> Styles {
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<w:styles xmlns:w="{W_NS}">
  <w:style w:styleId="Heading2"><w:name w:val="heading 2"/>
    <w:pPr><w:outlineLvl w:val="1"/></w:pPr>
  </w:style>
  <w:style w:styleId="C1Harbour"><w:name w:val="C1Harbour"/>
    <w:basedOn w:val="Heading2"/>
  </w:style>
  <w:style w:styleId="C2Harbour"><w:name w:val="C2Harbour"/>
    <w:basedOn w:val="C1Harbour"/>
    <w:pPr><w:numPr><w:ilvl w:val="1"/><w:numId w:val="7"/></w:numPr></w:pPr>
  </w:style>
  <w:style w:styleId="CycA"><w:basedOn w:val="CycB"/></w:style>
  <w:style w:styleId="CycB"><w:basedOn w:val="CycA"/></w:style>
</w:styles>"#
        );
        Styles::parse(xml.as_bytes()).unwrap()
    }

    #[test]
    fn outline_and_num_pr_resolve_along_the_chain() {
        let styles = styles();
        assert_eq!(styles.outline("C2Harbour"), Some(1));
        assert_eq!(styles.num_pr("C2Harbour"), Some((7, 1)));
        assert_eq!(styles.num_pr("C1Harbour"), None);
        assert_eq!(styles.outline("Missing"), None);
    }

    #[test]
    fn cycles_end_the_walk() {
        let styles = styles();
        assert_eq!(styles.outline("CycA"), None);
        assert_eq!(styles.num_pr("CycA"), None);
        assert_eq!(styles.heading_name_level("CycA", |_| None), None);
    }

    #[test]
    fn name_levels_read_ids_and_names_nearest_first() {
        let styles = styles();
        let level_of = |name: &str| (name == "Heading2" || name == "heading 2").then_some(2u8);
        assert_eq!(styles.heading_name_level("C2Harbour", level_of), Some(2));
    }
}
