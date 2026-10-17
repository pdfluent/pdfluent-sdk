//! List numbering: `word/numbering.xml` plus the per-list counters.
//!
//! A paragraph's `w:numPr` only names a list (`w:numId`) and a depth
//! (`w:ilvl`). Whether that list is bulleted or numbered, and at which number
//! it starts, lives here: each `w:num` points at an `w:abstractNum` whose
//! `w:lvl` entries carry the `w:numFmt` and `w:start`. The counters reproduce
//! the document's own ordinals the way the engine reports them for PDFs.

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

/// One level of an abstract list: how its items are marked, and where the
/// count starts.
#[derive(Debug, Clone, Copy)]
struct LevelDef {
    /// Whether items carry a number (`decimal`, `lowerLetter`, ...) rather
    /// than a bullet.
    ordered: bool,
    /// Whether the level marks nothing at all (`w:numFmt` "none"): its
    /// paragraphs are plain text, not list items.
    none: bool,
    /// The value of `w:start` (1 where the level states none).
    start: u32,
}

/// The numbering model of one document, with live per-list counters.
#[derive(Debug, Default)]
pub(crate) struct Numbering {
    /// `abstractNumId` -> (`ilvl` -> level). Levels Word never defines for a
    /// list are absent rather than defaulted.
    abstracts: HashMap<u32, HashMap<u8, LevelDef>>,
    /// `numId` -> `abstractNumId`.
    nums: HashMap<u32, u32>,
    /// (`numId`, `ilvl`) -> items seen so far at that depth.
    counters: HashMap<(u32, u8), u32>,
}

/// The half-open elements of `numbering.xml` while the event loop runs.
#[derive(Debug, Default)]
struct Open {
    abstract_id: Option<u32>,
    level: Option<u8>,
    ordered: bool,
    none: bool,
    start: u32,
    num: Option<u32>,
}

impl Numbering {
    /// Read `word/numbering.xml`. Level overrides (`w:lvlOverride`) are
    /// ignored in the MVP: the abstract definition wins.
    pub(crate) fn parse(xml: &[u8]) -> Result<Self, DocxError> {
        let mut numbering = Self::default();
        let mut open = Open {
            ordered: true,
            start: 1,
            ..Open::default()
        };

        let mut reader = Reader::from_reader(xml);
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Eof) => break,
                Ok(Event::Start(ref e)) => Self::start(&mut numbering, &mut open, e, &reader),
                Ok(Event::Empty(ref e)) => {
                    Self::start(&mut numbering, &mut open, e, &reader);
                    Self::end(&mut numbering, &mut open, e.local_name().as_ref());
                }
                Ok(Event::End(ref e)) => {
                    Self::end(&mut numbering, &mut open, e.local_name().as_ref());
                }
                Err(error) => return Err(DocxError::Xml(error)),
                _ => {}
            }
            buf.clear();
        }
        Ok(numbering)
    }

    /// Handle an opening (or self-closing) tag of `numbering.xml`.
    fn start<R: BufRead>(
        numbering: &mut Self,
        open: &mut Open,
        e: &BytesStart,
        reader: &Reader<R>,
    ) {
        match e.local_name().as_ref() {
            b"abstractNum" => {
                open.abstract_id =
                    attr_value(e, reader, "abstractNumId").and_then(|v| v.parse().ok());
            }
            b"lvl" => {
                open.level = Some(
                    attr_value(e, reader, "ilvl")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0),
                );
                open.ordered = true;
                open.none = false;
                open.start = 1;
            }
            b"numFmt" => {
                // `bullet` and `none` mark without a number; every other
                // format (`decimal`, `lowerLetter`, `upperRoman`, ...) counts.
                // `none` marks nothing at all, so its paragraphs stay plain.
                let fmt = attr_value(e, reader, "val");
                open.ordered = !matches!(fmt.as_deref(), Some("bullet") | Some("none"));
                open.none = matches!(fmt.as_deref(), Some("none"));
            }
            b"start" => {
                open.start = attr_value(e, reader, "val")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1);
            }
            b"num" => {
                open.num = attr_value(e, reader, "numId").and_then(|v| v.parse().ok());
            }
            b"abstractNumId" => {
                if let (Some(num), Some(abstract_id)) = (
                    open.num,
                    attr_value(e, reader, "val").and_then(|v| v.parse().ok()),
                ) {
                    numbering.nums.insert(num, abstract_id);
                }
            }
            _ => {}
        }
    }

    /// Handle a closing (or self-closing) tag of `numbering.xml`.
    fn end(numbering: &mut Self, open: &mut Open, name: &[u8]) {
        match name {
            b"lvl" => {
                if let (Some(abstract_id), Some(level)) = (open.abstract_id, open.level) {
                    numbering.abstracts.entry(abstract_id).or_default().insert(
                        level,
                        LevelDef {
                            ordered: open.ordered,
                            none: open.none,
                            start: open.start,
                        },
                    );
                }
                open.level = None;
            }
            b"abstractNum" => open.abstract_id = None,
            b"num" => open.num = None,
            _ => {}
        }
    }

    /// Whether this list reference marks no item: `numId` 0 means no
    /// numbering, and a level whose `numFmt` is "none" leaves its paragraphs
    /// plain. A `numId` with no definition still reads as bullets (see
    /// [`Numbering::ordinal`]), as does a level Word never defined.
    pub(crate) fn is_plain(&self, num_id: u32, ilvl: u8) -> bool {
        if num_id == 0 {
            return true;
        }
        let Some(abstract_id) = self.nums.get(&num_id) else {
            return false;
        };
        self.abstracts
            .get(abstract_id)
            .and_then(|levels| levels.get(&ilvl))
            .is_some_and(|def| def.none)
    }

    /// The ordinal of the next item of list `num_id` at depth `ilvl`, or
    /// `None` where the list marks with bullets.
    ///
    /// A `numId` with no definition in `numbering.xml` is read as a bullet
    /// list: a dangling reference still groups the paragraphs, and `-` is the
    /// rendering that loses the least. A defined list whose level is missing
    /// counts from 1.
    pub(crate) fn ordinal(&mut self, num_id: u32, ilvl: u8) -> Option<u32> {
        let abstract_id = *self.nums.get(&num_id)?;
        let (ordered, start) = match self
            .abstracts
            .get(&abstract_id)
            .and_then(|levels| levels.get(&ilvl))
        {
            Some(def) => (def.ordered, def.start),
            None if self.abstracts.contains_key(&abstract_id) => (true, 1),
            None => return None,
        };
        if !ordered {
            return None;
        }
        // A new item at this depth restarts every deeper counter of the same
        // list, the way Word restarts subordinate numbers under a new parent.
        self.counters
            .retain(|(num, level), _| *num != num_id || *level <= ilvl);
        let count = self.counters.entry((num_id, ilvl)).or_insert(0);
        *count = count.saturating_add(1);
        Some(start.saturating_add(count.saturating_sub(1)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two lists: 1 is bulleted, 2 counts from 1 (level 0) and 5 (level 1).
    fn numbering() -> Numbering {
        let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="0">
    <w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="bullet"/></w:lvl>
  </w:abstractNum>
  <w:abstractNum w:abstractNumId="1">
    <w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/></w:lvl>
    <w:lvl w:ilvl="1"><w:start w:val="5"/><w:numFmt w:val="decimal"/></w:lvl>
    <w:lvl w:ilvl="2"><w:start w:val="1"/><w:numFmt w:val="none"/></w:lvl>
  </w:abstractNum>
  <w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>
  <w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num>
</w:numbering>"#;
        Numbering::parse(xml).unwrap()
    }

    #[test]
    fn bullets_have_no_ordinal() {
        let mut numbering = numbering();
        assert_eq!(numbering.ordinal(1, 0), None);
        assert_eq!(numbering.ordinal(1, 0), None);
    }

    #[test]
    fn counters_count_and_restart_under_a_new_parent() {
        let mut numbering = numbering();
        assert_eq!(numbering.ordinal(2, 0), Some(1));
        assert_eq!(numbering.ordinal(2, 1), Some(5));
        assert_eq!(numbering.ordinal(2, 1), Some(6));
        assert_eq!(numbering.ordinal(2, 0), Some(2));
        // The new level-0 item restarted the level-1 counter.
        assert_eq!(numbering.ordinal(2, 1), Some(5));
    }

    #[test]
    fn dangling_num_id_reads_as_bullets() {
        let mut numbering = numbering();
        assert_eq!(numbering.ordinal(99, 0), None);
    }

    #[test]
    fn num_id_zero_and_num_fmt_none_mark_no_item() {
        let numbering = numbering();
        assert!(numbering.is_plain(0, 0));
        assert!(numbering.is_plain(2, 2));
        assert!(!numbering.is_plain(1, 0));
        assert!(!numbering.is_plain(2, 0));
        assert!(!numbering.is_plain(99, 0));
    }
}
