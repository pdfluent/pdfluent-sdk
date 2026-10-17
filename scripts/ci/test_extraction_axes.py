#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The extraction rig's arithmetic, on fixtures, in under a second (#367).

The corpus run needs 500 documents, two foreign readers and a release build, so
it lives in the wave. This does not: it is the part that can go wrong silently
and be believed, and it runs on every pull request.

Each test below is a way the rig could report a comfortable number while being
wrong. They are written as the mutation that would produce that number, so that
a future edit which reintroduces it goes red here rather than in a round three
weeks later.
"""

from __future__ import annotations

import pathlib
import subprocess
import sys


HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[1]
sys.path.insert(0, str(REPO / "scripts" / "extraction"))

import extraction_axes as axes  # noqa: E402


FAILURES: list = []


def check(label: str, got, want) -> None:
    if got != want:
        FAILURES.append(f"{label}: got {got!r}, expected {want!r}")


def test_recall_sees_substitution() -> None:
    """A length ratio calls this document perfect. Containment must not.

    text_retention_gate.py measures characters-out over characters-in, which is
    the right axis for asking whether a conversion threw content away. It is the
    wrong one for extraction: an extractor that returns the correct number of
    wrong characters scores 100% on a ratio.
    """
    oracle = axes.char_multiset("the quick brown fox")
    same_length_wrong = axes.char_multiset("XXX XXXXX XXXXX XXX")
    check("substituted text", axes.containment(oracle, same_length_wrong), 0.0)


def test_recall_is_not_fooled_by_repetition() -> None:
    """Multiplicity counts, or a document of one repeated word scores full marks."""
    check(
        "one word repeated is not the whole document",
        axes.containment(axes.char_multiset("aaab"), axes.char_multiset("aaaa")),
        75.0,
    )


def test_recall_ignores_whitespace_only() -> None:
    check(
        "line breaks placed differently are not lost text",
        axes.containment(axes.char_multiset("a b\nc"), axes.char_multiset("a\nb c")),
        100.0,
    )


def test_a_document_without_source_text_is_not_a_pass() -> None:
    """The one that matters most: an empty oracle must not score 100.

    Two hundred documents of the gate corpus have no text layer at all. If those
    scored 100% the corpus median would be a statement about the empty set.
    """
    check(
        "no oracle characters",
        axes.containment(axes.char_multiset("   \n "), axes.char_multiset("anything")),
        -1.0,
    )


def test_recall_does_not_count_the_readers_own_ignorance() -> None:
    """mutool prints U+FFFD and pdftotext the raw byte for a glyph it cannot map.

    Counting those as characters the oracle found scores us on our agreement with
    an admission of ignorance: TAMReview's oracle is 50,731 replacement characters
    out of 58,837, which put a document we read correctly at 13.8% (#428, #389
    class R-A).
    """
    check(
        "replacement characters",
        axes.containment(axes.oracle_multiset("ab\ufffd\ufffd"), axes.char_multiset("ab")),
        100.0,
    )
    check(
        "raw bytes in the control range",
        axes.containment(axes.oracle_multiset("ab\x83\x90"), axes.char_multiset("ab")),
        100.0,
    )
    # The same reader's same raw bytes, the ones that land outside the control
    # range: TAMReview has 102 of these against pdftotext.
    check(
        "raw bytes on unassigned code points",
        axes.containment(axes.oracle_multiset("ab\u0378\u0380"), axes.char_multiset("ab")),
        100.0,
    )
    check(
        "a character the oracle really found still counts",
        axes.containment(axes.oracle_multiset("abc"), axes.char_multiset("ab")),
        round(100.0 * 2 / 3, 3),
    )


def test_order_is_measured_per_page() -> None:
    """A page-by-page catastrophe must not read as a rounding error.

    Kendall's tau over a whole document is dominated by the pairs that span its
    pages, and those are right for anyone who emits the pages in order. Ten pages
    each read backwards score over 90 read whole (#389: tracemonkey scored 98.3
    while every one of its fourteen pages scored 73-94).
    """
    pages = [" ".join(f"p{n}w{i:02d}" for i in range(1, 26)) for n in range(10)]
    oracle = "\f".join(pages)
    ours = "\n\n".join(" ".join(reversed(page.split())) for page in pages)
    check("every page is scored", len(axes.order_per_page(oracle, ours)), 10)
    check("and each of them at nought", set(axes.order_per_page(oracle, ours)), {0.0})
    if axes.order_agreement(oracle, ours) <= 90.0:
        FAILURES.append(
            "read whole this document should still score over 90 -- if it does not, "
            "this test no longer demonstrates what per page is for"
        )


def test_a_short_page_has_no_reading_order() -> None:
    """The A12 floor, page by page: seven documents sat in the order tail on
    three to nine anchors before it was applied there (#428)."""
    long_page = " ".join(f"w{i:02d}" for i in range(1, 26))
    oracle = f"alpha beta gamma delta epsilon\f{long_page}"
    scores = axes.order_per_page(oracle, f"alpha beta gamma delta epsilon {long_page}")
    check("the five-word page is not measured", scores[0], -1.0)
    check("the page above the floor is", scores[1], 100.0)


def test_a_page_the_readers_read_differently_is_not_our_defect() -> None:
    """mutool emits content-stream order and pdftotext reconstructs geometry.

    Where they disagree there is no single reading order to be wrong about, so the
    page is scored against the better of the two and their disagreement is on the
    record -- which is what lets the summary count the pages where they agree and
    we agree with neither.
    """
    page = " ".join(f"w{i:02d}" for i in range(1, 26))
    rows = axes.order_pages(
        {"mutool": page, "poppler": " ".join(reversed(page.split()))}, page
    )
    check("scored against the reader we agree with", rows[0]["best"], 100.0)
    check("their disagreement is recorded", rows[0]["readers"], 0.0)


def test_order_is_independent_of_recall() -> None:
    """The pair that justifies two axes rather than one."""
    oracle, ours = "alpha beta gamma delta", "delta gamma beta alpha"
    check(
        "perfect recall on reversed text",
        axes.containment(axes.char_multiset(oracle), axes.char_multiset(ours)),
        100.0,
    )
    check("destroyed order on the same text", axes.order_agreement(oracle, ours), 0.0)


def test_order_refuses_to_judge_without_anchors() -> None:
    """Fewer than two shared unique tokens is no order at all, and says so."""
    check("one token", axes.order_agreement("alpha", "alpha"), -1.0)
    check("nothing in common", axes.order_agreement("alpha beta", "gamma delta"), -1.0)
    check(
        "a repeated token is not an anchor",
        axes.order_agreement("alpha alpha", "alpha alpha"),
        -1.0,
    )


def test_order_scores_a_single_swap_proportionally() -> None:
    """Not a yes/no: a block that moved should cost what it moved."""
    check(
        "one inverted pair of three",
        axes.order_agreement("a1 b2 c3", "b2 a1 c3"),
        round(100.0 * 2 / 3, 3),
    )
    check("same order", axes.order_agreement("a1 b2 c3", "a1 b2 c3"), 100.0)


def test_the_summary_states_its_denominator() -> None:
    """A quiet exclusion is the cheapest route to a flattering number (A27)."""
    documents = {
        "good.pdf": {"parsed": True, "char_recall": 100.0, "order_agreement": 100.0,
                     "extract_ms": 1.0, "short": False,
                     "order_pages": [{"page": 1, "best": 100.0, "readers": 100.0}]},
        "bad.pdf": {"parsed": True, "char_recall": 9.6, "order_agreement": 20.0,
                    "extract_ms": 2.0, "short": False,
                    "order_pages": [{"page": 1, "best": 20.0, "readers": 99.0},
                                    {"page": 2, "best": 60.0, "readers": 30.0}]},
        "two_words.pdf": {"parsed": True, "char_recall": 100.0, "order_agreement": -1.0,
                          "extract_ms": 1.0, "short": True},
        "scanned.pdf": {"parsed": True, "char_recall": -1.0, "order_agreement": -1.0,
                        "extract_ms": 1.0, "short": False},
        "hung.pdf": {"parsed": False, "verdict": "timeout", "char_recall": -1.0,
                     "order_agreement": -1.0, "extract_ms": None, "short": False},
    }
    s = axes.summarise(documents)
    check("every document is in the denominator", s["documents"], 5)
    check("the short one is excluded from recall", s["recall_assessed"], 2)
    check("and counted where a reader can see it", s["recall_too_short"], 1)
    check("documents without text are counted apart", s["recall_no_source_text"], 2)
    check("a hang is not a missing measurement", s["timed_out"], 1)
    check("the tail is a number, not a footnote", s["recall_below_95pct"], 1)
    check("the median is over the assessed only", s["recall_median_pct"], 54.8)
    # The arithmetic a reader uses to check nothing was excluded twice.
    check("the pages behind the documents are counted", s["order_pages_assessed"], 3)
    check("and the pages under the line", s["order_pages_below_95"], 2)
    # The pages that are ours beyond argument, apart from the ones where the two
    # readers do not agree with each other either.
    check(
        "a page both readers read the same way and we did not",
        s["order_pages_readers_agree_we_do_not"],
        1,
    )
    check(
        "the three recall buckets partition the corpus",
        s["recall_assessed"] + s["recall_too_short"] + s["recall_no_source_text"],
        s["documents"],
    )


def test_percentiles_do_not_hide_the_tail() -> None:
    check("p5 of a tail", axes.percentile([9.6, 100.0, 100.0, 100.0, 100.0], 0.05), 27.68)
    check("p95 of a spike", axes.percentile([1.0, 1.0, 1.0, 1.0, 60.0], 0.95), 48.2)


def test_the_manifest_is_pinned_and_matches_its_generator() -> None:
    """The corpus cannot move without somebody deciding that it should."""
    done = subprocess.run(
        [sys.executable, str(REPO / "scripts" / "extraction" / "build_extraction_manifest.py"),
         "--check"],
        capture_output=True,
        text=True,
        check=False,
    )
    if done.returncode != 0:
        FAILURES.append(f"the committed manifest is not what its generator makes: {done.stdout}")


def test_the_manifest_checker_rejects_what_it_must() -> None:
    done = subprocess.run(
        [sys.executable, str(REPO / "scripts" / "extraction" / "build_extraction_manifest.py"),
         "--self-test"],
        capture_output=True,
        text=True,
        check=False,
    )
    if done.returncode != 0:
        FAILURES.append(f"manifest self-test failed: {done.stdout}{done.stderr}")


def test_the_oracle_cannot_be_won_by_inventing_characters() -> None:
    """issue6127 in miniature, which is the document #548 is about.

    Its text is drawn in a non-embedded Identity-H font. pdftotext decodes it by
    shifting every code three places -- `YRLUQRWLFHHW` where the page draws
    `voir notice et` -- and that is how it comes to be the reader that found
    more: 11,319 characters against mutool's 11,027, of which 938 are characters
    mutool does not hold at all. Under "whichever found more" the
    document was therefore scored against a reading nobody claims is right, at
    92.208, where the same run against mutool reads 99.964; #430 and #437, which
    fixed OUR reading of that font, appeared in the baseline as recall falling.

    The mutation this goes red on is the old rule: pick by volume and ask no
    further questions.
    """
    right = "voir notice et " * 8
    shifted = "".join(chr(ord(c) + 3) if c.isalpha() else c for c in right)
    shifted += "and a page of characters nobody drew" * 3

    by_volume = max(
        [("mutool", right), ("poppler", shifted)],
        key=lambda pair: sum(axes.char_multiset(pair[1]).values()),
    )[0]
    check("the inventing reader is the one that found more", by_volume, "poppler")

    picked = axes.choose_oracle([("mutool", right), ("poppler", shifted)], right)
    check("the oracle is the reader we agree with", picked["oracle"], "mutool")
    check("under the stated rule", picked["oracle_rule"], "best-of-two")
    check(
        "and the document scores what our reading deserves",
        axes.containment(
            axes.oracle_multiset(picked["oracle_text"]), axes.char_multiset(right)
        ),
        100.0,
    )
    # The reader not chosen is on the record, because best-of-two can only
    # flatter and the number it did not use is the evidence for the number it
    # did.
    if not 0 <= picked["recall_by_reader"]["poppler"] < 100.0:
        FAILURES.append(
            "the reader not chosen should still be scored, below the one that "
            f"was; got {picked['recall_by_reader']['poppler']}"
        )


def test_the_oracle_rule_does_not_move_where_the_readers_agree() -> None:
    """The 261 of 276 the axis must keep meaning exactly what it meant.

    Two readings of one text differ over a ligature, a hyphen at a line break, a
    header one of them repeats. Those documents are the whole corpus bar a
    handful, and on them "whichever found more" is a tie-break between two
    readings of the same text rather than a verdict about which text it is. The
    mutation here is the opposite of the one above: applying best-of-two
    everywhere, which would raise numbers across the corpus for no measurement.
    """
    text = "the quick brown fox jumps over the lazy dog " * 6
    with_header = text + "page 1 of 1"
    picked = axes.choose_oracle([("mutool", text), ("poppler", with_header)], text)
    check("the readers are one text", picked["oracle_rule"], "volume")
    check("so the reader that found more still decides", picked["oracle"], "poppler")
    check(
        "and the score is the one the volume rule gave",
        axes.containment(
            axes.oracle_multiset(picked["oracle_text"]), axes.char_multiset(text)
        ),
        axes.containment(
            axes.oracle_multiset(with_header), axes.char_multiset(text)
        ),
    )
    if "recall_by_reader" in picked:
        FAILURES.append(
            "a document the volume rule decided has no best-of-two to report"
        )


def test_reader_agreement_is_the_worse_direction() -> None:
    """A reader that contains the other whole has not agreed with it.

    The minimum of the two containments, not the mean and not the maximum. A
    reader that emits everything the other did plus a text of its own contains
    the other completely: one direction reads 100 and the other reads what the
    invention cost. A mean or a maximum calls that pair one text, the volume
    rule then stands, and the inventing reader is the oracle -- which is the
    defect, restored.
    """
    honest = "the quick brown fox jumps over the lazy dog " * 6
    inventing = honest + "".join(chr(0x250 + n % 200) for n in range(len(honest)))
    check(
        "the inventing reader contains the honest one whole",
        axes.containment(
            axes.oracle_multiset(honest), axes.oracle_multiset(inventing)
        ),
        100.0,
    )
    agreement = axes.reader_agreement(honest, inventing)
    if agreement >= axes.READERS_AGREE_PCT:
        FAILURES.append(
            "a reader that found everything plus a text of its own is not the "
            f"same text: got {agreement}"
        )
    check(
        "and a reader that produced nothing is not a disagreement",
        axes.reader_agreement(honest, "   "),
        -1.0,
    )


def test_the_summary_counts_what_best_of_two_was_used_on() -> None:
    """Best-of-two can only raise a number, so the run says how often it did.

    The mutation is a summary that stops counting: the rule then applies
    silently, every round after it reads as a corpus that improved, and nothing
    on the page says which half of the change was the metric.
    """
    s = axes.summarise(
        {
            "disagreed.pdf": {
                "parsed": True, "char_recall": 100.0, "order_agreement": -1.0,
                "extract_ms": 1.0, "short": False, "oracle_rule": "best-of-two",
                "readers_agreement": 40.0,
            },
            "agreed.pdf": {
                "parsed": True, "char_recall": 99.0, "order_agreement": -1.0,
                "extract_ms": 1.0, "short": False, "oracle_rule": "volume",
                "readers_agreement": 99.5,
            },
            "one-reader.pdf": {
                "parsed": True, "char_recall": 98.0, "order_agreement": -1.0,
                "extract_ms": 1.0, "short": False, "oracle_rule": "volume",
                "readers_agreement": -1.0,
            },
        }
    )
    check("the documents best-of-two was used on", s["recall_scored_best_of_two"], 1)
    check("the documents whose readers disagree", s["recall_readers_disagree"], 1)
    check("and the denominator under both", s["recall_readers_compared"], 2)


def test_a_timeout_under_load_recovers_on_retry() -> None:
    """A document slow only on its first call ends up parsed (#636).

    On a loaded machine a document that normally takes four seconds timed out,
    flipped `parsed` from true to false, and aborted a landing as a
    regression. The fake extractor below is slow on its first call only --
    the shape of load, not of a hang -- and the retry runs it alone.

    The mutation this goes red on is no retry: without the second run the
    document stays "timeout" and unparsed.
    """
    check("the retry runs exactly once more", axes.TIMEOUT_RETRIES, 1)
    calls: list = []

    def fake_measure(name: str) -> dict:
        calls.append(name)
        first = sum(1 for c in calls if c == name) == 1
        if first:
            return {"parsed": False, "verdict": "timeout", "extract_ms": None,
                    "char_recall": -1.0, "order_agreement": -1.0, "short": False}
        return {"parsed": True, "verdict": "ok", "extract_ms": 4.0,
                "char_recall": 100.0, "order_agreement": 100.0, "short": False,
                "order_pages": []}

    documents = {"slow.pdf": fake_measure("slow.pdf")}
    check("the first pass times out under load",
          documents["slow.pdf"]["verdict"], "timeout")
    retries = axes.retry_timed_out(documents, fake_measure)
    check("the document is re-run exactly once more",
          calls.count("slow.pdf"), 2)
    check("it ends up parsed", documents["slow.pdf"]["parsed"], True)
    check("it no longer counts as timeout",
          documents["slow.pdf"]["verdict"], "ok")
    check("the retry is recorded", len(retries), 1)
    if retries:
        check("the retry names the document", retries[0]["document"], "slow.pdf")
        check("the retry records the outcome",
              retries[0]["second_verdict"], "ok")
        check("the retry records recovery", retries[0]["parsed"], True)
    else:
        FAILURES.append("a recovered timeout with no retry on the record is silent")


def test_a_timeout_that_persists_stays_timeout() -> None:
    """A document that hangs twice is not waiting for a quieter machine (#636).

    The fake extractor below is slow on every call. Only a second timeout
    counts as "timeout", and this one has it -- so the document stays
    unparsed. It must still appear in the retry list: a retry that left no
    record would read as a first-pass hang either way.

    The mutation this goes red on is no retry: without it the list is empty
    and the hang is indistinguishable from a run that never re-ran it.
    """
    def always_slow(name: str) -> dict:
        return {"parsed": False, "verdict": "timeout", "extract_ms": None,
                "char_recall": -1.0, "order_agreement": -1.0, "short": False}

    documents = {"hung.pdf": always_slow("hung.pdf")}
    retries = axes.retry_timed_out(documents, always_slow)
    check("it stays timeout", documents["hung.pdf"]["verdict"], "timeout")
    check("it stays unparsed", documents["hung.pdf"]["parsed"], False)
    check("the retry is still recorded", len(retries), 1)
    if retries:
        check("the retry names the document", retries[0]["document"], "hung.pdf")
        check("the retry records the second timeout",
              retries[0]["second_verdict"], "timeout")
        check("the retry records no recovery", retries[0]["parsed"], False)
    else:
        FAILURES.append("a persistent timeout with no retry on the record is silent")


def test_timeout_retries_are_recorded_and_never_silent() -> None:
    """The retry list names each retried document and its outcome (#636).

    One document recovers, one hangs twice, one parsed first time, and one
    answered with an error. Only the two timeouts are re-run -- an "error" is
    our extractor answering, which a quieter machine does not change -- and
    each retried document carries its first verdict on its own record.

    The mutation this goes red on is no retry: without it the list is empty,
    the recovered document stays unparsed, and nothing distinguishes the
    retried from the never-retried.
    """
    def rerun(name: str) -> dict:
        if name == "slow.pdf":
            return {"parsed": True, "verdict": "ok", "extract_ms": 4.0,
                    "char_recall": 100.0, "order_agreement": -1.0, "short": False,
                    "order_pages": []}
        return {"parsed": False, "verdict": "timeout", "extract_ms": None,
                "char_recall": -1.0, "order_agreement": -1.0, "short": False}

    documents = {
        "slow.pdf": {"parsed": False, "verdict": "timeout", "extract_ms": None,
                     "char_recall": -1.0, "order_agreement": -1.0, "short": False},
        "hung.pdf": {"parsed": False, "verdict": "timeout", "extract_ms": None,
                     "char_recall": -1.0, "order_agreement": -1.0, "short": False},
        "fine.pdf": {"parsed": True, "verdict": "ok", "extract_ms": 4.0,
                     "char_recall": 100.0, "order_agreement": -1.0, "short": False,
                     "order_pages": []},
        "broken.pdf": {"parsed": False, "verdict": "error", "extract_ms": 2.0,
                       "char_recall": -1.0, "order_agreement": -1.0, "short": False},
    }
    retries = axes.retry_timed_out(documents, rerun)
    check("only the timeouts are retried", len(retries), 2)
    by_name = {r["document"]: r for r in retries}
    check("the recovered document is on the record", "slow.pdf" in by_name, True)
    check("the persistent hang is on the record", "hung.pdf" in by_name, True)
    check("a document that parsed is not retried", "fine.pdf" in by_name, False)
    check("an error is not retried", "broken.pdf" in by_name, False)
    if "slow.pdf" in by_name:
        check("the recovered outcome is recorded",
              by_name["slow.pdf"]["second_verdict"], "ok")
    if "hung.pdf" in by_name:
        check("the persistent outcome is recorded",
              by_name["hung.pdf"]["second_verdict"], "timeout")
    check("the recovered document ends up parsed",
          documents["slow.pdf"]["parsed"], True)
    check("the persistent hang stays timeout",
          documents["hung.pdf"]["verdict"], "timeout")
    check("a retried document says so",
          documents["slow.pdf"].get("retried"), True)
    check("and carries its first verdict",
          documents["slow.pdf"].get("first_verdict"), "timeout")
    check("a document never retried carries no retry flag",
          "retried" in documents["fine.pdf"], False)


def test_measure_reruns_timeouts_alone_and_records_them() -> None:
    """The run file names each retried document and its outcome (#636).

    End to end through `measure` with a fake extractor slow on its first call
    only: the first pass times out, the retry alone recovers, and the run JSON
    carries the retry list so the recovery is visible and never silent.

    The mutation this goes red on is no retry in `measure`: without it the
    document stays "timeout" and unparsed, and the output carries an empty
    list -- or none at all.
    """
    import json
    import tempfile
    from unittest import mock

    calls: list = []

    def fake_measure_one(pdf, binary, mutool, poppler) -> dict:
        name = pathlib.Path(pdf).name
        calls.append(name)
        first = sum(1 for c in calls if c == name) == 1
        if first:
            return {"parsed": False, "verdict": "timeout", "extract_ms": None,
                    "char_recall": -1.0, "order_agreement": -1.0, "short": False}
        return {"parsed": True, "verdict": "ok", "extract_ms": 4.0,
                "char_recall": 100.0, "order_agreement": -1.0, "short": False,
                "order_pages": []}

    with tempfile.TemporaryDirectory() as tmp:
        corpus = pathlib.Path(tmp) / "corpus"
        corpus.mkdir()
        (corpus / "slow.pdf").write_bytes(b"%PDF-1.4 fake")
        out = pathlib.Path(tmp) / "run.json"
        args = mock.Mock()
        args.set = "gate"
        args.corpus_dir = str(corpus)
        args.binary = sys.executable
        args.mutool = "mutool"
        args.poppler = "pdftotext"
        args.out = str(out)
        args.run_id = "test-retry"
        args.machine_class = "test"
        args.commit = "test"
        args.progress = False
        args.quiet_machine = True
        with mock.patch.object(
            axes, "load_set", return_value=[{"name": "slow.pdf"}]
        ), mock.patch.object(
            axes, "measure_one", side_effect=fake_measure_one
        ), mock.patch(
            "shutil.which", return_value="/bin/true"
        ):
            rc = axes.measure(args)
        check("the run succeeds", rc, 0)
        if rc != 0:
            return
        run = json.loads(out.read_text(encoding="utf-8"))
        check("the retry list appears in the output",
              "timeout_retries" in run, True)
        retries = run.get("timeout_retries", [])
        check("the timeout was retried", len(retries), 1)
        if retries:
            check("the retry names the document",
                  retries[0].get("document"), "slow.pdf")
            check("the retry records the outcome",
                  retries[0].get("second_verdict"), "ok")
        check("the document ends up parsed",
              run["documents"]["slow.pdf"]["parsed"], True)
        check("a second timeout is what would count",
              run["summary"]["timed_out"], 0)


def test_the_axis_script_self_test_passes() -> None:
    done = subprocess.run(
        [sys.executable, str(REPO / "scripts" / "extraction" / "extraction_axes.py"),
         "--self-test"],
        capture_output=True,
        text=True,
        check=False,
    )
    if done.returncode != 0:
        FAILURES.append(f"axis self-test failed: {done.stdout}{done.stderr}")


def main() -> int:
    tests = [v for k, v in sorted(globals().items()) if k.startswith("test_")]
    for test in tests:
        test()
    for f in FAILURES:
        print(f"FAIL {f}")
    if FAILURES:
        print(f"\n{len(FAILURES)} failure(s) in {len(tests)} tests")
        return 1
    print(f"OK {len(tests)} tests: the extraction rig's arithmetic holds")
    return 0


if __name__ == "__main__":
    sys.exit(main())
