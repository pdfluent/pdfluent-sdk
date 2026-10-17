//! Which lines a document opens as list items, and how deep they nest (#569).
//!
//! A list is the one structure on a page that is written down twice. The
//! typesetter says it with a MARKER -- a bullet glyph, a number, a letter --
//! and says it again with a HANGING INDENT, the item's own continuation lines
//! set in from the marker. Neither half is enough on its own: prose starts a
//! line with a dash, a paragraph is indented, and a section heading is
//! numbered. Both halves together, over a RUN of items that agree with each
//! other, is what this file looks for.
//!
//! THE RUN IS THE EVIDENCE, and it is why this is not a regular expression.
//! One line opening `1.` is a sentence that begins with a date, a clause
//! reference or a footnote; three lines opening `1.`, `2.`, `3.` at the same
//! left edge are a list, and nothing else in typography produces that picture.
//! So a marker is a CANDIDATE, and a candidate becomes an item only when a
//! second candidate in the same column agrees with it: same kind of marker,
//! same left edge, and -- for a numbered one -- a value that climbs. That
//! single rule is what keeps dates, section numbers and clause references out,
//! and it is also why a one-item list is not detected. A list of one is a
//! paragraph with a bullet in front of it, and calling it a list on the
//! strength of the bullet alone would call every em dash in a novel one too.
//!
//! WHAT THE TYPE HAS TO SAY. A numbered heading -- `3. Method` over a page of
//! prose, `4. Results` over the next -- is a run of agreeing markers at one
//! left edge, and it is not a list. What separates it is the type: a list item
//! is set in the body of its column, and a numbered heading is set above it.
//! So a candidate whose line is larger or bolder than the column's own body is
//! refused here, and `crate::headings` gets it as the heading it is. The two
//! files therefore never argue over the same block: a block this one claims is
//! no longer a paragraph, and the classifier next door only ever promotes
//! paragraphs.
//!
//! WHAT IT REFUSES
//!
//! * A BULLET DRAWN AS A VECTOR PATH. A square drawn with `re f` is not a
//!   glyph and never reaches the text device, so the item opens with its own
//!   first word and there is nothing to find. The hanging indent is still
//!   there and still merges the item's lines correctly (#567) -- what is lost
//!   is the kind, not the text.
//! * A LINE IN A TABLE. A cell that holds a bulleted line is a cell; the page
//!   said "row and column" with a rule or a gutter, and that is a stronger
//!   statement than the bullet's. `crate::blocks` has already marked those
//!   lines tabular, and a tabular line is never a candidate.
//! * A NUMBER THAT IS NOT A MARKER. `2.1 Scope` and `1.5 million` carry a
//!   digit and a full stop and are neither; the delimiter has to be followed
//!   by white space before anything here looks twice.
//!
//! WHAT THE PAGE SAYS WHERE THE CHARACTERS DO NOT (#578): a bibliography's
//! `[3]`, a raised figure with no delimiter, a bullet drawn as a letter in a
//! font whose name says nothing. [`Lead`] carries what the caller measured off
//! the spans, and [`marker_at`] is where each of the three is decided.
//!
//! WHERE THE LEVEL COMES FROM. The indent steps of the column, ranked: the
//! items at the leftmost edge are level 0, the next edge in is level 1, and so
//! on. Not from a fixed measurement, for the same reason `crate::headings`
//! takes no fixed point size -- an indent step is two ems in one document and
//! half an inch in another, and only the document knows which of its own edges
//! are steps.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/// The most digits a number may have and still be a list marker.
///
/// Three, so `1.` through `999.` are markers and a year is not: `1984. The
/// committee met` opens a sentence with a date, and a list that has run past
/// its thousandth item is not a list anybody is reading as one.
const MAX_ORDINAL_DIGITS: usize = 3;

/// The most letters a roman numeral may have. `viii` is four, `xviii` is five,
/// and past that the numeral is longer than the word it would number.
const MAX_ROMAN_LETTERS: usize = 6;

/// The most parts an outline's number may have: `II.4.1` is three. Past that
/// the number no longer fits one `u32` read three digits a part, and a deeper
/// clause than that is rarer than a table of version numbers.
const MAX_OUTLINE_DEPTH: usize = 3;

/// Where a contents entry runs out to its page number: this many full stops
/// in a row. Four, because an ellipsis is three and a clause may well quote
/// one.
const DOT_LEADER: usize = 4;

/// The deepest nesting level this reports. Deeper edges fold into it, the way
/// heading clusters below the third fold into level 3: past four steps in, no
/// consumer of the model -- markdown, a chunker, a retrieval index -- treats
/// the depths as different.
pub(crate) const MAX_LIST_LEVEL: u8 = 4;

/// PostScript names whose glyphs are pictures rather than letters.
///
/// A bullet set in one of these is drawn with whatever code the font maps to
/// the picture -- `l` is a filled circle in ZapfDingbats, `n` a filled square
/// in Wingdings -- so the character extraction hands back says nothing, and
/// the font is the only thing that does. Matched on the stripped PostScript
/// name `TextSpan::font_name` already carries, case-insensitively and by
/// prefix, because the name arrives as `ZapfDingbats`, `Wingdings-Regular` and
/// `SymbolMT` in three files that mean the same font.
const SYMBOL_FONTS: [&str; 5] = [
    "zapfdingbats",
    "dingbats",
    "wingdings",
    "webdings",
    "symbol",
];

/// Characters a document uses as a bullet.
///
/// Every one of them is a mark with no reading: none can open a word, so a
/// line that starts with one and goes on in letters has put a marker there on
/// purpose. The hyphen-minus, the en dash and the em dash are the exception --
/// they are punctuation as well -- and they are admitted because a dash IS the
/// bullet of most plain-text and many typeset lists, with the run rule left to
/// throw out the line of dialogue that opens with one.
pub(crate) const BULLET_CHARS: [char; 20] = [
    '\u{2022}', // bullet
    '\u{2023}', // triangular bullet
    '\u{2043}', // hyphen bullet
    '\u{2219}', // bullet operator
    '\u{00B7}', // middle dot
    '\u{25AA}', // black small square
    '\u{25AB}', // white small square
    '\u{25A0}', // black square
    '\u{25A1}', // white square
    '\u{25CF}', // black circle
    '\u{25CB}', // white circle
    '\u{25E6}', // white bullet
    '\u{25C6}', // black diamond
    '\u{25C7}', // white diamond
    '\u{2756}', // black diamond minus white X
    '\u{2713}', // check mark
    '\u{2714}', // heavy check mark
    '-',        // hyphen-minus
    '\u{2013}', // en dash
    '\u{2014}', // em dash
];

/// What a document numbered an item with, before the run says how to read it.
///
/// `Letter` is deliberately unresolved. `i` is the ninth letter and the first
/// roman numeral, and one marker cannot tell which -- only the run can, and
/// [`is_roman`] is where it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ordinal {
    /// `7.`
    Arabic(u32),
    /// `g.` -- the ninth letter or the first numeral, not yet decided.
    Letter(char),
    /// `viii.` -- two or more roman letters, which no alphabetic list reaches
    /// in one marker, so this one is decided.
    Roman(u32),
    /// `II.4.1`, `10.2` -- a clause numbered by its place in an outline.
    ///
    /// `value` is the numbers read as one: each part below the head is three
    /// digits at most, so `II.4.1` is `2_004_001` and the order of the values
    /// is the order of the clauses. `depth` counts the parts, and `roman`
    /// says how the head is written: `II.4.1` and `2.4.1` are two schemes.
    Outline { value: u32, depth: u8, roman: bool },
    /// `§ 70`, `§ 74a` -- a statute's section, which its contents page lists
    /// one per line (#1973). The letter of `74a` sits between `74` and `75`,
    /// so the value is the number times 27 plus the letter's place.
    Section(u32),
}

/// A marker at the head of a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Marker {
    kind: MarkerKind,
    /// Length in bytes of the marker and the white space after it, measured
    /// from the start of the untrimmed line.
    pub(crate) len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MarkerKind {
    Bullet,
    Ordered(Ordinal),
    /// A glyph the page set apart as a marker, in a font that admits nothing.
    ///
    /// Carries the character, because that is the only thing holding this
    /// together: two of these are one list when they are the SAME glyph, and
    /// two different letters at one edge are a numbering scheme this file does
    /// not read rather than a bulleted list (#578).
    Glyph(char),
}

impl Marker {
    /// Whether two markers could be items of one list: both bullets, or both
    /// numbered in a way that could be the same sequence.
    ///
    /// An arabic marker and a lettered one are never one list; a lettered one
    /// and a roman one may be, because the lettered one may BE roman.
    fn agrees_with(&self, other: &Marker) -> bool {
        match (self.kind, other.kind) {
            (MarkerKind::Bullet, MarkerKind::Bullet) => true,
            (MarkerKind::Glyph(a), MarkerKind::Glyph(b)) => a == b,
            (MarkerKind::Ordered(a), MarkerKind::Ordered(b)) => {
                matches!(
                    (a, b),
                    (Ordinal::Arabic(_), Ordinal::Arabic(_))
                        | (
                            Ordinal::Letter(_) | Ordinal::Roman(_),
                            Ordinal::Letter(_) | Ordinal::Roman(_)
                        )
                ) || matches!(
                    (a, b),
                    (
                        Ordinal::Outline { depth: d, roman: r, .. },
                        Ordinal::Outline { depth: e, roman: s, .. },
                    ) if d == e && r == s
                ) || matches!((a, b), (Ordinal::Section(_), Ordinal::Section(_)))
            }
            _ => false,
        }
    }

    /// The number this marker carries once the run has said how to read a bare
    /// letter, or `None` for a bullet.
    pub(crate) fn value(&self, roman: bool) -> Option<u32> {
        match self.kind {
            MarkerKind::Bullet | MarkerKind::Glyph(_) => None,
            MarkerKind::Ordered(
                Ordinal::Arabic(n)
                | Ordinal::Roman(n)
                | Ordinal::Outline { value: n, .. }
                | Ordinal::Section(n),
            ) => Some(n),
            // `None` where the run reads roman and this letter is not a roman
            // digit: `a.` beside `iv.` is not the sequence `iv.` belongs to,
            // and a letter forced to a number there would let the two agree.
            MarkerKind::Ordered(Ordinal::Letter(c)) => {
                if roman {
                    roman_value(&c.to_string())
                } else {
                    Some(letter_value(c))
                }
            }
        }
    }

    pub(crate) fn is_ordered(&self) -> bool {
        matches!(self.kind, MarkerKind::Ordered(_))
    }

    /// Whether the marker is an outline's `II.4.1`: no one number says it, and
    /// an outline's short numbered lines over their body are its sub-heads.
    pub(crate) fn is_outline(&self) -> bool {
        matches!(self.kind, MarkerKind::Ordered(Ordinal::Outline { .. }))
    }

    /// Whether the item keeps its marker in its text: an outline's `II.4.1` and
    /// a statute's `§ 70`, which are the clause's name as much as its marker.
    /// Kept apart from [`Self::is_outline`], which also decides which numbered
    /// lines are an outline's sub-heads -- a question a contents page's short
    /// last entry must not be asked (codex, #2000).
    pub(crate) fn keeps_its_text(&self) -> bool {
        matches!(
            self.kind,
            MarkerKind::Ordered(Ordinal::Outline { .. } | Ordinal::Section(_))
        )
    }

    /// Whether this marker settles the question a bare letter leaves open.
    fn is_certain_roman(&self) -> bool {
        matches!(self.kind, MarkerKind::Ordered(Ordinal::Roman(_)))
    }
}

/// `a` is 1, `z` is 26; case is the typesetter's business, not the sequence's.
fn letter_value(c: char) -> u32 {
    let lower = c.to_ascii_lowercase();
    (lower as u32).saturating_sub('a' as u32) + 1
}

/// Whether what follows a pointer or a picture opens an item: a capital for a
/// pointer, white space before it or not -- `> 3 of the cases` is a quotation
/// and `>=` an operator -- and white space or a capital for a private-use
/// picture, whose bullet is set before lower-case items as often as not.
pub(crate) fn opens_like_an_item(marker: char, rest: &str) -> bool {
    let word = rest.trim_start().chars().next();
    if is_private_use(marker) {
        rest.starts_with(char::is_whitespace) || word.is_some_and(char::is_uppercase)
    } else {
        word.is_some_and(char::is_uppercase)
    }
}

/// A character in any of Unicode's three private use areas: the one in the
/// basic plane and the two supplementary planes, which a bullet font's
/// mapping reaches as readily (codex, #2002).
fn is_private_use(c: char) -> bool {
    matches!(c, '\u{e000}'..='\u{f8ff}' | '\u{f0000}'..='\u{ffffd}' | '\u{100000}'..='\u{10fffd}')
}

/// `>`, `›`, `»`, or a character in the private use area: what a page draws
/// its bullets with when it has no bullet, or when its bullet font maps the
/// picture to a code with no meaning of its own.
pub(crate) fn is_a_pointer_or_a_picture(c: char) -> bool {
    matches!(c, '>' | '\u{203a}' | '\u{bb}') || is_private_use(c)
}

/// A roman numeral's value, or `None` when the letters are not one.
///
/// Subtractive pairs included, because `iv.` and `ix.` are what a document
/// writes. Nothing here validates the numeral beyond its arithmetic: `iiii` is
/// read as four, which is what a typesetter who wrote it meant.
pub(crate) fn roman_value(text: &str) -> Option<u32> {
    let digit = |c: char| -> Option<i64> {
        match c.to_ascii_lowercase() {
            'i' => Some(1),
            'v' => Some(5),
            'x' => Some(10),
            'l' => Some(50),
            'c' => Some(100),
            'd' => Some(500),
            'm' => Some(1000),
            _ => None,
        }
    };
    let values: Vec<i64> = text.chars().map(digit).collect::<Option<Vec<i64>>>()?;
    if values.is_empty() {
        return None;
    }
    // Signed while it runs: `iv` subtracts before it adds, and a total that
    // clamped at zero on the way would come back as five.
    let mut total: i64 = 0;
    for (i, value) in values.iter().enumerate() {
        match values[i + 1..].iter().max() {
            Some(later) if later > value => total -= value,
            _ => total += value,
        }
    }
    u32::try_from(total).ok()
}

/// Whether a font draws pictures rather than letters.
pub(crate) fn is_symbol_font(name: Option<&str>) -> bool {
    let Some(name) = name else { return false };
    let lower = name.to_ascii_lowercase();
    SYMBOL_FONTS.iter().any(|family| lower.contains(family))
}

/// What the page said about the head of a line, where the characters do not
/// say it themselves.
///
/// Every field is a count of BYTES at the front of the line, measured by the
/// caller because only the caller still has the spans the line was assembled
/// from. `None` is "the page says nothing of this kind", which is what an
/// ordinary line of prose hands over.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Lead {
    /// Bytes drawn in a picture font while the rest of the line was not.
    pub(crate) symbol: Option<usize>,
    /// Bytes drawn smaller than the line and above its baseline: the raised
    /// figure a reference list is numbered with (#578).
    pub(crate) raised: Option<usize>,
    /// Bytes of a single leading glyph the page set apart with a gap no word
    /// space could be (#578).
    pub(crate) apart: Option<usize>,
}

/// A sign that belongs to a figure: a currency, a per cent, a digit. Set apart
/// at the head of a line it is the first cell of a column of money, and it
/// repeats down one edge exactly the way a glyph bullet does (#589).
fn is_a_figure_sign(c: char) -> bool {
    c.is_ascii_digit()
        || matches!(
            c,
            '$' | '\u{20ac}' | '\u{a3}' | '\u{a5}' | '\u{a2}' | '%' | '\u{2030}' | '#' | '+' | '='
        )
}

/// The marker a line opens with, if it opens with one.
///
/// `lead` is what the page said and the characters did not: a picture font, a
/// raised figure, a glyph set apart. Each of them is the one case where the
/// character is not the evidence, and each is measured by the caller.
///
/// Requires text after the marker, always. A line that is nothing but a bullet
/// is a glyph on its own, and an item with no text is not an item.
pub(crate) fn marker_at(text: &str, lead: Lead) -> Option<Marker> {
    let indent = text.len() - text.trim_start().len();
    let trimmed = &text[indent..];
    let first = trimmed.chars().next()?;

    // The picture font first: whatever the code decoded to, a glyph or two of
    // a dingbat face in front of a line of text is a bullet.
    if let Some(prefix) = lead.symbol {
        if prefix > indent && text.is_char_boundary(prefix) {
            return finish(text, prefix, MarkerKind::Bullet, false);
        }
    }

    if BULLET_CHARS.contains(&first) {
        // A dash needs the space after it. `well-known` opens no list, and
        // neither does a word the typesetter broke over a line and a reader
        // put back together.
        let needs_space = matches!(first, '-' | '\u{2013}' | '\u{2014}');
        return finish(
            text,
            indent + first.len_utf8(),
            MarkerKind::Bullet,
            needs_space,
        );
    }

    // A POINTER OR A PICTURE THE FONT HAD NO LETTER FOR (#1973). `>` and `›`
    // set as the bullets of an annual report (`>Our market share in retail
    // securities`, NYSE_MS on DocLayNet validation: 175 items, 8 per cent of
    // them read), and a Symbol or Wingdings bullet that decodes into the
    // private use area (`\u{f0b7}`). Neither is a bullet in running text, so
    // the next character has to be white space or a capital -- `x > y`,
    // `->` and `>=` are not -- and the run rule still wants a second one at
    // the same edge.
    if is_a_pointer_or_a_picture(first) && opens_like_an_item(first, &trimmed[first.len_utf8()..]) {
        {
            return finish(text, indent + first.len_utf8(), MarkerKind::Bullet, false);
        }
    }

    if let Some(marker) = ordinal_at(text, indent, trimmed, first) {
        return Some(marker);
    }
    if let Some(marker) = outline_at(text, indent, trimmed) {
        return Some(marker);
    }
    if let Some(marker) = section_at(text, indent, trimmed) {
        return Some(marker);
    }

    // A RAISED FIGURE AND NO DELIMITER. `12 E. B. Sonin, arXiv:...` is a
    // reference and `1984 was a year` is a sentence, and nothing in the
    // characters tells them apart -- so the type does: the figure is set
    // smaller than the line it opens and above its baseline, which is what the
    // caller measured. The run rule still has to find a second one climbing at
    // the same edge (#578).
    if let Some(prefix) = lead.raised {
        if prefix > indent && text.is_char_boundary(prefix) {
            let figure = text[indent..prefix].trim();
            if !figure.is_empty()
                && figure.len() <= MAX_ORDINAL_DIGITS
                && figure.chars().all(|c| c.is_ascii_digit())
            {
                if let Ok(value) = figure.parse() {
                    return finish(
                        text,
                        prefix,
                        MarkerKind::Ordered(Ordinal::Arabic(value)),
                        false,
                    );
                }
            }
        }
    }

    // A GLYPH THE PAGE SET APART. The last resort, and the narrowest: one
    // character, drawn in a run of its own with a gap after it, which
    // [`MarkerKind::Glyph`] then pairs only with the same character again.
    if let Some(prefix) = lead.apart {
        if prefix > indent && text.is_char_boundary(prefix) {
            // The SPACING the run carries is not the glyph. A bullet drawn in
            // a run of its own often takes its trailing space with it, and the
            // rule in `crate::blocks` that offered this candidate counts
            // characters rather than spaces -- so `"t "` arrives here as one
            // glyph by that count and as two by this one. The marker still
            // ends at `prefix`, because the space belongs to the marker and
            // not to the text the item says. (codex, #578)
            let mut glyph = text[indent..prefix].trim_end().chars();
            if let (Some(c), None) = (glyph.next(), glyph.next()) {
                if !c.is_whitespace() && !is_a_figure_sign(c) {
                    return finish(text, prefix, MarkerKind::Glyph(c), false);
                }
            }
        }
    }

    None
}

/// `1.`, `(a)`, `[3]`, `iv.`: a number or a letter, delimited.
///
/// The opening bracket is optional and decides its own closing one, which is
/// the whole of what admits a bibliography's `[3]` beside a contract's `(a)`
/// (#578).
fn ordinal_at(text: &str, indent: usize, trimmed: &str, first: char) -> Option<Marker> {
    let closing = match first {
        '(' => Some(')'),
        '[' => Some(']'),
        _ => None,
    };
    let body = if closing.is_some() {
        &trimmed[first.len_utf8()..]
    } else {
        trimmed
    };

    let word: String = body
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    let delimiter = body[word.len()..].chars().next()?;
    match closing {
        Some(closing) => {
            if delimiter != closing {
                return None;
            }
        }
        None => {
            if delimiter != '.' && delimiter != ')' {
                return None;
            }
        }
    }

    let ordinal = if word.chars().all(|c| c.is_ascii_digit()) {
        if word.is_empty() || word.len() > MAX_ORDINAL_DIGITS {
            return None;
        }
        Ordinal::Arabic(word.parse().ok()?)
    } else if word.chars().all(|c| c.is_ascii_alphabetic()) {
        match word.chars().count() {
            1 => Ordinal::Letter(word.chars().next()?),
            n if n <= MAX_ROMAN_LETTERS => Ordinal::Roman(roman_value(&word)?),
            _ => return None,
        }
    } else {
        return None;
    };

    // AN INITIAL BEFORE AN INITIAL IS A NAME. `A. G. Gavriliuk`, `I. S.
    // Lyubutin`, `V. V. Struzhkin`: an author list whose reference number was
    // set raised and read apart, so the line opens with the author's first
    // initial -- a capital and a full stop, which is also how `A.` and `IV.`
    // open a list. What tells them apart is the next word: an item's text does
    // not open with a second initial, and a name almost always does. Read as a
    // marker, the initial was cut off the name and the reference became an
    // item numbered by its author (DocLayNet validation, arXiv 1001.1327; #599).
    // One letter only: an initial is one letter, and `II. A. Smith` is a
    // second item whose text opens with a name (codex, #1970).
    if closing.is_none()
        && word.len() == 1
        && word.chars().all(|c| c.is_ascii_uppercase())
        && (opens_with_an_initial(&body[word.len() + delimiter.len_utf8()..])
            || opens_with_a_surname_then_an_initial(&body[word.len() + delimiter.len_utf8()..]))
    {
        return None;
    }

    // The opening bracket by its own width rather than by one: every bracket
    // this admits is one byte, and a length that assumes so is a trap for the
    // next one that is not.
    let opening = if closing.is_some() {
        first.len_utf8()
    } else {
        0
    };
    let len = indent + opening + word.len() + delimiter.len_utf8();
    finish(text, len, MarkerKind::Ordered(ordinal), true)
}

/// `II.4.1 The contractor`, `10.2 Director Equity Plan`: a clause numbered by
/// its place in an outline (#599).
///
/// A head -- a number, or a roman numeral in capitals -- and one or two parts
/// after it, each a number of at most three digits, joined by full stops, with
/// a full stop at the end or not. That alone is also how `1.5 million` and a
/// section's `2.1 The committee shall` wrapped onto a line of their own open,
/// so the text after it has to open with a capital: a sentence that wrapped
/// before a decimal carries on in lower case, and a clause starts afresh. A
/// numbered sub-head passes this too, and is told apart where the run is
/// read, by being a short line with its body under it.
fn outline_at(text: &str, indent: usize, trimmed: &str) -> Option<Marker> {
    let token = trimmed.split(char::is_whitespace).next()?;
    let body = token.strip_suffix('.').unwrap_or(token);
    let mut parts = body.split('.');
    let head = parts.next()?;
    let (head, roman) = if !head.is_empty() && head.chars().all(|c| c.is_ascii_digit()) {
        if head.len() > MAX_ORDINAL_DIGITS {
            return None;
        }
        (head.parse::<u32>().ok()?, false)
    } else if !head.is_empty()
        && head.len() <= MAX_ROMAN_LETTERS
        && head.chars().all(|c| c.is_ascii_uppercase())
    {
        (roman_value(head)?, true)
    } else {
        return None;
    };
    let rest: Vec<&str> = parts.collect();
    if rest.is_empty() || rest.len() > MAX_OUTLINE_DEPTH - 1 {
        return None;
    }
    let mut value = head;
    for part in &rest {
        if part.is_empty()
            || part.len() > MAX_ORDINAL_DIGITS
            || !part.chars().all(|c| c.is_ascii_digit())
        {
            return None;
        }
        value = value.checked_mul(1000)?.checked_add(part.parse().ok()?)?;
    }
    let after = trimmed[token.len()..].trim_start();
    if !after.chars().next().is_some_and(char::is_uppercase) {
        return None;
    }
    // A CONTENTS PAGE NUMBERS ITS ENTRIES THE SAME WAY. `I.4.1. Price of the
    // contract ........ 6` is the clause's title, a dot leader and the page it
    // is on, and a contents page is not a list: the EU contract whose clauses
    // this reads opens with one, and read as items its entries lost the
    // headings axis what the clauses gained (#599).
    if has_dot_leader(after) {
        return None;
    }
    let depth = u8::try_from(rest.len() + 1).ok()?;
    finish(
        text,
        indent + token.len(),
        MarkerKind::Ordered(Ordinal::Outline {
            value,
            depth,
            roman,
        }),
        true,
    )
}

/// `§ 70 Statthaftigkeit der Rechtsbeschwerde`: a statute's section, the way
/// its contents page lists them (#1973).
///
/// The sign, the number with a letter or not (`§ 74a`), white space, and text
/// that opens with a capital -- the title of the section, or its first
/// sentence. Like an outline's number it is the section's name as much as its
/// marker, so the item keeps it in its text. A line of prose that happens to
/// open `§ 5 Abs. 2` is one marker and no run: the run rule is what makes a
/// list of them.
fn section_at(text: &str, indent: usize, trimmed: &str) -> Option<Marker> {
    let body = trimmed.strip_prefix('\u{a7}')?;
    let number = body.trim_start();
    let digits = number.chars().take_while(char::is_ascii_digit).count();
    if !(1..=MAX_SECTION_DIGITS).contains(&digits) {
        return None;
    }
    let mut value = number[..digits].parse::<u32>().ok()?.checked_mul(27)?;
    let mut len = trimmed.len() - number.len() + digits;
    if let Some(letter) = number[digits..]
        .chars()
        .next()
        .filter(char::is_ascii_lowercase)
    {
        value += letter as u32 - 'a' as u32 + 1;
        len += 1;
    }
    let after = trimmed[len..].trim_start();
    if !after.chars().next().is_some_and(char::is_uppercase) {
        return None;
    }
    finish(
        text,
        indent + len,
        MarkerKind::Ordered(Ordinal::Section(value)),
        true,
    )
}

/// The most digits a statute's section number has: `§ 1717` is four.
const MAX_SECTION_DIGITS: usize = 4;

/// Whether `text` runs out to a page number the way a contents entry does:
/// [`DOT_LEADER`] full stops in a row, however the page spaced them (`. . . .`)
/// and whether it set them one by one or as ellipses (`……`) (codex, #1986).
fn has_dot_leader(text: &str) -> bool {
    let mut dots = 0;
    for c in text.chars() {
        match c {
            '.' => dots += 1,
            '\u{2026}' => dots += 3,
            c if c.is_whitespace() => {}
            _ => dots = 0,
        }
        if dots >= DOT_LEADER {
            return true;
        }
    }
    false
}

/// Whether `rest` -- what follows a marker's delimiter -- opens with an
/// initial: white space, one capital letter, and a full stop or a hyphen
/// (`G.`, `J.-P.`) before white space or the end. `U.S.` is not one: an item
/// may well open with an abbreviation, and only the bare initial is refused.
pub(crate) fn opens_with_an_initial(rest: &str) -> bool {
    let trimmed = rest.trim_start();
    if trimmed.len() == rest.len() {
        return false;
    }
    let mut chars = trimmed.chars();
    let (Some(letter), Some(stop)) = (chars.next(), chars.next()) else {
        return false;
    };
    letter.is_uppercase()
        && (stop == '.' || stop == '-')
        && chars.next().is_none_or(|c| c.is_whitespace() || c == '-')
}

/// Whether `rest` -- what follows a one-letter marker's full stop -- is the
/// rest of a name in an author list: a capitalised surname, then a comma,
/// `and` or `&`, then the next author's initial. `L. Lamata and E. Solano`,
/// `A. Cucchieri and T. Mendes`, `K. Makris, R. El-Ganainy` are the second
/// line of a reference whose authors ran over, and read as `L.` `A.` `K.`
/// they cut the reference in two and made its tail an item of its own (20
/// references on #632's proxy, arXiv reference lists; #574). An item that
/// opens with a word and `and` -- `A. Introduction and scope` -- has no
/// initial after it, and stays an item.
pub(crate) fn opens_with_a_surname_then_an_initial(rest: &str) -> bool {
    let trimmed = rest.trim_start();
    if trimmed.len() == rest.len() {
        return false;
    }
    let mut words = trimmed.split_whitespace();
    let Some(surname) = words.next() else {
        return false;
    };
    let (name, comma) = match surname.strip_suffix(',') {
        Some(name) => (name, true),
        None => (surname, false),
    };
    let capitalised = name.chars().next().is_some_and(char::is_uppercase)
        && name.chars().count() >= 2
        && name
            .chars()
            .all(|c| c.is_alphabetic() || c == '-' || c == '\'')
        && name.chars().skip(1).any(char::is_lowercase);
    if !capitalised {
        return false;
    }
    let next = if comma {
        words.next()
    } else {
        match words.next() {
            Some("and") | Some("&") => words.next(),
            _ => return false,
        }
    };
    next.is_some_and(|w| opens_with_an_initial(&format!(" {w}")))
}

/// The marker, once the text after it has been checked and the white space
/// between the two swallowed.
fn finish(text: &str, len: usize, kind: MarkerKind, needs_space: bool) -> Option<Marker> {
    let rest = &text[len..];
    let spaced = rest.starts_with(char::is_whitespace);
    if needs_space && !spaced {
        return None;
    }
    let gap = rest.len() - rest.trim_start().len();
    if rest[gap..].trim().is_empty() {
        return None;
    }
    Some(Marker {
        kind,
        len: len + gap,
    })
}

/// Whether a run of markers numbers itself in roman.
///
/// One certain numeral -- two letters or more -- settles it for every bare
/// letter beside it, which is how `i. ii. iii.` is told from `g. h. i.`. A run
/// of bare letters alone is alphabetic, which is the commoner list and the
/// reading that makes `a. b. c.` count from one.
pub(crate) fn is_roman(markers: &[Marker]) -> bool {
    markers.iter().any(Marker::is_certain_roman)
}

/// Whether a set of markers at one left edge is a list rather than a
/// coincidence: at least two of them, agreeing in kind, and climbing if they
/// are numbered.
///
/// Climbing, not counting by one: a document skips a number, and an item whose
/// marker the extraction lost leaves a gap. What it may not do is go back
/// down, which is what tells a list from the clause references of a contract --
/// `(a)` in one sentence and `(a)` again two paragraphs later.
pub(crate) fn is_a_run(markers: &[Marker]) -> bool {
    if markers.len() < 2 {
        return false;
    }
    let first = markers[0];
    if !markers.iter().all(|m| m.agrees_with(&first)) {
        return false;
    }
    if !first.is_ordered() {
        return true;
    }
    let roman = is_roman(markers);
    // A LETTERED LIST GOES A, B, C (#599). Numbers may skip -- a bibliography
    // continued from the column before, an item the page lost -- but letters
    // that jump are initials: the second and third lines of a reference,
    // `T. Fujiwara, E. Yamamoto, ...` and `Y. Onuki, J. Phys. ...`, read as
    // items 20 and 25 of a list and cut the reference into its lines. One
    // letter may be missing between two; more is not a list.
    let lettered = !roman
        && markers
            .iter()
            .all(|m| matches!(m.kind, MarkerKind::Ordered(Ordinal::Letter(_))));
    // The outline's own stepping rule, and not a section's: `§ 74a` after
    // `§ 74` climbs like any number.
    let outline = markers
        .iter()
        .all(|m| matches!(m.kind, MarkerKind::Ordered(Ordinal::Outline { .. })));
    let mut previous: Option<u32> = None;
    for marker in markers {
        // `Some` for every ordered marker, and the first one said they all
        // are; a bullet among them would already have failed `agrees_with`.
        let Some(value) = marker.value(roman) else {
            return false;
        };
        if previous.is_some_and(|last| value <= last || (lettered && value - last > 2)) {
            return false;
        }
        // AN OUTLINE STEPS TO THE NEXT CLAUSE OR OPENS THE NEXT SECTION (#599).
        // `II.4.7` is followed by `II.4.8` or by `II.6.1`, never by `II.6.4`:
        // a clause that opens a section is its first. Figures that happen to
        // open two lines -- `3.5 Million` and later `12.7 Percent` -- climb
        // too, and this is what they do not do.
        if outline && previous.is_some_and(|last| last / 1000 != value / 1000 && value % 1000 > 1) {
            return false;
        }
        previous = Some(value);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marker(text: &str) -> Option<Marker> {
        marker_at(text, Lead::default())
    }

    #[test]
    fn bullet_glyphs_open_an_item() {
        for text in ["\u{2022} apples", "\u{25AA} pears", "\u{25CF}quinces"] {
            assert!(marker(text).is_some(), "{text:?} opens no item");
        }
    }

    #[test]
    fn a_pointer_or_a_private_use_picture_is_a_bullet() {
        // NYSE_MS 2001 on DocLayNet validation sets its bullets as `>` glued to
        // the word, and a Symbol bullet decodes into the private use area
        // (#1973).
        for text in [
            ">Our market share in retail securities increased",
            "> Our market share",
            "\u{203a} Capital transactions",
            "\u{f0b7} the consolidated balance sheet",
            "\u{f0000} the supplementary private use area",
            "\u{100000} and the other one",
        ] {
            let m = marker(text).unwrap_or_else(|| panic!("{text:?} opens no item"));
            assert!(!m.is_ordered());
        }
        // Not in running text or code.
        for text in ["> 3 of the cases", ">= the threshold", "->Our"] {
            assert!(marker(text).is_none(), "{text:?} was read as a bullet");
        }
    }

    #[test]
    fn a_dash_needs_the_space_after_it() {
        assert!(marker("- apples").is_some());
        assert!(
            marker("-apples").is_none(),
            "a hyphenated word is not an item"
        );
    }

    #[test]
    fn a_number_that_is_not_a_marker() {
        for text in [
            "2.1 shall report to the members",
            "1.5 million tonnes",
            "1984 was a year",
            "1234. too many",
            "1.2.3.4 Too deep",
            "ii.3 Lower-case head",
        ] {
            assert!(marker(text).is_none(), "{text:?} was read as a marker");
        }
    }

    #[test]
    fn a_statute_numbers_its_sections() {
        let run = |texts: &[&str]| {
            let found: Vec<Marker> = texts.iter().filter_map(|t| marker(t)).collect();
            found.len() == texts.len() && is_a_run(&found)
        };
        // FamFG's contents page (DocLayNet validation, #1973): `74a` sits
        // between `74` and `75`.
        assert!(run(&[
            "\u{a7}  74 Entscheidung \u{fc}ber die Rechtsbeschwerde",
            "\u{a7}  74a Zur\u{fc}ckweisungsbeschluss",
            "\u{a7}  75 Sprungrechtsbeschwerde",
        ]));
        let section = marker("\u{a7} 70 Statthaftigkeit der Rechtsbeschwerde").expect("a section");
        assert!(
            section.keeps_its_text(),
            "a section keeps its number in the item's text"
        );
        // A sentence that cites a section in lower case, and a year, are not.
        assert!(marker("\u{a7} 5 abs. 2 gilt entsprechend").is_none());
        assert!(marker("\u{a7} 12345 Too many").is_none());
        // And a section does not join a list of plain numbers.
        assert!(!run(&["1. apples", "\u{a7} 2 Pears"]));
    }

    #[test]
    fn an_outline_numbers_its_clauses() {
        let run = |texts: &[&str]| {
            let found: Vec<Marker> = texts.iter().filter_map(|t| marker(t)).collect();
            found.len() == texts.len() && is_a_run(&found)
        };
        // The EU supply contract in DocLayNet validation (#599).
        assert!(run(&[
            "II.4.6 At the contracting authority's reasoned request",
            "II.4.7 The contractor must record and report",
            "II.6.1 The contracting authority is not liable",
        ]));
        // An annual report's exhibit index, which skips.
        assert!(run(&[
            "10.2 Director Equity Plan",
            "10.3 Standard form",
            "10.6 Omnicom Group"
        ]));
        // A trailing full stop is the same marker.
        assert!(run(&["II.22.1. If an amount", "II.22.2. The contractor"]));
        // Two figures that open lines climb, and are not one outline.
        assert!(!run(&["3.5 Million tonnes", "12.7 Percent of it"]));
        // A roman head and an arabic one are two schemes, and so are depths.
        assert!(!run(&["II.4.1 The first", "3.4.2 The second"]));
        assert!(!run(&["2.4 The first", "2.4.1 The second"]));
        // An outline and a plain number are not one list.
        assert!(!run(&["1. apples", "1.2 Pears"]));
        // A contents page's entry is numbered the same way, and is not one.
        for entry in [
            "I.4.1. Price of the contract ........ 6",
            "I.4.1. Price of the contract . . . . . 6",
            "I.4.1. Price of the contract \u{2026}\u{2026} 6",
        ] {
            assert!(marker(entry).is_none(), "{entry:?} was read as a clause");
        }
        // An ellipsis a clause quotes is not a leader.
        assert!(marker("II.4.1 The contractor... must comply").is_some());
        let clause = marker("II.4.1 The contractor").expect("an outline clause");
        assert!(clause.is_outline());
        assert_eq!(clause.len, "II.4.1 ".len());
    }

    #[test]
    fn numbering_styles() {
        assert_eq!(marker("1. apples").and_then(|m| m.value(false)), Some(1));
        assert_eq!(marker("7) pears").and_then(|m| m.value(false)), Some(7));
        assert_eq!(marker("(c) quinces").and_then(|m| m.value(false)), Some(3));
        assert_eq!(marker("iv. medlars").and_then(|m| m.value(false)), Some(4));
        assert_eq!(marker("ix. sloes").and_then(|m| m.value(false)), Some(9));
    }

    #[test]
    fn letters_that_jump_are_initials_not_a_list() {
        let run = |texts: &[&str]| {
            let found: Vec<Marker> = texts.iter().filter_map(|t| marker(t)).collect();
            found.len() == texts.len() && is_a_run(&found)
        };
        // The continuation lines of references (#599).
        assert!(!run(&[
            "T. Fujiwara, E. Yamamoto, Y. Haga",
            "Y. Onuki, J. Phys. Soc. Jpn."
        ]));
        // A lettered list, and one with an item the page lost.
        assert!(run(&["a. apples", "b. pears", "c. quinces"]));
        assert!(run(&["A. Scope", "C. Definitions"]));
        // Numbers may skip.
        assert!(run(&["1. apples", "4. pears"]));
    }

    #[test]
    fn an_initial_before_an_initial_is_a_name() {
        // An author list whose raised reference number was read apart opens
        // with the first author's initials, and the first of them is not a
        // marker (#599).
        for text in [
            "A. G. Gavriliuk, I. A. Trojan, JETP Lett. 100, 688 (2005).",
            "I. S. Lyubutin, S. G. Ovchinnikov, A. G. Gavriliuk, and",
            "V. V. Struzhkin, Phys. Rev. B 79, 085125 (2009).",
            "J. J.-P. Bouchaud and M. Potters, Theory of Financial Risk",
        ] {
            assert!(marker(text).is_none(), "{text:?} was read as a marker");
        }
        // Nor one initial before a surname and the next author (#574).
        for text in [
            "L. Lamata and E. Solano, Phys. Rev. Lett. 103, 070503 (2009).",
            "A. Cucchieri and T. Mendes, Phys. Rev. D 78, 094503 (2008)",
            "K. Makris, R. El-Ganainy, D. N. Christodoulides and",
            "H. Graafsma, C. Henriquet, R. Verbeni, J. Synch. Rad.",
        ] {
            assert!(marker(text).is_none(), "{text:?} was read as a marker");
        }
        // A list lettered in capitals is still one, abbreviation or not.
        for text in [
            "A. Scope of the works",
            "B. U.S. dollar amounts",
            "IV. Definitions",
            "(A) G. Gavriliuk and others",
            "II. A. Smith, the second witness",
            "IV. C. Brown, the fourth witness",
            "A. Introduction and scope",
            "B. Revenue, costs and margins",
            "C. Smith and Jones, the third party",
        ] {
            assert!(marker(text).is_some(), "{text:?} opens no item");
        }
    }

    #[test]
    fn a_marker_with_no_text_after_it_is_not_one() {
        assert!(marker("\u{2022}").is_none());
        assert!(marker("1.  ").is_none());
    }

    #[test]
    fn a_bare_letter_is_read_by_its_neighbours() {
        let roman: Vec<Marker> = ["i. one", "ii. two", "iii. three"]
            .iter()
            .filter_map(|t| marker(t))
            .collect();
        assert!(is_a_run(&roman), "i, ii, iii is not a run");
        assert_eq!(roman[0].value(is_roman(&roman)), Some(1));

        let latin: Vec<Marker> = ["g. one", "h. two", "i. three"]
            .iter()
            .filter_map(|t| marker(t))
            .collect();
        assert!(is_a_run(&latin), "g, h, i is not a run");
        assert_eq!(latin[2].value(is_roman(&latin)), Some(9));
    }

    #[test]
    fn a_run_climbs() {
        let repeated: Vec<Marker> = ["(a) first mention", "(a) second mention"]
            .iter()
            .filter_map(|t| marker(t))
            .collect();
        assert!(
            !is_a_run(&repeated),
            "a clause reference repeated is not a list"
        );
        let letters_and_numerals: Vec<Marker> = ["a. one", "iv. two"]
            .iter()
            .filter_map(|t| marker(t))
            .collect();
        assert!(
            !is_a_run(&letters_and_numerals),
            "a letter that is no roman digit joined a roman run"
        );
        let mixed: Vec<Marker> = ["1. one", "\u{2022} two"]
            .iter()
            .filter_map(|t| marker(t))
            .collect();
        assert!(!is_a_run(&mixed), "a number and a bullet are not one list");
    }

    #[test]
    fn a_picture_font_marks_with_whatever_it_decodes_to() {
        assert!(
            marker_at(
                "l apples",
                Lead {
                    symbol: Some(1),
                    ..Lead::default()
                }
            )
            .is_some(),
            "a dingbat bullet is a bullet"
        );
        assert!(
            marker_at("l apples", Lead::default()).is_none(),
            "the same letter in a text font is not"
        );
    }

    #[test]
    fn a_bibliography_is_numbered_in_brackets() {
        assert_eq!(
            marker("[3] S. Khalil, J. Phys. G 35, 055001 (2008).").and_then(|m| m.value(false)),
            Some(3),
            "a bracketed reference number is not a marker"
        );
        assert!(
            marker("[3x] not a number").is_none(),
            "the closing bracket is required where the opening one was written"
        );
        assert!(
            marker("[3) mismatched").is_none(),
            "a square bracket is not closed by a round one"
        );
    }

    #[test]
    fn a_raised_figure_numbers_a_reference() {
        let raised = Lead {
            raised: Some(2),
            ..Lead::default()
        };
        assert_eq!(
            marker_at("12 E. B. Sonin, arXiv:0909.3156v1 (2009).", raised)
                .and_then(|m| m.value(false)),
            Some(12),
            "a raised reference number is not a marker"
        );
        assert!(
            marker("12 E. B. Sonin, arXiv:0909.3156v1 (2009).").is_none(),
            "the same figure set in the line is not a marker"
        );
        assert!(
            marker_at(
                "1984 was a year",
                Lead {
                    raised: Some(4),
                    ..Lead::default()
                }
            )
            .is_none(),
            "a year is longer than a marker"
        );
    }

    #[test]
    fn a_glyph_set_apart_pairs_only_with_itself() {
        let apart = Lead {
            apart: Some(1),
            ..Lead::default()
        };
        let bullets: Vec<Marker> = ["t  apples", "t  pears"]
            .iter()
            .filter_map(|t| marker_at(t, apart))
            .collect();
        assert_eq!(bullets.len(), 2);
        assert!(is_a_run(&bullets), "the same glyph twice is not a run");

        let letters: Vec<Marker> = ["A  Introduction", "B  Method"]
            .iter()
            .filter_map(|t| marker_at(t, apart))
            .collect();
        assert!(
            !is_a_run(&letters),
            "two different letters were read as one bulleted list"
        );
        assert!(
            marker("t  apples").is_none(),
            "a letter the page did not set apart is not a marker"
        );
    }

    #[test]
    fn a_currency_sign_set_apart_is_a_column_of_figures() {
        // `$  5,091   $  2,927`, row after row: the sign stands apart from its
        // figure and repeats down one edge, which is everything a glyph bullet
        // is -- and it is a table of money (#589).
        let apart = Lead {
            apart: Some(1),
            ..Lead::default()
        };
        for row in ["$  5,091  $ 2,927", "%  12", "7  apples"] {
            assert!(
                marker_at(row, apart).is_none(),
                "{row:?} was read as a glyph bullet"
            );
        }
    }

    #[test]
    fn roman_numerals_add_and_subtract() {
        assert_eq!(roman_value("i"), Some(1));
        assert_eq!(roman_value("iv"), Some(4));
        assert_eq!(roman_value("xiv"), Some(14));
        assert_eq!(roman_value("mcmxciv"), Some(1994));
        assert_eq!(roman_value("apples"), None);
    }
}
