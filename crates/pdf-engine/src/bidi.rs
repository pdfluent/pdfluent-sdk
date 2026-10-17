//! Visual order to logical order, one assembled line at a time (#436, class
//! R-H of #389).
//!
//! A PDF says where each glyph sits on the page and nothing about the order a
//! reader should read them in. For left-to-right text the two coincide, and the
//! extractor's rule -- sort a line's spans by x -- is the reading order. For
//! Arabic or Hebrew it is the reverse of it: the writer laid the run out
//! right-to-left, so reading the page left to right hands back every word
//! spelled backwards. `العربية` came out `ةيبرعلا`, which is not a spelling
//! mistake but an unsearchable string: no reader, index or `grep` will ever
//! match it against the word a person types.
//!
//! What this module does is UAX #9's L2 -- the reordering step -- run on the
//! text as drawn. That works because L2 is its own inverse when the embedding
//! levels travel with the characters: reversing the same runs a second time
//! puts them back. So we resolve a level per character from the text we have
//! and apply L2 to it, and out comes the order the writer started from.
//!
//! WHAT IS NOT IMPLEMENTED, and why the algorithm is not the whole one:
//!
//! - **The paragraph direction (P2/P3) is counted, not found.** The rule reads
//!   the *first* strong character of the paragraph, and the first character in
//!   logical order is exactly what we do not have yet -- it is the answer. So
//!   the line is called right-to-left when it holds more strong right-to-left
//!   characters than left-to-right ones. `Hello <arabic> World.` is a
//!   left-to-right line with one embedded run, and a line of Arabic is a
//!   right-to-left one, which is the distinction the two failing shapes need.
//! - **Explicit embedding codes (X1-X8) are ignored.** RLE/LRE/RLO/PDF and the
//!   isolates are formatting characters; a PDF content stream has no way to
//!   carry them into a drawn glyph, so a line assembled from glyphs cannot
//!   contain one.
//! - **Mirroring (L4) is undone only for the brackets in `MIRRORED`.** A
//!   character outside that table that the property does mirror -- most of the
//!   mathematical relations -- comes back as it was drawn.
//! - **W2 does not separate R from AL.** The one rule that needs the
//!   distinction turns a European digit after an Arabic *letter* into an Arabic
//!   number, and both readings give that digit the same embedding level here,
//!   so the distinction cannot change an output.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::borrow::Cow;

/// The bidirectional character classes this reordering tells apart. UAX #9
/// names nineteen; the five below plus "everything else" are the ones that can
/// move a character, and every rule that distinguishes two classes we merged
/// gives both the same embedding level (see the module docs).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Class {
    /// Strong left-to-right (UAX #9 `L`).
    Left,
    /// Strong right-to-left: `R` and `AL` together.
    Right,
    /// European digits (`EN`).
    EuropeanNumber,
    /// Arabic-Indic digits and the Arabic number signs (`AN`).
    ArabicNumber,
    /// A non-spacing mark (`NSM`) -- a Hebrew point, an Arabic diacritic, a
    /// combining accent. Never moves on its own; see `clusters`.
    Mark,
    /// Whitespace, punctuation, symbols -- everything a run boundary is decided
    /// *around* rather than by (`WS`, `ON`, `ES`, `ET`, `CS`, `B`, `S`).
    Neutral,
}

/// The direction a resolved class contributes to a neutral run (UAX #9 N1,
/// which counts a number as right-to-left for this purpose).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Direction {
    Left,
    Right,
}

/// A base character with the marks that belong to it, as one unit.
///
/// UAX #9 gives a mark the class of its base (W1) and then has to put the two
/// back in order afterwards (L3), because reversing a run reverses the pair as
/// well and a mark that precedes its base is a different string. Reversing
/// whole clusters instead of characters is the same answer without the second
/// step: `שָׁ` stays `שָׁ` however often its run turns around.
#[derive(Clone, Copy, Debug)]
struct Cluster {
    /// Index of the base character in the line's character vector.
    start: usize,
    /// One past the last mark that belongs to it.
    end: usize,
    /// The base character's class.
    class: Class,
}

/// Strong right-to-left blocks: Hebrew, Arabic, Syriac, Thaana, N'Ko,
/// Samaritan, Mandaic, both Arabic presentation-form blocks, the right-to-left
/// mark, and the right-to-left historic planes (Kharoshthi through Adlam).
///
/// Whole blocks rather than the exact `R`/`AL` code points: the marks and
/// digits inside them are named by the two tables below, which are consulted
/// first, and what is left over in a right-to-left block is a right-to-left
/// letter or a punctuation mark that a run of them will carry along anyway.
const RIGHT_TO_LEFT: &[(u32, u32)] = &[
    (0x0590, 0x05FF), // Hebrew
    (0x0600, 0x06FF), // Arabic
    (0x0700, 0x074F), // Syriac
    (0x0750, 0x077F), // Arabic Supplement
    (0x0780, 0x07BF), // Thaana
    (0x07C0, 0x07FF), // N'Ko
    (0x0800, 0x083F), // Samaritan
    (0x0840, 0x085F), // Mandaic
    (0x0860, 0x08FF), // Syriac Supplement, Arabic Extended-A/B
    (0x200F, 0x200F), // RIGHT-TO-LEFT MARK
    (0x061C, 0x061C), // ARABIC LETTER MARK
    (0xFB1D, 0xFB4F), // Hebrew presentation forms
    (0xFB50, 0xFDFF), // Arabic Presentation Forms-A
    (0xFE70, 0xFEFF), // Arabic Presentation Forms-B
    (0x10800, 0x10FFF),
    (0x1E800, 0x1EFFF),
];

/// `AN`: the Arabic-Indic digits and the number signs that go with them. The
/// *extended* Arabic-Indic digits U+06F0..U+06F9 are deliberately absent -- the
/// character database calls those `EN`, and they are in `EUROPEAN_NUMBER`.
const ARABIC_NUMBER: &[(u32, u32)] = &[
    (0x0600, 0x0605), // Arabic number signs
    (0x0660, 0x0669), // Arabic-Indic digits
    (0x066B, 0x066C), // decimal separator, thousands separator
    (0x06DD, 0x06DD), // END OF AYAH
    (0x08E2, 0x08E2), // DISPUTED END OF AYAH
    (0x10D30, 0x10D39),
];

/// `EN`: the ASCII digits and the extended Arabic-Indic ones.
const EUROPEAN_NUMBER: &[(u32, u32)] = &[(0x0030, 0x0039), (0x06F0, 0x06F9)];

/// `NSM`: the combining marks of the scripts this reordering moves, plus the
/// general-purpose combining ranges. Not the whole of the `Mn`/`Me` categories,
/// which would be a Unicode table: a mark outside a right-to-left run never
/// crosses a run boundary, so the ones that matter are the ones that can sit
/// inside one.
const MARK: &[(u32, u32)] = &[
    (0x0300, 0x036F), // Combining Diacritical Marks
    (0x0483, 0x0489),
    (0x0591, 0x05BD), // Hebrew points (nikkud, cantillation)
    (0x05BF, 0x05BF),
    (0x05C1, 0x05C2),
    (0x05C4, 0x05C5),
    (0x05C7, 0x05C7),
    (0x0610, 0x061A), // Arabic honorifics
    (0x064B, 0x065F), // Arabic diacritics (harakat)
    (0x0670, 0x0670),
    (0x06D6, 0x06DC),
    (0x06DF, 0x06E4),
    (0x06E7, 0x06E8),
    (0x06EA, 0x06ED),
    (0x0711, 0x0711),
    (0x0730, 0x074A), // Syriac
    (0x07A6, 0x07B0), // Thaana
    (0x07EB, 0x07F3), // N'Ko
    (0x07FD, 0x07FD),
    (0x0816, 0x0819), // Samaritan
    (0x081B, 0x0823),
    (0x0825, 0x0827),
    (0x0829, 0x082D),
    (0x0859, 0x085B), // Mandaic
    (0x08D3, 0x08E1), // Arabic Extended-A marks
    (0x08E3, 0x08FF),
    (0x20D0, 0x20F0), // Combining Diacritical Marks for Symbols
    (0xFB1E, 0xFB1E), // HEBREW POINT JUDEO-SPANISH VARIKA
    (0xFE00, 0xFE0F), // variation selectors
    (0xFE20, 0xFE2F), // Combining Half Marks
];

fn in_ranges(code: u32, ranges: &[(u32, u32)]) -> bool {
    ranges.iter().any(|&(lo, hi)| code >= lo && code <= hi)
}

/// The class of one character. Order matters: a mark and a digit both live
/// inside a right-to-left block, so the narrow tables answer first.
fn class_of(ch: char) -> Class {
    let code = ch as u32;
    if in_ranges(code, MARK) {
        return Class::Mark;
    }
    if in_ranges(code, ARABIC_NUMBER) {
        return Class::ArabicNumber;
    }
    if in_ranges(code, EUROPEAN_NUMBER) {
        return Class::EuropeanNumber;
    }
    if in_ranges(code, RIGHT_TO_LEFT) {
        return Class::Right;
    }
    // Everything alphabetic that is not right-to-left is strong left-to-right:
    // Latin, Greek, Cyrillic, the Indic scripts, CJK. Everything else --
    // spaces, punctuation, symbols, currency -- is a neutral, whose direction
    // its neighbours decide.
    if ch.is_alphabetic() {
        Class::Left
    } else {
        Class::Neutral
    }
}

/// Group the line into base-plus-marks clusters. A mark with no base before it
/// -- a line that opens on one, which only a broken font produces -- is a
/// cluster of its own and counts as a neutral.
fn into_clusters(chars: &[char]) -> Vec<Cluster> {
    let mut out: Vec<Cluster> = Vec::with_capacity(chars.len());
    for (index, &ch) in chars.iter().enumerate() {
        let class = class_of(ch);
        if class == Class::Mark {
            if let Some(last) = out.last_mut() {
                last.end = index + 1;
                continue;
            }
            out.push(Cluster {
                start: index,
                end: index + 1,
                class: Class::Neutral,
            });
            continue;
        }
        out.push(Cluster {
            start: index,
            end: index + 1,
            class,
        });
    }
    out
}

/// P2/P3, by counting rather than by reading the first strong character --
/// which is the one thing the caller does not have. See the module docs.
fn base_is_right_to_left(clusters: &[Cluster]) -> bool {
    let mut right = 0usize;
    let mut left = 0usize;
    for cluster in clusters {
        match cluster.class {
            Class::Right => right += 1,
            Class::Left => left += 1,
            _ => {}
        }
    }
    right > left
}

/// The embedding level of every cluster: W2/W7 for the numbers, N1/N2 for the
/// neutrals, I1/I2 for the level itself.
fn resolve_levels(clusters: &[Cluster], base_is_rtl: bool) -> Vec<u8> {
    // W7 (and W2): a European digit belongs to the last strong character
    // before it. After a left-to-right one it *is* left-to-right; after a
    // right-to-left one it stays a number and is laid out inside the
    // right-to-left run.
    let mut last_strong = if base_is_rtl {
        Direction::Right
    } else {
        Direction::Left
    };
    let mut resolved: Vec<Option<Direction>> = Vec::with_capacity(clusters.len());
    let mut is_number: Vec<bool> = Vec::with_capacity(clusters.len());
    for cluster in clusters {
        match cluster.class {
            Class::Left => {
                last_strong = Direction::Left;
                resolved.push(Some(Direction::Left));
                is_number.push(false);
            }
            Class::Right => {
                last_strong = Direction::Right;
                resolved.push(Some(Direction::Right));
                is_number.push(false);
            }
            Class::EuropeanNumber => {
                let left_context = last_strong == Direction::Left;
                resolved.push(Some(Direction::Right));
                is_number.push(!left_context);
            }
            Class::ArabicNumber => {
                resolved.push(Some(Direction::Right));
                is_number.push(true);
            }
            Class::Mark | Class::Neutral => {
                resolved.push(None);
                is_number.push(false);
            }
        }
    }
    // A European digit that W7 handed to a left-to-right context is a
    // left-to-right character from here on.
    for (index, cluster) in clusters.iter().enumerate() {
        if cluster.class == Class::EuropeanNumber && !is_number[index] {
            resolved[index] = Some(Direction::Left);
        }
    }

    // N1/N2: a run of neutrals between two characters of the same direction
    // takes that direction; otherwise it takes the line's own. The text before
    // the first character and after the last is the line's own direction.
    let boundary = if base_is_rtl {
        Direction::Right
    } else {
        Direction::Left
    };
    let mut index = 0usize;
    while index < resolved.len() {
        if resolved[index].is_some() {
            index += 1;
            continue;
        }
        let start = index;
        while index < resolved.len() && resolved[index].is_none() {
            index += 1;
        }
        let before = resolved[..start]
            .iter()
            .rev()
            .find_map(|d| *d)
            .unwrap_or(boundary);
        let after = resolved[index..]
            .iter()
            .find_map(|d| *d)
            .unwrap_or(boundary);
        let side = if before == after { before } else { boundary };
        for slot in &mut resolved[start..index] {
            *slot = Some(side);
        }
    }

    // I1/I2. An even level reads left to right, an odd one right to left; a
    // number inside a run of either sits two levels up, so that it reads left
    // to right whichever way the run around it goes.
    let base: u8 = u8::from(base_is_rtl);
    resolved
        .iter()
        .enumerate()
        .map(|(slot, direction)| {
            if is_number[slot] {
                return 2;
            }
            match direction.unwrap_or(boundary) {
                Direction::Right => 1,
                Direction::Left => {
                    if base == 0 {
                        0
                    } else {
                        2
                    }
                }
            }
        })
        .collect()
}

/// UAX #9 L2, applied to clusters: from the highest level down to the lowest
/// odd one, reverse every run of clusters at that level or above.
fn reorder(chars: &[char], clusters: &[Cluster], levels: &[u8]) -> String {
    let mut order: Vec<usize> = (0..clusters.len()).collect();
    let highest = levels.iter().copied().max().unwrap_or(0);
    let lowest_odd = levels.iter().copied().filter(|l| l % 2 == 1).min();
    if let Some(lowest_odd) = lowest_odd {
        for level in (lowest_odd..=highest).rev() {
            let mut index = 0usize;
            while index < order.len() {
                if levels[order[index]] < level {
                    index += 1;
                    continue;
                }
                let start = index;
                while index < order.len() && levels[order[index]] >= level {
                    index += 1;
                }
                order[start..index].reverse();
            }
        }
    }
    let mut out = String::with_capacity(chars.len());
    for &slot in &order {
        let cluster = clusters[slot];
        // L4, undone. The writer mirrored the neutral characters that sat in a
        // right-to-left run before it drew them, so what is on the page at the
        // place of a logical `(` is the glyph `)`. Mirroring an odd-level
        // neutral back is the last step from what was drawn to what was meant:
        // BidiSample.pdf's `(يناير)` came out `)يناير(` without it.
        let base = chars[cluster.start];
        if cluster.class == Class::Neutral && levels[slot] % 2 == 1 {
            out.push(mirrored(base).unwrap_or(base));
        } else {
            out.push(base);
        }
        out.extend(&chars[cluster.start + 1..cluster.end]);
    }
    out
}

/// The `Bidi_Mirrored` characters a document actually uses, and what each one
/// pairs with. Not the whole property, which runs to several hundred
/// mathematical symbols: a bracket that is not in this table comes back as it
/// was drawn, which is what happened to every one of them before.
const MIRRORED: &[(char, char)] = &[
    ('(', ')'),
    ('[', ']'),
    ('{', '}'),
    ('<', '>'),
    ('\u{ab}', '\u{bb}'),     // « »
    ('\u{2039}', '\u{203a}'), // ‹ ›
    ('\u{2045}', '\u{2046}'), // ⁅ ⁆
    ('\u{207d}', '\u{207e}'), // ⁽ ⁾
    ('\u{208d}', '\u{208e}'), // ₍ ₎
    ('\u{2308}', '\u{2309}'), // ⌈ ⌉
    ('\u{230a}', '\u{230b}'), // ⌊ ⌋
    ('\u{2329}', '\u{232a}'), // 〈 〉
    ('\u{27e6}', '\u{27e7}'), // ⟦ ⟧
    ('\u{27e8}', '\u{27e9}'), // ⟨ ⟩
    ('\u{3008}', '\u{3009}'),
    ('\u{300a}', '\u{300b}'),
    ('\u{300c}', '\u{300d}'),
    ('\u{300e}', '\u{300f}'),
    ('\u{3010}', '\u{3011}'),
    ('\u{3014}', '\u{3015}'),
    ('\u{3016}', '\u{3017}'),
    ('\u{3018}', '\u{3019}'),
    ('\u{301a}', '\u{301b}'),
    ('\u{ff08}', '\u{ff09}'),
    ('\u{ff1c}', '\u{ff1e}'),
    ('\u{ff3b}', '\u{ff3d}'),
    ('\u{ff5b}', '\u{ff5d}'),
    ('\u{ff5f}', '\u{ff60}'),
    ('\u{ff62}', '\u{ff63}'),
];

fn mirrored(ch: char) -> Option<char> {
    MIRRORED.iter().find_map(|&(open, close)| {
        if ch == open {
            Some(close)
        } else if ch == close {
            Some(open)
        } else {
            None
        }
    })
}

/// Whether every character of `text` is a combining mark, which is to say the
/// run drew ink on the glyph before it and moved the pen nowhere.
///
/// The extractor's own geometry asks this: a glyph that declares no advance has
/// its width clamped up to a quarter of the font size before anything sees it
/// (`glyph_width_and_source`), so the width cannot answer and the character
/// has to.
pub(crate) fn is_only_marks(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|ch| class_of(ch) == Class::Mark)
}

/// One assembled line of extracted text, as drawn, returned in logical order.
///
/// A line with no strong right-to-left character and no Arabic number is
/// returned untouched and without allocating -- which is every line of the
/// corpus that is not this ticket's.
pub(crate) fn logical_order(visual: &str) -> Cow<'_, str> {
    if !visual
        .chars()
        .any(|ch| matches!(class_of(ch), Class::Right | Class::ArabicNumber))
    {
        return Cow::Borrowed(visual);
    }
    let chars: Vec<char> = visual.chars().collect();
    let clusters = into_clusters(&chars);
    let base_is_rtl = base_is_right_to_left(&clusters);
    let levels = resolve_levels(&clusters, base_is_rtl);
    Cow::Owned(reorder(&chars, &clusters, &levels))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `العربية`, and the same word as the page draws it: right to left, so a
    /// left-to-right scan of the glyphs hands back the letters backwards.
    const ARABIC_LOGICAL: &str = "\u{627}\u{644}\u{639}\u{631}\u{628}\u{64a}\u{629}";
    const ARABIC_VISUAL: &str = "\u{629}\u{64a}\u{628}\u{631}\u{639}\u{644}\u{627}";

    #[test]
    fn a_latin_line_is_returned_untouched_and_unallocated() {
        let line = "The quick brown fox, 1970-01-01.";
        assert!(matches!(logical_order(line), Cow::Borrowed(_)));
        assert_eq!(logical_order(line), line);
    }

    #[test]
    fn a_line_of_arabic_reads_right_to_left() {
        assert_eq!(logical_order(ARABIC_VISUAL), ARABIC_LOGICAL);
    }

    #[test]
    fn an_arabic_run_inside_a_latin_line_reverses_alone() {
        let visual = format!("Hello {ARABIC_VISUAL} World.");
        let logical = format!("Hello {ARABIC_LOGICAL} World.");
        assert_eq!(logical_order(&visual), logical);
    }

    /// The line the other way round: mostly Arabic with an embedded Latin word,
    /// which the writer lays out left to right inside a right-to-left line.
    #[test]
    fn a_latin_run_inside_an_arabic_line_keeps_its_own_order() {
        let visual = format!("{ARABIC_VISUAL} PDF {ARABIC_VISUAL}");
        let logical = format!("{ARABIC_LOGICAL} PDF {ARABIC_LOGICAL}");
        assert_eq!(logical_order(&visual), logical);
    }

    /// Arabic-Indic digits are laid out left to right inside a right-to-left
    /// line, so the run around them turns and they do not: `١٤ إبريل ١٤٣٤`
    /// (14 April 1434), which is BidiSample.pdf's date line.
    #[test]
    fn arabic_indic_digits_stay_in_their_own_order() {
        let month = "\u{625}\u{628}\u{631}\u{64a}\u{644}"; // إبريل
        let month_visual: String = month.chars().rev().collect();
        let visual = format!("\u{661}\u{664}\u{663}\u{664} {month_visual} \u{661}\u{664}");
        let logical = format!("\u{661}\u{664} {month} \u{661}\u{664}\u{663}\u{664}");
        assert_eq!(logical_order(&visual), logical);
    }

    /// European digits inside a right-to-left line behave the same way.
    #[test]
    fn european_digits_inside_a_right_to_left_line_stay_in_their_own_order() {
        let visual = format!("2026 {ARABIC_VISUAL}");
        let logical = format!("{ARABIC_LOGICAL} 2026");
        assert_eq!(logical_order(&visual), logical);
    }

    /// And in a left-to-right line they are left alone entirely.
    #[test]
    fn european_digits_in_a_latin_line_are_untouched() {
        let visual = format!("Chapter 12 {ARABIC_VISUAL} end");
        let logical = format!("Chapter 12 {ARABIC_LOGICAL} end");
        assert_eq!(logical_order(&visual), logical);
    }

    /// A Hebrew point follows the letter it belongs to in memory and is drawn
    /// on top of it, so it must still follow that letter after the run turns
    /// around. `שָׁלוֹם` -- three of its letters carry points.
    #[test]
    fn hebrew_points_stay_behind_their_letter() {
        let logical = "\u{5e9}\u{5b8}\u{5c1}\u{5dc}\u{5d5}\u{5b9}\u{5dd}";
        // As drawn: the letters right to left, each still followed by its own
        // points, which have no advance of their own.
        let visual = "\u{5dd}\u{5d5}\u{5b9}\u{5dc}\u{5e9}\u{5b8}\u{5c1}";
        assert_eq!(logical_order(visual), logical);
    }

    /// An Arabic diacritic is the same case: `مُحَمَّد` keeps its harakat.
    #[test]
    fn arabic_diacritics_stay_behind_their_letter() {
        let logical = "\u{645}\u{64f}\u{62d}\u{64e}\u{645}\u{64e}\u{651}\u{62f}";
        let visual = "\u{62f}\u{645}\u{64e}\u{651}\u{62d}\u{64e}\u{645}\u{64f}";
        assert_eq!(logical_order(visual), logical);
    }

    /// A mark with nothing before it is a broken string, not a crash.
    #[test]
    fn a_leading_mark_has_no_base_and_is_left_where_it_is() {
        let visual = format!("\u{5b8}{ARABIC_VISUAL}");
        assert_eq!(
            logical_order(&visual).chars().count(),
            visual.chars().count()
        );
    }

    #[test]
    fn an_empty_line_is_empty() {
        assert_eq!(logical_order(""), "");
    }

    /// Every reordering is a permutation: the characters that went in are the
    /// characters that come out. This is what keeps the change invisible to the
    /// character-recall axis and is the reason the fixture tests assert on the
    /// string rather than on a percentage.
    #[test]
    fn reordering_moves_characters_and_never_loses_them() {
        for line in [
            ARABIC_VISUAL,
            "Hello \u{62f}\u{645}\u{62d}\u{645} World.",
            "\u{661}\u{664}\u{663}\u{664}-\u{644}\u{64a}\u{631}\u{628}\u{625}-\u{661}\u{664}",
            "\u{5dd}\u{5d5}\u{5b9}\u{5dc}\u{5e9}",
        ] {
            let mut before: Vec<char> = line.chars().collect();
            let mut after: Vec<char> = logical_order(line).chars().collect();
            before.sort_unstable();
            after.sort_unstable();
            assert_eq!(
                before, after,
                "reordering changed the characters of {line:?}"
            );
        }
    }

    /// The guillemets of BidiSample.pdf's `«إختبار ذاتي»` line and the
    /// parentheses of its `(يناير)` one. The writer mirrored them on the way
    /// to the page; reading them back has to mirror them again, or the line
    /// opens on a closing bracket.
    #[test]
    fn a_bracket_the_writer_mirrored_is_mirrored_back() {
        let visual = format!("\u{ab}{ARABIC_VISUAL}\u{bb}");
        assert_eq!(
            logical_order(&visual),
            format!("\u{ab}{ARABIC_LOGICAL}\u{bb}")
        );
        let visual = format!("({ARABIC_VISUAL})");
        assert_eq!(logical_order(&visual), format!("({ARABIC_LOGICAL})"));
    }

    /// And a bracket in a left-to-right line is never touched: it sits at an
    /// even level, which is the level the writer left it alone at.
    #[test]
    fn a_bracket_in_a_left_to_right_line_is_left_alone() {
        let visual = format!("Note (see {ARABIC_VISUAL}) below");
        let logical = format!("Note (see {ARABIC_LOGICAL}) below");
        assert_eq!(logical_order(&visual), logical);
    }
}
