// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Reading a CMap far enough to say which CIDs a drawn string asks for.
//!
//! A Type0 font's `/Encoding` is a CMap, and a CMap does two things at once:
//! it says how many bytes the next character code is (`codespacerange`), and
//! it says which CID that code means (`cidrange`, `cidchar`). `Identity-H`
//! answers both without a table -- two bytes, and the code is the CID -- and
//! that is the only shape the rest of this crate reads without help.
//!
//! Everything here exists for the other shape. `GBKp-EUC-H` writes one byte
//! for `<00>..<80>` and two for `<8140>..<FEFE>`, and code `0xCED2` means CID
//! 3809; nothing about those numbers can be guessed from the bytes. Without
//! the table a pass has two honest options -- leave the font alone, or read
//! the code as if it were a CID, which is the mistake `pdfa_fonts` documents
//! at length and refuses to repeat.
//!
//! This module is the third option: read the table. It is deliberately not
//! the cidrange reader in `pdfa_fonts`, which answers "what CID is this code"
//! for the width and `.notdef` repairs and never has to find a code boundary.
//! Decoding a *string* needs the codespace, and a reader that guesses the
//! boundary is the `90ms-RKSJ-H` defect all over again.
//!
//! WHAT IT REFUSES
//!
//! `codes` answers `None` for a byte the codespace does not cover, and
//! [`CodeCidMap::parse`] answers `None` for a CMap that names another one it
//! cannot find, or that maps its codes with `bfchar`/`bfrange` -- a shape a
//! reader here resolves and this file deliberately does not. A caller that cannot resolve every byte of every string must
//! treat the font as unreadable, exactly as it did before this module existed.
//! Over-retention is safe here and under-retention is not: a CID we fail to
//! list is an outline somebody deletes.

use std::collections::BTreeSet;

/// A code range in the codespace: how many bytes, and the inclusive bounds of
/// those bytes read big-endian.
type Codespace = (usize, u32, u32);

/// One mapping: `(byte length, low code, high code, CID)`.
///
/// The byte length is part of the key, not decoration. A CMap may give
/// different meanings to a one-byte `<80>` and a two-byte `<0080>`, and both
/// are the number 128: keyed by value alone the second entry answers for the
/// first, the wrong CID comes back, and this module's callers delete the glyph
/// the document really draws. `pdf-font`'s CMap keys by length for the same
/// reason (Codex, on the review of #503).
type Entry = (usize, u32, u32, u32);

/// A character code as this module carries it: `(byte length, value)`.
pub(crate) type Code = (usize, u32);

/// The code-to-CID table of one CMap, with the codespace that finds the codes.
#[derive(Debug, Clone, Default)]
pub(crate) struct CodeCidMap {
    codespace: Vec<Codespace>,
    /// What `cidrange` says -- the CIDs run with the code.
    ranges: Vec<Entry>,
    /// What `cidchar` says, as a one-code range so the lookup reads one shape.
    chars: Vec<Entry>,
    /// What `notdefrange` and `notdefchar` say: the glyph to draw for a code
    /// the CMap does NOT otherwise map.
    ///
    /// Kept apart from `chars` rather than merged into it, because the two
    /// carry a precedence that source order must not decide. A CMap may write
    /// a broad `notdefrange` after a `cidchar` inside it, and a lookup that
    /// took the last match would answer `.notdef` for a code the document
    /// really draws -- and this module's callers delete the glyphs nobody
    /// asks for, so a wrong `.notdef` here is a deleted outline. `pdf-font`'s
    /// `CMap::lookup_cid_code` reads them in the same order for the same
    /// reason (Codex, on the review of #503).
    notdefs: Vec<Entry>,
    /// The CMap this one names with `usecmap`, kept nested rather than merged.
    ///
    /// Merging loses a precedence that matters: a child may override a code its
    /// base maps with `cidrange` by naming it in its own `cidchar`, and a
    /// lookup that read all ranges before any chars would answer the base's
    /// CID. `CMap::lookup_cid_code` consults its base only after everything at
    /// its own level, and so does this (Codex, on the review of #503).
    base: Option<Box<CodeCidMap>>,
}

/// The longest character code any predefined CMap uses (ISO 32000-1 9.7.6.2).
const MAX_CODE_BYTES: usize = 4;

fn hex_value(token: &str) -> Option<(usize, u32)> {
    let body = token.strip_prefix('<')?.strip_suffix('>')?;
    if body.is_empty() || body.len() > MAX_CODE_BYTES * 2 || !body.len().is_multiple_of(2) {
        return None;
    }
    Some((body.len() / 2, u32::from_str_radix(body, 16).ok()?))
}

impl CodeCidMap {
    /// Parse a CMap program: the predefined files, and the embedded streams
    /// that carry the same syntax.
    ///
    /// `resolve` supplies the body of a CMap this one names with `usecmap`.
    /// A `usecmap` that cannot be resolved ends the parse, because the ranges
    /// that are here are then only half the table and half a table maps a code
    /// to the wrong CID rather than to none.
    pub(crate) fn parse(
        data: &[u8],
        resolve: &dyn Fn(&str) -> Option<Vec<u8>>,
    ) -> Option<CodeCidMap> {
        Self::parse_at_depth(data, resolve, 0)
    }

    fn parse_at_depth(
        data: &[u8],
        resolve: &dyn Fn(&str) -> Option<Vec<u8>>,
        depth: usize,
    ) -> Option<CodeCidMap> {
        // A CMap that names itself, directly or in a cycle, is not a table.
        if depth > 4 {
            return None;
        }
        let text = String::from_utf8_lossy(data);
        let mut map = CodeCidMap::default();

        let tokens: Vec<&str> = text
            .lines()
            // A CMap is PostScript: everything after % is a comment, and the
            // Adobe files carry their licence in one.
            .map(|line| line.split('%').next().unwrap_or(""))
            .flat_map(str::split_whitespace)
            .collect();

        for (i, token) in tokens.iter().enumerate() {
            if *token == "usecmap" {
                let named = tokens.get(i.checked_sub(1)?)?.strip_prefix('/')?;
                if map.base.is_some() {
                    // Two bases is not a shape ISO 32000-1 9.7.5.3 describes,
                    // and guessing which one wins is guessing at a glyph.
                    return None;
                }
                let base = CodeCidMap::parse_at_depth(&resolve(named)?, resolve, depth + 1)?;
                // The code space is inherited -- it decides where a code ends,
                // not what it means, so more of it only makes more codes
                // readable. The MAPPINGS stay at their own level.
                map.codespace.extend(base.codespace.iter().copied());
                map.base = Some(Box::new(base));
            }
        }

        // The sections are read as whole windows rather than as a token
        // stream, so that a stray token inside one cannot shift the arity of
        // everything after it.
        let mut cursor = 0usize;
        while cursor < tokens.len() {
            let opener = tokens[cursor];
            cursor += 1;
            let (closer, arity) = match opener {
                "begincodespacerange" => ("endcodespacerange", 2),
                "begincidrange" => ("endcidrange", 3),
                "beginnotdefrange" => ("endnotdefrange", 3),
                "begincidchar" => ("endcidchar", 2),
                "beginnotdefchar" => ("endnotdefchar", 2),
                // A `/Encoding` CMap is supposed to map codes to CIDs, and a
                // malformed one maps them with `bfchar`/`bfrange` instead --
                // `pdf-font`'s `CMap::lookup_cid_code` reads those as CIDs on
                // purpose, citing `pdfjs_bug920426`, so a reader DOES draw
                // them. Skipping the section would leave every such code
                // answering CID 0, and a caller here deletes the glyphs
                // nothing asks for: the page would lose the text it draws.
                //
                // Refused rather than parsed. The destination is a hex string
                // and may be a UTF-16 pair or an array, and a mistake reading
                // it costs an outline; leaving the font whole costs bytes.
                // That is the trade this whole module makes (Codex, on the
                // review of #503).
                "beginbfchar" | "beginbfrange" => return None,
                _ => continue,
            };
            let end = tokens[cursor..]
                .iter()
                .position(|t| *t == closer)
                .map(|at| cursor + at)?;
            let body = &tokens[cursor..end];
            cursor = end + 1;
            if !body.len().is_multiple_of(arity) {
                return None;
            }
            for entry in body.chunks(arity) {
                match opener {
                    "begincodespacerange" => {
                        let (bytes, low) = hex_value(entry[0])?;
                        let (high_bytes, high) = hex_value(entry[1])?;
                        if bytes != high_bytes || low > high {
                            return None;
                        }
                        map.codespace.push((bytes, low, high));
                    }
                    "begincidrange" | "beginnotdefrange" => {
                        let (bytes, low) = hex_value(entry[0])?;
                        let (high_bytes, high) = hex_value(entry[1])?;
                        // A range whose ends disagree about their width says
                        // nothing this file can act on.
                        if bytes != high_bytes {
                            return None;
                        }
                        let cid: u32 = entry[2].parse().ok()?;
                        // A CID is 0..65535 (ISO 32000-1 9.7.4.2). A table that
                        // names anything else is not one this file will act on:
                        // the number indexes no glyph, and a caller that
                        // believed it would delete the ones it does.
                        if cid > u32::from(u16::MAX) {
                            return None;
                        }
                        if low <= high {
                            let into = if opener == "begincidrange" {
                                &mut map.ranges
                            } else {
                                &mut map.notdefs
                            };
                            into.push((bytes, low, high, cid));
                        }
                    }
                    // `cidchar` and `notdefchar` are both one code, one CID,
                    // and they land in different lists for the reason above.
                    other => {
                        let (bytes, code) = hex_value(entry[0])?;
                        let cid: u32 = entry[1].parse().ok()?;
                        if cid > u32::from(u16::MAX) {
                            return None;
                        }
                        if other == "beginnotdefchar" {
                            map.notdefs.push((bytes, code, code, cid));
                        } else {
                            map.chars.push((bytes, code, code, cid));
                        }
                    }
                }
            }
        }

        (!map.codespace.is_empty()).then_some(map)
    }

    /// The byte length of the code starting at `bytes`, or `None` when the
    /// codespace does not cover it.
    ///
    /// ISO 32000-1 9.7.6.2 matches the shortest code whose bytes fall inside a
    /// range of that length, which is what a reader does with the same table.
    fn code_length(&self, bytes: &[u8]) -> Option<usize> {
        for len in 1..=MAX_CODE_BYTES.min(bytes.len()) {
            let value = bytes[..len]
                .iter()
                .fold(0u32, |acc, byte| (acc << 8) | u32::from(*byte));
            if self
                .codespace
                .iter()
                .any(|(width, low, high)| *width == len && (*low..=*high).contains(&value))
            {
                return Some(len);
            }
        }
        None
    }

    /// The CIDs of a set of codes.
    ///
    /// Codes are gathered before they are looked up because the lookup is a
    /// scan of the table and a document repeats its codes: `UniJIS-UTF16-H`
    /// has some ten thousand ranges, and a page of text has some thousands of
    /// codes but only hundreds of distinct ones.
    pub(crate) fn cids_of(&self, codes: &BTreeSet<Code>, out: &mut BTreeSet<u32>) {
        out.extend(codes.iter().map(|code| self.cid(*code)));
    }

    /// Whether any code this table maps can select `cid`.
    ///
    /// A `cidrange` runs its CIDs with its codes, so it produces a span as long
    /// as its code range; `cidchar` and the notdef entries produce one each,
    /// and a base named with `usecmap` produces what it produces. CID 0 is
    /// always among them: ISO 32000-1 9.7.6.3 gives it to every code no level
    /// names.
    ///
    /// The question a caller asks with this is not "which CID is this code" but
    /// "is this number a CID of this font at all" -- a /W array whose keys this
    /// encoding can never produce is keyed by something that is not a CID
    /// (#512).
    pub(crate) fn produces(&self, cid: u32) -> bool {
        let hit = |table: &[Entry], run: bool| {
            table.iter().any(|(_, low, high, first)| {
                let last = if run {
                    first.saturating_add(high - low)
                } else {
                    *first
                };
                (*first..=last).contains(&cid)
            })
        };
        cid == 0
            || hit(&self.ranges, true)
            || hit(&self.chars, false)
            || hit(&self.notdefs, false)
            || self.base.as_ref().is_some_and(|base| base.produces(cid))
    }

    /// Every CID this table can produce, which is the whole repertoire the
    /// encoding can address.
    ///
    /// [`produces`](Self::produces) answers the same question one CID at a
    /// time, and a caller that has to ask it of a whole face asks it sixty-five
    /// thousand times over a table ten thousand entries long. This walks the
    /// entries instead, so the cost is the table rather than the product of the
    /// table and the CID space.
    ///
    /// What it is for: a font program an interactive form field could draw NEW
    /// text with has no census bound -- nobody has typed the text yet -- and
    /// the analogue of the full simple-font repertoire (#379 round 3) is this
    /// set. CID 0 is included for the same reason `produces` includes it.
    ///
    /// CIDs above 65535 are dropped rather than returned: ISO 32000-1 9.7.4.2
    /// has no such CID, and a malformed `cidrange` that names one would make
    /// this set as long as the arithmetic allows.
    ///
    /// The spans are merged before anything is inserted, and that is not tidying
    /// up. Expanding each `cidrange` on its own costs its length whether or not
    /// another range already covered it, so a thousand repeated
    /// `<0000> <ffff> 0` entries -- which a document may embed and which a
    /// hundred bytes of CMap can write -- would be sixty-five million
    /// insertions for a set of 65,536 (Codex, on the review of #513). Merged,
    /// the cost is the table plus the answer.
    pub(crate) fn produced_cids(&self, out: &mut BTreeSet<u32>) {
        let mut spans = Vec::new();
        self.cid_spans(&mut spans);
        spans.sort_unstable();
        let mut open: Option<(u32, u32)> = None;
        for (low, high) in spans {
            match open {
                // `spans` is sorted, so a span either extends the run being
                // held or starts past its end.
                Some((_, last)) if low <= last.saturating_add(1) => {
                    open = open.map(|(first, last)| (first, last.max(high)));
                }
                Some((first, last)) => {
                    out.extend(first..=last);
                    open = Some((low, high));
                }
                None => open = Some((low, high)),
            }
        }
        if let Some((first, last)) = open {
            out.extend(first..=last);
        }
    }

    /// The inclusive CID spans of this table and everything it inherits.
    fn cid_spans(&self, out: &mut Vec<(u32, u32)>) {
        const LAST_CID: u32 = u16::MAX as u32;
        // CID 0 is always among them: ISO 32000-1 9.7.6.3 gives it to every code
        // no level names.
        out.push((0, 0));
        for (_, low, high, first) in &self.ranges {
            if *first > LAST_CID {
                continue;
            }
            out.push((*first, first.saturating_add(high - low).min(LAST_CID)));
        }
        for table in [&self.chars, &self.notdefs] {
            out.extend(
                table
                    .iter()
                    .map(|(_, _, _, cid)| *cid)
                    .filter(|cid| *cid <= LAST_CID)
                    .map(|cid| (cid, cid)),
            );
        }
        if let Some(base) = &self.base {
            base.cid_spans(out);
        }
    }

    /// The CID a code means at THIS level, or nothing.
    ///
    /// Order is the contract: a real mapping first -- `cidrange`, then
    /// `cidchar` -- and only a code neither of them names falls through to a
    /// `notdefrange`. Within one kind the LAST entry wins. A base named with
    /// `usecmap` comes after all three, so a child overrides what it inherited
    /// whichever kind either of them used.
    fn lookup(&self, code: Code) -> Option<u32> {
        let (bytes, value) = code;
        let hit = |table: &[Entry], run: bool| {
            table
                .iter()
                .rev()
                .find(|(width, low, high, _)| *width == bytes && (*low..=*high).contains(&value))
                .map(|(_, low, _, cid)| if run { cid + (value - low) } else { *cid })
        };
        hit(&self.ranges, true)
            .or_else(|| hit(&self.chars, false))
            .or_else(|| hit(&self.notdefs, false))
            .or_else(|| self.base.as_ref()?.lookup(code))
    }

    fn cid(&self, code: Code) -> u32 {
        // A code no level names at all is CID 0 by ISO 32000-1 9.7.6.3, and
        // glyph 0 is retained unconditionally.
        self.lookup(code).unwrap_or(0)
    }

    /// Every character code a drawn string asks for, or `None` when a byte
    /// falls outside the codespace -- which means this table is not the one
    /// that string was written against, and nothing may be concluded from it.
    ///
    /// Codes rather than CIDs, and that is the whole point of the split: a
    /// caller reads a document's worth of strings into one code set and pays
    /// for the table lookup once per distinct code. Resolving per string
    /// instead repeats the scan for every occurrence, and `UniJIS-UTF16-H` has
    /// some thirteen thousand entries to scan (Codex, on the review of #503).
    pub(crate) fn codes(&self, bytes: &[u8], out: &mut BTreeSet<Code>) -> Option<()> {
        let mut at = 0;
        while at < bytes.len() {
            let len = self.code_length(&bytes[at..])?;
            out.insert((
                len,
                bytes[at..at + len]
                    .iter()
                    .fold(0u32, |acc, byte| (acc << 8) | u32::from(*byte)),
            ));
            at += len;
        }
        Some(())
    }

    /// Every CID a drawn string asks for, or `None` for a byte the codespace
    /// does not cover. The one-string convenience over [`Self::codes`].
    #[cfg(test)]
    pub(crate) fn decode(&self, bytes: &[u8], out: &mut BTreeSet<u32>) -> Option<()> {
        let mut codes = BTreeSet::new();
        self.codes(bytes, &mut codes)?;
        self.cids_of(&codes, out);
        Some(())
    }

    /// Every code any reading of this string could ask for.
    ///
    /// The census that feeds this is document-wide on purpose, so most strings
    /// handed to a given table were drawn with some other font and need not
    /// decode at all: a Latin `(Introduction)` under `GBK-EUC-H` has bytes the
    /// code space does not cover, and refusing the whole program over it was
    /// costing `issue14438` its subset entirely.
    ///
    /// Dropping such a string would be the unsafe answer -- ISO 32000-1 9.7.6.2
    /// leaves a reader that meets an uncovered code free to consume bytes and
    /// carry on, so the string may still be this font's. This takes every code
    /// that starts at any offset with any width the code space uses, which is a
    /// superset of whatever the reader lands on. It costs outlines, never
    /// characters.
    pub(crate) fn codes_or_superset(&self, bytes: &[u8], out: &mut BTreeSet<Code>) {
        if self.codes(bytes, out).is_some() {
            return;
        }
        let widths: BTreeSet<usize> = self.codespace.iter().map(|(width, _, _)| *width).collect();
        for at in 0..bytes.len() {
            for width in &widths {
                let Some(slice) = bytes.get(at..at + width) else {
                    continue;
                };
                out.insert((
                    *width,
                    slice
                        .iter()
                        .fold(0u32, |acc, byte| (acc << 8) | u32::from(*byte)),
                ));
            }
        }
    }
}

/// The predefined CMap tables this build carries, verbatim from Adobe's public
/// CMap resources (`crates/pdf-manip/resources/cid-tables`, BSD-3-Clause, the
/// notice inside each file).
///
/// They are compiled in rather than read from the host because the answer has
/// to be the same everywhere: a converter that shrinks a CJK document on a
/// machine with `poppler-data` installed and ships a 410 kB substitute face on
/// one without it is not a converter anybody can measure.
///
/// WHY THEY ARE NOT IN `resources/cmap`. That directory is the first entry of
/// `pdfa_fonts::PREDEFINED_CMAP_SEARCH_DIRS`, the *host* search path for the
/// width and `.notdef` repairs, and a file dropped into it changes which
/// documents those passes act on -- putting these five there embedded a 79 kB
/// `/CMap` stream into `issue14438` and cost more than the subset saved. The
/// subsetter's own tables therefore live in their own directory and are read
/// only from here.
///
/// The browser has no filesystem and no substitute faces to shrink (#383), so
/// it carries none of this.
#[cfg(not(target_arch = "wasm32"))]
const BUNDLED: &[(&str, &[u8])] = &[
    (
        "GB-EUC-H",
        include_bytes!("../resources/cmap/GB-EUC-H").as_slice(),
    ),
    (
        "GBK-EUC-H",
        include_bytes!("../resources/cid-tables/GBK-EUC-H"),
    ),
    (
        "GBKp-EUC-H",
        include_bytes!("../resources/cid-tables/GBKp-EUC-H"),
    ),
    (
        "90ms-RKSJ-H",
        include_bytes!("../resources/cid-tables/90ms-RKSJ-H"),
    ),
    (
        "UniGB-UTF16-H",
        include_bytes!("../resources/cid-tables/UniGB-UTF16-H"),
    ),
    (
        "UniJIS-UTF16-H",
        include_bytes!("../resources/cid-tables/UniJIS-UTF16-H"),
    ),
];

#[cfg(target_arch = "wasm32")]
const BUNDLED: &[(&str, &[u8])] = &[];

fn bundled(name: &str) -> Option<Vec<u8>> {
    BUNDLED
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, data)| data.to_vec())
}

/// The code-to-CID table of a named predefined CMap, or nothing.
///
/// `Identity-H`/`Identity-V` deliberately answer `None`: the code is the CID
/// there, and a caller that has to ask this question at all already handles
/// that case without a table.
pub(crate) fn predefined(name: &str) -> Option<CodeCidMap> {
    CodeCidMap::parse(&bundled(name)?, &bundled)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A two-width codespace with one range in each, which is the shape every
    /// EUC CMap has and the shape a fixed two-byte reader gets wrong.
    const MIXED: &str = "\
/CIDInit /ProcSet findresource begin
2 begincodespacerange
  <00>   <80>
  <8140> <FEFE>
endcodespacerange
1 beginnotdefrange
<00> <1f> 1
endnotdefrange
2 begincidrange
<20> <7e> 814
<8140> <8140> 7000
endcidrange
1 begincidchar
<8141> 7777
endcidchar
endcmap
";

    fn decode(map: &CodeCidMap, bytes: &[u8]) -> Option<Vec<u32>> {
        let mut out = BTreeSet::new();
        map.decode(bytes, &mut out)?;
        Some(out.into_iter().collect())
    }

    #[test]
    fn a_one_byte_code_is_not_half_of_a_two_byte_one() {
        let map = CodeCidMap::parse(MIXED.as_bytes(), &|_| None).expect("parse");
        // 0x41 is a single byte; 0x8140 and 0x8141 are two.
        assert_eq!(decode(&map, &[0x41]), Some(vec![814 + 0x41 - 0x20]));
        assert_eq!(decode(&map, &[0x81, 0x40]), Some(vec![7000]));
        assert_eq!(decode(&map, &[0x81, 0x41]), Some(vec![7777]));
        // Read as fixed pairs, <4181> is not a code at all and <8140> is lost.
        assert_eq!(
            decode(&map, &[0x41, 0x81, 0x40]),
            Some(vec![814 + 0x41 - 0x20, 7000])
        );
    }

    /// A `notdefrange` written AFTER a `cidchar` it overlaps. Source order must
    /// not decide this: `.notdef` is what a code means when nothing else maps
    /// it, and answering it for a code that IS mapped deletes a drawn glyph.
    /// A child that overrides one code of the base it names. The override is a
    /// `cidchar` and the base maps the code with a `cidrange`, so a lookup that
    /// read every range before any char would answer the base and drop the
    /// glyph the child selects.
    #[test]
    fn a_child_cmap_overrides_the_base_across_mapping_kinds() {
        let base = "1 begincodespacerange\n<00> <ff>\nendcodespacerange\n\
                    1 begincidrange\n<41> <45> 100\nendcidrange\n";
        let child = "/Base usecmap\n1 begincodespacerange\n<00> <ff>\nendcodespacerange\n\
                     1 begincidchar\n<42> 900\nendcidchar\n";
        let map = CodeCidMap::parse(child.as_bytes(), &|name| {
            (name == "Base").then(|| base.as_bytes().to_vec())
        })
        .expect("parse with base");
        assert_eq!(decode(&map, &[0x42]), Some(vec![900]), "the child wins");
        assert_eq!(
            decode(&map, &[0x41]),
            Some(vec![100]),
            "the base still answers the rest"
        );
        assert_eq!(
            decode(&map, &[0x43]),
            Some(vec![102]),
            "and a run keeps running"
        );
    }

    #[test]
    fn a_notdef_range_never_masks_a_real_mapping() {
        let text = "\
1 begincodespacerange
<00> <ff>
endcodespacerange
1 begincidchar
<41> 900
endcidchar
1 beginnotdefrange
<00> <ff> 1
endnotdefrange
";
        let map = CodeCidMap::parse(text.as_bytes(), &|_| None).expect("parse");
        assert_eq!(decode(&map, &[0x41]), Some(vec![900]), "the cidchar wins");
        assert_eq!(
            decode(&map, &[0x42]),
            Some(vec![1]),
            "and .notdef covers the rest"
        );
    }

    /// A malformed `/Encoding` CMap that maps its codes with `bfchar`. A
    /// reader resolves those (`CMap::lookup_cid_code`, `pdfjs_bug920426`), so
    /// answering CID 0 for them would delete glyphs the page draws. The table
    /// is refused instead, and the font stays whole.
    #[test]
    fn a_cmap_that_maps_with_bfchar_is_refused_rather_than_half_read() {
        for section in [
            "1 beginbfchar\n<41> <0050>\nendbfchar\n",
            "1 beginbfrange\n<41> <45> <0050>\nendbfrange\n",
        ] {
            let text = format!(
                "1 begincodespacerange\n<00> <ff>\nendcodespacerange\n\
                 1 begincidrange\n<20> <2f> 100\nendcidrange\n{section}"
            );
            assert!(
                CodeCidMap::parse(text.as_bytes(), &|_| None).is_none(),
                "a table that maps codes this file cannot read is not a table"
            );
        }
    }

    /// A CMap with a one-byte and a two-byte code space, mapping the number
    /// 128 differently in each. Keyed by value alone the two collide, one
    /// answers for the other, and the glyph the other draws is deleted. Both
    /// code spaces are reachable from a real string here, so the test measures
    /// the decode as well as the table.
    #[test]
    fn a_one_byte_code_and_a_two_byte_code_of_the_same_value_are_not_one_code() {
        let text = "\
2 begincodespacerange
<81> <81>
<0080> <00ff>
endcodespacerange
2 begincidrange
<80> <80> 5
<0080> <0080> 900
endcidrange
1 begincidchar
<81> 7
endcidchar
";
        let map = CodeCidMap::parse(text.as_bytes(), &|_| None).expect("parse");
        // <0080> is a two-byte code and reaches 900; the one-byte entry for
        // the same number is a different mapping and must not answer for it.
        assert_eq!(decode(&map, &[0x00, 0x80]), Some(vec![900]));
        assert_eq!(decode(&map, &[0x81]), Some(vec![7]));
        // And the one-byte <80> entry is reachable only as a one-byte code.
        let mut cids = BTreeSet::new();
        map.cids_of(&BTreeSet::from([(1usize, 0x80u32)]), &mut cids);
        assert_eq!(cids.into_iter().collect::<Vec<_>>(), vec![5]);
    }

    /// A CID a table cannot legally name. ISO 32000-1 9.7.4.2 stops at 65535,
    /// and a bigger number would be multiplied into a `/CIDToGIDMap` offset --
    /// on a 32-bit target that overflows rather than misses.
    #[test]
    fn a_cid_outside_the_cid_range_is_not_a_table() {
        for section in [
            "1 begincidchar\n<41> 2147483648\nendcidchar\n",
            "1 begincidrange\n<41> <45> 70000\nendcidrange\n",
        ] {
            let text = format!("1 begincodespacerange\n<00> <ff>\nendcodespacerange\n{section}");
            assert!(CodeCidMap::parse(text.as_bytes(), &|_| None).is_none());
        }
    }

    #[test]
    fn a_notdef_range_is_one_cid_and_not_a_run() {
        let map = CodeCidMap::parse(MIXED.as_bytes(), &|_| None).expect("parse");
        assert_eq!(decode(&map, &[0x00, 0x1f]), Some(vec![1]));
    }

    #[test]
    fn a_byte_outside_the_codespace_answers_nothing() {
        let map = CodeCidMap::parse(MIXED.as_bytes(), &|_| None).expect("parse");
        // 0x81 alone has no second byte, and <8100> is below <8140> while 0x81
        // is above <80>: no width covers it.
        assert_eq!(decode(&map, &[0x81]), None);
        assert_eq!(decode(&map, &[0x81, 0x00]), None);
    }

    #[test]
    fn a_usecmap_that_cannot_be_resolved_is_not_half_a_table() {
        let text = "/Base usecmap\n1 begincodespacerange\n<00> <ff>\nendcodespacerange\n";
        assert!(CodeCidMap::parse(text.as_bytes(), &|_| None).is_none());
        let base = "1 begincodespacerange\n<00> <ff>\nendcodespacerange\n\
                    1 begincidrange\n<00> <ff> 5\nendcidrange\n";
        let map = CodeCidMap::parse(text.as_bytes(), &|name| {
            (name == "Base").then(|| base.as_bytes().to_vec())
        })
        .expect("parse with base");
        assert_eq!(decode(&map, &[0x02]), Some(vec![7]));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn the_bundled_tables_answer_the_codes_their_documents_draw() {
        // `pdfjs__test_pdfs_issue3521.pdf` draws these seven GBK codes.
        let gbkp = predefined("GBKp-EUC-H").expect("GBKp-EUC-H is bundled");
        let mut cids = BTreeSet::new();
        gbkp.decode(
            &[
                0xCE, 0xD2, 0xC3, 0xC7, 0xB6, 0xBC, 0xCA, 0xC7, 0xBA, 0xDA, 0xCC, 0xE5, 0xD7, 0xD6,
            ],
            &mut cids,
        )
        .expect("every byte is in the codespace");
        assert_eq!(
            cids.into_iter().collect::<Vec<_>>(),
            vec![1531, 1937, 2764, 3422, 3640, 3809, 4659]
        );

        for name in [
            "GB-EUC-H",
            "GBK-EUC-H",
            "90ms-RKSJ-H",
            "UniGB-UTF16-H",
            "UniJIS-UTF16-H",
        ] {
            assert!(predefined(name).is_some(), "{name} did not parse");
        }
        assert!(predefined("Identity-H").is_none());
    }

    /// The repertoire a form font gets when the census cannot bound it (#513).
    #[test]
    fn the_produced_cids_are_the_repertoire_the_encoding_addresses() {
        let map = CodeCidMap::parse(MIXED.as_bytes(), &|_| None).expect("parse");
        let mut cids = BTreeSet::new();
        map.produced_cids(&mut cids);
        // The `cidrange` <20>..<7e> runs its CIDs with its codes, so 814..=908
        // are all in; <8140> gives 7000; the `cidchar` gives 7777; the
        // `notdefrange` gives 1; and CID 0 is always reachable.
        assert!(cids.contains(&0) && cids.contains(&1));
        assert!((814..=908).all(|cid| cids.contains(&cid)));
        assert!(cids.contains(&7000) && cids.contains(&7777));
        assert!(!cids.contains(&813) && !cids.contains(&909));
        // What the set is FOR: everything `produces` answers yes to, and
        // nothing else. Asked of a face one glyph at a time, that question is
        // the whole reason this method exists.
        assert!(cids.iter().all(|cid| map.produces(*cid)));
        assert_eq!(
            cids.len(),
            (0..=u32::from(u16::MAX))
                .filter(|cid| map.produces(*cid))
                .count(),
            "the set and the per-CID answer have to agree"
        );
    }

    /// Overlapping and adjacent `cidrange` entries are one run, and the run is
    /// the same set the entries name.
    #[test]
    fn overlapping_cid_ranges_merge_into_the_set_they_name() {
        const OVERLAPPING: &str = "\
/CIDInit /ProcSet findresource begin
1 begincodespacerange
  <0000> <FFFF>
endcodespacerange
4 begincidrange
<0000> <0009> 100
<0005> <000e> 105
<0100> <0100> 200
<0101> <0101> 201
endcidrange
endcmap
";
        let map = CodeCidMap::parse(OVERLAPPING.as_bytes(), &|_| None).expect("parse");
        let mut cids = BTreeSet::new();
        map.produced_cids(&mut cids);
        assert_eq!(
            cids.iter().copied().collect::<Vec<_>>(),
            [0].into_iter()
                .chain(100..=114)
                .chain([200, 201])
                .collect::<Vec<_>>()
        );
    }

    /// A CMap a document embeds may repeat a range that covers the whole CID
    /// space. Expanded one range at a time that is sixty-five million
    /// insertions for a set of 65,536, from a hundred bytes of CMap (Codex, on
    /// the review of #513).
    ///
    /// The bound is loose on purpose: the merged walk does about 65,000
    /// insertions and finishes in milliseconds, so ten seconds is four orders
    /// of magnitude of headroom for a loaded machine and still an order of
    /// magnitude under what expanding each range separately costs here.
    #[test]
    fn a_repeated_whole_space_range_is_not_paid_for_once_per_entry() {
        let mut cmap = String::from(
            "/CIDInit /ProcSet findresource begin\n\
             1 begincodespacerange\n  <0000> <FFFF>\nendcodespacerange\n",
        );
        const REPEATS: usize = 1_000;
        for chunk in 0..REPEATS / 100 {
            cmap.push_str("100 begincidrange\n");
            for _ in 0..100 {
                cmap.push_str("<0000> <ffff> 0\n");
            }
            cmap.push_str("endcidrange\n");
            let _ = chunk;
        }
        cmap.push_str("endcmap\n");
        let map = CodeCidMap::parse(cmap.as_bytes(), &|_| None).expect("parse");
        let started = std::time::Instant::now();
        let mut cids = BTreeSet::new();
        map.produced_cids(&mut cids);
        let took = started.elapsed();
        assert_eq!(cids.len(), 1 << 16, "the whole CID space, counted once");
        assert!(
            took < std::time::Duration::from_secs(10),
            "{REPEATS} copies of one range took {took:?}: the ranges are being \
             expanded one at a time"
        );
    }

    /// A predefined table is a repertoire far smaller than the CID space, which
    /// is the whole reason a readable CMap bounds a form font and `Identity`
    /// does not.
    #[test]
    fn a_predefined_table_produces_far_fewer_cids_than_identity_reaches() {
        let table = predefined("UniJIS-UTF16-H").expect("the table is compiled in");
        let mut cids = BTreeSet::new();
        table.produced_cids(&mut cids);
        assert!(
            (1000..40000).contains(&cids.len()),
            "UniJIS-UTF16-H produces the CIDs of Adobe-Japan1, not 65,536 of them: {}",
            cids.len()
        );
        assert!(cids.iter().all(|cid| *cid <= u32::from(u16::MAX)));
    }
}
