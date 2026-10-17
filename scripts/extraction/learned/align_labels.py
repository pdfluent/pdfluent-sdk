#!/usr/bin/env python3
"""Put DocLayNet's regions onto OUR blocks, and say what the joining loses (#599).

    align_labels.py --features out.jsonl --labels doclaynet-labels.jsonl
    align_labels.py --self-test

WHY A JOIN IS A MEASUREMENT AND NOT A STEP

A learned classifier is trained on rows that carry a label, and the label comes
from somebody else's idea of where a block begins. DocLayNet draws a region
round what a human saw; we cut blocks out of the lines a PDF drew. Most of the
time those agree, and the interesting question is the rest: a region we split
in two, two regions we merged into one block, a region we emitted nothing for.
Each of those is a row that cannot be labelled honestly, and a training set
that hides them teaches the model our own cutting mistakes as if they were the
page's.

So this prints the loss per class before anything is trained, and it prints it
in the units the axes are scored in: regions, not pixels.

WHAT COUNTS AS THE SAME THING

The overlap is measured on OUR SPANS and not on the boxes. A block's box is the
hull of its lines, so a paragraph that wraps round a figure has a box covering
the figure too, and box overlap would hand the figure's caption to the
paragraph. The spans are where the ink is: a block belongs to the region that
holds most of its ink, and a region is covered by the blocks whose ink it
holds.

THE CLASSES ARE DOCLAYNET'S ELEVEN, NOT THE RIG'S FIVE

`scripts/ci/loop/structured_bench.py` maps the eleven onto five because the
axes score five things, and that mapping folds Caption into paragraph. This
file keeps the eleven: a caption is exactly the family a heading rule promotes
by mistake, and a training set that calls it body text cannot teach the
difference it exists to learn.
"""

from __future__ import annotations

import argparse
import collections
import gzip
import json
import pathlib
import sys

# DocLayNet's own eleven, by the integer its annotation carries.
DOCLAYNET = {
    1: "caption", 2: "footnote", 3: "formula", 4: "list_item",
    5: "page_footer", 6: "page_header", 7: "picture", 8: "section_header",
    9: "table", 10: "text", 11: "title",
}

# How much of a block's ink has to sit in a region before the region may claim
# it. Two thirds, because a block that straddles a boundary is the case this
# file exists to COUNT rather than to resolve: at half, a block cut across two
# regions would be handed to whichever is a hair bigger and the loss would go
# unreported.
CLAIM = 2.0 / 3.0


def label_of(region: dict) -> str:
    """The class of one region, whatever spelling the labels file uses.

    DocLayNet's own annotation carries the category as an INTEGER, and a driver
    that passes it through unchanged is being faithful rather than lazy. A
    table keyed on a mix of `8` and `"section_header"` prints two rows for one
    class and raises on the first `:s` besides. (codex, #1956)
    """
    label = region.get("label")
    if isinstance(label, bool):
        return str(label)
    if isinstance(label, int):
        return DOCLAYNET.get(label, f"category {label}")
    return str(label) if label is not None else "unlabelled"


def frame_of(page: dict) -> tuple[float, float] | None:
    """The frame DocLayNet's own boxes are drawn in, if the dump said so."""
    frame = page.get("label_frame") or {}
    width, height = frame.get("coco_width"), frame.get("coco_height")
    if width and height:
        return float(width), float(height)
    return None


def to_user_space(box: list[float], page: dict, frame: tuple[float, float]) -> list[float]:
    """One DocLayNet box in the space our blocks are in.

    DocLayNet draws on a square image of 1025 by 1025 whatever the page is, in
    `[x, y, w, h]` from the TOP left; our blocks are in PDF user space with y
    up. So the page is scaled PER AXIS and the y is flipped.

    Per axis rather than uniformly with padding, and that is a measurement
    rather than a reading of the documentation: over the 546 regions of 40
    validation pages, per-axis scaling puts a region on one of our blocks with
    a median IoU of 0.567 and uniform scaling with padding 0.101. A square
    frame over a page that is not square leaves no other honest reading.
    """
    page_width, page_height = float(page["width"]), float(page["height"])
    scale_x, scale_y = page_width / frame[0], page_height / frame[1]
    x, y, width, height = box
    top, bottom = y * scale_y, (y + height) * scale_y
    return [x * scale_x, page_height - bottom,
            (x + width) * scale_x, page_height - top]


def regions_of(page: dict, assume: tuple[float, float] | None = None) -> list[dict]:
    """The page's regions, in our space, or a refusal saying what is missing.

    A CONVERSION IS NEVER ASSUMED IN SILENCE. A frame that is wrong by a factor
    produces exactly the table a bad model produces -- every region a little
    beside every block -- and nothing in the output would say which of the two
    it was. So a page whose dump did not record the frame is refused unless the
    caller names one on the command line, where it stays visible.
    """
    labels = page.get("labels")
    if labels is None:
        return page.get("regions") or []
    frame = frame_of(page) or assume
    if frame is None:
        raise SystemExit(
            f"[align] page {page.get('page_id')} carries DocLayNet boxes and no "
            "`label_frame`, so the scale between them and our blocks is not "
            "known. Re-dump with a driver that records it, or pass "
            "--assume-coco WxH to say which frame you are taking on faith.")
    return [{"label": label_of({"label": region.get("category_id",
                                                    region.get("label"))}),
             "bbox": to_user_space(region["bbox"], page, frame)}
            for region in labels]


def area(box: list[float]) -> float:
    return max(0.0, box[2] - box[0]) * max(0.0, box[3] - box[1])


def overlap(one: list[float], other: list[float]) -> float:
    return (max(0.0, min(one[2], other[2]) - max(one[0], other[0]))
            * max(0.0, min(one[3], other[3]) - max(one[1], other[1])))


def ink_of(block: dict, words: list[dict]) -> list[dict]:
    """The words of one block: where its ink is, rather than where its hull is."""
    return [w for w in words if w.get("block") == block.get("i")]


def claim(block: dict, words: list[dict], regions: list[dict]) -> tuple[str | None, float]:
    """The region that holds most of a block's ink, and the share it holds."""
    ink = ink_of(block, words)
    total = sum(area(w["bbox"]) for w in ink) or area(block["bbox"])
    if total <= 0.0:
        return None, 0.0
    best, share = None, 0.0
    for region in regions:
        held = sum(overlap(w["bbox"], region["bbox"]) for w in ink)
        if not ink:
            held = overlap(block["bbox"], region["bbox"])
        if held / total > share:
            best, share = region, held / total
    if best is None or share < CLAIM:
        return None, share
    return label_of(best), share


def lines_of(block: dict, words: list[dict]) -> list[list[dict]]:
    """The words of one block grouped into lines, top to bottom.

    By the baseline rather than by an index, because `Block` keeps no line
    indices -- it carries cloned spans, and a span may hold a whole line or a
    word of one. Two words are on one line when their boxes overlap
    vertically by more than half of the shorter of the two, which is what
    separates a line from the next at any leading and still holds a
    superscript on the line it belongs to.
    """
    ink = sorted(ink_of(block, words), key=lambda w: (-w["bbox"][3], w["bbox"][0]))
    lines: list[list[dict]] = []
    for word in ink:
        box = word["bbox"]
        for line in lines:
            # AGAINST THE LINE'S OWN SPAN, not against the word that opened it:
            # a line grows as its words arrive, and a superscript opening one
            # would otherwise decide the whole line's height.
            low = min(w["bbox"][1] for w in line)
            high = max(w["bbox"][3] for w in line)
            shorter = min(box[3] - box[1], high - low) or 1.0
            share = (min(box[3], high) - max(box[1], low)) / shorter
            if share > 0.5:
                line.append(word)
                break
        else:
            lines.append([word])
    for line in lines:
        line.sort(key=lambda w: w["bbox"][0])
    return in_reading_order([piece for line in lines
                             for piece in across_a_column(line)])


# How wide a gap inside a line has to be, in ems of the line's own height,
# before it is a column boundary rather than a space.
#
# MEASURED, not chosen. Over 250 validation pages, the widest gap inside a line
# that crosses two side-by-side regions has a median of 1.77 em and the widest
# gap inside every other line has a median of 0.27. At 1.5 em the cut catches
# 91.5 per cent of the crossing lines and touches 7.2 per cent of the others;
# at 1.0 it catches 98.9 and touches 8.5; at 2.0 it catches 47.5 (#602).
COLUMN_GAP_EM = 1.5


def across_a_column(line: list[dict]) -> list[list[dict]]:
    """One y-grouped line, cut where it crosses from one column into another.

    WHY A LINE IS NOT A LINE. Words are grouped by their vertical overlap, so
    on a two-column page the left column's line and the right column's line at
    the same height come out as ONE line. Everything downstream then reads that
    invention: the line features (its width, its x, the gap to its neighbour)
    describe a line that is not on the page, and the join cannot give it to a
    region because it lies half in one and half in another.

    It is not a rare shape. Of the lines no region would claim, 81.5 per cent
    straddle two regions and 77.4 per cent of those are side by side -- and
    over 300 validation pages the fix takes the unlabelled lines from 707 to
    294 and quadruples the buried headings that are visible line by line at
    all, from 6 to 25, every one of them with a labelled line above it. That
    was the shortage `heading_starts` starved on (#602).
    """
    if len(line) < 2:
        return [line]
    heights = sorted(w["bbox"][3] - w["bbox"][1] for w in line)
    height = heights[len(heights) // 2] or 1.0
    out: list[list[dict]] = []
    run = [line[0]]
    for before, word in zip(line, line[1:]):
        if (word["bbox"][0] - before["bbox"][2]) / height >= COLUMN_GAP_EM:
            out.append(run)
            run = [word]
        else:
            run.append(word)
    out.append(run)
    return out


def in_reading_order(pieces: list[list[dict]]) -> list[list[dict]]:
    """The pieces of a block, column by column and top to bottom inside each.

    WHY ORDER IS PART OF THE CUT. A cut is an INDEX, and everything downstream
    reads it as a monotonic boundary: `parts_of` puts every line at or after the
    cut in the part after it. Split by height alone, a two-column block comes
    out interleaved -- left row 1, right row 1, left row 2 -- so no index
    separates the left column from the right, and a cut meant for one column
    takes half of the other with it (codex, #1992). Sorted into reading order,
    a contiguous range of indices is a contiguous piece of the page again.

    COLUMNS ARE FOUND BY OVERLAP, not by a grid: a piece joins a column when it
    shares more than half of the narrower of the two x-ranges with it. A block
    of one column therefore comes back exactly as it went in, which is what
    keeps this change invisible everywhere it is not needed.
    """
    if len(pieces) < 2:
        return pieces
    columns: list[dict] = []
    for piece in pieces:
        left = min(w["bbox"][0] for w in piece)
        right = max(w["bbox"][2] for w in piece)
        for column in columns:
            width = min(right - left, column["right"] - column["left"]) or 1.0
            shared = min(right, column["right"]) - max(left, column["left"])
            if shared / width > 0.5:
                column["left"] = min(column["left"], left)
                column["right"] = max(column["right"], right)
                column["pieces"].append(piece)
                break
        else:
            columns.append({"left": left, "right": right, "pieces": [piece]})
    columns.sort(key=lambda c: c["left"])
    return [piece
            for column in columns
            for piece in sorted(column["pieces"],
                                key=lambda p: -max(w["bbox"][3] for w in p))]


def claimed_by(line: list[dict], region: dict) -> bool:
    """Whether a line belongs to a region, by the same two thirds as a block."""
    ink = sum(area(w["bbox"]) for w in line) or 1.0
    return sum(overlap(w["bbox"], region["bbox"]) for w in line) / ink >= CLAIM


def align_page(page: dict, labels: dict,
               assume: tuple[float, float] | None = None) -> dict:
    """One page joined, with what the join lost named per class."""
    words = page.get("words") or []
    regions = labels.get("regions") or regions_of(page, assume)
    rows, loss, line_rows = [], collections.Counter(), []
    covered: dict[int, list[int]] = {i: [] for i, _ in enumerate(regions)}
    for block in page.get("blocks") or []:
        ink = ink_of(block, words)
        total = sum(area(w["bbox"]) for w in ink) or area(block["bbox"])
        held = []
        for index, region in enumerate(regions):
            if ink:
                share = sum(overlap(w["bbox"], region["bbox"]) for w in ink)
            else:
                share = overlap(block["bbox"], region["bbox"])
            held.append(share / total if total > 0 else 0.0)
            if share > 0.0:
                covered[index].append(block.get("i"))
        best = max(range(len(regions)), key=lambda i: held[i]) if regions else None
        if best is None or held[best] < CLAIM:
            loss["a block no region claims"] += 1
            label, region_index, share = None, None, (held[best] if best is not None else 0.0)
        else:
            label, region_index, share = label_of(regions[best]), best, held[best]
        # WHICH region, and not only which class: a list's boundaries are the
        # question of which ITEM owns a block, and two items of one list carry
        # the same label. (T5, #599)
        rows.append({"page_id": page.get("page_id"), "i": block.get("i"),
                     "kind": block.get("kind"), "label": label,
                     "region": region_index, "share": round(share, 4)})
        # PER LINE, for the classes whose structure is inside the block: an
        # item opens where a region opens, and a continuation line does not.
        # Lines are grouped from the word boxes, because `Block` keeps no line
        # index of its own. (T5, #599)
        # ONCE PER BLOCK: the openings below ask about the block's other lines,
        # and re-grouping them inside the loop is a page of words regrouped per
        # line -- which on a shard of a thousand pages is the whole run.
        lines = lines_of(block, words)
        for at, line in enumerate(lines):
            owner, owned = None, 0.0
            ink = sum(area(w["bbox"]) for w in line) or 1.0
            for index, region in enumerate(regions):
                got = sum(overlap(w["bbox"], region["bbox"]) for w in line)
                if got > owned:
                    owner, owned = index, got
            # THE SAME TWO THIRDS A BLOCK IS HELD TO. A line split evenly over
            # two regions belongs to neither, and handing it to whichever is a
            # hair bigger -- or to the first, on a tie -- would put an `opens`
            # on a line that opens nothing. (codex, #1956)
            if owner is not None and owned / ink < CLAIM:
                owner = None
            # ONE OPENING PER REGION, AND IT IS THE FIRST LINE IN ORDER. This
            # asked whether the line sits at or above every other line of the
            # region, which two fragments at the SAME height both answer yes --
            # so a line split by a wide gap inside one region produced two
            # openings and taught `list_opens` to cut inside a single item
            # (codex, #1992). The lines are in reading order, so "first" is a
            # question about the order and not about the geometry.
            opens = owner is not None and at == next(
                (k for k, other in enumerate(lines)
                 if claimed_by(other, regions[owner])), -1)
            line_rows.append({"page_id": page.get("page_id"), "i": block.get("i"),
                              "line": at, "region": owner,
                              "label": label_of(regions[owner]) if owner is not None else None,
                              "opens": bool(opens)})
    # WHO OWNS A REGION is asked from the region's side: the block that holds
    # most of the region's own ink. A block claims the region its ink is in
    # (above); a region is owned by the block that covers it. The two are not
    # the same question, and the difference is where the losses live.
    per_class: collections.Counter = collections.Counter()
    claimed: collections.Counter = collections.Counter()
    for row in rows:
        if row["region"] is not None:
            claimed[row["region"]] += 1
    owns: dict[int, list[int]] = collections.defaultdict(list)
    for index, region in enumerate(regions):
        best, most = None, 0.0
        for block in page.get("blocks") or []:
            ink = ink_of(block, words)
            got = (sum(overlap(w["bbox"], region["bbox"]) for w in ink) if ink
                   else overlap(block["bbox"], region["bbox"]))
            if got > most:
                best, most = block.get("i"), got
        if best is not None and area(region["bbox"]) > 0 and most / area(region["bbox"]) >= 0.25:
            owns[best].append(index)
    # EVERY REGION IS COUNTED ONCE, in the order the shapes exclude each other:
    # nothing covers it; its owner owns another region of the same class (two
    # items read as one); more than one of our blocks is claimed by it (one
    # heading read as two); nothing of ours is claimed by it at all (the
    # heading merged into the paragraph under it, which the block's side calls
    # a paragraph honestly). Counting a region under two of those would inflate
    # the loss and hide which repair it is waiting for.
    # The loss of a merge is the regions it BURIED, not the group: one block
    # holding two items keeps one of them, and the row that is gone is the
    # other. Counted once per group, and the regions of the group are then
    # not counted again by the rules below.
    merged: set[int] = set()
    for owned in owns.values():
        counts = collections.Counter(label_of(regions[i]) for i in owned)
        for label, count in counts.items():
            if count < 2:
                continue
            # ONE PER REGION BURIED, not one per group: three items read as
            # one block lose two rows, and a count of one would say the same
            # as a pair does. (codex, #1956)
            loss[f"{label}: merged into one block with another of its class"] += count - 1
            merged.update(i for i in owned if label_of(regions[i]) == label)
    for index, region in enumerate(regions):
        blocks = covered[index]
        per_class[label_of(region)] += 1
        label = label_of(region)
        if index in merged:
            continue
        if not blocks:
            loss[f"{label}: no block of ours"] += 1
        elif claimed[index] > 1:
            loss[f"{label}: split over more than one block of ours"] += 1
        elif not claimed[index]:
            loss[f"{label}: swallowed by a block of another class"] += 1
    return {"rows": rows, "lines": line_rows, "loss": loss, "regions": per_class}


def report(pages: list[dict], labels: dict[str, dict],
           assume: tuple[float, float] | None = None) -> dict:
    """Every page joined, and the loss counted per class."""
    loss, regions, rows = collections.Counter(), collections.Counter(), 0
    labelled, line_rows = collections.Counter(), 0
    for page in pages:
        page_labels = labels.get(page.get("page_id")) or {}
        done = align_page(page, page_labels, assume)
        loss.update(done["loss"])
        regions.update(done["regions"])
        rows += len(done["rows"])
        line_rows += len(done["lines"])
        for row in done["rows"]:
            if row["label"]:
                labelled[row["label"]] += 1
    return {"pages": len(pages), "blocks": rows, "lines": line_rows,
            "regions": regions, "labelled": labelled, "loss": loss}


def table_of(done: dict) -> str:
    out = [f"{done['pages']} page(s), {done['blocks']} block(s) of ours, "
           f"{done['lines']} line(s)\n",
           f"{'class':16s} {'regions':>8s} {'blocks labelled':>16s}"]
    for name, count in sorted(done["regions"].items()):
        out.append(f"{name:16s} {count:8d} {done['labelled'][name]:16d}")
    out.append("\nwhat the join loses")
    if not done["loss"]:
        out.append("  nothing: every region has exactly one block and every "
                   "block has a region")
    for why, count in done["loss"].most_common():
        out.append(f"  {count:6d}  {why}")
    return "\n".join(out)


def read_jsonl(path: pathlib.Path) -> list[dict]:
    """One JSON object per line, gzipped or not.

    The dumps are kept gzipped: a shard is 110 MB of JSONL and 17 MB zipped,
    and the training split is the difference between 7 GB and 1 GB on a disk
    whose push gate stops every lane on the machine below 35 GB free.
    """
    # split("\n"), not splitlines(): JSON may carry U+2028 raw (#598).
    if path.suffix == ".gz":
        with gzip.open(path, "rt") as lines:
            return [json.loads(line) for line in lines if line.strip()]
    return [json.loads(line) for line in path.read_text().split("\n") if line.strip()]


def self_test() -> int:
    """The three shapes the join can take, on boxes small enough to read."""
    failures: list[str] = []

    def check(name: str, got, want, why: str) -> None:
        if got != want:
            failures.append(f"{name}: got {got!r}, want {want!r} -- {why}")

    def word(x0, y0, x1, y1, block):
        return {"bbox": [x0, y0, x1, y1], "block": block}

    # One heading and one paragraph, each in its own region: nothing is lost.
    page = {"page_id": "p1",
            "blocks": [{"i": 0, "kind": "paragraph", "bbox": [10, 90, 90, 100]},
                       {"i": 1, "kind": "paragraph", "bbox": [10, 40, 90, 80]}],
            "words": [word(10, 90, 90, 100, 0), word(10, 40, 90, 80, 1)]}
    labels = {"p1": {"regions": [
        {"label": "section_header", "bbox": [8, 88, 92, 102]},
        {"label": "text", "bbox": [8, 38, 92, 82]}]}}
    done = report([page], labels)
    check("a clean page loses nothing", dict(done["loss"]), {},
          "two blocks, two regions, one each")
    check("and each block takes its region's class",
          dict(done["labelled"]), {"section_header": 1, "text": 1},
          "the label is the region that holds the block's ink")

    # A block of ours that runs across two regions: the heading was merged into
    # the paragraph under it, which is the case #584's promoter is about.
    merged = {"page_id": "p2",
              "blocks": [{"i": 0, "kind": "paragraph", "bbox": [10, 40, 90, 100]}],
              "words": [word(10, 90, 90, 100, 0), word(10, 40, 90, 80, 0)]}
    done = report([merged], labels | {"p2": labels["p1"]})
    check("the heading merged into its paragraph is named as swallowed",
          done["loss"]["section_header: swallowed by a block of another class"], 1,
          "four fifths of the block's ink is body text, so the row is labelled "
          "`text` honestly and the heading has no row at all -- which is the "
          "loss this file exists to print, and which counting regions without "
          "any block would miss")
    check("and the paragraph it swallowed the heading into is not lost",
          [why for why in done["loss"] if why.startswith("text")], [],
          "the body of the region is there and carries its own class")

    # A block cut across two regions in equal halves: neither may claim it, and
    # the row is dropped rather than guessed. At a claim threshold of a half
    # this row would go to whichever region is a hair bigger, and the model
    # would be taught our own cut as if it were the page's.
    halved = {"page_id": "p5",
              "blocks": [{"i": 0, "kind": "paragraph", "bbox": [10, 40, 90, 100]}],
              "words": [word(10, 88, 90, 100, 0), word(10, 40, 90, 52, 0)]}
    done = report([halved], {"p5": labels["p1"]})
    check("a block halved between two regions is claimed by neither",
          done["loss"]["a block no region claims"], 1,
          "half of its ink is in each, so no region holds the two thirds this "
          "file asks for before it labels a row")
    check("and both regions say what became of them",
          sorted(why for why in done["loss"] if ":" in why),
          ["section_header: swallowed by a block of another class",
           "text: swallowed by a block of another class"],
          "a region covered only by a block nobody could label has no row of "
          "its own either, and saying so twice is right: two classes lost a row")

    # Two list items read as one block: the continuation line of the first
    # swallowed the second. Both regions have a block, that block is labelled
    # with their own class, and neither the split count nor the swallowed
    # count can see it -- which is why it is counted from the region's side.
    # (T5, #599)
    two_items = {"page_id": "p6",
                 "blocks": [{"i": 0, "kind": "list_item", "bbox": [10, 40, 90, 100]}],
                 "words": [word(10, 90, 90, 100, 0), word(10, 70, 90, 80, 0),
                           word(10, 40, 90, 60, 0)]}
    done = report([two_items], {"p6": {"regions": [
        {"label": "list_item", "bbox": [8, 68, 92, 102]},
        {"label": "list_item", "bbox": [8, 38, 92, 62]}]}})
    check("two items read as one block are named",
          done["loss"]["list_item: merged into one block with another of its class"], 1,
          "one block owning two regions of one class is the shape a list loses "
          "most items to, and it is invisible from the block's side")

    # And the lines of that block say which region each belongs to, so that
    # `opens an item` can be trained without a line index in the engine.
    page_lines = align_page(two_items, {"regions": [
        {"label": "list_item", "bbox": [8, 68, 92, 102]},
        {"label": "list_item", "bbox": [8, 38, 92, 62]}]})["lines"]
    check("every line says which region it is in",
          [(row["line"], row["region"]) for row in page_lines],
          [(0, 0), (1, 0), (2, 1)],
          "the first two lines are the first item and the third is the second, "
          "which is what a model has to learn to cut")
    check("and which of them opens its region",
          [row["opens"] for row in page_lines], [True, False, True],
          "an item opens where its region opens; a continuation line does not")

    # Lines are grouped by how much their boxes overlap, and the threshold is
    # what tells a tight leading from a superscript. Here the second line's box
    # reaches half a point into the first: at any threshold near zero the two
    # become one line and every per-line row is wrong, while the superscript --
    # two thirds inside its own line -- has to stay where it belongs.
    tight = {"page_id": "p7",
             "blocks": [{"i": 0, "kind": "paragraph", "bbox": [10, 80, 90, 102]}],
             "words": [word(10, 90, 60, 100, 0), word(62, 96, 70, 102, 0),
                       word(10, 80, 90, 90.5, 0)]}
    rows = align_page(tight, {"regions": [
        {"label": "text", "bbox": [8, 78, 92, 104]}]})["lines"]
    # THE LINE THAT IS TWO LINES. Two columns at the same height are one
    # y-group and must come back as two (#602).
    wide = [{"text": "left", "bbox": [10, 90, 40, 100], "block": 0},
            {"text": "right", "bbox": [200, 90, 240, 100], "block": 0}]
    got = lines_of({"i": 0, "bbox": [10, 90, 240, 100]}, wide)
    check("a line crossing a column comes back as two",
          [[w["text"] for w in piece] for piece in got], [["left"], ["right"]],
          "on a two-column page the two columns' lines sit at the same height, "
          "and grouped by height alone they are one line that is on no page")
    near = [{"text": "one", "bbox": [10, 90, 40, 100], "block": 0},
            {"text": "two", "bbox": [45, 90, 70, 100], "block": 0}]
    check("an ordinary space is not a column",
          len(lines_of({"i": 0, "bbox": [10, 90, 70, 100]}, near)), 1,
          "a word gap of half an em must not split a line, or every line of "
          "justified text becomes several")

    # READING ORDER, because a cut is an index. Two columns of two rows each:
    # split by height alone they interleave, and no index separates the columns.
    # NOT CALLED `word`: this function lives beside a `word(x0, y0, x1, y1,
    # block)` helper that every other fixture in this file uses, and shadowing
    # it silently feeds those fixtures a text where a coordinate goes.
    def says(text, x0, y0):
        return {"text": text, "bbox": [x0, y0, x0 + 30, y0 + 10], "block": 0}
    two_columns = [says("left-top", 10, 90), says("right-top", 200, 90),
                   says("left-low", 10, 70), says("right-low", 200, 70)]
    got = [[w["text"] for w in piece]
           for piece in lines_of({"i": 0, "bbox": [10, 70, 240, 100]}, two_columns)]
    check("a two-column block comes back column by column",
          got, [["left-top"], ["left-low"], ["right-top"], ["right-low"]],
          "a cut is an index and `parts_of` reads it as a monotonic boundary, "
          "so interleaved pieces make a cut meant for one column take half of "
          "the other with it")

    # ONE OPENING PER REGION. Two fragments of one line, one region, one item.
    done = align_page(
        {"page_id": "p", "width": 100, "height": 100,
         "words": [says("first", 10, 90), says("second", 60, 90)],
         "blocks": [{"i": 0, "kind": "list_item", "text": "first second",
                     "bbox": [10, 90, 90, 100]}]},
        {"regions": [{"label": "list_item", "bbox": [8, 88, 95, 102]}]})
    check("a line split inside one region opens it once",
          sum(1 for row in done["lines"] if row["opens"]), 1,
          "two fragments at one height both answered `opens`, which teaches "
          "the list model to cut inside a single item")

    check("a leading of half a point is still two lines",
          [row["line"] for row in rows], [0, 1],
          "two lines whose boxes touch are two lines; grouping them would put "
          "every per-line row on the wrong line")

    # DocLayNet's own annotation carries the category as an integer, and a
    # driver that passes it through is being faithful. The table has to read
    # the same class either way, or it prints two rows for one class -- and
    # formats an int with `:s` on the way. (codex, #1956)
    by_number = report([page], {"p1": {"regions": [
        {"label": 8, "bbox": [8, 88, 92, 102]},
        {"label": 10, "bbox": [8, 38, 92, 82]}]}})
    check("a category given as a number reads as its class",
          dict(by_number["labelled"]), {"section_header": 1, "text": 1},
          "DocLayNet numbers its eleven, and `8` is a section header wherever "
          "the driver did not spell it out")

    # Three items read as one block bury two rows, not one.
    three = {"page_id": "p8",
             "blocks": [{"i": 0, "kind": "list_item", "bbox": [10, 20, 90, 100]}],
             "words": [word(10, 90, 90, 100, 0), word(10, 60, 90, 70, 0),
                       word(10, 20, 90, 40, 0)]}
    done = report([three], {"p8": {"regions": [
        {"label": "list_item", "bbox": [8, 88, 92, 102]},
        {"label": "list_item", "bbox": [8, 58, 92, 72]},
        {"label": "list_item", "bbox": [8, 18, 92, 42]}]}})
    check("a three-way merge buries two rows",
          done["loss"]["list_item: merged into one block with another of its class"], 2,
          "one block holding three items keeps one of them, so two rows are "
          "gone and a count of one would say what a pair says")

    # A line split evenly over two regions belongs to neither.
    straddle = {"page_id": "p9",
                "blocks": [{"i": 0, "kind": "paragraph", "bbox": [10, 40, 90, 60]}],
                "words": [word(10, 40, 50, 60, 0), word(52, 40, 90, 60, 0)]}
    rows = align_page(straddle, {"regions": [
        {"label": "text", "bbox": [8, 38, 51, 62]},
        {"label": "caption", "bbox": [51, 38, 92, 62]}]})["lines"]
    check("a line straddling two regions is given to neither",
          [(row["region"], row["opens"]) for row in rows], [(None, False)],
          "half of its ink is in each, and an `opens` on such a line would "
          "teach a cut that nobody drew")

    # A region of ours that nothing covers: the letter-spaced capitals we read
    # as separate characters and never emit as a block.
    absent = {"page_id": "p3", "blocks": [], "words": []}
    done = report([absent], {"p3": {"regions": [
        {"label": "title", "bbox": [8, 88, 92, 102]}]}})
    check("a region with no block of ours is named",
          done["loss"]["title: no block of ours"], 1,
          "that is a row the training set cannot have, and the axis reports it "
          "as a miss")

    # A region our cut split in two: one heading emitted as two blocks.
    split = {"page_id": "p4",
             "blocks": [{"i": 0, "kind": "paragraph", "bbox": [10, 95, 50, 100]},
                        {"i": 1, "kind": "paragraph", "bbox": [52, 95, 90, 100]}],
             "words": [word(10, 95, 50, 100, 0), word(52, 95, 90, 100, 1)]}
    done = report([split], {"p4": {"regions": [
        {"label": "section_header", "bbox": [8, 88, 92, 102]}]}})
    check("a region split over two blocks is named",
          done["loss"]["section_header: split over more than one block of ours"], 1,
          "both rows carry the same label and the model would learn to cut "
          "where we cut")

    # The conversion from DocLayNet's square image to our page, per axis and
    # with the y flipped. One region covering the top left quarter of the image
    # is the top left quarter of the page, whatever the page's shape.
    page = {"page_id": "p10", "width": 680.0, "height": 870.0,
            "label_frame": {"coco_width": 1025, "coco_height": 1025},
            "labels": [{"category_id": 8, "bbox": [0.0, 0.0, 512.5, 512.5]}],
            "blocks": [], "words": []}
    got = regions_of(page)
    check("a region is scaled per axis and flipped",
          [round(v, 3) for v in got[0]["bbox"]], [0.0, 435.0, 340.0, 870.0],
          "the top left quarter of a 1025-square image is the top left quarter "
          "of the page, and in our space the top of the page is y = height")
    check("and it carries its class, not its number",
          got[0]["label"], "section_header",
          "DocLayNet numbers its eleven and the table is keyed on the class")

    # And a page whose dump recorded no frame is refused rather than guessed.
    silent = dict(page)
    silent.pop("label_frame")
    try:
        regions_of(silent)
    except SystemExit as refusal:
        check("a page with no frame is refused", "label_frame" in str(refusal), True,
              "a scale that is wrong by a factor produces exactly the table a "
              "bad model produces, and nothing in the output would say which")
    else:
        failures.append("a page with no frame was converted on an assumption")

    for line in failures:
        print(f"FAIL {line}", file=sys.stderr)
    if failures:
        print(f"\n{len(failures)} problem(s) in the label join.", file=sys.stderr)
        return 1
    print("✓ the join names what it loses: a block no region claims, a region "
          "no block covers, and a region split over blocks")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--features", type=pathlib.Path, nargs="+",
                        help="the feature dump(s), one JSON object per page, "
                             "plain or gzipped; several are read ONE AT A TIME "
                             "so that a training half does not have to fit in "
                             "memory at once")
    parser.add_argument("--labels", type=pathlib.Path,
                        help="the regions, one JSON object per page")
    parser.add_argument("--rows", type=pathlib.Path,
                        help="write the joined rows as JSONL: one object per "
                             "block and one per line, each carrying the page, "
                             "the region and the class")
    parser.add_argument("--assume-coco", metavar="WxH",
                        help="the frame DocLayNet's boxes are in, where the "
                             "dump did not record it; taken on faith and "
                             "printed with the table")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if not args.features:
        parser.error("--features, with --labels where they are in a file of "
                     "their own, or --self-test")
    assume = None
    if args.assume_coco:
        width, _, height = args.assume_coco.partition("x")
        assume = (float(width), float(height))
    labels = ({page["page_id"]: page for page in read_jsonl(args.labels)}
              if args.labels else {})
    if args.rows:
        # The rows a model trains on, written as they are joined: a block row
        # carries the class the region gave it and the region it came from, a
        # line row carries whether it OPENS that region. A row whose label is
        # null is kept and says so, because dropping it here would hide from
        # the training set exactly what the table above counts.
        with args.rows.open("w") as out:
            for path in args.features:
                for page in read_jsonl(path):
                    done = align_page(page, labels.get(page.get("page_id")) or {},
                                      assume)
                    for row in done["rows"]:
                        out.write(json.dumps(dict(row, row="block")) + "\n")
                    for row in done["lines"]:
                        out.write(json.dumps(dict(row, row="line")) + "\n")
    # ONE SHARD AT A TIME. A training half is four gigabytes of pages; read at
    # once it is the largest thing on the machine, and this one has been
    # killing background work all evening.
    whole: dict = {}
    for path in args.features:
        done = report(read_jsonl(path), labels, assume)
        for key, value in done.items():
            if isinstance(value, int):
                whole[key] = whole.get(key, 0) + value
            else:
                whole.setdefault(key, collections.Counter()).update(value)
    print(table_of(whole))
    if assume:
        print(f"\nthe frame {args.assume_coco} was taken on faith: the dump "
              "recorded none")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
