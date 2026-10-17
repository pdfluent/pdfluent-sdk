#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Measure the four text-extraction axes per document, and write a run file (#367).

The four axes, and what each one cannot see -- the pairing is the point, because
a single axis can be improved by destroying what another one measures
(STRATEGY_2026H2 4.3, and the withdrawal of A11 that the rule comes from):

  parses           Our extractor returns text or a typed error within the
                   timeout. A hang is a failure, not a missing measurement.
                   Blind: a document that "parses" to an empty string passes.
  char_recall      Of the characters the oracle found, the share ours also
                   produced, as multiset containment. Tokenisation-immune, so a
                   reader that splits words differently does not move it. The
                   oracle's own "unknown glyph" markers are not characters it
                   found and are struck before the comparison (see
                   `oracle_multiset`). Blind: order, and anything we invent that
                   the oracle lacks.
  order_agreement  Per page: of the token pairs both readers agree exist exactly
                   once on that page, the share we put in the same relative
                   order, scored against whichever reader we agree with more.
                   A document's figure is the mean of its pages. Blind: text
                   that is missing entirely -- a document we extract three words
                   from can score a perfect order.
  extract_ms       Wall time to extract, best of three runs. Blind: correctness
                   of any kind; the fastest extractor returns nothing.

THE ORACLE, AND ITS BLIND SPOT. Two foreign readers, mutool (MuPDF) and
pdftotext (poppler). Per document the oracle is *the reader that found more
non-whitespace characters* -- but only while the two readers agree with each
other about what the document says. Taking the stronger of two is the harsher
test wherever both are reading the same text: it is the largest claim anybody
with a free tool can make about what the document contains, and we are measured
against that rather than against the reader that flatters us.

Where the readers disagree there is no single text to be the larger claim about,
and the volume rule is then winnable by the reader that invents: pdftotext reads
issue6127's non-embedded Identity-H font by shifting every code three places,
and it is the reader that found more, 11,319 characters against mutool's 11,027,
on a text of which 938 characters are ones mutool does not hold at all (#548).
Below READERS_AGREE_PCT the document is therefore scored against
the reader we agree with more, as the order axis has done per page since #428,
and the summary counts those documents because best-of-two can only flatter.

What that cannot see: both readers share failure modes. Neither reconstructs
text without a ToUnicode map any better than we do, both mangle Type 3 fonts,
and both read vertical CJK poorly. On such a document all three agree and recall
is 100% while every one of them is wrong. Recall against a foreign reader is a
statement about agreement, never about truth; only a document with known text
can say the latter, which is what the per-cause fixtures of #389 are for.

Documents whose oracle text holds fewer than 20 words are counted separately and
never averaged in, for the reason A12 records: on a two-word document a
percentage is noise with a reassuring number in front of it. The same floor
applies to order, page by page: a page of six words has no reading order worth a
percentage, and before #428 seven such documents sat in the order tail on three
to nine anchors.

WHY ORDER IS MEASURED PER PAGE. Kendall's tau over a whole document is dominated
by the pairs that span pages, and those are ordered correctly by anyone who emits
the pages in order. tracemonkey scores 98.3 over the document while every one of
its fourteen pages scores 73-94 against both readers, which agree with each other
at 99-100: the document-level number reports a two-column interleave that ruins
every page as a 1.7-point problem. The page is the unit a reader reads, so the
page is the unit this axis scores (#389 class O-A).

Usage:
    python3 scripts/extraction/extraction_axes.py \
        --set gate --corpus-dir /tmp/gate-corpus \
        --binary target/release/pdfluent-cli \
        --out benchmarks/runs/extraction/<run-id>.json
    python3 scripts/extraction/extraction_axes.py --self-test
"""

from __future__ import annotations

import argparse
import bisect
import datetime
import json
import pathlib
import re
import shutil
import subprocess
import sys
import time
import unicodedata
from collections import Counter


REPO = pathlib.Path(__file__).resolve().parents[2]
MANIFEST = REPO / "corpus" / "extraction" / "tuning.json"

# Below this many oracle words a percentage says nothing (A12, MIN_WOORDEN).
MIN_WORDS = 20

# A document that has not answered in this long has hung, and a hang is a
# failure of the "parses" axis rather than a measurement we failed to take.
TIMEOUT_S = 60

# A timeout under load is not a hang (#636). A document that normally takes
# four seconds timed out on a loaded machine, flipped `parsed` from true to
# false, and aborted a landing as a regression. So every document that times
# out is re-run once more, alone, with the same timeout; only a second timeout
# counts as "timeout". One and not more: a document that hangs twice is not
# waiting for a quieter machine, it hangs.
TIMEOUT_RETRIES = 1

# Three runs, and the minimum of the three. The minimum is the least noisy
# estimator of what the work actually costs: it is the run that lost the least
# time to everything else on the machine. A mean would import the load of
# whatever else the terminal was doing.
REPEATS = 3

WORD = re.compile(r"[0-9A-Za-zÀ-ɏ]{2,}")

# What a reader prints when it has no idea. mutool prints U+FFFD for a glyph with
# no Unicode mapping; pdftotext prints the byte it read, which lands in the C0/C1
# control range. Neither is a character the reader found, and counting them as
# oracle characters measures our agreement with an admission of ignorance:
# TAMReview's oracle is 50,731 replacement characters out of 58,837, which scored
# us at 13.8% on a document where we miss three real words (#389 class R-A).
UNKNOWN_MARKER = "\ufffd"

# Cc control characters, Cf formatting characters, Co private-use code points and
# Cn unassigned ones. Co is how a reader passes a symbol font's own code through:
# the glyph is real, the code point means nothing outside that font, and neither
# reader nor we can be right or wrong about it.
#
# Cn is here although #428's text names only the first three, because it is the
# same defect on the same document rather than a second decision: pdftotext
# prints the byte it read, and on TAMReview 102 of those bytes land on unassigned
# code points while the rest land in the control range. Striking one and keeping
# the other would clean half of one reader's raw bytes. Measured over the gate
# set, Cn occurs in exactly two reader texts (TAMReview 102 characters, one
# veraPDF font document 1).
#
# Cs (surrogates) is deliberately absent: a lone surrogate cannot survive the
# UTF-8 decode this rig does with errors="replace" -- it arrives as U+FFFD, which
# is struck above. Over the gate set there are zero of them, as there must be.
MARKER_CATEGORIES = frozenset(("Cc", "Cf", "Co", "Cn"))


def words(text: str) -> list:
    return WORD.findall(text.lower())


def char_multiset(text: str) -> Counter:
    """Non-whitespace characters. Whitespace is where readers disagree most and
    agree least meaningfully -- a line break placed differently is not lost text."""
    return Counter(c for c in text if not c.isspace())


def oracle_multiset(text: str) -> Counter:
    """The oracle's characters, minus the ones that say it did not know (#428).

    Struck from the oracle only. Ours is left whole on purpose: containment scores
    the oracle's characters, so anything we emit that the oracle does not hold is
    already invisible to it, and cleaning ours as well would only hide the case
    where a marker of ours matches a marker of theirs.
    """
    out: Counter = Counter()
    for ch in text:
        if ch.isspace() or ch == UNKNOWN_MARKER:
            continue
        if unicodedata.category(ch) in MARKER_CATEGORIES:
            continue
        out[ch] += 1
    return out


def page_split(text: str) -> list:
    """A reader's output as pages. Both write U+000C between them (pdftotext
    writes one after the last page as well, which is not a page of its own)."""
    pages = text.split("\f")
    while pages and not pages[-1].strip():
        pages.pop()
    return pages


def containment(oracle: Counter, ours: Counter) -> float:
    """Share of the oracle's characters we also produced, counting multiplicity.

    Containment rather than a length ratio, which is what text_retention_gate.py
    measures: a length ratio calls a document perfect when we produced the same
    number of characters as the oracle and every one of them is wrong.
    """
    total = sum(oracle.values())
    if total == 0:
        return -1.0
    kept = sum(min(n, ours.get(c, 0)) for c, n in oracle.items())
    return round(100.0 * kept / total, 3)


# Two readers that tell the same story disagree about a few characters: a
# ligature expanded or not, a hyphen at a line break, a header one of them
# repeats. Below this much agreement they are not telling the same story, and
# "whichever found more" stops being a tie-break between two readings of one
# text and becomes a choice of which reading is the document (#548).
#
# 95 and not a tighter number because the figure below is a minimum over both
# directions and is therefore already the harsher of the two: a reader that
# merely finds MORE than the other is capped by its own containment in the
# other.
#
# The line is placed where the corpus is empty. Measured over the 500-document
# gate set, 276 documents have two readers that both produced text; 254 of them
# agree at 99 or above and 15 fall below 95. The nearest measured values either
# side of the line are 92.5 and 96.509, so it sits in a four-point band with no
# document in it, and moving it anywhere inside that band changes nothing.
READERS_AGREE_PCT = 95.0


def reader_agreement(left: str, right: str) -> float:
    """How much of each other's text two readers found, the worse direction.

    Symmetric on purpose, and the minimum rather than the mean: a reader that
    emits everything the other did plus a text of its own contains the other
    completely, and a mean would call that pair 95 % agreed. What the
    recall axis needs to know is whether there is one text here or two, and the
    direction that says "no" is the one that answers it.

    -1.0 when either reader produced nothing to compare, which is not a
    disagreement: it is the absence of the question. That falls out of the
    minimum rather than being guarded for -- `containment` answers -1.0 for an
    empty left-hand side and -1.0 is the smallest thing this function can
    return -- and it is stated here because the two facts have to stay true
    together.
    """
    ours, theirs = oracle_multiset(left), oracle_multiset(right)
    return min(containment(ours, theirs), containment(theirs, ours))


def choose_oracle(candidates: list, our_text: str | None) -> dict:
    """Which reader the recall axis scores a document against, and under which rule.

    Until #548 this was one line: whichever reader found more non-whitespace
    characters. That is the harsher test wherever the two readers are reading
    the same text, and it is unwinnable by a reader that finds LESS than the
    truth -- which is the failure mode of a reader that cannot map a glyph.

    It is winnable by a reader that INVENTS. `issue6127` draws its text in a
    non-embedded Identity-H font; pdftotext decodes it by shifting every code
    three places, printing `YRLUQRWLFHHW` where the page draws `voir notice et`,
    and that is how it comes to be the reader that found more: 11,319 characters
    against mutool's 11,027, of which 938 are characters mutool does not hold at
    all. The document was then scored against a reading nobody claims is right
    -- 92.208, against 99.964 for the same run measured against mutool --
    and #430 and #437, which fixed OUR reading of that font, showed up in the
    baseline as recall falling.

    So the volume rule is kept where it is a tie-break and dropped where it is a
    verdict:

    * The readers agree with each other (`reader_agreement` at or above
      `READERS_AGREE_PCT`): whichever found more, exactly as before. This is
      261 of the gate set's 276 two-reader documents, so the axis keeps its
      meaning where its meaning was never in doubt.
    * They disagree: there is no single text to be measured against, so the
      document is scored against the reader we agree with more, and the choice
      is recorded per document together with the figure for the reader not
      chosen. This is the rule the order axis has used per page since #428, for
      the same reason and with the same admission: best-of-two can only flatter,
      which is why it is counted in the summary rather than folded in silently.

    A reader cannot win the second rule by inventing, because characters it made
    up are characters we do not have. It can only win it by being the reading we
    already produce -- which is the objection to best-of-two, and it is answered
    by counting the documents it applies to, not by pretending the volume rule
    was neutral.

    Returns the record fields the caller merges: which reader, the rule, the two
    readers' agreement, and (under best-of-two) what each reader would have
    scored us.
    """
    by_volume = max(candidates, key=lambda pair: sum(char_multiset(pair[1]).values()))
    if len(candidates) < 2:
        return {
            "oracle": by_volume[0],
            "oracle_text": by_volume[1],
            "oracle_rule": "volume",
            "readers_agreement": -1.0,
        }

    agreement = reader_agreement(candidates[0][1], candidates[1][1])
    chosen = by_volume
    rule = "volume"
    per_reader = None
    if 0 <= agreement < READERS_AGREE_PCT and our_text is not None:
        ours = char_multiset(our_text)
        per_reader = {
            name: containment(oracle_multiset(text), ours) for name, text in candidates
        }
        rule = "best-of-two"
        # Ties go to the reader the volume rule would have picked, so that a
        # document where both readers score us identically does not change its
        # oracle -- and with it its order rows -- on a coin toss.
        chosen = max(
            candidates,
            key=lambda pair: (per_reader[pair[0]], pair[0] == by_volume[0]),
        )
    record = {
        "oracle": chosen[0],
        "oracle_text": chosen[1],
        "oracle_rule": rule,
        "readers_agreement": agreement,
    }
    if per_reader is not None:
        record["recall_by_reader"] = per_reader
    return record


def anchor_positions(oracle_words: list, our_words: list) -> dict:
    """Where our output put each anchor.

    An anchor is a token that occurs exactly once in the oracle and exactly once
    in ours, so its position is unambiguous in each. Uniqueness is judged over
    the whole of both texts even when a single page is being scored: a token that
    appears once on page 1 and once on page 5 has no unambiguous position in our
    output either, and admitting it would score the page against a coin flip.
    """
    oracle_count = Counter(oracle_words)
    ours_count = Counter(our_words)
    anchors = {w for w in oracle_count if oracle_count[w] == 1 and ours_count.get(w) == 1}
    return {w: i for i, w in enumerate(our_words) if w in anchors}


def tau(sequence: list) -> float:
    """Kendall's tau-a of a permutation, on 0..100. -1 when there is no pair.

    Counting inversions with a sorted insert, O(n log n), which matters because a
    large document carries tens of thousands of anchors.
    """
    seen: list = []
    inversions = 0
    for value in reversed(sequence):
        position = bisect.bisect_left(seen, value)
        inversions += position
        seen.insert(position, value)
    n = len(sequence)
    pairs = n * (n - 1) // 2
    if pairs == 0:
        return -1.0
    return round(100.0 * (pairs - inversions) / pairs, 3)


def order_agreement(oracle_text: str, our_text: str) -> float:
    """Share of anchor pairs we put in the oracle's relative order, over the
    whole document.

    Kept because it is what a single-page document means and what the pages below
    are built from, and reported per document as `order_document_level` for
    continuity with the runs before #428. It is not the axis: over a multi-page
    document it is dominated by the pairs that span pages, which anybody who
    emits the pages in order gets right, and it therefore reports a page-by-page
    catastrophe as a rounding error.

    Only anchors are used, which is what makes this axis independent of recall:
    a document where we found half the text is judged on the half we share, and
    the missing half is recall's business. Fewer than two anchors means there is
    no order to agree about, and the axis reports -1 (not measured) rather than
    a flattering 100.
    """
    ours_words = words(our_text)
    oracle_words = words(oracle_text)
    positions = anchor_positions(oracle_words, ours_words)
    return tau([positions[w] for w in oracle_words if w in positions])


def order_per_page(oracle_text: str, our_text: str) -> list:
    """One order figure per page of the oracle, against our whole output.

    Our text is not re-extracted page by page. Splitting the input into
    single-page files means measuring a different document -- one rebuilt by a
    third-party writer, whose repairs of a broken page tree we would then be
    scored on -- while the anchors of one oracle page, read in the order our
    output puts them, already say whether we read that page in the reader's
    order. Text we moved to another page shows up here as the inversion it is.

    A page under MIN_WORDS oracle words is -1, not measured: below the A12 floor
    a percentage is noise, and it was such pages (3-9 anchors) that filled the
    order tail before #428.

    What this cannot see is the pages themselves in the wrong order: a document
    read backwards page by page scores 100 on every page. That is what the
    document-level figure is good at, which is why the run file keeps it beside
    this one as `order_document_level`.
    """
    our_words = words(our_text)
    positions = anchor_positions(words(oracle_text), our_words)
    scores = []
    for page in page_split(oracle_text):
        page_words = words(page)
        if len(page_words) < MIN_WORDS:
            scores.append(-1.0)
            continue
        scores.append(tau([positions[w] for w in page_words if w in positions]))
    return scores


def order_pages(readers: dict, our_text: str) -> list:
    """Per page: our agreement with each reader, the better of the two, and
    whether the two readers agreed with each other.

    The better of the two, because a page on which mutool (content-stream order)
    and pdftotext (reconstructed geometry) disagree with each other has no single
    reading order to be wrong about, and scoring us against the one that happens
    to have found more characters measures which reader won a coin toss. The
    reader-agreement column is what makes that visible instead of assumed: a page
    where the readers agree and we agree with neither is ours, and the summary
    counts those separately.
    """
    per_reader = {
        name: order_per_page(text, our_text) for name, text in readers.items() if text is not None
    }
    if not per_reader:
        return []
    mutool_text = readers.get("mutool")
    poppler_text = readers.get("poppler")
    between = (
        order_per_page(mutool_text, poppler_text)
        if mutool_text is not None and poppler_text is not None
        else []
    )

    rows = []
    for index in range(max(len(scores) for scores in per_reader.values())):
        row = {"page": index + 1}
        for name, scores in per_reader.items():
            row[name] = scores[index] if index < len(scores) else -1.0
        best = max(row.get(name, -1.0) for name in per_reader)
        if best < 0:
            continue
        row["best"] = best
        row["readers"] = between[index] if index < len(between) else -1.0
        rows.append(row)
    return rows


def mean_of_pages(rows: list) -> float:
    """A document's order figure: the mean of the pages that were measured.

    The mean and not the median: fourteen pages of a paper are fourteen chances
    to interleave two columns, and a median would let half of them fail in
    silence.
    """
    measured = [row["best"] for row in rows if row.get("best", -1.0) >= 0]
    if not measured:
        return -1.0
    return round(sum(measured) / len(measured), 3)


def run_tool(cmd: list, timeout: int = TIMEOUT_S) -> str | None:
    try:
        done = subprocess.run(
            cmd, capture_output=True, timeout=timeout, check=False
        )
    except (subprocess.TimeoutExpired, OSError):
        return None
    if done.returncode != 0:
        return None
    return done.stdout.decode("utf-8", "replace")


def mutool_text(pdf: pathlib.Path, tool: str) -> str | None:
    return run_tool([tool, "draw", "-F", "txt", "-o", "-", str(pdf)])


def poppler_text(pdf: pathlib.Path, tool: str) -> str | None:
    return run_tool([tool, "-q", "-enc", "UTF-8", str(pdf), "-"])


def our_text(pdf: pathlib.Path, binary: str) -> tuple:
    """(text | None, milliseconds, verdict).

    The verdict separates the two ways of not producing text, because they are
    different defects: `error` is our extractor saying so, which is the contract;
    `timeout` is it saying nothing at all, which is the one this axis exists for.
    """
    best_ms = None
    text = None
    verdict = "ok"
    for _ in range(REPEATS):
        started = time.perf_counter()
        try:
            done = subprocess.run(
                [binary, "extract-text", str(pdf)],
                capture_output=True,
                timeout=TIMEOUT_S,
                check=False,
            )
        except subprocess.TimeoutExpired:
            return None, None, "timeout"
        except OSError as exc:
            return None, None, f"unrunnable: {exc}"
        elapsed = (time.perf_counter() - started) * 1000.0
        best_ms = elapsed if best_ms is None else min(best_ms, elapsed)
        if done.returncode != 0:
            verdict = "error"
            text = None
            continue
        verdict = "ok"
        text = done.stdout.decode("utf-8", "replace")
    return text, (round(best_ms, 3) if best_ms is not None else None), verdict


def percentile(values: list, share: float) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    if len(ordered) == 1:
        return round(ordered[0], 3)
    position = share * (len(ordered) - 1)
    low = int(position)
    high = min(low + 1, len(ordered) - 1)
    weight = position - low
    return round(ordered[low] * (1 - weight) + ordered[high] * weight, 3)


def measure_one(pdf: pathlib.Path, binary: str, mutool: str, poppler: str) -> dict:
    text, ms, verdict = our_text(pdf, binary)

    from_mutool = mutool_text(pdf, mutool) if mutool else None
    from_poppler = poppler_text(pdf, poppler) if poppler else None

    candidates = []
    if from_mutool is not None:
        candidates.append(("mutool", from_mutool))
    if from_poppler is not None:
        candidates.append(("poppler", from_poppler))

    record: dict = {
        "parsed": verdict == "ok",
        "verdict": verdict,
        "extract_ms": ms,
    }

    if not candidates:
        # No oracle answered. Not a pass and not a failure of ours: it is an
        # unmeasured document, and it says so rather than scoring 100.
        record["char_recall"] = -1.0
        record["order_agreement"] = -1.0
        record["oracle"] = None
        record["why"] = "neither mutool nor poppler produced text"
        return record

    # Which reader this document is scored against, and under which of the two
    # rules -- `choose_oracle` carries the argument for both.
    #
    # Where the volume rule applies it counts the RAW characters, markers
    # included, which looks wrong and is not. Cleaning identifies the ignorance a
    # reader admits to -- U+FFFD, a control code -- and cannot identify the
    # ignorance it hides: pdftotext prints the byte it read, and a byte that
    # lands on a printable code point survives every category test there is.
    # Measured on TAMReview, the document this whole class is named after: mutool
    # emits 58,837 characters of which 8,106 survive cleaning, and those 8,106
    # are real text; pdftotext emits 57,130 of which 10,792 survive, and those
    # are raw bytes reinterpreted as Latin Extended-B and Greek. Choosing on the
    # cleaned count therefore hands the document to the reader with more
    # surviving noise and scores us 75.1 on a page we extract correctly, where
    # the raw count picks the reader whose remaining text is true and scores us
    # 100.
    #
    # So: the raw count decides which reader is the oracle, and the cleaned
    # multiset decides what that oracle is owed. The two questions are different.
    chosen = choose_oracle(candidates, text)
    oracle_text = chosen.pop("oracle_text")
    oracle_name = chosen["oracle"]
    oracle_words = words(oracle_text)
    record.update(chosen)
    record["oracle_chars"] = sum(oracle_multiset(oracle_text).values())
    record["oracle_marker_chars"] = (
        sum(char_multiset(oracle_text).values()) - record["oracle_chars"]
    )
    record["oracle_words"] = len(oracle_words)
    # Under twenty words a percentage is noise with a reassuring number in front
    # of it (A12). Zero words is the extreme of that case, not an exemption from
    # it: a document whose oracle yields characters but no words -- a page of
    # bare digits, a table of figures -- is exactly what the rule excludes.
    # Which bucket such a document lands in is decided in summarise(), by
    # precedence, so that the buckets partition the corpus without this flag
    # having to know about the recall value.
    record["short"] = len(oracle_words) < MIN_WORDS
    if len(candidates) == 2:
        other = [t for name, t in candidates if name != oracle_name][0]
        record["oracle_disagreement_chars"] = abs(
            sum(oracle_multiset(oracle_text).values())
            - sum(oracle_multiset(other).values())
        )

    if text is None:
        record["char_recall"] = 0.0 if record["oracle_chars"] else -1.0
        record["order_agreement"] = -1.0
        record["order_pages"] = []
        return record

    record["char_recall"] = containment(oracle_multiset(oracle_text), char_multiset(text))
    # The axis, per page and against the better of the two readers, and the whole
    # of the evidence behind it: which reader each page was scored against, and
    # whether the readers agreed with each other there.
    rows = order_pages(dict(candidates), text)
    record["order_pages"] = rows
    record["order_agreement"] = mean_of_pages(rows)
    record["order_pages_assessed"] = len(rows)
    # What the runs before #428 reported, kept so that the two can be compared
    # once. It is provenance, not an axis: nothing gates on it.
    record["order_document_level"] = order_agreement(oracle_text, text)
    record["our_chars"] = sum(char_multiset(text).values())
    return record


def summarise(documents: dict) -> dict:
    """The corpus-level shape a round reports (A22/A27), stated with denominators.

    Every figure names how many documents are behind it. A quiet exclusion is the
    cheapest route to a flattering number, which is the sentence A27 carries
    about its own tail.
    """
    total = len(documents)
    parsed = [d for d in documents.values() if d.get("parsed")]
    # Disjoint by construction, and asserted below: no document is counted twice
    # and none is dropped. A number whose denominator does not add up is the
    # cheapest way to publish a flattering median.
    no_source = [d for d in documents.values() if d.get("char_recall", -1) < 0]
    short = [
        d
        for d in documents.values()
        if d.get("char_recall", -1) >= 0 and d.get("short")
    ]
    assessable = [
        d
        for d in documents.values()
        if d.get("char_recall", -1) >= 0 and not d.get("short")
    ]
    assert len(no_source) + len(short) + len(assessable) == total, (
        "the recall buckets must partition the corpus"
    )
    recalls = [d["char_recall"] for d in assessable]
    orders = [
        d["order_agreement"]
        for d in documents.values()
        if d.get("order_agreement", -1) >= 0
    ]
    # Every measured page of every document, which is the population the order
    # axis is actually about. A document's own figure is the mean of its pages,
    # so a fourteen-page paper counts once there and fourteen times here: the
    # first says how bad that document is, the second how much of the corpus is.
    pages = [
        row
        for d in documents.values()
        for row in d.get("order_pages", [])
        if row.get("best", -1) >= 0
    ]
    page_scores = [row["best"] for row in pages]
    # The pages that are ours beyond argument: both readers found the same order
    # and we found neither. Counted apart because the rest of the tail is pages
    # without a single reading order, and averaging the two together is how a
    # defect hides behind an ambiguity.
    readers_agree_we_do_not = [
        row for row in pages if row.get("readers", -1) >= 95.0 and row["best"] < 95.0
    ]
    times = [d["extract_ms"] for d in documents.values() if d.get("extract_ms") is not None]
    return {
        "documents": total,
        "parsed": len(parsed),
        "failed_to_parse": total - len(parsed),
        "timed_out": sum(1 for d in documents.values() if d.get("verdict") == "timeout"),
        "recall_assessed": len(assessable),
        "recall_too_short": len(short),
        "recall_no_source_text": len(no_source),
        "recall_median_pct": percentile(recalls, 0.5),
        "recall_p5_pct": percentile(recalls, 0.05),
        "recall_below_95pct": sum(1 for r in recalls if r < 95.0),
        # The flattering half of #548, counted rather than folded in. Best-of-two
        # can only raise a number, so the only honest way to ship it is to say on
        # every run how many documents it was used on and how many documents had
        # two readers that disagreed at all. A round where these grow is a round
        # where the readers came apart, and that is a finding about the corpus,
        # not about us.
        "recall_readers_disagree": sum(
            1
            for d in documents.values()
            if 0 <= d.get("readers_agreement", -1) < READERS_AGREE_PCT
        ),
        "recall_scored_best_of_two": sum(
            1 for d in documents.values() if d.get("oracle_rule") == "best-of-two"
        ),
        "recall_readers_compared": sum(
            1 for d in documents.values() if d.get("readers_agreement", -1) >= 0
        ),
        "order_assessed": len(orders),
        "order_median_pct": percentile(orders, 0.5),
        "order_p5_pct": percentile(orders, 0.05),
        "order_pages_assessed": len(page_scores),
        "order_page_median_pct": percentile(page_scores, 0.5),
        "order_page_p5_pct": percentile(page_scores, 0.05),
        "order_pages_below_95": sum(1 for score in page_scores if score < 95.0),
        "order_pages_readers_agree_we_do_not": len(readers_agree_we_do_not),
        "order_documents_readers_agree_we_do_not": len(
            {
                name
                for name, d in documents.items()
                for row in d.get("order_pages", [])
                if row.get("readers", -1) >= 95.0 and 0 <= row.get("best", -1) < 95.0
            }
        ),
        "extract_ms_p50": percentile(times, 0.5),
        "extract_ms_p95": percentile(times, 0.95),
    }


def retry_timed_out(documents: dict, rerun) -> list:
    """Re-run every document that timed out, once, alone (#636).

    `rerun(name)` measures one document again with the same timeout and
    returns its record. Only verdict "timeout" is retried: an "error" is our
    extractor answering, which a quieter machine does not change.

    The retry is sequential, one document at a time, whatever the first pass
    did: the point is that the document no longer shares the machine with the
    rest of the corpus. The caller's `documents` dict is updated in place and
    the returned list -- one entry per retried document, with its outcome --
    is what the run file records under "timeout_retries", so a retry is
    visible and never silent.
    """
    retries: list = []
    for name, record in list(documents.items()):
        if record.get("verdict") != "timeout":
            continue
        second = rerun(name)
        retries.append(
            {
                "document": name,
                "first_verdict": "timeout",
                "second_verdict": second.get("verdict"),
                "parsed": bool(second.get("parsed")),
            }
        )
        second["retried"] = True
        second["first_verdict"] = "timeout"
        documents[name] = second
    return retries


# The two sets of #461, each in a manifest of its own because both are drawn by
# a rule and redrawn by a guard on every push; a regenerated set does not belong
# inside a file that is also maintained by hand.
OWN_MANIFEST = {
    "tuning1000": REPO / "corpus" / "extraction" / "EXTRACTION_TUNING_1000.json",
    "test300": REPO / "corpus" / "extraction" / "EXTRACTION_TEST_300.json",
}


def load_set(set_name: str) -> list:
    """The documents of one set, from whichever manifest holds it."""
    own = OWN_MANIFEST.get(set_name)
    if own is not None:
        if not own.is_file():
            raise SystemExit(f"set {set_name} names {own}, which is missing")
        with own.open(encoding="utf-8") as f:
            return json.load(f)["entries"]
    with MANIFEST.open(encoding="utf-8") as f:
        manifest = json.load(f)
    if set_name not in manifest["sets"]:
        raise SystemExit(
            f"unknown set {set_name}; the manifest has "
            f"{sorted(manifest['sets'])} and {sorted(OWN_MANIFEST)} have their own")
    return manifest["sets"][set_name]["entries"]


def platform_key() -> str:
    return "darwin" if sys.platform == "darwin" else "linux"


def measure(args) -> int:
    entries = load_set(args.set)
    corpus = pathlib.Path(args.corpus_dir)
    binary = args.binary
    if not pathlib.Path(binary).exists() and shutil.which(binary) is None:
        print(f"FAIL extractor {binary} not found; build it first")
        return 2

    mutool = shutil.which(args.mutool)
    poppler = shutil.which(args.poppler)
    if not mutool and not poppler:
        print("FAIL neither mutool nor pdftotext is on PATH; there is no oracle")
        return 2
    if not mutool or not poppler:
        # One oracle is a measurement with a narrower claim, not a broken run,
        # but the run file says so and the comparison across runs can see it.
        print(f"NOTE only one foreign reader available: {'mutool' if mutool else 'poppler'}")

    documents: dict = {}
    missing = []
    for i, entry in enumerate(entries, 1):
        pdf = corpus / entry["name"]
        if not pdf.exists():
            missing.append(entry["name"])
            continue
        documents[entry["name"]] = measure_one(pdf, binary, mutool, poppler)
        if args.progress and i % 25 == 0:
            print(f"  {i}/{len(entries)}", flush=True)

    if missing:
        print(f"FAIL {len(missing)} document(s) of set {args.set} are not in {corpus}")
        print(f"     first missing: {', '.join(missing[:3])}")
        return 2

    # Every timeout, once more, alone (#636). The first pass shares the machine
    # with the rest of the corpus; the second runs one document at a time with
    # the same timeout, so a document that only timed out under load recovers
    # and only a second timeout counts as "timeout".
    def rerun(name: str) -> dict:
        return measure_one(corpus / name, binary, mutool, poppler)

    timeout_retries: list = []
    for _ in range(TIMEOUT_RETRIES):
        timeout_retries.extend(retry_timed_out(documents, rerun))

    run = {
        "run_id": args.run_id,
        "capability": "extraction",
        "set": args.set,
        "tuning_or_holdout": "tuning",
        "date": datetime.date.today().isoformat(),
        "platform": platform_key(),
        "machine_class": args.machine_class,
        "commit": args.commit,
        "timing": {
            "repeats": REPEATS,
            "statistic": "minimum",
            "quiet_machine": bool(args.quiet_machine),
            "why": (
                "The minimum of the repeats, because it is the run that lost the "
                "least time to whatever else the machine was doing. A later run "
                "with a different number of repeats is not comparable with this "
                "one, which is why the count is recorded rather than assumed."
            ),
        },
        "tools": {
            "mutool": args.mutool if mutool else None,
            "poppler": args.poppler if poppler else None,
            "extractor": binary,
        },
        "oracle": (
            "per document: whichever of mutool and pdftotext produced more "
            "non-whitespace characters where the two agree with each other to "
            f"within {READERS_AGREE_PCT}%, and where they do not, whichever of "
            "the two we agree with more, with the choice recorded per document "
            "(#548)"
        ),
        "summary": summarise(documents),
        "documents": documents,
        "timeout_retries": timeout_retries,
    }
    out = pathlib.Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    with out.open("w", encoding="utf-8") as f:
        json.dump(run, f, indent=1, sort_keys=True)
        f.write("\n")
    s = run["summary"]
    print(f"OK wrote {out}")
    print(
        f"   parsed {s['parsed']}/{s['documents']} "
        f"(timed out {s['timed_out']}); "
        f"recall median {s['recall_median_pct']} p5 {s['recall_p5_pct']} "
        f"over {s['recall_assessed']} assessed "
        f"({s['recall_too_short']} too short, {s['recall_no_source_text']} no source text), "
        f"{s['recall_below_95pct']} below 95%; "
        f"{s['recall_scored_best_of_two']} scored best-of-two of "
        f"{s['recall_readers_disagree']} where the readers disagree "
        f"(of {s['recall_readers_compared']} with two readers)"
    )
    print(
        f"   order per document median {s['order_median_pct']} p5 {s['order_p5_pct']} "
        f"over {s['order_assessed']}; "
        f"per page median {s['order_page_median_pct']} p5 {s['order_page_p5_pct']} "
        f"over {s['order_pages_assessed']} pages, "
        f"{s['order_pages_below_95']} below 95% of which "
        f"{s['order_pages_readers_agree_we_do_not']} where the readers agree and we do not "
        f"({s['order_documents_readers_agree_we_do_not']} documents)"
    )
    print(
        f"   extract p50 {s['extract_ms_p50']} ms p95 {s['extract_ms_p95']} ms"
    )
    if timeout_retries:
        recovered = sum(1 for r in timeout_retries if r["parsed"])
        print(
            f"   retried {len(timeout_retries)} timed-out document(s) alone, "
            f"{recovered} recovered"
        )
    if not args.quiet_machine:
        print(
            "   NOTE timings measured on a machine that was not declared quiet; "
            "they are recorded and will not be judged. Pass --quiet-machine on a "
            "machine running nothing else."
        )
    return 0


def self_test() -> int:
    """Every axis must be able to go down, and must not move when it should not."""
    failures = []

    def check(label, got, want):
        if got != want:
            failures.append(f"{label}: got {got}, expected {want}")

    # Containment sees substitution, which a length ratio cannot.
    check("perfect recall", containment(char_multiset("hello"), char_multiset("hello")), 100.0)
    check(
        "substituted text of the same length",
        containment(char_multiset("hello"), char_multiset("xxxxx")),
        0.0,
    )
    check(
        "half the characters",
        containment(char_multiset("aabb"), char_multiset("ab")),
        50.0,
    )
    check(
        "noise in ours is not penalised by recall",
        containment(char_multiset("ab"), char_multiset("abzzzz")),
        100.0,
    )
    check("no oracle characters", containment(char_multiset("  "), char_multiset("x")), -1.0)

    # Order is independent of recall: same tokens, reversed, is nought.
    check("same order", order_agreement("alpha beta gamma", "alpha beta gamma"), 100.0)
    check("reversed order", order_agreement("alpha beta gamma", "gamma beta alpha"), 0.0)
    check(
        "one swap of three",
        order_agreement("alpha beta gamma", "beta alpha gamma"),
        round(100.0 * 2 / 3, 3),
    )
    check(
        "too few anchors to judge",
        order_agreement("alpha", "alpha"),
        -1.0,
    )
    check(
        "recall perfect, order destroyed",
        order_agreement("alpha beta gamma delta", "delta gamma beta alpha"),
        0.0,
    )
    # And the pairing that justifies having both axes: the same pair of texts
    # scores 100 on recall and 0 on order.
    if containment(
        char_multiset("alpha beta gamma delta"), char_multiset("delta gamma beta alpha")
    ) != 100.0:
        failures.append("the reversed text should still be perfect recall")

    # --- #428, the three the ticket names -------------------------------------
    # Each is a way the rig reported a comfortable number while being wrong, and
    # each is written as the mutation that brings the comfortable number back.

    # 1. Two pages, each reversed, the pages themselves in order. The
    # document-level tau calls this a good document because every pair that
    # spans the two pages is right; per page it is nought, twice.
    page_one = " ".join(f"w{n:02d}" for n in range(1, 26))
    page_two = " ".join(f"x{n:02d}" for n in range(1, 26))
    oracle_two_pages = f"{page_one}\f{page_two}"
    ours_reversed_within_pages = (
        " ".join(reversed(page_one.split())) + "\n\n" + " ".join(reversed(page_two.split()))
    )
    per_page = order_per_page(oracle_two_pages, ours_reversed_within_pages)
    check("both pages are measured", len(per_page), 2)
    for number, score in enumerate(per_page, 1):
        if not 0 <= score < 50:
            failures.append(
                f"page {number} of two reversed pages: got {score}, expected under 50"
            )
    # The same document read whole: half its pairs span the two pages and every
    # one of them is right, so the figure the runs before #428 reported is fifty
    # points above the truth. Ten pages of the same defect -- tracemonkey's
    # shape -- and the cross-page pairs bury it above ninety.
    whole = order_agreement(oracle_two_pages, ours_reversed_within_pages)
    if whole < 50.0:
        failures.append(
            f"the document-level figure on two reversed pages: got {whole}, expected "
            "at least 50 -- if it is not, this fixture no longer demonstrates what "
            "per page is for"
        )
    ten_pages = [
        " ".join(f"p{page}w{n:02d}" for n in range(1, 26)) for page in range(10)
    ]
    ours_ten = "\n\n".join(" ".join(reversed(page.split())) for page in ten_pages)
    oracle_ten = "\f".join(ten_pages)
    buried = order_agreement(oracle_ten, ours_ten)
    if buried <= 90.0:
        failures.append(
            f"ten reversed pages read whole: got {buried}, expected over 90 -- the "
            "number this axis stopped reporting"
        )
    if any(score != 0.0 for score in order_per_page(oracle_ten, ours_ten)):
        failures.append("every one of the ten reversed pages should score nought")

    # 2. The reader's own "I do not know" is not a character it found.
    check(
        "the oracle's replacement characters are not ours to miss",
        containment(oracle_multiset("ab\ufffd\ufffd"), char_multiset("ab")),
        100.0,
    )
    check(
        "and neither are its control codes",
        containment(oracle_multiset("ab\x83\x90"), char_multiset("ab")),
        100.0,
    )
    check(
        "nor the code points Unicode has not assigned",
        containment(oracle_multiset("ab\u0378\u0380"), char_multiset("ab")),
        100.0,
    )
    check(
        "while a character it really found still counts",
        containment(oracle_multiset("ab\ufffdc"), char_multiset("ab")),
        round(100.0 * 2 / 3, 3),
    )

    # 3. Under the A12 floor a page has no reading order worth a percentage.
    short_page = "alpha beta gamma delta epsilon"
    check(
        "a five-word page is not measured",
        order_per_page(f"{short_page}\f{page_one}", short_page + " " + page_one)[0],
        -1.0,
    )

    # --- and the rest of #428's shape -----------------------------------------
    check("a trailing form feed is not a page", len(page_split("a\fb\f")), 2)
    check("a single-page document is one page", len(page_split("a b c")), 1)

    rows = order_pages(
        {"mutool": oracle_two_pages, "poppler": f"{page_one}\f{page_two}"},
        ours_reversed_within_pages,
    )
    check("every page carries a row", len(rows), 2)
    check("and the readers' own agreement", rows[0]["readers"], 100.0)
    check("the document figure is the mean of its pages", mean_of_pages(rows), 0.0)
    check(
        "a document with no measured page is not measured",
        mean_of_pages([{"page": 1, "best": -1.0}]),
        -1.0,
    )
    # The mean, not the best page and not the median: one good page must not pay
    # for a bad one, which is the whole complaint against the document-level tau.
    check(
        "one good page does not carry a bad one",
        mean_of_pages([{"page": 1, "best": 100.0}, {"page": 2, "best": 0.0}]),
        50.0,
    )
    check(
        "and an unmeasured page is not counted as nought",
        mean_of_pages(
            [{"page": 1, "best": 100.0}, {"page": 2, "best": -1.0},
             {"page": 3, "best": 50.0}]
        ),
        75.0,
    )
    # Best of the two readers, because a page the readers order differently has
    # no single right answer to be wrong about.
    disagreeing = order_pages(
        {"mutool": page_one, "poppler": " ".join(reversed(page_one.split()))},
        page_one,
    )
    check("scored against the reader we agree with", disagreeing[0]["best"], 100.0)
    check("and the disagreement is on the record", disagreeing[0]["readers"], 0.0)

    # --- #548, the oracle that a reader can win by inventing ------------------
    # issue6127 in miniature. The right reading, the shifted reading that is
    # longer because the shift invents, and ours, which is the right reading.
    right = "voir notice et " * 8
    shifted = "".join(
        chr(ord(ch) + 3) if ch.isalpha() else ch for ch in right
    ) + "phantom characters that nobody drew"
    check(
        "the inventing reader really is the one that found more",
        sum(char_multiset(shifted).values()) > sum(char_multiset(right).values()),
        True,
    )
    agreeing = reader_agreement(right, shifted)
    if not 0 <= agreeing < READERS_AGREE_PCT:
        failures.append(
            f"a shifted alphabet should read as two texts, not one: got {agreeing}"
        )
    candidates = [("mutool", right), ("poppler", shifted)]
    volume = max(candidates, key=lambda pair: sum(char_multiset(pair[1]).values()))
    if volume[0] != "poppler":
        failures.append("this fixture no longer demonstrates the volume rule losing")
    against_volume = containment(oracle_multiset(volume[1]), char_multiset(right))
    picked = choose_oracle(candidates, right)
    check("the reader we agree with is the oracle", picked["oracle"], "mutool")
    check("and the rule says so", picked["oracle_rule"], "best-of-two")
    check(
        "scored against the reader we agree with",
        containment(oracle_multiset(picked["oracle_text"]), char_multiset(right)),
        100.0,
    )
    if against_volume >= 100.0:
        failures.append(
            "the volume oracle should have scored this document below the reader "
            f"we agree with; got {against_volume}"
        )
    check(
        "the reader not chosen is on the record",
        picked["recall_by_reader"]["poppler"],
        against_volume,
    )

    # And the other half of the rule: where the readers agree, the volume oracle
    # stands and the number does not move. Two readings of one text that differ
    # by a ligature and a repeated header -- the ordinary case, 261 of the gate
    # set's 276 two-reader documents.
    same = "the quick brown fox jumps over the lazy dog " * 6
    longer = same + "page 1 of 1"
    agreed = choose_oracle([("mutool", same), ("poppler", longer)], same)
    check("the readers agree", agreed["readers_agreement"] >= READERS_AGREE_PCT, True)
    check("so the volume rule still decides", agreed["oracle_rule"], "volume")
    check("and it picks the reader that found more", agreed["oracle"], "poppler")
    check(
        "which is the number this document had before #548",
        containment(oracle_multiset(agreed["oracle_text"]), char_multiset(same)),
        containment(oracle_multiset(longer), char_multiset(same)),
    )
    check(
        "a document with one reader has nothing to disagree with",
        choose_oracle([("mutool", same)], same)["readers_agreement"],
        -1.0,
    )
    check(
        "and it is scored by the volume rule",
        choose_oracle([("mutool", same)], same)["oracle_rule"],
        "volume",
    )
    # A reader that found nothing is not a reader that disagrees.
    check(
        "an empty reader is the absence of the question",
        reader_agreement(same, "   "),
        -1.0,
    )
    check(
        "in either direction",
        reader_agreement("   ", same),
        -1.0,
    )
    # The minimum and not the mean or the maximum, which is the whole of what
    # makes this rule proof against a reader that invents: a reader that emits
    # everything the other did AND a text of its own contains the other
    # completely, so one direction reads 100 while the other reads 63. Under
    # either of the other two statistics that pair would count as one text, the
    # volume rule would stand, and the inventing reader would be the oracle --
    # which is issue6127.
    superset = same + "".join(chr(0x250 + n % 200) for n in range(len(same)))
    one_way = containment(oracle_multiset(same), oracle_multiset(superset))
    check("the inventing reader contains the honest one whole", one_way, 100.0)
    if reader_agreement(same, superset) >= READERS_AGREE_PCT:
        failures.append(
            "a reader that found everything plus a text of its own is not the "
            f"same text: got {reader_agreement(same, superset)}"
        )
    # Without our own text there is nothing to be the better of, so the rule
    # cannot be applied and does not pretend to have been.
    check(
        "no text of ours, no best-of-two",
        choose_oracle(candidates, None)["oracle_rule"],
        "volume",
    )
    # The summary counts what best-of-two was used on. It has to: the rule can
    # only raise a number.
    counted = summarise(
        {
            "shifted.pdf": {
                "parsed": True, "char_recall": 100.0, "order_agreement": -1.0,
                "extract_ms": 1.0, "short": False, "oracle_rule": "best-of-two",
                "readers_agreement": 40.0,
            },
            "agreed.pdf": {
                "parsed": True, "char_recall": 99.0, "order_agreement": -1.0,
                "extract_ms": 1.0, "short": False, "oracle_rule": "volume",
                "readers_agreement": 99.5,
            },
        }
    )
    check("the documents best-of-two was used on", counted["recall_scored_best_of_two"], 1)
    check("the documents whose readers disagree", counted["recall_readers_disagree"], 1)
    check("and the denominator under both", counted["recall_readers_compared"], 2)

    check("p50 of one", percentile([7.0], 0.5), 7.0)
    check("p50 of two", percentile([1.0, 3.0], 0.5), 2.0)
    check("p95 picks the tail", percentile([1.0, 2.0, 3.0, 4.0, 100.0], 0.95), 80.8)

    # The summary must count what it excludes, not hide it.
    documents = {
        "a.pdf": {"parsed": True, "char_recall": 100.0, "order_agreement": 100.0,
                  "extract_ms": 1.0, "short": False,
                  "order_pages": [{"page": 1, "best": 100.0, "readers": 100.0}]},
        "b.pdf": {"parsed": True, "char_recall": 40.0, "order_agreement": 50.0,
                  "extract_ms": 3.0, "short": False,
                  "order_pages": [{"page": 1, "best": 80.0, "readers": 99.0},
                                  {"page": 2, "best": 20.0, "readers": 40.0}]},
        "short.pdf": {"parsed": True, "char_recall": 100.0, "order_agreement": -1.0,
                      "extract_ms": 2.0, "short": True},
        "notext.pdf": {"parsed": True, "char_recall": -1.0, "order_agreement": -1.0,
                       "extract_ms": 2.0, "short": False},
        "hung.pdf": {"parsed": False, "verdict": "timeout", "char_recall": -1.0,
                     "order_agreement": -1.0, "extract_ms": None, "short": False},
    }
    s = summarise(documents)
    check("summary counts every document", s["documents"], 5)
    check("summary counts the hang", s["timed_out"], 1)
    check("the short document is not assessed", s["recall_assessed"], 2)
    check("the short document is counted apart", s["recall_too_short"], 1)
    check("documents without source text are counted apart", s["recall_no_source_text"], 2)
    check("the tail is reported", s["recall_below_95pct"], 1)
    check("pages are counted, not only documents", s["order_pages_assessed"], 3)
    check("and the pages under the line", s["order_pages_below_95"], 2)
    check(
        "the pages that are ours beyond argument are counted apart",
        s["order_pages_readers_agree_we_do_not"],
        1,
    )
    check(
        "and so are the documents they sit in",
        s["order_documents_readers_agree_we_do_not"],
        1,
    )

    for f in failures:
        print(f"FAIL {f}")
    if failures:
        return 1
    print(
        "OK self-test: recall past the readers' unknown markers, order per page, "
        "percentiles and the summary all behave"
    )
    return 0


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--set", default="gate", help="which set of the pinned manifest")
    p.add_argument("--corpus-dir")
    # `pdfluent-cli`, which since the 30-09-2026 takeover (#636) is an alias
    # binary for the public `pdfluent` CLI: both names run the same program.
    # Before the takeover `xfa-cli` also produced a binary called `pdfluent`,
    # and that one had no `extract-text` subcommand -- a run with the old
    # default reported `error` on all 500 documents and recall 0, which is loud
    # but is still half an hour spent on a measurement of the wrong program.
    # Every recorded run and every command in EXTRACTION_RIG.md names
    # `pdfluent-cli`; the default stays with them.
    p.add_argument("--binary", default="target/release/pdfluent-cli")
    p.add_argument("--out")
    p.add_argument("--run-id", default="")
    p.add_argument("--machine-class", default="dev-macbook-m1pro")
    p.add_argument("--commit", default="")
    p.add_argument("--mutool", default="mutool")
    p.add_argument("--poppler", default="pdftotext")
    p.add_argument(
        "--quiet-machine",
        action="store_true",
        help=(
            "assert this machine is running nothing else, so the timings may be "
            "compared with another run; without it the speed axis is reported "
            "and not judged"
        ),
    )
    p.add_argument("--progress", action="store_true")
    p.add_argument("--self-test", action="store_true")
    args = p.parse_args()

    if args.self_test:
        return self_test()
    if not args.corpus_dir or not args.out:
        p.error("--corpus-dir and --out are required unless --self-test")
    if not args.run_id:
        # THE FILE IT IS WRITTEN TO, not the date and the machine. Two runs on
        # one day, on one machine, over one set got the same id -- and the
        # baseline's `measured_from` then resolves, by the `<run-id>.json`
        # convention the runbook documents, to whichever of them was written
        # first. It happened on 16-09-2026: the second run's evidence pointed at
        # the first run's numbers (codex, #1918). `--run-id` still wins where a
        # caller wants to say it out loud.
        args.run_id = pathlib.Path(args.out).stem
    return measure(args)


if __name__ == "__main__":
    sys.exit(main())
