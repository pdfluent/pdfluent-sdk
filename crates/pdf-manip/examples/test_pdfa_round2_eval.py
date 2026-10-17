#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""What the page eval renders, what it borrows, and when it calls a run failed.

The three properties of #454, each with a case that goes red if the property is
undone:

  1. One candidate renders two sets. Passing the same directory as `--before`
     and `--after` rendered every page of the corpus a third time and bought
     duplicate columns plus a self-comparison that is true for any converter.
  2. A second candidate can be borrowed from a prior run's artifact instead of
     rendered, and only where the source digest and the dpi match.
  3. A refused document does not fail the measurement. One conversion error in
     a thousand documents made the whole fidelity step red on the tuning1000
     run 34344290509, which had produced every file it was asked for.

And the one property of #547:

  4. Two candidates share one render of the source, and neither file carries a
     trace of the other candidate. Two arms of one experiment were two runs of
     this script, and each rendered the source: four sets for two candidates.

WHY THE RENDERING ITSELF IS NOT EXERCISED HERE. The eval needs mutool,
pdfinfo, numpy, Pillow and scikit-image; the fidelity job installs all five and
the gate machines carry none of them. So the arithmetic that decides what is
rendered, borrowed and reported is tested here, in under a second and with no
readers, and the end-to-end equivalence -- the same after_* columns whichever
mode produced them -- is a separate command:

    python3 crates/pdf-manip/examples/test_pdfa_round2_eval.py --end-to-end

It builds three one-page PDFs, runs the eval in all three modes against them,
and requires the judged columns to be identical. It is run by hand where the
readers are, and its result belongs in the round it was run for.
"""
from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parent))
import pdfa_round2_eval as eval_module  # noqa: E402

FAILURES: list[str] = []


def check(label, got, want):
    if got != want:
        FAILURES.append(f"{label}: got {got!r}, expected {want!r}")


def prior_row(name="a.pdf", dpi=72):
    """A row as a previous run wrote it: one candidate, measured."""
    return {
        "file": name, "dpi": dpi, "errors": [],
        "source_sha256": "s" * 64,
        "after_sha256": "c" * 64, "after_bytes": 4096,
        "after_page_count": 2, "after_info_exit": 0, "after_render_exit": 0,
        "after_render_warning_sha256": "w" * 64, "after_rendered_pages": 2,
        "after_text": {"exit": 0, "sha256": "t" * 64, "bytes": 12},
        "after_text_scores": {"word_retention": 1.0},
        "pages": [
            {"page": 1, "after_mae": 0.5, "after_ssim": 0.9},
            {"page": 2, "after_dimensions_changed": True},
        ],
    }


def one_candidate_renders_two_sets():
    check("no --before is one candidate",
          eval_module.rendered_labels(None, Path("/x/after")),
          ["source", "after"])
    check("--before naming --after is one candidate",
          eval_module.rendered_labels(Path("/x/after"), Path("/x/after")),
          ["source", "after"])
    check("a spelling of the same directory is the same directory",
          eval_module.rendered_labels(Path("/x/./after/"), Path("/x/after")),
          ["source", "after"])
    check("two candidates still render three sets",
          eval_module.rendered_labels(Path("/x/before"), Path("/x/after")),
          ["source", "before", "after"])


def a_borrowed_candidate_is_pinned_to_its_source():
    borrowed = eval_module.borrow_before(prior_row(), "s" * 64, 72)
    check("the prior candidate becomes this run's before",
          borrowed["columns"]["before_sha256"], "c" * 64)
    check("its per-document columns come with it",
          (borrowed["columns"]["before_page_count"],
           borrowed["columns"]["before_text"]["sha256"],
           borrowed["columns"]["before_text_scores"]["word_retention"]),
          (2, "t" * 64, 1.0))
    check("its per-page numbers come with it",
          borrowed["pages"][1], {"before_mae": 0.5, "before_ssim": 0.9})
    check("a page it could not judge stays unjudged, not clean",
          borrowed["pages"][2], {"before_dimensions_changed": True})

    check("a row measured against other source bytes is not borrowed from",
          eval_module.borrow_before(prior_row(), "d" * 64, 72), None)
    check("a row measured at another dpi is not borrowed from",
          eval_module.borrow_before(prior_row(), "s" * 64, 150), None)
    check("a run with no source digest borrows nothing",
          eval_module.borrow_before(prior_row(), None, 72), None)
    errored = prior_row()
    errored["errors"] = ["after: missing PDF"]
    check("a row that carried errors is not borrowed from",
          eval_module.borrow_before(errored, "s" * 64, 72), None)
    no_candidate = prior_row()
    del no_candidate["after_sha256"]
    check("a row with no candidate digest is not borrowed from",
          eval_module.borrow_before(no_candidate, "s" * 64, 72), None)
    check("no prior row at all is not an error, it is a render",
          eval_module.borrow_before(None, "s" * 64, 72), None)


def merged_row():
    """A row as one pass measuring two candidates holds it, before it is split."""
    return {
        "file": "a.pdf", "dpi": 72,
        "errors": ["other: missing PDF"],
        "source_sha256": "s" * 64,
        "after_sha256": "c" * 64, "after_text_scores": {"word_retention": 0.5},
        "other_sha256": "o" * 64, "other_text_scores": {"word_retention": 0.9},
        "before_sha256": "b" * 64, "before_borrowed_from": {"note": "borrowed"},
        "output_text_identical": False,
        "_pages": {"after": {1, 2}, "other": {1, 3}},
        "pages": [
            {"page": 1, "after_mae": 0.1, "other_mae": 0.2,
             "output_pixels_identical": False},
            {"page": 2, "after_mae": 0.3},
            {"page": 3, "other_mae": 0.4},
        ],
    }


def two_candidates_share_one_source_render():
    check("a second candidate is a set of its own, not a second source",
          eval_module.rendered_labels(None, Path("/x/after"), Path("/x/other")),
          ["source", "after", "other"])
    check("and it is the fourth when a before is rendered too",
          eval_module.rendered_labels(Path("/x/before"), Path("/x/after"),
                                      Path("/x/other")),
          ["source", "before", "after", "other"])

    first = eval_module.project(merged_row(), ("other",), "after")
    check("the other candidate's columns are not in the first file",
          [k for k in first if k.startswith("other_")], [])
    check("nor its errors", first["errors"], [])
    check("nor the pages only it rendered",
          [p["page"] for p in first["pages"]], [1, 2])
    check("what the first file measured is untouched",
          (first["after_sha256"], first["pages"][0]["after_mae"]),
          ("c" * 64, 0.1))
    check("a before it borrowed rather than rendered stays",
          (first["before_sha256"], "before_borrowed_from" in first),
          ("b" * 64, True))
    check("and the before-against-after comparison with it",
          (first["output_text_identical"],
           first["pages"][0]["output_pixels_identical"]),
          (False, False))
    check("the private page map does not reach the file", "_pages" in first, False)

    second = eval_module.project(merged_row(), ("before", "after"), "other")
    check("the second file's candidate is `after`, which is what the axis reads",
          (second["after_sha256"], second["after_text_scores"]["word_retention"]),
          ("o" * 64, 0.9))
    check("its pages are the ones it rendered",
          [(p["page"], p.get("after_mae")) for p in second["pages"]],
          [(1, 0.2), (3, 0.4)])
    check("its error is its own, under the name it now has",
          second["errors"], ["after: missing PDF"])
    check("the first candidate is not in it",
          [k for k in second if k.startswith("before_")], [])
    check("nor a comparison of two candidates it does not both have",
          ("output_text_identical" in second,
           "output_pixels_identical" in second["pages"][0]),
          (False, False))
    check("the source it was measured against is",
          second["source_sha256"], "s" * 64)

    # A summary path names two more files by suffix, so two summaries that
    # differ only in theirs are one JSONL written by both candidates -- with an
    # exit code that finds a file there and calls the measurement done.
    collisions = eval_module.colliding_outputs(
        [("--out", Path("/x/result.json")), ("--also-out", Path("/x/result.txt"))])
    check("two summaries can share a rows file, and that is refused",
          [(path.name, first, second) for path, first, second in collisions],
          [("result.jsonl", "--out", "--also-out"),
           ("result.inputs.json", "--out", "--also-out")])
    check("two summaries that share nothing are two measurements",
          eval_module.colliding_outputs(
              [("--out", Path("/x/after/all-pages.json")),
               ("--also-out", Path("/x/other/all-pages.json"))]),
          [])
    check("a summary named .jsonl would overwrite its own rows",
          [(path.name, first, second) for path, first, second
           in eval_module.colliding_outputs([("--out", Path("/x/rows.jsonl"))])],
          [("rows.jsonl", "--out", "--out")])

    rows = [{"file": "a.pdf", "errors": [], "pages": [{"page": 1}]}]
    summary = eval_module.summarise(rows, ["source", "after"], 72, 1.0,
                                    ["source", "after", "other"])
    check("the file says what its own columns rest on",
          summary["rendered_sets_per_document"], 2)
    check("and what the pass that wrote it rendered",
          (summary["run_rendered_sets_per_document"], summary["shared_source_render"]),
          (3, True))
    alone = eval_module.summarise(rows, ["source", "after"], 72, 1.0,
                                  ["source", "after"])
    check("a pass measuring one candidate says nothing about sharing",
          "shared_source_render" in alone, False)


def a_self_comparison_is_not_a_result():
    one = [{"file": "a.pdf", "errors": [], "pages": [
        {"page": 1, "after_mae": 0.0}, {"page": 2, "after_mae": 0.1}]}]
    summary = eval_module.summarise(one, ["source", "after"], 72, 1.0)
    check("a single-candidate run renders two sets",
          summary["rendered_sets_per_document"], 2)
    check("and says it measured one candidate", summary["candidates"], 1)
    check("a run with nothing to compare reports no pixel comparison",
          "output_pixels_identical" in summary, False)
    check("nor a text one", "output_text_identical" in summary, False)
    check("every page is still counted", summary["pages"], 2)

    two = [{"file": "a.pdf", "errors": [], "output_text_identical": True,
            "pages": [{"page": 1, "output_pixels_identical": True},
                      {"page": 2, "output_pixels_identical": False}]}]
    summary = eval_module.summarise(two, ["source", "before", "after"], 72, 1.0)
    check("two candidates render three sets",
          summary["rendered_sets_per_document"], 3)
    check("and their comparison is reported",
          summary["output_pixels_identical"], 1)
    check("text alongside it", summary["output_text_identical"], 1)

    borrowed = [{"file": "a.pdf", "errors": [], "before_borrowed_from": {},
                 "output_text_identical": True, "pages": [{"page": 1}]}]
    summary = eval_module.summarise(borrowed, ["source", "after"], 72, 1.0)
    check("a borrowed before is counted", summary["documents_with_borrowed_before"], 1)
    check("it compares text, having the digest",
          summary["output_text_identical"], 1)
    check("but not pixels, having none of them",
          "output_pixels_identical" in summary, False)


def a_refused_document_does_not_fail_the_measurement():
    with tempfile.TemporaryDirectory() as tmp:
        jsonl = Path(tmp) / "all-pages.jsonl"
        jsonl.write_text("{}\n", encoding="utf-8")
        measured = [{"file": "a.pdf", "errors": [], "pages": []},
                    {"file": "b.pdf", "errors": ["after: missing PDF"],
                     "pages": []}]
        check("one refusal in a completed measurement is not a failed run",
              eval_module.exit_code(measured, jsonl), 0)
        check("the count is what travels",
              eval_module.summarise(measured, ["source", "after"], 72, 1.0)
              ["documents_with_errors"], 1)
        check("a run that measured nothing did fail",
              eval_module.exit_code([], jsonl), 1)
        jsonl.unlink()
        check("and so did one that lost its rows",
              eval_module.exit_code(measured, jsonl), 1)


PDF_TEMPLATE = """%PDF-1.4
1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj
2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj
3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 100]/Resources<</Font<</F1 5 0 R>>>>/Contents 4 0 R>>endobj
4 0 obj<</Length {length}>>stream
{content}
endstream
endobj
5 0 obj<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>endobj
trailer<</Root 1 0 R/Size 6>>
"""


def write_pdf(path: Path, text: str) -> None:
    """A one-page PDF small enough to write by hand and real enough to render.

    No xref table: mutool and pdfinfo both rebuild one, which is all this needs.
    """
    content = f"BT /F1 24 Tf 20 40 Td ({text}) Tj ET"
    path.write_text(
        PDF_TEMPLATE.format(length=len(content), content=content),
        encoding="latin-1")


def end_to_end() -> int:
    """The judged columns must not depend on how many sets were rendered."""
    script = Path(__file__).resolve().parent / "pdfa_round2_eval.py"
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        source, first, second = (root / d for d in ("source", "first", "second"))
        for folder in (source, first, second):
            folder.mkdir()
        names = ["one.pdf", "two.pdf"]
        for name in names:
            write_pdf(source / name, "Source")
            write_pdf(first / name, "Source")
            write_pdf(second / name, "Changed")
        listing = root / "list.txt"
        listing.write_text("\n".join(names) + "\n", encoding="utf-8")

        def eval_run(out, *extra, after=None):
            command = [sys.executable, str(script), "--source", str(source),
                       "--after", str(after or second), "--list", str(listing),
                       "--out", str(out), "--workers", "2", *extra]
            result = subprocess.run(command, capture_output=True, text=True)
            if result.returncode != 0:
                FAILURES.append(f"end-to-end: {' '.join(extra)} exited "
                                f"{result.returncode}\n{result.stderr}")
                return None
            return json.loads(Path(out).read_text(encoding="utf-8"))

        # The first invocation pays for importing numpy, Pillow and
        # scikit-image, which is a second of the run and none of the work. It
        # goes first so the two timings printed below are comparable.
        prior = eval_run(root / "prior.json")
        two = eval_run(root / "two.json")
        three = eval_run(root / "three.json", "--before", str(first))
        if not (three and two and prior):
            return 1
        check("three sets when a second candidate is named",
              three["rendered_sets_per_document"], 3)
        check("two when it is not", two["rendered_sets_per_document"], 2)

        def judged(summary):
            return [{k: v for k, v in row.items()
                     if k.startswith("after_") or k in ("file", "dpi", "errors")}
                    | {"pages": [{k: v for k, v in page.items()
                                  if k.startswith("after_") or k == "page"}
                                 for page in row["pages"]]}
                    for row in summary["rows"]]

        check("the judged columns do not depend on the sets rendered",
              judged(two), judged(three))

        borrowed = eval_run(root / "borrowed.json",
                            "--baseline-result", str(root / "prior.json"))
        if not borrowed:
            return 1
        check("borrowing changes nothing about the candidate either",
              judged(borrowed), judged(two))
        check("every document borrowed its before",
              borrowed["documents_with_borrowed_before"], len(names))
        check("the borrowed page numbers are the prior candidate's",
              [p["before_mae"] for r in borrowed["rows"] for p in r["pages"]],
              [p["after_mae"] for r in prior["rows"] for p in r["pages"]])
        # ONE PASS, TWO CANDIDATES, THREE SETS (#547).
        #
        # The second candidate is `first`, which differs from `second` on every
        # page, so a file that carried the wrong candidate's columns would say
        # so loudly. And one document is taken away from it: a refusal on one
        # arm must not reach the other arm's file, which is the whole claim of
        # measuring them together.
        (first / names[1]).unlink()
        alone = eval_run(root / "alone.json", after=first)
        also = root / "merged-other.json"
        merged = eval_run(root / "merged.json",
                          "--also-after", str(first), "--also-out", str(also))
        if not (alone and merged):
            return 1
        other = json.loads(also.read_text(encoding="utf-8"))
        check("one pass renders three sets for two candidates",
              (merged["run_rendered_sets_per_document"],
               other["run_rendered_sets_per_document"]), (3, 3))
        check("and each file rests on two of them",
              (merged["rendered_sets_per_document"],
               other["rendered_sets_per_document"]), (2, 2))
        check("the first arm is what it was when it was measured alone",
              judged(merged), judged(two))
        check("and so is the second, refusal and all",
              judged(other), judged(alone))
        check("the refusal belongs to the arm that refused",
              (merged["documents_with_errors"], other["documents_with_errors"]),
              (0, 1))
        print(f"end-to-end: {three['seconds']:.2f}s for three sets, "
              f"{two['seconds']:.2f}s for two, "
              f"{borrowed['seconds']:.2f}s borrowing a before, "
              f"{merged['seconds']:.2f}s for two candidates in one pass "
              f"against {two['seconds'] + alone['seconds']:.2f}s in two")
    return 0


def main() -> int:
    if "--end-to-end" in sys.argv:
        end_to_end()
    else:
        one_candidate_renders_two_sets()
        two_candidates_share_one_source_render()
        a_borrowed_candidate_is_pinned_to_its_source()
        a_self_comparison_is_not_a_result()
        a_refused_document_does_not_fail_the_measurement()
    for failure in FAILURES:
        print(f"FAIL {failure}")
    if FAILURES:
        return 1
    print("OK one candidate renders two sets, two candidates share one source "
          "render, a borrowed before is pinned to its source, and a refused "
          "document is not a failed measurement")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
