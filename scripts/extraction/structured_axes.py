#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The five axes of structured extraction, per page (#566).

`extraction_axes.py` grades a wall of text: did we produce the characters the
document holds, and in the order a foreign reader puts them. That is the whole
of what a plain extractor can be wrong about, and it is deliberately blind to
everything this milestone is about -- a heading that came out as a paragraph, a
list that lost its items, a table that arrived as a run of numbers. All three
score perfectly on recall.

So this file is a second instrument beside that one rather than a replacement,
and the two never share a number. Its axes are:

    text            normalised edit distance over the page's running text
    headings        F1 over the headings the page has
    lists           F1 over the list items the page has
    tables          TEDS -- tree edit distance similarity over table structure
    reading order   normalised edit distance over the order of matched blocks

WHY EDIT DISTANCE AND NOT CONTAINMENT
=====================================
The extraction rig measures containment, because an extractor that returns the
right characters in the wrong order has still read the document, and order is
measured separately by an axis that cannot be bought with recall. Here the unit
is a BLOCK the annotator drew, the ground truth is a human transcription of that
block, and the question is how close our text of it is -- which is what an edit
distance answers and containment does not: containment cannot see inserted text
at all, and a structured extractor that emits a page's furniture into the body
is wrong in exactly that direction.

The two are kept apart in the run file for the same reason the two corpora are:
a number that is sometimes containment and sometimes edit distance describes
neither.

WHY F1 AND NOT ACCURACY
=======================
A page has a handful of headings and no negatives to count. "Accuracy" over the
blocks we happened to emit rewards emitting nothing: no false positives, no
score. Precision and recall are reported separately beside the F1 for every
category, because they fail in different directions and a single number hides
which -- a tool that calls every line a heading and a tool that finds none both
land near zero, and they need different work.

WHAT THIS CANNOT SEE
====================
A block matched on text alone. Two paragraphs with the same words in different
places on the page are one block to this file, and so is a heading that we put
on the wrong page. Position is the annotator's and we do not emit boxes, so the
matching is by normalised text with a similarity floor; where that is
ambiguous -- two identical strings in one page -- the pairing is greedy and in
reading order, which is stated rather than hidden because it decides a score.

It also cannot see whether a number is worth anything on a page the ground truth
barely annotates. `MIN_CHARS` and `MIN_ITEMS` are the floors, and a page under
them is reported as unscored on that axis rather than as a perfect or an empty
score -- the same rule `extraction_axes.py` learnt the hard way when seven of
its worst documents turned out to have three anchor words each.
"""
from __future__ import annotations

import argparse
import functools
import hashlib
import html.parser
import json
import pathlib
import re
import sys
import unicodedata

# A page whose ground-truth running text is shorter than this says nothing about
# an edit distance: one wrong character in twenty is a five percent score, and
# the tail of any corpus would fill with title slides.
MIN_CHARS = 40

# A category with fewer than this many ground-truth items on the page is not
# scored for that category. One heading present or absent is a score of 0 or 1,
# and a corpus of those is a corpus of coin tosses.
MIN_ITEMS = 1

# How alike two blocks' texts must be before they are the same block. Measured
# on the normalised strings, so it is not sensitive to spacing or case. The
# value is deliberately low: the question a match answers is "did the tool find
# THIS block at all", and how well it read it is the text axis, which is scored
# separately. A high floor would report a badly-read heading as a missing one
# and count it twice.
MATCH_FLOOR = 0.6

WHITESPACE = re.compile(r"\s+")

# The markers a reader writes when it could not map a glyph. Struck from both
# sides before anything is compared, for the reason extraction_axes.py gives:
# they are a statement about the reader rather than about the page.
UNKNOWN_MARKER = "�"
MARKER_CATEGORIES = ("Cc", "Cf", "Co", "Cs", "Cn")
# THE ANNOTATION'S SUB- AND SUPERSCRIPT NOTATION (#1973). DocLayNet writes a
# lowered or raised run as `$_{1}$` and `$^{+}$` where the page reads `1` and
# `+`: 22,481 subscripts and 3,862 superscripts in the validation half alone,
# most of them in physics papers. Kept, every column that writes what the page
# shows paid five characters of edit distance a mark -- 0.013 of our 1-NED on
# validation, and up to 0.09 on one paper -- for a notation no reader sees.
# Only this notation: a `$` that is money (`$ 10,210 $`, `US$`) is text, and
# the braces must hold no `$` or brace, so nothing outside one mark is eaten.
SCRIPT_MARKUP = re.compile(r"\$[_^]\{([^{}$]*)\}\$")


def normalise(text: str) -> str:
    """One spelling of a string, so that two readings of one block compare.

    NFKC, because a ligature and its letters are the same text to a reader and
    the annotator typed whichever their keyboard produced. Whitespace collapsed,
    because a line break inside a paragraph is a property of the column width.
    Case is KEPT: a heading in capitals that we emit in lower case has lost
    something the page said, and a benchmark that folds case cannot see it.
    The annotation's `$_{1}$` and `$^{+}$` read as the characters they mark
    ([`SCRIPT_MARKUP`]), on every column alike.
    """
    # Until nothing changes: a mark inside a mark (`$_{P$_{h}$}$`) comes free
    # one layer at a time.
    while True:
        unmarked = SCRIPT_MARKUP.sub(r"\1", text)
        if unmarked == text:
            break
        text = unmarked
    text = unicodedata.normalize("NFKC", text)
    out = []
    for ch in text:
        if ch == UNKNOWN_MARKER:
            continue
        if not ch.isspace() and unicodedata.category(ch) in MARKER_CATEGORIES:
            continue
        out.append(ch)
    return WHITESPACE.sub(" ", "".join(out)).strip()


# ---------------------------------------------------------------------------
# Matching v2: dictionary-gated joining of in-word breaks (#574).
# ---------------------------------------------------------------------------
# DocLayNet's truth text comes from the PDF's own glyph cells. It keeps breaks
# inside words where the PDF draws a word in pieces: tracked display type
# ('PRINCIPAL A CTIVITIES', 'OVER VIEW', 'Thin king together') and, above all,
# line-break hyphenation, where the truth keeps 'indi vidual' and we emit the
# de-hyphenated word. On the 389-page proxy that is 689 pairs. v1 counts our
# correct words as errors on the text, words and headings axes, and a parser
# that emits pieces like the truth is favoured for it. v2 measures that, as an
# EXTRA column: v1 stays the reference (COS decision 03-10).
#
# The rule joins two or three adjacent alphabetic tokens into one where the
# joined form is a word of the lexicon, and it is applied to BOTH sides, for
# EVERY tool, in the same way. English pages only: the page flag is decided
# from the truth page, so it is the same for every tool.
LEXICON_DIR = pathlib.Path(__file__).resolve().parent / "lexicon"

_LEXICON_CACHE: dict[tuple[str, str], frozenset] = {}


def lexicon_meta() -> dict:
    """The pin beside the vendored word list: file, tag, commit and sha256."""
    return json.loads((LEXICON_DIR / "LEXICON.json").read_text(encoding="utf-8"))


def load_lexicon(path: str | pathlib.Path | None = None,
                 expected_sha: str | None = None) -> frozenset:
    """The vendored word list as a set, refused unless its sha256 matches.

    No download at run time: the list is committed beside this file and the
    digest is verified at every load, so a corrupted or substituted file is an
    error rather than a silently different dictionary. The set itself is cached
    by path and digest; the file is still read and hashed on every call.
    """
    resolved = pathlib.Path(path) if path is not None else LEXICON_DIR / lexicon_meta()["file"]
    digest = hashlib.sha256(resolved.read_bytes()).hexdigest()
    want = expected_sha if expected_sha is not None else lexicon_meta()["sha256"]
    if digest != want:
        raise ValueError(
            f"lexicon {resolved} has sha256 {digest}, want {want}: refusing to "
            "score against a word list that is not the pinned one")
    key = (str(resolved), digest)
    cached = _LEXICON_CACHE.get(key)
    if cached is None:
        cached = frozenset(resolved.read_text(encoding="utf-8").split())
        _LEXICON_CACHE[key] = cached
    return cached


@functools.lru_cache(maxsize=8)
def _lexicon_max_len(lexicon: frozenset) -> int:
    """The longest word, so the segmentation below knows where to stop."""
    return max((len(word) for word in lexicon), default=0)


# A token the join pass may touch: letters only. Punctuation attached to a
# word ('word,', 'e.g.') breaks a join rather than joining through it, which
# is the conservative direction: v2 joins in-word breaks, not phrases.
ALPHA_TOKEN = re.compile(r"[a-z]+")
# A piece of tracked display type. The pieces are usually one or two letters,
# but the annotation's cuts land mid-word wherever the glyph cells do
# ('P R I NC I PAL', 'D I R E CTOR S'), so the run rule allows up to four --
# and then demands that the run become FEWER tokens (below), which is what
# keeps ordinary short words ('of the best') from joining.
RUN_TOKEN = re.compile(r"[a-z]{1,4}")
# A page is English for v2 where at least this share of its truth's alphabetic
# tokens are lexicon words. Deterministic and langdetect-free.
ENGLISH_SHARE = 0.6
# A join has to make a word of at least this many letters: 'a' + 'I' is not a
# word no matter what the lexicon says about 'ai'.
MIN_JOIN_LEN = 4
# A segmentation piece has to be a word of at least this many letters. Single
# letters are what the pieces are made of, so a run containing one ('a', 'I',
# 'of', 'to') can only join as a whole word, never by re-segmentation.
MIN_SEGMENT_LEN = 3


def normalise_v2(text: str, lexicon: frozenset | set, english: bool = True) -> str:
    """`normalise`, then dictionary-gated joining of in-word breaks, case kept.

    The join DECISION reads the lowercased form -- 'PRINCIPAL' has to read as
    'principal' before 'A CTIVITIES' can join -- but the OUTPUT keeps the
    original characters: the joined token is the concatenation of the original
    pieces ('PRINCIPAL A CTIVITIES' -> 'PRINCIPAL ACTIVITIES'). v1 keeps case
    on purpose, and a v2 that folded it would forgive case errors and mix them
    into the join delta. The rule is the same for both sides and every tool,
    and `english=False` skips it entirely: the non-English page keeps v1's
    rules, byte for byte.
    """
    base = normalise(text)
    if not english:
        return base
    if not isinstance(lexicon, frozenset):
        lexicon = frozenset(lexicon)
    return _join_breaks(base, lexicon)


def _segment_all(text: str, lexicon: frozenset, max_len: int) -> list[str] | None:
    """`text` as lexicon words of at least `MIN_SEGMENT_LEN` letters, or None.

    Longest first word wins, then recursively: deterministic, because a run
    that segments two ways and takes a different one per side would join the
    truth against itself. Complete coverage or nothing -- a run that does not
    segment ('xqzw') is left unchanged by the caller.
    """
    end = len(text)
    # dp[i] is the segmentation of text[i:], or None where there is none.
    dp: list[list[str] | None] = [None] * (end + 1)
    dp[end] = []
    for i in range(end - 1, -1, -1):
        for size in range(min(max_len, end - i), MIN_SEGMENT_LEN - 1, -1):
            if text[i:i + size] in lexicon and dp[i + size] is not None:
                dp[i] = [text[i:i + size]] + dp[i + size]
                break
    return dp[0]


def _try_run(run: list[str], lexicon: frozenset, max_len: int) -> list[str] | None:
    """A run of piece-like tokens as one string, where it is a word or words.

    The decision reads the lowercased form; the output keeps the original
    characters. Joined where the whole string is a lexicon word ('DIRECTORS'
    via 'directors'), segmented where it falls completely into lexicon words
    ('PRINCIPALACTIVITIES' -> 'PRINCIPAL ACTIVITIES' via 'principalactivities'
    -> 'principal activities'), and None otherwise ('xqzw'). A segmentation
    only counts where the run becomes FEWER tokens: without that, an ordinary
    run of short words re-spells itself ('of the best' -> 'oft heb est' --
    all three in the lexicon) and v2 would rewrite text that has no breaks.
    """
    low = [token.lower() for token in run]
    joined_low = "".join(low)
    if joined_low in lexicon:
        return ["".join(run)]
    pieces = _segment_all(joined_low, lexicon, max_len)
    if pieces is not None and len(pieces) < len(run):
        joined_orig = "".join(run)
        if len(joined_orig) != len(joined_low):
            return None
        out: list[str] = []
        pos = 0
        for piece in pieces:
            out.append(joined_orig[pos:pos + len(piece)])
            pos += len(piece)
        return out
    return None


# A maximal run longer than this is left to the pair pass: the sub-run search
# below is quadratic in the run length, and a tracked word is a handful of
# pieces, not forty.
MAX_RUN_FOR_SUBRUNS = 40


def _run_pass(tokens: list[str], lexicon: frozenset, max_len: int) -> list[str]:
    """One string out of every run of piece-like tokens, where it is a word.

    This runs BEFORE the two/three-token pass, because that pass cannot start
    on single letters: 'ov' and 'ove' are not words, so 'O V E R V I E W'
    would never join. A run of three or more piece-like tokens is tried as one
    string, and where the whole run is no word, its sub-runs are tried
    longest-first: a tracked word inside a sentence glues its neighbours' short
    words onto its own run ('an o v e r v i e w of next page'), and only the
    middle of that run is the word. Piece-likeness reads the lowercased token;
    the joined output keeps the original characters. Left unchanged where
    nothing joins.
    """
    out: list[str] = []
    i = 0
    while i < len(tokens):
        j = i
        while j < len(tokens) and RUN_TOKEN.fullmatch(tokens[j].lower()):
            j += 1
        if j - i >= 3:
            out.extend(_join_run(tokens[i:j], lexicon, max_len))
            i = j
        else:
            out.append(tokens[i])
            i += 1
    return out


def _join_run(run: list[str], lexicon: frozenset, max_len: int) -> list[str]:
    """A maximal run with its joinable part joined, or unchanged.

    The whole run first; then contiguous sub-runs, longest first and leftmost
    first, joining the first that is a word or words and leaving the rest for
    the fixpoint to rescan. Deterministic: the same run always joins the same
    sub-run.
    """
    whole = _try_run(run, lexicon, max_len)
    if whole is not None:
        return whole
    if len(run) > MAX_RUN_FOR_SUBRUNS:
        return run
    for size in range(len(run) - 1, 2, -1):
        for start in range(0, len(run) - size + 1):
            joined = _try_run(run[start:start + size], lexicon, max_len)
            if joined is not None:
                return run[:start] + joined + run[start + size:]
    return run


def _pairs_join(left: str, right: str, lexicon: frozenset) -> bool:
    """Whether two adjacent tokens are one broken word.

    Joining needs a positive lexicon hit: 'thin' + 'king' joins because
    'thinking' is a word, and 'next' + 'page' stays two because 'nextpage' is
    not -- even though 'thin', 'king', 'next' and 'page' are all words
    themselves. That is the whole of the third clause of the rule: where the
    joined form is in the lexicon the pieces' own status does not matter. The
    test reads the lowercased tokens; the caller concatenates the originals.
    """
    low_left, low_right = left.lower(), right.lower()
    return (ALPHA_TOKEN.fullmatch(low_left) is not None
            and ALPHA_TOKEN.fullmatch(low_right) is not None
            and len(low_left) + len(low_right) >= MIN_JOIN_LEN
            and low_left + low_right in lexicon)


def _triples_join(first: str, second: str, third: str, lexicon: frozenset) -> bool:
    """The same for three tokens: 'ac' + 'h' is not a word, 'achieving' is."""
    low = (first.lower(), second.lower(), third.lower())
    return (all(ALPHA_TOKEN.fullmatch(t) is not None for t in low)
            and sum(len(t) for t in low) >= MIN_JOIN_LEN
            and "".join(low) in lexicon)


def _pair_pass(tokens: list[str], lexicon: frozenset) -> list[str]:
    """Adjacent pairs and triples into words, left to right, until nothing changes.

    Two tokens first and three where two do not join: 'ac h ieving' joins in
    one step where 'ach' is not a word, and in two ('ach ieving' ->
    'achieving') where it is.
    """
    changed = True
    while changed:
        changed = False
        out: list[str] = []
        i = 0
        while i < len(tokens):
            if i + 1 < len(tokens) and _pairs_join(tokens[i], tokens[i + 1], lexicon):
                out.append(tokens[i] + tokens[i + 1])
                i += 2
                changed = True
            elif (i + 2 < len(tokens)
                    and _triples_join(tokens[i], tokens[i + 1], tokens[i + 2], lexicon)):
                out.append(tokens[i] + tokens[i + 1] + tokens[i + 2])
                i += 3
                changed = True
            else:
                out.append(tokens[i])
                i += 1
        tokens = out
    return tokens


def _join_breaks(base: str, lexicon: frozenset) -> str:
    """The join passes over one normalised string, to a fixpoint, case kept.

    The run pass and the pair pass alternate until neither changes anything.
    They have to: a run the pair pass shortens at its edges ('thin king o v e
    r v i e w indi vidual' first joins at both ends) leaves a clean run of
    single letters behind it, which only a second run pass joins. Every round
    strictly shortens the token list or stops, so this terminates -- and a
    second call starts from the fixpoint, so normalise_v2 is idempotent. Each
    pass decides on the lowercased tokens and concatenates the originals.
    """
    tokens = base.split(" ")
    max_len = _lexicon_max_len(lexicon)
    while True:
        stepped = _pair_pass(_run_pass(tokens, lexicon, max_len), lexicon)
        if stepped == tokens:
            return " ".join(stepped)
        tokens = stepped


def page_is_english(texts: list[str], lexicon: frozenset | set) -> bool:
    """Whether a truth page's text is English enough for v2.

    At least `ENGLISH_SHARE` of the page's alphabetic tokens are lexicon
    words. Decided from the TRUTH page, so the flag -- and with it the rule --
    is the same for every tool on the page. A page with no alphabetic tokens
    at all is not English: there is nothing the lexicon could join.
    """
    alpha = [lower
             for text in texts
             for token in normalise(text).split(" ")
             if (lower := WORD_EDGE.sub("", token).lower())
             and ALPHA_TOKEN.fullmatch(lower)]
    if not alpha:
        return False
    return sum(1 for token in alpha if token in lexicon) / len(alpha) >= ENGLISH_SHARE


def edit_distance(left: str, right: str) -> int:
    """Levenshtein, exactly, by Myers' bit-parallel algorithm.

    Written out rather than imported: the rig runs on a machine bought for the
    round, and a metric that depends on a wheel from an index is a metric whose
    value can change between two runs without a commit.

    WHY NOT THE ROW-AT-A-TIME VERSION, which this used to be and which
    `_edit_distance_rows` below still is. A page of this benchmark holds two to
    four thousand characters, so one page against one tool is up to sixteen
    million Python-level operations -- about five seconds. Run
    structured-doclaynet-20260917T192519Z spent roughly thirty-five minutes of a
    PAID machine there, more than the two tools it was grading spent between
    them, and the money bought no measurement at all.

    Myers (1999) keeps the same dynamic-programming table and packs one column
    of it into machine words, so the work per column is a handful of big-integer
    operations instead of a loop over every row. Python's integers are already
    arbitrary-width, which is exactly what the algorithm wants. It is the SAME
    distance -- not an approximation and not a bound -- and `self_test` asserts
    that on random strings against the straightforward version, because "faster
    and subtly different" is the one outcome that would be worse than slow.
    """
    if left == right:
        return 0
    if not left:
        return len(right)
    if not right:
        return len(left)
    # The pattern goes in the bit vectors, so it is the shorter of the two: the
    # cost is O(len(text) * ceil(len(pattern)/word)), and the distance is
    # symmetric.
    if len(left) < len(right):
        left, right = right, left
    pattern, text = right, left
    size = len(pattern)
    # One bit mask per distinct character of the pattern: bit i is set where the
    # pattern's i-th character is this one.
    masks: dict[str, int] = {}
    for i, ch in enumerate(pattern):
        masks[ch] = masks.get(ch, 0) | (1 << i)
    full = (1 << size) - 1
    top = 1 << (size - 1)
    # `plus` and `minus` are the column's vertical deltas, one bit per row.
    plus, minus = full, 0
    distance = size
    for ch in text:
        equal = masks.get(ch, 0)
        vertical = equal | minus
        horizontal = ((((equal & plus) + plus) ^ plus) | equal) & full
        plus_h = (minus | ~(horizontal | plus)) & full
        minus_h = plus & horizontal
        if plus_h & top:
            distance += 1
        elif minus_h & top:
            distance -= 1
        plus_h = ((plus_h << 1) | 1) & full
        minus_h = (minus_h << 1) & full
        plus = (minus_h | ~(vertical | plus_h)) & full
        minus = plus_h & vertical
    return distance


def _edit_distance_rows(left: str, right: str) -> int:
    """The straightforward Levenshtein, one row at a time.

    Kept because it is the definition the fast one is checked against. Nothing
    in the measuring path calls it; `self_test` does, on random strings, and
    that is the whole of its job.
    """
    if left == right:
        return 0
    if not left:
        return len(right)
    if not right:
        return len(left)
    if len(left) < len(right):
        left, right = right, left
    previous = list(range(len(right) + 1))
    for i, lc in enumerate(left, start=1):
        current = [i]
        for j, rc in enumerate(right, start=1):
            current.append(min(
                previous[j] + 1,           # deletion
                current[j - 1] + 1,        # insertion
                previous[j - 1] + (lc != rc),  # substitution
            ))
        previous = current
    return previous[-1]


def normalised_edit_distance(left: str, right: str) -> float:
    """0 when the two strings are the same, 1 when nothing survives.

    Divided by the LONGER of the two, which is what bounds it at 1 and what
    makes an invented page cost as much as a lost one. Dividing by the ground
    truth alone -- which some published tables do -- lets a tool that emits the
    page three times score above 1 and be clipped back to it.
    """
    if not left and not right:
        return 0.0
    return edit_distance(left, right) / max(len(left), len(right))


def text_similarity(left: str, right: str) -> float:
    """The edit distance read as a score, so that every axis here is "up is
    better". The run file carries both spellings for the text axis, because the
    published tables of this benchmark family quote the distance."""
    return 1.0 - normalised_edit_distance(left, right)


def match_items(truth: list[str], produced: list[str], norm=normalise,
                ) -> list[tuple[int, int, float]]:
    """Pair ground-truth items with produced ones, greedily and in order.

    Greedy rather than optimal (Hungarian) on purpose: an optimal assignment
    over a page's headings can pair a heading with a distant one to buy a
    hundredth of a point elsewhere, and the pairing is then not the one a reader
    would make. Greedy in reading order pairs what a reader pairs, and on a page
    with two identical strings it takes the first -- which is the documented
    behaviour rather than an accident of dictionary order.

    `norm` prepares both sides; it defaults to today's `normalise`, and every
    caller already passes normalised strings, so the default re-normalises
    idempotently and v1 is bit-identical. The v2 column passes its own.
    """
    truth = [norm(t) for t in truth]
    produced = [norm(p) for p in produced]
    pairs: list[tuple[int, int, float]] = []
    taken: set[int] = set()
    for i, want in enumerate(truth):
        best: tuple[float, int] | None = None
        for j, got in enumerate(produced):
            if j in taken:
                continue
            score = text_similarity(want, got)
            if score < MATCH_FLOOR:
                continue
            if best is None or score > best[0]:
                best = (score, j)
        if best is not None:
            taken.add(best[1])
            pairs.append((i, best[1], best[0]))
    return pairs


def category_score(truth: list[str], produced: list[str], norm=normalise) -> dict:
    """Precision, recall and F1 over one category of block on one page."""
    truth = [prepared for t in truth if (prepared := norm(t))]
    produced = [prepared for p in produced if (prepared := norm(p))]
    if len(truth) < MIN_ITEMS:
        return {"scored": False, "why": "fewer ground-truth items than the floor",
                "truth": len(truth), "produced": len(produced)}
    pairs = match_items(truth, produced, norm=norm)
    hit = len(pairs)
    precision = hit / len(produced) if produced else 0.0
    recall = hit / len(truth)
    f1 = 0.0 if precision + recall == 0 else 2 * precision * recall / (precision + recall)
    # The mean similarity of the matched pairs, so that "found it" and "read it"
    # stay tellable apart: a tool can find every heading and mangle each one.
    read = sum(p[2] for p in pairs) / hit if hit else 0.0
    return {"scored": True, "truth": len(truth), "produced": len(produced),
            "matched": hit, "precision": round(precision, 6),
            "recall": round(recall, 6), "f1": round(f1, 6),
            "text_of_matched": round(read, 6)}


# ---------------------------------------------------------------------------
# Words, order-free (#1973).
# ---------------------------------------------------------------------------
# A word's own punctuation is not the word: `A ,` and `A,` are the same text,
# and a reader that attaches a comma differently has not read a word wrongly.
WORD_EDGE = re.compile(r"^[^\w]+|[^\w]+$")
# DocLayNet writes math with markup (`q$_{d}$`) where the page reads `qd`.
# Struck from both sides, so neither column is charged for the annotation's
# notation.
MATH_MARKUP = re.compile(r"[$_{}^]")


def words_of(text: str, norm=normalise) -> list[str]:
    """The words of a text as the order-free axis counts them."""
    out = []
    for word in MATH_MARKUP.sub("", norm(text)).split(" "):
        word = WORD_EDGE.sub("", word)
        if word:
            out.append(word)
    return out


def without_overlap(blocks: list[list[str]],
                    pages: list | None = None) -> list[list[str]]:
    """Each block's words without the part that repeats the end of the one before.

    DocLayNet's neighbouring list items share text -- `... (2004). 2` ends one
    item and opens the next -- because its cells are assigned by box, and a
    reference that counts those words twice charges every tool for words the
    page drew once. The longest run of two words or more that ends one block
    and opens the next is dropped from the second.

    ONLY ON ONE PAGE. `pages`, where given, is each block's page: a document
    read whole is its pages' blocks in order, and the last block of one page
    and the first of the next are two drawings, so words they share are two
    words (codex, #1976).
    """
    out: list[list[str]] = []
    for at, words in enumerate(blocks):
        same_page = pages is None or (at > 0 and pages[at] == pages[at - 1])
        if out and same_page:
            before = out[-1]
            k = next((k for k in range(min(len(before), len(words)), 1, -1)
                      if before[-k:] == words[:k]), 0)
            words = words[k:]
        out.append(words)
    return out


def bag_of_words_score(truth: list[str], produced: list[str],
                       furniture: list[str] | None = None,
                       recall_over: list[str] | None = None,
                       truth_pages: list | None = None,
                       norm=normalise) -> dict:
    """How much of the text is right, whatever order it came in.

    THE QUESTION THE OWNER ASKED ON 24-09 (#1973): how much of the text a tool
    extracts is actually the page's? The edit-distance axis answers it and the
    order at once, so a page read in another order and a page read wrongly
    look alike there. This counts words as a bag, per page: `matched` is, word
    by word, the smaller of how often the tool and the page have it.

      precision   matched / produced  -- falls with text doubled or invented
      recall      matched / truth     -- falls with text lost
      duplication produced beyond the page's own count of a word it does have
      invented    produced words the page does not have at all
      furniture   of those, words of the page's running heads and feet, which
                  are on the page but not in its body
      missing     1 - recall

    `truth` is the body blocks' texts, in the page's order, so that
    `without_overlap` can find the text two neighbours share, and
    `truth_pages` each block's page where `truth` spans several. `recall_over`,
    where given, is what recall counts against instead of `produced`: a set
    that annotates tables only asks whether a table's words were read at all,
    wherever the tool put them, and whether its table blocks hold anything else.
    """
    from collections import Counter
    truth_words = [w for ws in without_overlap([words_of(t, norm) for t in truth], truth_pages)
                   for w in ws]
    if len(truth_words) < MIN_ITEMS:
        return {"scored": False, "why": "the page has no body text to count"}
    page = Counter(truth_words)
    ours = Counter(w for t in produced for w in words_of(t, norm))
    heads = Counter(w for t in (furniture or []) for w in words_of(t, norm))
    matched = sum(min(n, page[w]) for w, n in ours.items())
    n_ours, n_page = sum(ours.values()), sum(page.values())
    duplicated = sum(n - page[w] for w, n in ours.items() if page[w] and n > page[w])
    strays = {w: n for w, n in ours.items() if not page[w]}
    in_heads = sum(min(n, heads[w]) for w, n in strays.items())
    invented = sum(strays.values()) - in_heads
    precision = matched / n_ours if n_ours else 0.0
    if recall_over is not None:
        wherever = Counter(w for t in recall_over for w in words_of(t, norm))
        recall = sum(min(n, page[w]) for w, n in wherever.items()) / n_page
    else:
        recall = matched / n_page
    f1 = 0.0 if precision + recall == 0 else 2 * precision * recall / (precision + recall)
    share = (lambda n: round(n / n_ours, 6)) if n_ours else (lambda n: 0.0)
    return {"scored": True, "truth": n_page, "produced": n_ours, "matched": matched,
            "precision": round(precision, 6), "recall": round(recall, 6),
            "f1": round(f1, 6), "duplication": share(duplicated),
            "invented": share(invented), "furniture": share(in_heads),
            "missing": round(1 - recall, 6)}


def text_score(truth: str, produced: str, norm=normalise) -> dict:
    """The page's running text, as a distance and as a similarity."""
    truth_n, produced_n = norm(truth), norm(produced)
    if len(truth_n) < MIN_CHARS:
        return {"scored": False, "why": "ground-truth text shorter than the floor",
                "chars": len(truth_n)}
    distance = normalised_edit_distance(truth_n, produced_n)
    return {"scored": True, "chars": len(truth_n),
            "edit_distance": round(distance, 6),
            "similarity": round(1.0 - distance, 6)}


# ---------------------------------------------------------------------------
# Tables: TEDS.
# ---------------------------------------------------------------------------
class _Node:
    """One cell or one row of a table, as a tree node.

    `tag` is the element, `span` the (colspan, rowspan) pair and `text` the
    normalised cell text. The three are compared together, because a table whose
    cells hold the right text under the wrong spans is a table a RAG pipeline
    will read wrongly, and TEDS exists to say so.
    """

    __slots__ = ("tag", "span", "text", "children")

    def __init__(self, tag: str, span: tuple[int, int] = (1, 1), text: str = "") -> None:
        self.tag = tag
        self.span = span
        self.text = text
        self.children: list[_Node] = []

    def size(self) -> int:
        return 1 + sum(c.size() for c in self.children)


class _TableParser(html.parser.HTMLParser):
    """The subset of HTML a table is: rows, cells, spans and their text.

    Anything else -- a `<b>` inside a cell, a `<thead>` around the rows -- is
    flattened rather than kept. Formatting inside a cell is not structure, and
    a tree that carried it would score two readings of one table apart for a
    difference no reader of the table can see.
    """

    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.root = _Node("table")
        self.row: _Node | None = None
        self.cell: _Node | None = None
        self.buffer: list[str] = []

    def handle_starttag(self, tag: str, attrs: list) -> None:
        if tag == "tr":
            self._close_cell()
            self.row = _Node("tr")
            self.root.children.append(self.row)
        elif tag in ("td", "th"):
            self._close_cell()
            values = dict(attrs)
            span = (_as_int(values.get("colspan"), 1), _as_int(values.get("rowspan"), 1))
            if self.row is None:
                # A cell outside a row is malformed HTML, and a tool that emits
                # it has still emitted a cell. Rather than dropping it -- which
                # would flatter that tool -- it gets a row of its own.
                self.row = _Node("tr")
                self.root.children.append(self.row)
            self.cell = _Node(tag, span)
            self.buffer = []

    def handle_endtag(self, tag: str) -> None:
        if tag in ("td", "th"):
            self._close_cell()
        elif tag == "tr":
            self._close_cell()
            self.row = None

    def handle_data(self, data: str) -> None:
        if self.cell is not None:
            self.buffer.append(data)

    def _close_cell(self) -> None:
        if self.cell is not None and self.row is not None:
            self.cell.text = normalise("".join(self.buffer))
            self.row.children.append(self.cell)
        self.cell = None
        self.buffer = []

    def close(self) -> None:  # type: ignore[override]
        super().close()
        self._close_cell()


def _as_int(value, fallback: int) -> int:
    try:
        return max(1, int(str(value)))
    except (TypeError, ValueError):
        return fallback


def parse_table(markup: str) -> _Node:
    parser = _TableParser()
    parser.feed(markup or "")
    parser.close()
    return parser.root


def _label(node: _Node) -> tuple:
    return (node.tag, node.span, node.text)


def _tree_edit_distance(left: _Node, right: _Node) -> float:
    """Tree edit distance between two tables, as the TEDS paper defines it.

    The tables here are two levels deep -- rows under a table, cells under a
    row -- and the general Zhang-Shasha algorithm on a tree of that shape
    reduces to a sequence alignment of the rows, where the cost of aligning two
    rows is itself a sequence alignment of their cells. So that is what this
    computes, rather than the general algorithm: the same numbers on the shape
    these trees actually have, in a form the next reader can check by hand.

    The cost of substituting one cell for another is 0 when the tag, the spans
    AND the text agree, and 1 otherwise -- the "TEDS-struct" reading is the one
    that ignores the text, and this file reports both.
    """
    return _align(left.children, right.children, _row_cost)


def _row_cost(left: _Node, right: _Node) -> float:
    """What it costs to turn one row into another: its own label, plus the cells.

    The label term is zero on every table that parses -- both sides are `<tr>`
    with no span -- and it is written out anyway so that the two levels have one
    shape. The alternative, an extra term applied inside `_align` itself, was
    added to BOTH levels and so charged a cell with a wrong span twice.
    """
    return _cell_cost(left, right) + _align(left.children, right.children, _cell_cost)


def _cell_cost(left: _Node, right: _Node) -> float:
    """What it costs to turn one cell into another.

    GRADED ON THE TEXT, which is what TEDS means and what the first spelling of
    this got wrong: a one-character typo in a long cell cost exactly as much as
    a cell holding something else entirely, so a table read almost perfectly and
    a table read wrongly scored the same. (codex, #1922)

    The structure comes first and is not graded: a different tag or a different
    span is a cell the grid puts somewhere else, and no amount of matching text
    makes that the same cell. Where tag and span agree, the cost is the
    normalised edit distance between the two texts -- 0 when they read the same,
    1 when nothing survives, and in between when it is in between.
    """
    if left.tag != right.tag or left.span != right.span:
        return 1.0
    return normalised_edit_distance(left.text, right.text)


def _align(left: list[_Node], right: list[_Node], cost) -> float:
    """Sequence alignment with insertion and deletion priced by subtree size.

    Deleting a row costs the row and every cell in it, which is what keeps a
    tool that drops half a table from scoring like one that mistypes a cell.
    """
    rows = len(left) + 1
    cols = len(right) + 1
    table = [[0.0] * cols for _ in range(rows)]
    for i in range(1, rows):
        table[i][0] = table[i - 1][0] + left[i - 1].size()
    for j in range(1, cols):
        table[0][j] = table[0][j - 1] + right[j - 1].size()
    for i in range(1, rows):
        for j in range(1, cols):
            table[i][j] = min(
                table[i - 1][j] + left[i - 1].size(),
                table[i][j - 1] + right[j - 1].size(),
                table[i - 1][j - 1] + cost(left[i - 1], right[j - 1]),
            )
    return table[-1][-1]


def teds(truth_html: str, produced_html: str) -> float:
    """Tree edit distance similarity, in [0, 1], 1 when the tables agree.

    Normalised by the size of the LARGER tree, so that a tool which emits a
    fragment of the table and one which emits it three times are both penalised.
    An empty prediction against a real table is 0 rather than undefined.
    """
    want = parse_table(truth_html)
    got = parse_table(produced_html)
    biggest = max(want.size(), got.size())
    if biggest <= 1:
        # Two empty tables. Nothing was asked and nothing is claimed.
        return 1.0
    if want.size() <= 1 or got.size() <= 1:
        # One side produced no table at all. There is nothing to align, and the
        # alignment would report the empty side as a table with every row
        # deleted -- which on a two-row table is a score of 0.14 for having
        # emitted nothing. A tool that did not find the table scores zero.
        return 0.0
    distance = _tree_edit_distance(want, got)
    return max(0.0, 1.0 - distance / biggest)


def teds_struct(truth_html: str, produced_html: str) -> float:
    """TEDS with the cell texts struck out, which is the structure alone.

    Reported beside TEDS because the two answer different questions and a
    milestone that fixes one without the other should be able to see it: a table
    read into the right grid with the wrong characters is an OCR problem, and
    the same characters in the wrong grid is a structure problem.
    """
    return teds(_strip_text(truth_html), _strip_text(produced_html))


CELL_TEXT = re.compile(r"(<t[dh][^>]*>)(.*?)(</t[dh]>)", re.S | re.I)


def _strip_text(markup: str) -> str:
    return CELL_TEXT.sub(r"\1\3", markup or "")


def table_structure_score(truth: list[str], produced: list[str]) -> dict:
    """TEDS over a page's tables: how close the grids are, not just the words.

    WHY A PAGE AND NOT A TABLE. `teds()` grades one table against one table,
    which is the metric as its paper defines it and the unit a table corpus
    annotates. A page is what every other axis here scores, and a page may hold
    two tables or none -- so this is the layer that turns one into the other,
    and it is here rather than in the protocol because it is arithmetic and
    arithmetic belongs where the self-test can reach it.

    WHY EVERY ANNOTATED TABLE COUNTS AND A SPARE ONE DOES NOT.
    The mean is over the tables the ground truth holds, and a table the
    annotator drew that no produced table was paired with scores zero. That is
    the direction a table metric has to fail in: a tool that emits nothing is
    worse than a tool that emits a wrong grid, and a mean over "the tables we
    managed to pair" would rank them the other way round.

    A table a tool emitted where the annotator saw none is not folded into this
    mean either -- there is no grid to compare it with. It is counted
    (`produced`) beside the score, because a column whose count there is large
    has a precision problem this number cannot see, which is the same hole the
    category scores count `*_where_none_annotated` for.

    THE PAIRING IS BY THE SCORE ITSELF, greedily, best first. Matching on the
    cell text instead would pair on one of the two things being graded, and
    matching in page order would hand a tool that emitted its tables the other
    way round two zeroes for an ordering mistake the reading-order axis already
    charges for.
    """
    truth = [t for t in truth if t]
    if not truth:
        return {"scored": False, "why": "no table with cell structure is "
                                        "annotated on this page",
                "truth": 0, "produced": len(produced)}
    pairs: list[tuple[int, int, float]] = []
    for i, want in enumerate(truth):
        for j, got in enumerate(produced):
            pairs.append((i, j, teds(want, got)))
    pairs.sort(key=lambda p: -p[2])
    used_truth: set[int] = set()
    used_produced: set[int] = set()
    scores: dict[int, tuple[int, float]] = {}
    for i, j, score in pairs:
        if i in used_truth or j in used_produced or score <= 0.0:
            continue
        used_truth.add(i)
        used_produced.add(j)
        scores[i] = (j, score)
    per_table = [scores.get(i, (None, 0.0))[1] for i in range(len(truth))]
    struct = [teds_struct(truth[i], produced[scores[i][0]]) if i in scores else 0.0
              for i in range(len(truth))]
    return {"scored": True, "truth": len(truth), "produced": len(produced),
            "matched": len(scores),
            "teds": round(sum(per_table) / len(per_table), 6),
            "teds_struct": round(sum(struct) / len(struct), 6),
            "per_table": [round(v, 6) for v in per_table]}


# ---------------------------------------------------------------------------
# Reading order.
# ---------------------------------------------------------------------------
def reading_order_score(truth: list[str], produced: list[str]) -> dict:
    """How close the order we emitted blocks in is to the annotated order.

    The blocks are matched by text first -- the same greedy pairing the category
    axes use -- and the answer is then the share of PAIRS of matched blocks that
    both readings put the same way round: one minus the discordant pairs over
    every pair there is, which is Kendall's tau distance normalised.

    PAIRS, AND NOT AN EDIT DISTANCE OVER THE POSITIONS, and the reason is a
    defect the ratchet caught this axis in (#577). An edit distance divides by
    the LENGTH of the matched sequence, so the denominator shrinks when a tool
    finds fewer blocks -- and a tool that found one MORE block could lose half
    its score for it. On `ASX_MRM_2000.pdf` page `f59f736e...` three matched
    blocks in annotated sequence scored a perfect 1.000 and four, one of them a
    table the reader had newly grouped, scored 0.500: the axis charged for
    finding the table. A share of pairs cannot do that. Adding a block adds
    pairs, and a block the two readings agree about adds concordant ones, so
    finding a block can only raise the score or leave it alone; a block put in
    the wrong place costs the pairs it is wrong about and no more.

    What that gives up is the property the edit distance was chosen for: one
    block moved from the top of a page to the bottom inverts every pair it is
    in, where a reader would call it one move. That is the right charge for a
    block in the wrong PLACE and the wrong one for a block a tool merely found,
    and only the second of those is a defect the axis can create out of
    nothing.

    A page whose matched blocks number fewer than two has no pair to be wrong
    about and is not scored.
    """
    pairs = match_items([normalise(t) for t in truth], [normalise(p) for p in produced])
    if len(pairs) < 2:
        return {"scored": False, "why": "fewer than two blocks matched",
                "matched": len(pairs), "truth": len(truth), "produced": len(produced)}
    # Ours, in the order WE emitted them, expressed as the annotator's indices.
    ours = [i for i, _, _ in sorted(pairs, key=lambda p: p[1])]
    discordant = sum(
        1
        for a in range(len(ours))
        for b in range(a + 1, len(ours))
        if ours[a] > ours[b]
    )
    total = len(ours) * (len(ours) - 1) // 2
    score = 1.0 - discordant / total
    return {"scored": True, "matched": len(pairs), "truth": len(truth),
            "produced": len(produced), "pairs": total, "discordant": discordant,
            "score": round(max(0.0, score), 6)}


# ---------------------------------------------------------------------------
# The shape of a corpus.
# ---------------------------------------------------------------------------
def percentile(values: list, share: float) -> float | None:
    """The same percentile extraction_axes.py reports, spelt the same way, so
    that two tables about one corpus cannot disagree about what p5 means."""
    if not values:
        return None
    ordered = sorted(values)
    if len(ordered) == 1:
        return round(ordered[0], 6)
    position = share * (len(ordered) - 1)
    low = int(position)
    high = min(low + 1, len(ordered) - 1)
    weight = position - low
    return round(ordered[low] * (1 - weight) + ordered[high] * weight, 6)


def tail(rows: list[dict], key: str, share: float = 0.05, worst_is_low: bool = True) -> list[dict]:
    """The worst `share` of the pages on one axis, named.

    The tail is the point of the baseline: a mean over a thousand pages says
    whether to celebrate and never says what to fix. Rounded UP to at least one
    page, so a small set still has a tail.
    """
    scored = [r for r in rows if r.get(key) is not None]
    if not scored:
        return []
    count = max(1, int(round(len(scored) * share)))
    ordered = sorted(scored, key=lambda r: r[key], reverse=not worst_is_low)
    return ordered[:count]


def summarise(rows: list[dict], key: str, worst_is_low: bool = True) -> dict:
    """Mean, median, the two tail percentiles and the count, for one axis."""
    values = [r[key] for r in rows if r.get(key) is not None]
    if not values:
        return {"pages": 0}
    return {
        "pages": len(values),
        "mean": round(sum(values) / len(values), 6),
        "median": percentile(values, 0.5),
        "p5": percentile(values, 0.05),
        "p95": percentile(values, 0.95),
        "worst": round(min(values) if worst_is_low else max(values), 6),
    }


# ---------------------------------------------------------------------------
# The self-test. Every axis, on fixtures, with the failure each one exists to
# catch written next to it.
# ---------------------------------------------------------------------------
def self_test() -> int:
    failures: list[str] = []

    def check(name: str, got, want, why: str) -> None:
        if isinstance(want, float):
            ok = abs(got - want) < 1e-6
        else:
            ok = got == want
        if not ok:
            failures.append(f"{name}: got {got!r}, want {want!r} -- {why}")

    # --- the fast distance IS the distance --------------------------------
    # The one property that makes Myers usable here. Random strings over a small
    # alphabet, which is where an off-by-one in the carry hides, and the two
    # implementations must agree on every pair.
    import random
    random.seed("pdfluent-structured-axes")
    alphabet = "abcXY "
    disagreements = 0
    for _ in range(400):
        a = "".join(random.choice(alphabet) for _ in range(random.randint(0, 40)))
        b = "".join(random.choice(alphabet) for _ in range(random.randint(0, 40)))
        if edit_distance(a, b) != _edit_distance_rows(a, b):
            disagreements += 1
    check("the bit-parallel distance is the textbook distance", disagreements, 0,
          "faster and subtly different is worse than slow: every page of every "
          "round would move for a reason no commit explains")

    # --- normalisation ---------------------------------------------------
    check("normalise collapses whitespace", normalise("a \n b\t c"), "a b c",
          "a line break inside a paragraph is the column width, not the text")
    check("normalise reads a subscript as its character", normalise("q$_{d}$ and P$_{h}$$_{1}$"),
          "qd and Ph1", "the annotation's notation for a lowered run is not text the page shows")
    check("normalise reads a superscript as its character", normalise("e$^{+}$e$^{-}$"),
          "e+e-", "the annotation's notation for a raised run is not text the page shows")
    check("normalise unpicks a mark inside a mark", normalise("$_{P$_{h}$}$"), "Ph",
          "stacked marks come free one layer at a time")
    check("normalise leaves money alone", normalise("$ 10,210 $ and US$ 5"),
          "$ 10,210 $ and US$ 5", "a dollar sign that is not the notation is text")
    check("the same page scores 1.0 whichever side carries the notation",
          text_score("the e$^{+}$e$^{-}$ pair and q$_{d}$ " * 5,
                     "the e+e- pair and qd " * 5)["similarity"], 1.0,
          "a truth in notation against a column in characters is one text")
    check("a table cell reads the notation as its character", teds(
              "<table><tr><td>H$_{2}$O</td><td>$ 10,210 $</td></tr></table>",
              "<table><tr><td>H2O</td><td>$ 10,210 $</td></tr></table>"), 1.0,
          "a cell is normalised like a page, so TEDS is not charged for it either")
    check("a table cell keeps its money", teds(
              "<table><tr><td>H2O</td><td>$ 10,210 $</td></tr></table>",
              "<table><tr><td>H2O</td><td>10,210</td></tr></table>") < 1.0, True,
          "the notation rule must not strip a dollar that is money from a cell")
    check("normalise keeps case", normalise("Heading"), "Heading",
          "a heading emitted in lower case has lost what the page said")
    check("normalise strikes the unknown marker", normalise("a�b"), "ab",
          "a replacement character is a statement about the reader")

    # --- matching v2: the lexicon -----------------------------------------
    # The dictionary behind the v2 column: SCOWL v2 American size 60, vendored
    # and pinned by sha256. Loaded once here; the bench loads it once per
    # process the same way.
    lexicon = load_lexicon()
    check("the lexicon is the pinned size-60 list", len(lexicon), 88750,
          "a derived list that changed size is a different dictionary")
    check("the pin names its release", lexicon_meta()["release"], "rel-2026.02.25",
          "the tag the list was derived from, beside its sha256")
    import tempfile
    with tempfile.TemporaryDirectory() as tmp:
        tampered = pathlib.Path(tmp) / "tampered.txt"
        tampered.write_text("\n".join(sorted(lexicon)) + "\nnotawordxyz\n")
        try:
            load_lexicon(tampered)
            failures.append("load_lexicon(tampered) returned instead of refusing "
                            "-- a substituted word list would silently re-score "
                            "every v2 column")
        except ValueError:
            pass

    # --- matching v2: the joins --------------------------------------------
    v2 = functools.partial(normalise_v2, lexicon=lexicon, english=True)
    check("tracked type joins", v2("Thin king together"), "Thinking together",
          "the PDF drew one word in pieces and the truth kept the break")
    check("a hyphenation break joins", v2("indi vidual"), "individual",
          "the truth keeps 'indi vidual' where we emit the word")
    check("a one-letter piece joins right", v2("PRINCIPAL A CTIVITIES"),
          "PRINCIPAL ACTIVITIES", "'a' + 'ctivities' is the word, not 'principala'")
    check("single letters join", v2("O V E R V I E W"), "OVERVIEW",
          "the two-token pass cannot start here ('ov' is not a word), so the "
          "run pass tries the whole string first")
    check("a tracked heading segments",
          v2("P R I NC I PAL ACT I V I T I E S"), "PRINCIPAL ACTIVITIES",
          "the run is not one word, but it falls completely into two")
    check("pieces of four letters still run", v2("D I R E CTOR S"), "DIRECTORS",
          "the annotation cuts mid-word wherever the glyph cells do")
    check("a run that is no word stays", v2("X Q Z W"), "X Q Z W",
          "'xqzw' neither is a word nor segments into any")
    check("a tracked word joins inside its sentence",
          v2("An O V E R V I E W of next page results follows"),
          "An OVERVIEW of next page results follows",
          "the run glues the neighbours' short words on; only its middle is "
          "the word, so sub-runs are tried longest-first")
    check("a glued run of words segments whole",
          v2("the P R I NC I PAL ACT I V I T I E S are many"),
          "the PRINCIPAL ACTIVITIES are many",
          "complete coverage into fewer tokens still counts with neighbours on")
    check("three pieces join at once", v2("Ac h ieving"), "Achieving",
          "'ach' is not a word, so the two-token pass yields to the three-token one")
    check("an ordinary heading is untouched", v2("Installing the pump"),
          "Installing the pump", "v2 joins breaks; text without any keeps its words")
    check("short words do not re-spell themselves", v2("of the best"), "of the best",
          "a run joins only where it becomes FEWER tokens -- 'oft heb est' "
          "are all words, and accepting them would rewrite unbroken text")
    # THE MUTATION ANCHOR. Every vector above pins the joined spelling, so a
    # normalise_v2 with its join pass removed (v1, case kept) goes red on each
    # joining vector -- and this one says why in a single line.
    check("a join makes fewer tokens", len(v2("Thin king together").split()), 2,
          "without the join pass the pieces stay three tokens")
    check("and the joined spelling differs from v1",
          v2("Thin king together") != normalise("Thin king together"), True,
          "v1 keeps the break; v2 is the column that does not")
    # CASE IS NEVER FOLDED. The join decision reads lowercased; the output
    # keeps the characters as written. A v2 that lowercased the string would
    # forgive case errors, and the v1 -> v2 delta would mix joins with case.
    check("v2 keeps a capitalised word", v2("Overview"), "Overview",
          "the join decision reads lowercased; the output keeps the case")
    check("and capitals stay capitals", v2("OVERVIEW"), "OVERVIEW", "")
    check("so Overview still mismatches OVERVIEW under v2",
          v2("Overview") != v2("OVERVIEW"), True,
          "a heading emitted in the wrong case has lost what the page said")
    check("exactly as under v1",
          text_similarity(v2("Overview"), v2("OVERVIEW"))
          == text_similarity(normalise("Overview"), normalise("OVERVIEW")), True,
          "the v2 column measures joins only; no other normalisation rides along")

    # --- matching v2: what must NOT join ------------------------------------
    check("two words stay two", v2("next page"), "next page",
          "joining needs a positive hit, and 'nextpage' is not a word")
    check("one word stays one", v2("nextpage"), "nextpage", "")
    check("so they still mismatch", v2("next page") != v2("nextpage"), True,
          "the truth's two tokens against the produced one is a real difference")
    check("a phrase of short words stays", v2("a first base clock"), "a first base clock",
          "no adjacent pair or triple of it is a word")
    check("and stays different from its joined twin",
          v2("a first base clock") != v2("afirst baseclock"), True, "")
    # THE KNOWN FALSE JOIN. 'a' + 'part' is a word, so v2 joins what v1 keeps
    # apart -- asymmetrically, since 'apart' was already one token. Amendment 3
    # counts these on the proxy rather than pretending the rule has none.
    check("the known false join joins", v2("a part"), "apart",
          "'apart' is a word and the rule is dictionary-gated, not mind-reading")

    # --- matching v2: symmetry and the English flag --------------------------
    broken = "the indi vidual proce dures of the company are set out below for all readers"
    whole = "the individual procedures of the company are set out below for all readers"
    check("v2 converges a broken page with its whole twin",
          text_score(broken, whole, norm=v2)["similarity"], 1.0,
          "both sides read through the same rule, so the break stops counting")
    check("v2 is symmetric",
          text_score(broken, whole, norm=v2)["similarity"]
          == text_score(whole, broken, norm=v2)["similarity"], True,
          "the same result whether a string stands on the truth or the produced side")
    check("english=False is v1 byte for byte",
          normalise_v2("Thin king TOGETHER", lexicon, english=False),
          normalise("Thin king TOGETHER"),
          "a non-English page falls back to v1 rules inside the v2 column")
    check("an English page is English",
          page_is_english(["The quick brown fox jumps over the lazy dog"], lexicon), True,
          "every token a lexicon word")
    check("a German page is not",
          page_is_english(["Der Hund bellt laut im Garten hinter dem Haus"], lexicon), False,
          "fewer than three fifths of its tokens are English words")
    check("a page with no words is not",
          page_is_english(["12 34 56", "1999-2003"], lexicon), False,
          "there is nothing the lexicon could join")

    # --- the norm hook leaves v1 alone ----------------------------------------
    check("normalise is idempotent",
          normalise(normalise("q$_{d}$  and\ne$^{+}$")) == normalise("q$_{d}$  and\ne$^{+}$"),
          True, "every match_items caller passes normalised strings already")
    check("normalise_v2 is idempotent",
          v2(v2("Thin king O V E R V I E W indi vidual")) == v2("Thin king O V E R V I E W indi vidual"),
          True, "a prepared string prepared again is the same string")
    check("match_items defaults to v1",
          match_items(["a b c"], ["a b c"]) == match_items(["a b c"], ["a b c"], norm=normalise),
          True, "")
    check("category_score defaults to v1",
          category_score(["Introduction"], ["Intraduction"])
          == category_score(["Introduction"], ["Intraduction"], norm=normalise), True, "")
    check("text_score defaults to v1",
          text_score("the quick brown fox", "the quick brown fox")
          == text_score("the quick brown fox", "the quick brown fox", norm=normalise),
          True, "")
    check("words_of defaults to v1",
          words_of("Rev. A , 63") == words_of("Rev. A , 63", norm=normalise), True, "")
    check("bag_of_words_score defaults to v1",
          bag_of_words_score(["one two three four"], ["one two"])
          == bag_of_words_score(["one two three four"], ["one two"], norm=normalise),
          True, "")

    # --- words, order-free (#1973) ----------------------------------------
    check("a word's punctuation is not the word", words_of("Rev. A , 63"),
          ["Rev", "A", "63"], "`A ,` and `A,` are one text")
    check("math markup is struck", words_of("q$_{d}$ = E$_{F}$"), ["qd", "EF"],
          "DocLayNet's notation, not the page's text")
    check("neighbours' shared words count once",
          without_overlap([["a", "b", "c", "2"], ["c", "2", "d", "e"]]),
          [["a", "b", "c", "2"], ["d", "e"]],
          "a word the page drew once is one word")
    check("but not across a page break",
          without_overlap([["a", "b", "c", "2"], ["c", "2", "d", "e"]], [0, 1]),
          [["a", "b", "c", "2"], ["c", "2", "d", "e"]],
          "the last block of one page and the first of the next are two drawings")
    same = bag_of_words_score(["The cat sat."], ["sat. The cat"])
    check("order does not count", (same["precision"], same["recall"]), (1.0, 1.0),
          "the axis asks whether the words are right, not where they stand")
    doubled = bag_of_words_score(["Less than"], ["Less thanLess than"])
    check("text drawn twice costs precision",
          (doubled["precision"], doubled["recall"]), (round(2 / 3, 6), 1.0),
          "`thanLess` is a word the page does not have")
    lost = bag_of_words_score(["one two three four"], ["one two"])
    check("lost text costs recall", (lost["recall"], lost["missing"]), (0.5, 0.5),
          "half the page is gone")
    twice = bag_of_words_score(["one two"], ["one one two"])
    check("a word once more is duplication", twice["duplication"], round(1 / 3, 6),
          "the page has one `one` and we wrote two")
    head = bag_of_words_score(["body text"], ["REPORT body text"], furniture=["REPORT"])
    check("a running head's words are furniture, not invented",
          (head["furniture"], head["invented"]), (round(1 / 3, 6), 0.0),
          "they are on the page, in another role")
    table = bag_of_words_score(["a b c d"], ["a b"], recall_over=["a b", "c d"])
    check("a table set's recall counts the words wherever they landed",
          (table["precision"], table["recall"]), (1.0, 1.0),
          "the words were read; that they left the table is the tables axis")

    # --- text ------------------------------------------------------------
    check("identical text scores zero distance",
          normalised_edit_distance("the quick brown fox", "the quick brown fox"), 0.0,
          "the axis must bottom out at agreement")
    check("an empty prediction scores one",
          normalised_edit_distance("the quick brown fox", ""), 1.0,
          "producing nothing is the worst possible reading")
    # The property that stops a tool buying a score by emitting more: the
    # divisor is the longer string, so three copies of the page cost.
    thrice = "abc abc abc"
    check("an inflated prediction is not free",
          round(normalised_edit_distance("abc", thrice), 6), round(8 / 11, 6),
          "dividing by the ground truth alone would let a tool emit the page twice")
    short = text_score("too short", "too short")
    check("a page under the floor is not scored", short["scored"], False,
          "one wrong character in twenty is a five percent score")

    # --- headings and lists ---------------------------------------------
    perfect = category_score(["Introduction", "Method"], ["Introduction", "Method"])
    check("every heading found scores one", perfect["f1"], 1.0,
          "the category axis must bottom out at agreement")
    none = category_score(["Introduction", "Method"], [])
    check("no heading found scores zero", none["f1"], 0.0,
          "a tool that emits no headings must not score for the ones it did not get wrong")
    greedy = category_score(["Introduction"], ["Introduction", "Method", "Results"])
    check("inventing headings costs precision", round(greedy["precision"], 6),
          round(1 / 3, 6), "calling every line a heading must not be free")
    check("inventing headings does not cost recall", greedy["recall"], 1.0,
          "precision and recall fail in different directions and are reported apart")
    misread = category_score(["Introduction"], ["Intraduction"])
    check("a nearly-right heading still matches", misread["matched"], 1,
          "how well a block was read is the text axis, not the finding axis")
    check("and its reading is recorded", misread["text_of_matched"] < 1.0, True,
          "a match at the floor must not read as a perfect transcription")

    # --- tables ----------------------------------------------------------
    one = "<table><tr><td>a</td><td>b</td></tr><tr><td>c</td><td>d</td></tr></table>"
    check("a table against itself is one", teds(one, one), 1.0,
          "TEDS must bottom out at agreement")
    check("a table against nothing is zero", teds(one, ""), 0.0,
          "an empty prediction claims nothing")
    moved = "<table><tr><td>b</td><td>a</td></tr><tr><td>c</td><td>d</td></tr></table>"
    check("two swapped cells cost", teds(one, moved) < 1.0, True,
          "a table whose cells are in the wrong place is read wrongly")
    check("and the structure alone does not", teds_struct(one, moved), 1.0,
          "TEDS-struct answers the grid question, TEDS the whole one")
    # GRADED, not pass/fail. A typo in a cell is not the same defect as a cell
    # holding somebody else's number, and a TEDS that cannot tell them apart is
    # not TEDS.
    typo = "<table><tr><td>alpha</td><td>b</td></tr><tr><td>c</td><td>d</td></tr></table>"
    other = "<table><tr><td>zzzzz</td><td>b</td></tr><tr><td>c</td><td>d</td></tr></table>"
    long_cell = ("<table><tr><td>alphb</td><td>b</td></tr>"
                 "<tr><td>c</td><td>d</td></tr></table>")
    check("a one-character typo costs less than a wrong cell",
          teds(typo, long_cell) > teds(typo, other), True,
          "TEDS grades cell content by edit distance where the grid agrees")
    check("and a cell that reads the same costs nothing",
          teds(typo, typo), 1.0, "")
    spanned = "<table><tr><td colspan=\"2\">a</td></tr><tr><td>c</td><td>d</td></tr></table>"
    check("a wrong span costs", teds(one, spanned) < 1.0, True,
          "a span is structure a RAG pipeline reads")
    # THE DOUBLE CHARGE. One cell with a wrong colspan is ONE substitution. The
    # first spelling of the alignment added a label term on top of the cell cost
    # at both levels, so that cell cost two -- which on a small table is the
    # difference between 0.857 and 0.714 and on a published table is a number
    # nobody can reproduce.
    one_span_wrong = ("<table><tr><td colspan=\"2\">a</td><td>b</td></tr>"
                      "<tr><td>c</td><td>d</td></tr></table>")
    plain = ("<table><tr><td>a</td><td>b</td></tr>"
             "<tr><td>c</td><td>d</td></tr></table>")
    check("one wrong span costs one substitution",
          round(teds(plain, one_span_wrong), 6), round(1 - 1 / 7, 6),
          "a label term on top of the cell cost charges the same cell twice")
    halved = "<table><tr><td>a</td><td>b</td></tr></table>"
    check("a dropped row costs its cells", round(teds(one, halved), 6), round(1 - 3 / 7, 6),
          "deleting a row must cost the row and everything in it")

    # --- TEDS over a PAGE of tables ---------------------------------------
    other = ("<table><tr><td>x</td><td>y</td></tr>"
             "<tr><td>z</td><td>w</td></tr></table>")
    page = table_structure_score([one, other], [other, one])
    check("two tables emitted the other way round still score",
          page["teds"], 1.0,
          "the pairing is by the score, so a table metric does not charge "
          "twice for an ordering mistake the reading-order axis already sees")
    missed = table_structure_score([one, other], [one])
    check("a table nobody emitted scores zero in the mean",
          round(missed["teds"], 6), 0.5,
          "a mean over the tables that were paired would rank a tool that "
          "emitted nothing above one that emitted a wrong grid")
    check("and the page says how many were paired", missed["matched"], 1, "")
    spare = table_structure_score([one], [one, other])
    check("a table the annotator never saw is counted, not averaged in",
          (spare["teds"], spare["produced"]), (1.0, 2),
          "there is no grid to compare it with, and a column with many of them "
          "has a precision problem this number cannot see")
    check("a page with no annotated table is not scored",
          table_structure_score([], [one])["scored"], False,
          "a page without a table says nothing about table structure, and "
          "scoring it would put a free 1.0 or a free 0.0 in the axis")
    check("the structure reading is reported beside the whole one",
          table_structure_score([one], [moved])["teds_struct"], 1.0,
          "the same characters in the wrong grid and the right grid with the "
          "wrong characters are two defects")

    # --- reading order ---------------------------------------------------
    blocks = ["alpha block", "beta block", "gamma block", "delta block"]
    check("the annotated order scores one",
          reading_order_score(blocks, blocks)["score"], 1.0,
          "the axis must bottom out at agreement")
    reversed_score = reading_order_score(blocks, list(reversed(blocks)))["score"]
    check("the reversed order scores badly", reversed_score < 0.5, True,
          "an inverted page is the failure this axis exists for")
    # THE PAIRING WITH THE TEXT AXIS. The same two texts, reversed, must score
    # perfectly on text and badly on order -- the property extraction_axes.py
    # asserts for its own pair, and for the same reason: an axis that can be
    # bought with another is not an axis (A11, #187).
    one_way = " ".join(blocks)
    other_way = " ".join(reversed(blocks))
    check("order and text are independent",
          (text_score(one_way, one_way)["similarity"] == 1.0
           and text_score(one_way, other_way)["similarity"] < 1.0
           and reversed_score < 0.5), True,
          "an axis that can be bought with another is not an axis (A11, #187)")
    single = reading_order_score(["only one block here"], ["only one block here"])
    check("one block has no order", single["scored"], False,
          "a page with one block cannot be in the wrong order")
    # FINDING A BLOCK MAY NOT COST A TOOL ITS SCORE, which is why this axis
    # counts pairs and not positions (#577). The tool that emits three of the
    # page's four blocks in the annotated order and the tool that emits all
    # four, with the fourth in the one place the annotation does not put it,
    # are the two columns here: under an edit distance over positions the
    # second scored half of the first, because the denominator is the length of
    # what was matched. The block is found either way, so the fuller reading
    # may not be the worse one.
    three_of_four = [blocks[0], blocks[2], blocks[3]]
    found_it_late = [blocks[0], blocks[2], blocks[3], blocks[1]]
    fewer = reading_order_score(blocks, three_of_four)["score"]
    fuller = reading_order_score(blocks, found_it_late)["score"]
    check("finding a block does not cost the score", fuller >= fewer - 1e-9, True,
          "an axis a tool can improve by emitting less is an axis that pays "
          "for losing the page")
    check("and a block in the wrong place costs the pairs it is wrong about",
          round(fuller, 6), round(1.0 - 2 / 6, 6),
          "the charge for a misplaced block is the pairs it inverts, not the "
          "length of the sequence it sits in")

    # --- the corpus shape ------------------------------------------------
    rows = [{"v": float(i)} for i in range(1, 101)]
    check("p5 is the fifth percentile", percentile([r["v"] for r in rows], 0.05), 5.95,
          "a tail percentile that is not the percentile makes every tail wrong")
    worst = tail(rows, "v", 0.05)
    check("the tail is the worst five percent", len(worst), 5,
          "the tail is what a baseline is for")
    check("and it is the low end", worst[0]["v"], 1.0,
          "a tail taken from the wrong end reports the pages that need no work")

    for line in failures:
        print(f"FAIL {line}", file=sys.stderr)
    if failures:
        print(f"\n{len(failures)} of the structured axes are wrong.", file=sys.stderr)
        return 1
    print("✓ structured axes: text, headings, lists, tables (TEDS) and reading "
          "order all measure what they claim")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true",
                        help="check the arithmetic of every axis and stop")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    parser.print_help()
    return 2


if __name__ == "__main__":
    sys.exit(main())
