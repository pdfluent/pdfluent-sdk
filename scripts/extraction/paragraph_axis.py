#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Where a paragraph ends, measured against the readers that also say (#567).

THE AXIS

`paragraph_agreement`: of the pairs of lines the probe reader put next to each
other on a page, the share we and it answer the same way about -- same
paragraph, or a boundary between them. It is a percentage per document and the
mean of nothing: every pair on every page of the document counts once, so a
document of forty pages is not outvoted by one of two.

Pairs and not blocks, for the same reason `order_agreement` is pairs and not a
sequence (#428): a count of blocks is won by a reading that splits every line,
and a comparison of block texts is won by a reading that joins the whole page.
The pair is the decision the merge actually makes, and it is the one thing two
readers can be asked the same question about.

WHAT IT CANNOT SEE. A reading that puts the right boundaries in the wrong place
-- both readers agree line 4 ends a paragraph, and we agree, and we have also
swallowed a caption three lines up that neither of them sees as body text --
loses only the pairs around the caption. And it says nothing at all about a
document neither reader finds text in.

THE PROBE AND THE ORACLE

The probe is mutool: its `stext` output is a page divided into blocks, each
divided into lines with a box, and the LINES are what the pairs are taken from.
Poppler's `-bbox-layout` gives the same shape, and it is used as the probe only
where mutool produced nothing.

The probe is also the oracle of the first column, and the second reader -- when
there is one -- fills the other two:

  paragraph_agreement        ours against the probe
  agreement_vs_other_reader  ours against the reader that is not the probe
  readers_agree              the probe against that other reader

Per document, which is how `extraction_axes.py` chooses its oracle too. A
document only one reader could read is measured against that reader rather than
dropped, and the run file records which one under `probe`.

The third column is not decoration. Two mature readers disagree with each other
about where a paragraph ends more often than anybody expects, and without their
figure beside ours there is no scale to read ours on -- the same reason
`run_extractor_matrix.py` puts the readers in the table as columns.

A reader is never its own oracle: `readers_agree` is the probe's blocks against
the other reader's, over the probe's line pairs, and a document only one reader
could read carries `null` there rather than a hundred.

MATCHING A LINE TO A BLOCK is by the line's centre point, which is the only
thing three readers with three ideas of a block can be compared through. A point
inside no block of a reading makes that pair uncountable for that reading and is
counted apart, so a run where the matching failed looks like a run where the
matching failed and not like a run where we disagreed.

Usage:
  paragraph_axis.py --set gate --corpus-dir /tmp/gate-corpus \\
      --binary target/release/examples/paragraph_blocks \\
      --out benchmarks/runs/structured/<run-id>.json --progress
  paragraph_axis.py --self-test
"""

from __future__ import annotations

import argparse
import datetime
import json
import pathlib
import re
import shutil
import statistics
import subprocess
import sys
import xml.etree.ElementTree as ET

REPO = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts" / "extraction"))

from extraction_axes import load_set, percentile, platform_key  # noqa: E402

TIMEOUT_S = 60

# How far outside every block of a reading a probe point may fall and still be
# given to the nearest one, in points. A line's centre lands just outside a
# block whose box was computed from glyph bounds rather than from the line boxes
# the probe reader drew; three points is under a line of six-point type and well
# under the gap between two blocks.
MATCH_SLACK_PT = 3.0

# Below this, a document has too few decisions for a percentage to mean
# anything: a page with three lines on it offers two pairs, and one of them
# going the other way moves the figure by fifty points. Recorded, not counted.
MIN_PAIRS = 20

# The line under which a document is in the tail, as the extraction axes use it.
TAIL_PCT = 95.0


def run_tool(cmd: list[str]) -> str | None:
    """A reader's output, or None when it failed, hung or wrote nothing."""
    try:
        done = subprocess.run(cmd, capture_output=True, timeout=TIMEOUT_S, check=False)
    except (subprocess.TimeoutExpired, OSError):
        return None
    if done.returncode != 0:
        return None
    out = done.stdout.decode("utf-8", errors="replace")
    return out or None


def parse_float(value: str | None) -> float | None:
    try:
        return float(value)  # type: ignore[arg-type]
    except (TypeError, ValueError):
        return None


def mutool_reading(pdf: pathlib.Path, mutool: str) -> tuple[dict, dict] | None:
    """mutool's blocks and lines, per page, in its own y-down page space.

    Returns `(blocks, lines)`: `{page: [(id, bbox)]}` and `{page: [bbox]}`.
    """
    out = run_tool([mutool, "draw", "-F", "stext", "-o", "-", str(pdf)])
    if out is None:
        return None
    try:
        root = ET.fromstring(out)
    except ET.ParseError:
        return None
    blocks: dict[int, list] = {}
    lines: dict[int, list] = {}
    for page_index, page in enumerate(root.iter("page")):
        page_blocks: list = []
        page_lines: list = []
        for block in page.iter("block"):
            box = boxed(block.get("bbox"))
            if box is None:
                continue
            block_id = len(page_blocks)
            page_blocks.append((block_id, box))
            for line in block.iter("line"):
                line_box = boxed(line.get("bbox"))
                if line_box is not None:
                    page_lines.append(line_box)
        blocks[page_index] = page_blocks
        lines[page_index] = page_lines
    return blocks, lines


def boxed(bbox: str | None) -> tuple[float, float, float, float] | None:
    """`"x0 y0 x1 y1"` as four numbers."""
    if not bbox:
        return None
    parts = bbox.split()
    if len(parts) != 4:
        return None
    try:
        x0, y0, x1, y1 = (float(p) for p in parts)
    except ValueError:
        return None
    return (min(x0, x1), min(y0, y1), max(x0, x1), max(y0, y1))


def poppler_reading(pdf: pathlib.Path, poppler: str) -> tuple[dict, dict] | None:
    """poppler's blocks and lines, per page, in its own y-down page space."""
    out = run_tool([poppler, "-bbox-layout", str(pdf), "-"])
    if out is None:
        return None
    # The output is XHTML with a DOCTYPE and a namespace; the elements that
    # matter are unprefixed inside <doc>, so the namespace is stripped rather
    # than resolved -- poppler has spelled it two ways across releases.
    out = re.sub(r"<!DOCTYPE[^>]*>", "", out, count=1)
    out = re.sub(r'\sxmlns="[^"]*"', "", out, count=1)
    try:
        root = ET.fromstring(out)
    except ET.ParseError:
        return None
    blocks: dict[int, list] = {}
    lines: dict[int, list] = {}
    for page_index, page in enumerate(root.iter("page")):
        page_blocks: list = []
        page_lines: list = []
        for block in page.iter("block"):
            box = cornered(block)
            if box is None:
                continue
            block_id = len(page_blocks)
            page_blocks.append((block_id, box))
            for line in block.iter("line"):
                line_box = cornered(line)
                if line_box is not None:
                    page_lines.append(line_box)
        blocks[page_index] = page_blocks
        lines[page_index] = page_lines
    return blocks, lines


def cornered(element) -> tuple[float, float, float, float] | None:
    """poppler's `xMin`/`yMin`/`xMax`/`yMax` attributes as a box."""
    values = [
        parse_float(element.get(name)) for name in ("xMin", "yMin", "xMax", "yMax")
    ]
    if any(v is None for v in values):
        return None
    x0, y0, x1, y1 = values  # type: ignore[misc]
    return (min(x0, x1), min(y0, y1), max(x0, x1), max(y0, y1))


def our_reading(pdf: pathlib.Path, binary: str) -> tuple[dict, dict] | None:
    """Our blocks, per page, turned into the readers' y-down page space.

    The second element is empty: our reading offers no lines, and the pairs are
    the probe reader's. Returned in the same shape so the three readings can be
    handled by one function.
    """
    out = run_tool([binary, str(pdf)])
    if out is None:
        return None
    heights: dict[int, float] = {}
    blocks: dict[int, list] = {}
    for record in out.split("\n"):
        fields = record.split("\t")
        if fields[0] == "P" and len(fields) >= 5:
            page = int(fields[1])
            heights[page] = float(fields[3])
            blocks.setdefault(page, [])
        elif fields[0] == "B" and len(fields) >= 8:
            page = int(fields[1])
            height = heights.get(page)
            if height is None:
                continue
            x0, y0, x1, y1 = (float(f) for f in fields[3:7])
            # y up to y down: the readers measure from the top of the page.
            box = (x0, height - y1, x1, height - y0)
            blocks.setdefault(page, []).append((int(fields[2]), box))
    return blocks, {}


def group_at(page_blocks: list, point: tuple[float, float]) -> int | None:
    """The block of a reading a probe point falls in, or the nearest within
    `MATCH_SLACK_PT`, or None."""
    x, y = point
    best: tuple[float, int] | None = None
    for block_id, (x0, y0, x1, y1) in page_blocks:
        dx = max(x0 - x, 0.0, x - x1)
        dy = max(y0 - y, 0.0, y - y1)
        distance = max(dx, dy)
        if distance == 0.0:
            return block_id
        if distance <= MATCH_SLACK_PT and (best is None or distance < best[0]):
            best = (distance, block_id)
    return best[1] if best else None


def centre(box: tuple[float, float, float, float]) -> tuple[float, float]:
    return ((box[0] + box[2]) / 2.0, (box[1] + box[3]) / 2.0)


def agreement(probe_lines: dict, a: dict, b: dict) -> tuple[int, int, int]:
    """Two readings judged on the probe's pairs.

    Returns `(agreed, counted, uncountable)`: pairs both readings placed and
    answered the same way, pairs both placed, and pairs at least one of them
    could not place.
    """
    agreed = counted = uncountable = 0
    for page, lines in probe_lines.items():
        blocks_a = a.get(page, [])
        blocks_b = b.get(page, [])
        for first, second in zip(lines, lines[1:]):
            points = (centre(first), centre(second))
            ids_a = tuple(group_at(blocks_a, p) for p in points)
            ids_b = tuple(group_at(blocks_b, p) for p in points)
            if None in ids_a or None in ids_b:
                uncountable += 1
                continue
            counted += 1
            if (ids_a[0] == ids_a[1]) == (ids_b[0] == ids_b[1]):
                agreed += 1
    return agreed, counted, uncountable


def pct(agreed: int, counted: int) -> float | None:
    if counted <= 0:
        return None
    return round(100.0 * agreed / counted, 3)


def choose_readers(mu, po):
    """The probe, and the other reader, out of what each one produced.

    mutool first where it produced lines, poppler where it did not, and the one
    that is left over as the second column. Both `None` when neither reader
    divided the document into paragraphs at all.
    """
    ranked = [
        (name, reading)
        for name, reading in (("mutool", mu), ("poppler", po))
        if reading and any(reading[1].values())
    ]
    first = ranked[0] if ranked else (None, None)
    second = ranked[1] if len(ranked) > 1 else (None, None)
    return first, second


def measure_one(pdf: pathlib.Path, binary: str, mutool: str | None,
                poppler: str | None) -> dict:
    """One document's paragraph agreement, and the readers' own."""
    ours = our_reading(pdf, binary)
    mu = mutool_reading(pdf, mutool) if mutool else None
    po = poppler_reading(pdf, poppler) if poppler else None

    record: dict = {
        "parsed": ours is not None,
        "paragraph_agreement": None,
        "agreement_vs_other_reader": None,
        "readers_agree": None,
        "pairs": 0,
        "uncountable_pairs": 0,
        "blocks": sum(len(v) for v in ours[0].values()) if ours else 0,
        "probe": None,
        "other_reader": None,
    }
    if ours is None:
        record["why"] = "our extractor produced no reading"
        return record

    # THE PROBE IS ALSO THE ORACLE OF THE FIRST COLUMN. Reading the pairs off
    # one reader and scoring them against another is only the same measurement
    # while both produced a reading, and on a document where mutool produced
    # none it is not a measurement at all: `prefilled_f1040.pdf` offers 718
    # countable pairs against poppler and came back with `paragraph_agreement`
    # null, out of the corpus figures entirely, while the poppler number sat in
    # a column the summary does not read (codex, round 1).
    (probe_name, probe), (other_name, other) = choose_readers(mu, po)
    if probe is None:
        record["why"] = "no foreign reader produced a paragraph reading"
        return record
    record["probe"] = probe_name
    record["other_reader"] = other_name

    probe_lines = probe[1]
    agreed, counted, uncountable = agreement(probe_lines, ours[0], probe[0])
    record["paragraph_agreement"] = pct(agreed, counted)
    record["pairs"] = counted
    record["uncountable_pairs"] = uncountable
    if other:
        agreed_o, counted_o, _ = agreement(probe_lines, ours[0], other[0])
        record["agreement_vs_other_reader"] = pct(agreed_o, counted_o)
        agreed_r, counted_r, _ = agreement(probe_lines, probe[0], other[0])
        record["readers_agree"] = pct(agreed_r, counted_r)

    if record["pairs"] < MIN_PAIRS:
        record["short"] = True
        record["why"] = (
            f"the probe reader offered {record['pairs']} line pairs, under the "
            f"{MIN_PAIRS} below which a percentage is noise; the figure is "
            "recorded but not counted in the corpus numbers"
        )
    return record


def summarise(documents: dict) -> dict:
    """The corpus shape: what was measured, the middle, and the tail."""
    assessed = [
        d for d in documents.values()
        if d.get("paragraph_agreement") is not None and not d.get("short")
    ]
    ours = sorted(d["paragraph_agreement"] for d in assessed)
    readers = sorted(
        d["readers_agree"] for d in assessed if d.get("readers_agree") is not None
    )
    beyond_argument = [
        d for d in assessed
        if d.get("readers_agree") is not None
        and d["readers_agree"] >= TAIL_PCT > d["paragraph_agreement"]
    ]
    return {
        "documents": len(documents),
        "parsed": sum(1 for d in documents.values() if d.get("parsed")),
        "assessed": len(assessed),
        "too_few_pairs": sum(1 for d in documents.values() if d.get("short")),
        "no_reader_reading": sum(
            1 for d in documents.values() if d.get("probe") is None
        ),
        "pairs": sum(d.get("pairs", 0) for d in documents.values()),
        "uncountable_pairs": sum(
            d.get("uncountable_pairs", 0) for d in documents.values()
        ),
        "agreement_median_pct": round(statistics.median(ours), 3) if ours else None,
        "agreement_p5_pct": percentile(ours, 0.05) if ours else None,
        "agreement_below_95pct": sum(1 for v in ours if v < TAIL_PCT),
        "readers_agree_median_pct": (
            round(statistics.median(readers), 3) if readers else None
        ),
        "documents_readers_agree_we_do_not": len(beyond_argument),
    }


def measure(args) -> int:
    entries = load_set(args.set)
    corpus = pathlib.Path(args.corpus_dir)
    binary = args.binary
    if not pathlib.Path(binary).exists() and shutil.which(binary) is None:
        print(f"FAIL the block dumper {binary} is not there; build it first")
        return 2
    mutool = shutil.which(args.mutool)
    poppler = shutil.which(args.poppler)
    if not mutool and not poppler:
        print("FAIL neither mutool nor pdftotext is on PATH; there is no probe")
        return 2
    if not mutool or not poppler:
        print(f"NOTE only one foreign reader: {'mutool' if mutool else 'poppler'}")

    documents: dict = {}
    missing: list[str] = []
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

    run = {
        "run_id": args.run_id,
        "capability": "structured-extraction",
        "axis": "paragraph_agreement",
        "set": args.set,
        "tuning_or_holdout": "tuning",
        "date": datetime.date.today().isoformat(),
        "platform": platform_key(),
        "machine_class": args.machine_class,
        "commit": args.commit,
        "tools": {
            "blocks": binary,
            "mutool": args.mutool if mutool else None,
            "poppler": args.poppler if poppler else None,
        },
        "probe": (
            "mutool's stext lines where it produced any, poppler's -bbox-layout "
            "lines otherwise; a pair is two lines the probe put next to each "
            "other on one page"
        ),
        "summary": summarise(documents),
        "documents": documents,
    }
    out = pathlib.Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    with out.open("w", encoding="utf-8") as f:
        json.dump(run, f, indent=1, sort_keys=True)
        f.write("\n")
    s = run["summary"]
    print(
        f"OK {s['assessed']} document(s) assessed, {s['pairs']} pairs: "
        f"median {s['agreement_median_pct']}, p5 {s['agreement_p5_pct']}, "
        f"{s['agreement_below_95pct']} under {TAIL_PCT}; the readers agree with "
        f"each other at median {s['readers_agree_median_pct']}"
    )
    print(f"   {out}")
    return 0


def self_test() -> int:
    """The arithmetic, on readings written out by hand."""
    failures: list[str] = []

    def check(what: str, got, want) -> None:
        if got != want:
            failures.append(f"{what}: got {got!r}, want {want!r}")

    # Three lines on one page. The probe reader says the first two are one
    # paragraph and the third is another.
    probe_lines = {0: [
        (72.0, 100.0, 300.0, 110.0),
        (72.0, 112.0, 300.0, 122.0),
        (72.0, 140.0, 300.0, 150.0),
    ]}
    probe_blocks = {0: [(0, (72.0, 100.0, 300.0, 122.0)), (1, (72.0, 140.0, 300.0, 150.0))]}

    same_as_probe = {0: [(0, (72.0, 99.0, 300.0, 123.0)), (1, (72.0, 139.0, 300.0, 151.0))]}
    check("a reading that agrees everywhere", agreement(probe_lines, same_as_probe, probe_blocks),
          (2, 2, 0))

    one_block = {0: [(0, (72.0, 99.0, 300.0, 151.0))]}
    check("a reading that joins the page", agreement(probe_lines, one_block, probe_blocks),
          (1, 2, 0))

    every_line = {0: [
        (0, (72.0, 99.0, 300.0, 111.0)),
        (1, (72.0, 111.5, 300.0, 123.0)),
        (2, (72.0, 139.0, 300.0, 151.0)),
    ]}
    check("a reading that splits every line", agreement(probe_lines, every_line, probe_blocks),
          (1, 2, 0))

    nothing = {0: []}
    check("a reading that placed nothing", agreement(probe_lines, nothing, probe_blocks),
          (0, 0, 2))

    check("a point inside a block", group_at(probe_blocks[0], (100.0, 105.0)), 0)
    check("a point just outside one", group_at(probe_blocks[0], (100.0, 124.0)), 0)
    check("a point far from every block", group_at(probe_blocks[0], (100.0, 400.0)), None)

    # The probe falls back to poppler, and the first column is then poppler's.
    # Read the other way round -- pairs from poppler, agreement against mutool --
    # a document mutool could not read comes back `null` on the axis with its
    # pairs still counted, and drops out of the corpus figures.
    lines_only = ({0: [(0, (0.0, 0.0, 1.0, 1.0))]}, {0: [(0.0, 0.0, 1.0, 1.0)]})
    blocks_only = ({0: [(0, (0.0, 0.0, 1.0, 1.0))]}, {0: []})
    check("both readers read it", choose_readers(lines_only, lines_only)[0][0], "mutool")
    check("mutool found no lines", choose_readers(blocks_only, lines_only)[0][0], "poppler")
    check("and is then not the second column either",
          choose_readers(blocks_only, lines_only)[1][0], None)
    check("neither read it", choose_readers(blocks_only, blocks_only)[0][0], None)

    check("a percentage of nothing", pct(0, 0), None)
    check("a percentage", pct(3, 4), 75.0)

    documents = {
        "good.pdf": {"parsed": True, "paragraph_agreement": 99.0, "readers_agree": 99.5,
                     "pairs": 100, "uncountable_pairs": 0, "probe": "mutool"},
        "tail.pdf": {"parsed": True, "paragraph_agreement": 80.0, "readers_agree": 99.0,
                     "pairs": 50, "uncountable_pairs": 2, "probe": "mutool"},
        "hard.pdf": {"parsed": True, "paragraph_agreement": 80.0, "readers_agree": 70.0,
                     "pairs": 50, "uncountable_pairs": 0, "probe": "mutool"},
        "short.pdf": {"parsed": True, "paragraph_agreement": 50.0, "readers_agree": 50.0,
                      "pairs": 4, "uncountable_pairs": 0, "probe": "mutool", "short": True},
        "blank.pdf": {"parsed": True, "paragraph_agreement": None, "pairs": 0,
                      "uncountable_pairs": 0, "probe": None},
    }
    s = summarise(documents)
    check("every document is counted", s["documents"], 5)
    check("the short one is not assessed", s["assessed"], 3)
    check("and is counted apart", s["too_few_pairs"], 1)
    check("a document without a reader reading is counted apart",
          s["no_reader_reading"], 1)
    check("the tail is reported", s["agreement_below_95pct"], 2)
    check("a document the readers agree about and we do not is counted apart",
          s["documents_readers_agree_we_do_not"], 1)
    check("uncountable pairs are reported", s["uncountable_pairs"], 2)

    for f in failures:
        print(f"FAIL {f}")
    if failures:
        return 1
    print("OK self-test: matching, the pair agreement and the summary all behave")
    return 0


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--set", default="gate", help="which set of the pinned manifest")
    p.add_argument("--corpus-dir")
    p.add_argument("--binary", default="target/release/examples/paragraph_blocks")
    p.add_argument("--out")
    p.add_argument("--run-id", default="")
    p.add_argument("--machine-class", default="dev-macbook-m1pro")
    p.add_argument("--commit", default="")
    p.add_argument("--mutool", default="mutool")
    p.add_argument("--poppler", default="pdftotext")
    p.add_argument("--progress", action="store_true")
    p.add_argument("--self-test", action="store_true")
    args = p.parse_args()

    if args.self_test:
        return self_test()
    if not args.corpus_dir or not args.out:
        p.error("--corpus-dir and --out are required unless --self-test")
    if not args.run_id:
        # The file it is written to, for the reason `extraction_axes.py` gives:
        # two runs on one day over one set would otherwise share an id and the
        # evidence of the second would point at the first (#1918).
        args.run_id = pathlib.Path(args.out).stem
    return measure(args)


if __name__ == "__main__":
    sys.exit(main())
