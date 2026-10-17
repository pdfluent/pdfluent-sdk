//! Combining marks from the AMS name table, placed after the glyph they mark
//! (spec `specs/text/circlecopyrt.md`, C4 and C5a).
//!
//! TeX draws some marks as glyphs of their own over the glyph they belong
//! to: `\copyright` is a `c` with `cmsy10`'s `circlecopyrt` drawn over it
//! afterwards, its box starting left of the `c`; `\not=` is `cmsy10`'s
//! zero-width `negationslash` drawn before the `=` it strikes. The AMS table
//! reads those names as combining marks (U+20DD, U+0338), and a combining
//! mark belongs directly after its base, whichever of the two the page drew
//! first and wherever its box starts.
//!
//! Only this crate knows both where a glyph's text came from and where the
//! glyphs stand, so the placement is decided here, as the page is
//! interpreted, and handed out through `as_unicode`. A mark is placed only
//! on the glyph drawn directly before it or directly after it, on the same
//! line, whose box centre lies inside the mark's box; any other geometry
//! leaves both glyphs reading as they did before the row existed.
//!
//! A placement is offered, not imposed: it holds once the reader has read
//! the side that changes last -- the base, for a mark drawn after it; the
//! mark, for a mark drawn before its base -- and is refused when the other
//! side is read first. Taken, the base reads as its own text followed by the
//! mark and the mark's glyph reads as nothing. Refused, both read as they did
//! before the row, so a reader that cannot put the mark after its base (the
//! base's text replaced by `/ActualText`, or already merged out of reach)
//! never loses the mark's old text and never sees a lone row-derived mark.
//!
//! Where a mark's text came from travels beside the text, never in it: a
//! glyph's text is a row-derived mark when it is one combining mark and the
//! same code, read with the mark rows set aside, gives something else. A
//! `ToUnicode` value is the same either way, so no code point it gives --
//! U+FFFF included -- is ever taken for a row's. Which rows are marks is read
//! off the data: a row whose character is a combining mark. A later plain
//! row needs no change here.
//!
//! No public type holds new shared state for this: a glyph carries two plain
//! numbers, and the page's record lives in a thread-local registry for as
//! long as the interpretation that owns it.

use super::outline::OutlineFont;
use super::{BfString, Glyph, OutlineGlyph};
use kurbo::{Affine, Shape};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// Whether `c` is a combining mark: one of the Unicode blocks of combining
/// diacritical marks, which hold every mark a math font draws over a base.
#[inline]
pub(crate) fn is_combining_mark(c: char) -> bool {
    // Below the first block, as nearly every character of a page is.
    if c < '\u{0300}' {
        return false;
    }
    matches!(
        c,
        '\u{0300}'..='\u{036F}'
            | '\u{1AB0}'..='\u{1AFF}'
            | '\u{1DC0}'..='\u{1DFF}'
            | '\u{20D0}'..='\u{20FF}'
            | '\u{FE20}'..='\u{FE2F}'
    )
}

thread_local! {
    /// Set while [`without_mark_rows`] reads a code as it read before the
    /// combining-mark rows existed.
    static MARK_ROWS_SET_ASIDE: Cell<bool> = const { Cell::new(false) };
}

/// Whether the combining-mark rows are set aside on this thread.
pub(crate) fn mark_rows_set_aside() -> bool {
    MARK_ROWS_SET_ASIDE.get()
}

/// Puts [`MARK_ROWS_SET_ASIDE`] back as it was when dropped, so that a panic
/// inside the read cannot leave the rows set aside for the rest of the
/// thread.
struct RestoreMarkRows(bool);

impl Drop for RestoreMarkRows {
    fn drop(&mut self) {
        MARK_ROWS_SET_ASIDE.set(self.0);
    }
}

/// `read` run with the combining-mark rows set aside: their names read as no
/// table knows them, and a per-code table entry built from one counts as
/// absent, so the font's chain goes on exactly as it did before the rows.
pub(crate) fn without_mark_rows<T>(read: impl FnOnce() -> T) -> T {
    let _restore = RestoreMarkRows(MARK_ROWS_SET_ASIDE.replace(true));
    read()
}

/// One entry of a per-code table built from glyph names, with where its text
/// came from beside it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NamedEntry {
    pub(crate) text: BfString,
    /// Whether a combining-mark row of the AMS table gave the text.
    pub(crate) from_mark_row: bool,
}

/// A per-code table entry as the chain should see it: an entry a
/// combining-mark row wrote is absent while the rows are set aside.
pub(crate) fn table_entry(entry: &Option<NamedEntry>) -> Option<BfString> {
    match entry {
        Some(entry) if !(entry.from_mark_row && mark_rows_set_aside()) => Some(entry.text.clone()),
        _ => None,
    }
}

/// Whether `text` is one combining mark, the only text a row-derived mark
/// can be: the first test of [`row_mark_of`], which costs no read.
#[inline]
pub(crate) fn may_be_row_mark(text: &Option<BfString>) -> bool {
    lone_combining_mark(text).is_some()
}

/// The one combining mark `text` is, if it is one.
#[inline]
fn lone_combining_mark(text: &Option<BfString>) -> Option<char> {
    let mark = match text {
        Some(BfString::Char(c)) => *c,
        Some(BfString::String(s)) => {
            let mut chars = s.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => c,
                _ => return None,
            }
        }
        None => return None,
    };
    is_combining_mark(mark).then_some(mark)
}

/// The mark a code's text stands for when a combining-mark row gave it.
///
/// `text` is what the code reads as, and `again` reads it once more. Only a
/// text of one combining mark can be a row's, and it is one exactly when the
/// same code read with the rows set aside gives something else: then the row
/// spoke, since nothing else changes when the rows are set aside.
pub(crate) fn row_mark_of(
    text: &Option<BfString>,
    again: impl FnOnce() -> Option<BfString>,
) -> Option<char> {
    let mark = lone_combining_mark(text)?;
    (without_mark_rows(again) != *text).then_some(mark)
}

/// Which side of a placement decides it when read first.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Side {
    Base,
    Mark,
}

/// Where a placement stands.
#[derive(Clone, Copy, Debug, PartialEq)]
enum State {
    Offered,
    Taken,
    Refused,
}

/// One mark offered to one base.
#[derive(Debug)]
struct Placement {
    mark: char,
    /// The side whose read takes the offer; a read of the other side while
    /// it is still offered refuses it.
    taker: Side,
    state: State,
}

impl Placement {
    /// The state after `side` reads it.
    fn read_by(&mut self, side: Side) -> State {
        if self.state == State::Offered {
            self.state = if side == self.taker {
                State::Taken
            } else {
                State::Refused
            };
        }
        self.state
    }
}

/// Hashes the numbers this module draws itself (records, glyphs): they are
/// small, distinct and not chosen by a document, so one multiplication
/// spreads them well enough, where the default hasher costs a glyph's read
/// on a page with a placement more than the read itself (spec,
/// "Performance").
#[derive(Default)]
struct NumberHasher(u64);

impl Hasher for NumberHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = (self.0.rotate_left(8) ^ u64::from(byte)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        }
    }

    fn write_u64(&mut self, number: u64) {
        self.0 = number.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
}

/// A map keyed by one of this module's numbers.
type Numbered<V> = HashMap<u64, V, BuildHasherDefault<NumberHasher>>;

/// The placements made while one page is interpreted.
#[derive(Debug, Default)]
struct MarkLog {
    placements: Vec<Placement>,
    /// The placements each base glyph was offered, by glyph number.
    by_base: Numbered<Vec<usize>>,
    /// The placement each mark glyph was offered, by glyph number.
    by_mark: Numbered<usize>,
    /// For each glyph whose drawing made an offer across it and the glyph
    /// drawn directly before it: that glyph's number, and what it takes to
    /// read it again.
    rereads: Numbered<(u64, Candidate)>,
}

thread_local! {
    /// The record of every page interpretation running on this thread, by
    /// the number its [`MarkPlacement`] drew.
    static MARK_LOGS: RefCell<Numbered<MarkLog>> = RefCell::new(HashMap::default());
}

/// The number the next [`MarkPlacement`] draws. Drawn across all threads, so
/// that no two records anywhere share a number: a [`MarkSite`] carried to
/// another thread never names a record of that thread.
static NEXT_LOG: AtomicU64 = AtomicU64::new(0);

/// How many placements the records in [`MARK_LOGS`] hold, on all threads
/// together. While it is zero no glyph has a placement to read, so a read
/// answers without looking a record up: a page without a row-derived mark
/// pays one plain load per read (spec, "Performance"). Counted across
/// threads so that the load needs no thread-local; a placement on another
/// thread only sends this thread's reads the long way, to the same answer.
static PLACEMENTS_HELD: AtomicUsize = AtomicUsize::new(0);

/// Whether any record holds a placement.
#[inline]
pub(crate) fn placements_held() -> bool {
    PLACEMENTS_HELD.load(Ordering::Relaxed) > 0
}

/// How many interpretation records this thread holds.
#[cfg(test)]
fn records_held() -> usize {
    MARK_LOGS.with(|logs| logs.borrow().len())
}

#[cfg(test)]
thread_local! {
    /// How many times a record was looked up on this thread.
    static RECORD_LOOKUPS: Cell<usize> = const { Cell::new(0) };
}

/// `f` run on the record numbered `log`, if it still exists.
fn with_log<T>(log: u64, f: impl FnOnce(&mut MarkLog) -> T) -> Option<T> {
    #[cfg(test)]
    RECORD_LOOKUPS.set(RECORD_LOOKUPS.get() + 1);
    MARK_LOGS
        .try_with(|logs| logs.borrow_mut().get_mut(&log).map(f))
        .ok()
        .flatten()
}

/// A glyph's place in its page's record: plain numbers, nothing shared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MarkSite {
    log: u64,
    number: u64,
}

impl MarkSite {
    /// The marks this glyph reads with after its own text: every mark
    /// offered to it whose placement holds once this read is counted.
    pub(crate) fn base_marks(&self) -> String {
        if !placements_held() {
            return String::new();
        }
        with_log(self.log, |log| {
            let Some(offered) = log.by_base.get(&self.number) else {
                return String::new();
            };
            let mut marks = String::new();
            for &index in offered {
                let placement = &mut log.placements[index];
                if placement.read_by(Side::Base) == State::Taken {
                    marks.push(placement.mark);
                }
            }
            marks
        })
        .unwrap_or_default()
    }

    /// What the glyph at `previous` reads as now, when drawing this glyph
    /// made an offer across the two: `None` when it did not, and so the
    /// glyph before this one reads as it did. Reading it is what decides
    /// the offer, as [`OutlineGlyph::as_unicode`] does.
    pub(crate) fn reread_previous(&self, previous: MarkSite) -> Option<Option<BfString>> {
        if !placements_held() || previous.log != self.log {
            return None;
        }
        let glyph = with_log(self.log, |log| {
            log.rereads
                .get(&self.number)
                .filter(|(number, _)| *number == previous.number)
                .map(|(_, glyph)| glyph.clone())
        })
        .flatten()?;
        Some(glyph.glyph(previous).as_unicode())
    }

    /// Whether this mark glyph's placement holds once this read is counted.
    pub(crate) fn mark_taken(&self) -> bool {
        if !placements_held() {
            return false;
        }
        with_log(self.log, |log| {
            let index = *log.by_mark.get(&self.number)?;
            Some(log.placements[index].read_by(Side::Mark) == State::Taken)
        })
        .flatten()
        .unwrap_or(false)
    }
}

/// Where a glyph stands on its line, in page space.
#[derive(Clone, Copy, Debug)]
struct Spot {
    number: u64,
    /// The left and right edges of the glyph's box.
    left: f64,
    right: f64,
    baseline: f64,
    size: f64,
}

impl Spot {
    /// Where `glyph`, drawn under `placed`, stands: its advance box, widened
    /// to its ink for a mark. `None` for a glyph that is not upright -- the
    /// rule is about marks on a horizontal line -- or has no box at all.
    fn of(glyph: &OutlineGlyph, number: u64, placed: Affine, ink: bool) -> Option<Self> {
        let outline = || glyph.outline();
        Self::boxed(
            number,
            placed,
            || glyph.advance_width(),
            ink.then_some(&outline as &dyn Fn() -> kurbo::BezPath),
        )
    }

    /// [`Spot::of`] from its parts: the glyph's advance and, for a mark,
    /// its outline, each asked only once the glyph is known to be upright.
    fn boxed(
        number: u64,
        placed: Affine,
        advance: impl FnOnce() -> Option<f32>,
        ink: Option<&dyn Fn() -> kurbo::BezPath>,
    ) -> Option<Self> {
        let [a, b, c, d, x, y] = placed.as_coeffs();
        let upright = a > 0.0 && d > 0.0 && b.abs() <= a * 1e-6 && c.abs() <= d * 1e-6;
        if !upright {
            return None;
        }
        let size = a * 1000.0;
        let advance = f64::from(advance().unwrap_or(0.0)) / 1000.0 * size;
        let (mut left, mut right) = (x, x + advance.max(0.0));
        if let Some(outline) = ink {
            let drawn = (placed * outline()).bounding_box();
            if drawn.width() > 0.0 && drawn.x0.is_finite() && drawn.x1.is_finite() {
                left = left.min(drawn.x0);
                right = right.max(drawn.x1);
            }
        }
        (right > left).then_some(Self {
            number,
            left,
            right,
            baseline: y,
            size,
        })
    }

    /// Whether this mark's box holds the centre of `base`'s box on the same
    /// line.
    fn holds_centre_of(&self, base: &Spot) -> bool {
        let centre = (base.left + base.right) * 0.5;
        let same_line = (self.baseline - base.baseline).abs() <= self.size.max(base.size) * 0.35;
        same_line && self.left <= centre && centre <= self.right
    }
}

/// A glyph with text of its own whose box is worked out only when a mark
/// next to it asks for it (spec, "Performance"): on a page without a
/// row-derived mark no base's box is ever needed.
#[derive(Clone, Debug)]
struct Candidate {
    number: u64,
    placed: Affine,
    font: OutlineFont,
    char_code: u32,
}

/// Whether `glyph` can be the base of a mark: built inside a page
/// interpretation, with text of its own.
fn has_own_text(glyph: &OutlineGlyph) -> bool {
    glyph.marks.is_some()
        && match glyph.cached_text() {
            Some(BfString::Char(c)) => !c.is_whitespace(),
            Some(BfString::String(s)) => !s.trim().is_empty(),
            None => false,
        }
}

impl Candidate {
    /// `glyph`, drawn under `placed`, as a base still to be placed: `None`
    /// for a glyph built outside a page interpretation or without text of
    /// its own.
    fn of(glyph: &OutlineGlyph, placed: Affine) -> Option<Self> {
        let site = glyph.marks?;
        has_own_text(glyph).then(|| Self {
            number: site.number,
            placed,
            font: glyph.font.clone(),
            char_code: glyph.char_code,
        })
    }

    /// Make this candidate `glyph`, drawn under `placed`, in place: what
    /// [`Candidate::of`] gives for a glyph with text of its own, keeping the
    /// font already held when it is the same one.
    fn refill(&mut self, glyph: &OutlineGlyph, number: u64, placed: Affine) {
        self.number = number;
        self.placed = placed;
        if !self.font.is_same(&glyph.font) {
            self.font = glyph.font.clone();
        }
        self.char_code = glyph.char_code;
    }

    /// The glyph again, at `site`, to be read once more.
    fn glyph(&self, site: MarkSite) -> OutlineGlyph {
        OutlineGlyph {
            id: skrifa::GlyphId::NOTDEF,
            font: self.font.clone(),
            char_code: self.char_code,
            resource_name: None,
            marks: Some(site),
            text: self.font.char_code_to_unicode(self.char_code),
        }
    }

    /// Where the glyph stands: what [`Seen::of`] would have made of it had
    /// it worked the box out at once.
    fn spot(&self) -> Option<Spot> {
        Spot::boxed(
            self.number,
            self.placed,
            || self.font.glyph_advance_width(self.char_code),
            None,
        )
    }
}

/// The glyph drawn last, as a base for a mark drawn after it.
#[derive(Debug)]
enum LastBase {
    Placed(Spot),
    Unplaced(Candidate),
}

impl LastBase {
    fn spot(self) -> Option<Spot> {
        match self {
            Self::Placed(spot) => Some(spot),
            Self::Unplaced(candidate) => candidate.spot(),
        }
    }
}

/// What a drawn glyph is to the placement.
#[derive(Clone, Debug)]
enum Seen {
    /// A row-derived mark, where it stands.
    Mark(Spot, char),
    /// A glyph that can be the base of a mark: an upright outline glyph with
    /// text of its own.
    Base(Spot),
    /// A glyph with text of its own, its box not yet worked out: a
    /// [`Seen::Base`] where it is upright and has a box, else
    /// [`Seen::Other`].
    Candidate(Candidate),
    /// Anything else: it ends the run, so no mark reaches past it.
    Other,
}

impl Seen {
    /// What `glyph`, drawn under `placed`, is.
    fn of(glyph: &Glyph<'_>, placed: Affine) -> Self {
        let Glyph::Outline(outline) = glyph else {
            return Self::Other;
        };
        let Some(site) = outline.marks else {
            return Self::Other;
        };
        let text = outline.cached_text();
        if let Some(mark) = row_mark_of(text, || outline.font_text()) {
            return Spot::of(outline, site.number, placed, true)
                .map_or(Self::Other, |spot| Self::Mark(spot, mark));
        }
        Candidate::of(outline, placed).map_or(Self::Other, Self::Candidate)
    }
}

/// The interpreter's side of the placement: the glyph drawn last, as each
/// new one is drawn.
///
/// A mark is offered to the glyph drawn directly before it when that glyph's
/// centre lies inside the mark's box, and otherwise to the glyph drawn
/// directly after it on the same terms. Only an outline glyph with text of
/// its own can be a base; a mark, a glyph without text, or any glyph that is
/// not upright ends the run, so nothing reaches past it.
#[derive(Debug, Default)]
pub(crate) struct MarkPlacement {
    /// The number of this interpretation's record in [`MARK_LOGS`], drawn
    /// with the first glyph: an interpretation that draws none (a pattern,
    /// a soft mask, most Type 3 glyph procedures) keeps no record.
    log: Option<u64>,
    /// The number the next glyph gets.
    next: u64,
    /// The glyph drawn last, when it can be the base of a mark drawn after
    /// it.
    base: Option<LastBase>,
    /// The mark drawn last, when the glyph before it was not its base.
    pending: Option<(Spot, char)>,
    /// What it takes to read again the glyph `base` (once placed) or
    /// `pending` stands for, when the slow path set it.
    held: Option<Candidate>,
}

impl Drop for MarkPlacement {
    fn drop(&mut self) {
        // The record goes with the interpretation that owns it; a glyph that
        // outlives it reads as before the rows' placement.
        let Some(log) = self.log else {
            return;
        };
        let removed = MARK_LOGS
            .try_with(|logs| logs.borrow_mut().remove(&log))
            .ok()
            .flatten();
        if let Some(log) = removed {
            PLACEMENTS_HELD.fetch_sub(log.placements.len(), Ordering::Relaxed);
        }
    }
}

impl MarkPlacement {
    /// The site a new glyph of this page reads its placements from.
    pub(crate) fn site(&mut self) -> MarkSite {
        let log = *self.log.get_or_insert_with(|| {
            let log = NEXT_LOG.fetch_add(1, Ordering::Relaxed);
            MARK_LOGS.with(|logs| logs.borrow_mut().insert(log, MarkLog::default()));
            log
        });
        let number = self.next;
        self.next += 1;
        MarkSite { log, number }
    }

    /// Note that `glyph` is drawn under `placed` (CTM times glyph transform),
    /// offering a mark where it, or the glyph directly before it, is one.
    #[inline]
    pub(crate) fn observe(&mut self, glyph: &Glyph<'_>, placed: Affine) {
        // The common case, taken without building a [`Seen`]: no mark waits
        // for this glyph and its text cannot be a row's mark, so it is at
        // most a base for a mark drawn after it, its box not worked out
        // (spec, "Performance"). [`Self::step`] does the same with it.
        if self.pending.is_none()
            && let Glyph::Outline(outline) = glyph
            && !may_be_row_mark(outline.cached_text())
        {
            match (&mut self.base, outline.marks) {
                (Some(LastBase::Unplaced(held)), Some(site)) if has_own_text(outline) => {
                    held.refill(outline, site.number, placed);
                }
                (base, _) => *base = Candidate::of(outline, placed).map(LastBase::Unplaced),
            }
            return;
        }
        let seen = Seen::of(glyph, placed);
        let this = match (&seen, glyph) {
            (Seen::Mark(..) | Seen::Candidate(_), Glyph::Outline(outline)) => Some(Candidate {
                number: outline.marks.map_or(0, |site| site.number),
                placed,
                font: outline.font.clone(),
                char_code: outline.char_code,
            }),
            _ => None,
        };
        self.step_with(seen, this);
    }

    /// Take one drawn glyph into account: offer a mark across it and the
    /// glyph drawn directly before it where the geometry says so, and
    /// remember it for the glyph drawn directly after it.
    #[cfg(test)]
    fn step(&mut self, seen: Seen) {
        self.step_with(seen, None);
    }

    /// [`Self::step`], with what it takes to read the glyph again (`this`)
    /// so that an offer across it and the glyph before it can say how that
    /// glyph is read again.
    fn step_with(&mut self, seen: Seen, this: Option<Candidate>) {
        let pending = self.pending.take();
        let base = self.base.take();
        let held = self.held.take();
        let seen = match seen {
            // Nothing waits for this glyph's box: it is worked out only if
            // the next glyph is a mark.
            Seen::Candidate(candidate) if pending.is_none() => {
                self.base = Some(LastBase::Unplaced(candidate));
                return;
            }
            Seen::Candidate(candidate) => candidate.spot().map_or(Seen::Other, Seen::Base),
            seen => seen,
        };
        match seen {
            Seen::Other | Seen::Candidate(_) => {}
            Seen::Mark(spot, mark) => {
                let before = match &base {
                    Some(LastBase::Unplaced(candidate)) => Some(candidate.clone()),
                    _ => held,
                };
                match base.and_then(LastBase::spot) {
                    Some(base) if spot.holds_centre_of(&base) => {
                        self.offer(mark, spot.number, base.number, Side::Base);
                        self.note_reread(spot.number, base.number, before);
                    }
                    _ => {
                        self.pending = Some((spot, mark));
                        self.held = this;
                    }
                }
            }
            Seen::Base(spot) => {
                if let Some((mark_spot, mark)) = pending
                    && mark_spot.holds_centre_of(&spot)
                {
                    self.offer(mark, mark_spot.number, spot.number, Side::Mark);
                    self.note_reread(spot.number, mark_spot.number, held);
                }
                self.base = Some(LastBase::Placed(spot));
                self.held = this;
            }
        }
    }

    /// Note that drawing glyph `at` made an offer across it and glyph
    /// `previous`, which `glyph` reads again.
    fn note_reread(&self, at: u64, previous: u64, glyph: Option<Candidate>) {
        let (Some(log), Some(glyph)) = (self.log, glyph) else {
            return;
        };
        with_log(log, |log| log.rereads.insert(at, (previous, glyph)));
    }

    /// Offer `mark`, drawn by glyph `mark_glyph`, to glyph `base`.
    fn offer(&self, mark: char, mark_glyph: u64, base: u64, taker: Side) {
        let Some(log) = self.log else {
            return;
        };
        with_log(log, |log| {
            let index = log.placements.len();
            log.placements.push(Placement {
                mark,
                taker,
                state: State::Offered,
            });
            log.by_base.entry(base).or_default().push(index);
            log.by_mark.insert(mark_glyph, index);
            PLACEMENTS_HELD.fetch_add(1, Ordering::Relaxed);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    /// The placement count is shared by every thread, so the tests that
    /// make placements, and the one that needs none to exist, take turns.
    fn placements_alone() -> MutexGuard<'static, ()> {
        static TURN: Mutex<()> = Mutex::new(());
        TURN.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Only a text of one combining mark that the rows changed is a row's.
    #[test]
    fn a_row_mark_is_one_combining_mark_the_rows_gave() {
        let circle = Some(BfString::Char('\u{20DD}'));
        // The rows gave it: set aside, the code reads as nothing.
        let from_row = || {
            if mark_rows_set_aside() {
                None
            } else {
                circle.clone()
            }
        };
        assert_eq!(row_mark_of(&circle, from_row), Some('\u{20DD}'));
        // `ToUnicode` gave it: the same either way.
        assert_eq!(row_mark_of(&circle, || circle.clone()), None);
        // U+FFFF, alone or before a mark, is never a row's.
        for value in ["\u{FFFF}", "\u{FFFF}\u{20DD}"] {
            let text = Some(BfString::String(value.into()));
            assert_eq!(row_mark_of(&text, || None), None, "{value:?}");
        }
        assert_eq!(row_mark_of(&Some(BfString::Char('A')), || None), None);
        assert_eq!(row_mark_of(&None, || None), None);
    }

    /// Fix 4: the set-aside flag is put back even when the read panics.
    #[test]
    fn a_panic_inside_the_set_aside_read_puts_the_rows_back() {
        assert!(!mark_rows_set_aside());
        let outcome = std::panic::catch_unwind(|| {
            without_mark_rows(|| {
                assert!(mark_rows_set_aside());
                panic!("inside the read");
            })
        });
        assert!(outcome.is_err());
        assert!(!mark_rows_set_aside());
        // Nested reads restore the outer state, not `false`.
        without_mark_rows(|| {
            without_mark_rows(|| ());
            assert!(mark_rows_set_aside());
        });
        assert!(!mark_rows_set_aside());
    }

    fn spot(left: f64, right: f64, baseline: f64) -> Spot {
        Spot {
            number: 0,
            left,
            right,
            baseline,
            size: 10.0,
        }
    }

    /// The centre must lie inside the box, on the same line.
    #[test]
    fn a_mark_holds_a_centre_inside_its_box_on_its_line() {
        let circle = spot(100.0, 110.0, 700.0);
        assert!(circle.holds_centre_of(&spot(102.8, 107.8, 700.0)));
        assert!(!circle.holds_centre_of(&spot(110.0, 115.0, 700.0)));
        assert!(!circle.holds_centre_of(&spot(94.0, 99.0, 700.0)));
        assert!(!circle.holds_centre_of(&spot(102.8, 107.8, 712.0)));
    }

    /// The taker's read takes the offer; the other side's read refuses it,
    /// and once decided it stays decided.
    #[test]
    fn an_offer_is_decided_by_the_side_read_first() {
        let _turn = placements_alone();
        let mut placement = MarkPlacement::default();
        let base = placement.site();
        let mark = placement.site();
        placement.offer('\u{20DD}', mark.number, base.number, Side::Base);
        assert_eq!(base.base_marks(), "\u{20DD}");
        assert!(mark.mark_taken());
        assert_eq!(base.base_marks(), "\u{20DD}");

        let base = placement.site();
        let mark = placement.site();
        placement.offer('\u{20DD}', mark.number, base.number, Side::Base);
        assert!(!mark.mark_taken());
        assert_eq!(base.base_marks(), "");
        assert!(!mark.mark_taken());
    }

    fn at(number: u64, left: f64) -> Spot {
        Spot {
            number,
            left,
            right: left + 10.0,
            baseline: 700.0,
            size: 10.0,
        }
    }

    /// Rule 2 as narrowed (review 14-10): a mark is offered only across
    /// neighbours. Of two circles drawn one after the other over a `c`, only
    /// the first is offered to it; a slash drawn before a space and then an
    /// `=` is offered to nothing.
    #[test]
    fn a_mark_is_offered_only_to_the_glyph_next_to_it() {
        let _turn = placements_alone();
        let mut placement = MarkPlacement::default();
        let c = placement.site();
        let first = placement.site();
        let second = placement.site();
        placement.step(Seen::Base(at(c.number, 101.0)));
        placement.step(Seen::Mark(at(first.number, 100.0), '\u{20DD}'));
        placement.step(Seen::Mark(at(second.number, 100.0), '\u{20DD}'));
        assert_eq!(c.base_marks(), "\u{20DD}");
        assert!(first.mark_taken());
        assert!(!second.mark_taken());

        let slash = placement.site();
        let equals = placement.site();
        placement.step(Seen::Mark(at(slash.number, 200.0), '\u{0338}'));
        placement.step(Seen::Other);
        placement.step(Seen::Base(at(equals.number, 201.0)));
        assert!(!slash.mark_taken());
        assert_eq!(equals.base_marks(), "");
    }

    /// Review 14-10 02:00 (i): the record goes when the interpretation
    /// ends, normally or by a panic inside it.
    #[test]
    fn the_registry_is_empty_after_an_interpretation_and_after_a_panic() {
        let before = records_held();
        {
            let mut placement = MarkPlacement::default();
            let _ = placement.site();
            assert_eq!(records_held(), before + 1);
        }
        assert_eq!(records_held(), before);
        let outcome = std::panic::catch_unwind(|| {
            let mut placement = MarkPlacement::default();
            let _ = placement.site();
            assert_eq!(records_held(), before + 1);
            panic!("inside the interpretation");
        });
        assert!(outcome.is_err());
        assert_eq!(records_held(), before);
    }

    /// Spec "Performance": while no placement exists, reading a glyph
    /// never looks a record up, and an interpretation that draws no glyph
    /// keeps none. Once a placement exists, reads go the long way again.
    #[test]
    fn a_page_without_a_placement_never_looks_a_record_up() {
        let _turn = placements_alone();
        let before = records_held();
        let empty = MarkPlacement::default();
        assert_eq!(records_held(), before, "no glyph, no record");
        drop(empty);

        let mut placement = MarkPlacement::default();
        let sites: Vec<MarkSite> = (0..64).map(|_| placement.site()).collect();
        for (i, site) in sites.iter().enumerate() {
            placement.step(Seen::Base(at(site.number, 10.0 * i as f64)));
        }
        let lookups = RECORD_LOOKUPS.get();
        for site in &sites {
            assert_eq!(site.base_marks(), "");
            assert!(!site.mark_taken());
        }
        assert_eq!(RECORD_LOOKUPS.get(), lookups, "no placement, no lookup");

        let base = placement.site();
        let mark = placement.site();
        placement.offer('\u{20DD}', mark.number, base.number, Side::Base);
        assert_eq!(base.base_marks(), "\u{20DD}");
        assert!(RECORD_LOOKUPS.get() > lookups, "a placement is looked up");
    }

    /// Spec "Performance, round 2": on a page where nothing was offered,
    /// asking whether drawing a glyph changed the one before it answers
    /// "no" without looking a record up, so the text device keeps no glyph
    /// and reads nothing again.
    #[test]
    fn a_glyph_on_a_page_without_an_offer_changes_nothing_before_it() {
        let _turn = placements_alone();
        let mut placement = MarkPlacement::default();
        let sites: Vec<MarkSite> = (0..64).map(|_| placement.site()).collect();
        let lookups = RECORD_LOOKUPS.get();
        for pair in sites.windows(2) {
            assert_eq!(pair[1].reread_previous(pair[0]), None);
        }
        assert_eq!(RECORD_LOOKUPS.get(), lookups, "no offer, no lookup");
    }

    /// The record goes when the interpretation does.
    #[test]
    fn the_record_goes_with_its_interpretation() {
        let _turn = placements_alone();
        let mut placement = MarkPlacement::default();
        let base = placement.site();
        let mark = placement.site();
        placement.offer('\u{0338}', mark.number, base.number, Side::Mark);
        drop(placement);
        assert!(!mark.mark_taken());
        assert_eq!(base.base_marks(), "");
    }
}
