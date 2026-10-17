#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""What the page raster shows around a line: the line network's image inputs (#574).

    python3 scripts/extraction/learned/raster.py --self-test
    python3 scripts/extraction/learned/raster.py --write-parity <fixture.json>

The Python twin of `pdf_engine::learned::raster::line_image`, value for
value; `learned_raster_match_python` holds the two together on the fixture
this writes. There is no `--check-dump`: the dump carries the features but
not the raster they were read from, so there is nothing to check against.
"""
from __future__ import annotations

import base64
import json
import math
import random
import sys

# The dump key `line_image`'s row, T3's schema on #574. ONE constant, so a
# rename is a one-line change.
IMAGE_FEATURES = ["ink_density", "ink_luma", "bg_luma",
                  "rule_px_above", "rule_px_below", "left_ink"]
FEATURES = IMAGE_FEATURES
# A pixel is ink when it differs from the line's background by at least
# this: pale type, and light type on a dark band, are ink too. An absolute
# `g < 160` read 12 per cent of the proxy's lines as blank -- grey body text
# set light, and the white-on-grey headings of annual reports.
INK_CONTRAST = 48
# How far the background window grows around the line box, in ems, on every side.
BG_GROW_EM = 0.3
# How far above and below the line box a pixel rule is looked for, in ems.
RULE_REACH_EM = 1.0
# The deepest run of spanning rows that is still a rule, in pixel rows.
RULE_MAX_ROWS = 2
# How much of the line box's columns one unbroken run of ink in a row must
# cover for the row to be a rule.
RULE_SPAN_SHARE = 0.6
# How far left of the line's left edge the marker band reaches, in ems.
LEFT_BAND_EM = 2.0
# How far left of the line's left edge the marker band starts, in ems.
LEFT_GAP_EM = 0.15


def _pixel_box(bbox, width, height, page_height_pt):
    """Columns [c0, c1) and rows [r0, r1) of a point box, top-left origin.

    The same arithmetic as the engine's `pixel_box` at 72 dpi: columns
    `floor(x0)..ceil(x1)`, rows `H - ceil(y1)..H - floor(y0)`, clamped to the
    raster, with the point box's y clamped to the page first."""
    x0, y0, x1, y1 = bbox
    top = page_height_pt if page_height_pt > 0 else math.inf
    y0 = min(max(y0, 0.0), top)
    y1 = min(max(y1, 0.0), top)
    c0 = min(max(math.floor(min(x0, x1)), 0), width)
    c1 = min(max(math.ceil(max(x0, x1)), 0), width)
    r0 = min(max(height - math.ceil(max(y0, y1)), 0), height)
    r1 = min(max(height - math.floor(min(y0, y1)), 0), height)
    return c0, c1, r0, r1


def line_image(bbox, size, gray, width, height, page_height_pt):
    """The image features of one line, in `IMAGE_FEATURES` order.

    bbox [x0, y0, x1, y1] (y up); size its type size in points; gray the
    72-dpi Gray8 page raster as a flat list or bytes, row-major, top-left
    origin, 0 is black."""
    x0, y0, x1, y1 = bbox
    em = max(size, 1.0)
    # The line's ground: the middle of the ring the grown box adds around the
    # line box, which the line's own type does not reach. A ring clipped away
    # at the page's edge falls back to the whole grown box.
    grow = BG_GROW_EM * em
    g0, g1, gr0, gr1 = _pixel_box([x0 - grow, y0 - grow, x1 + grow, y1 + grow],
                                 width, height, page_height_pt)
    c0, c1, r0, r1 = _pixel_box(bbox, width, height, page_height_pt)
    around = sorted(gray[r * width + c] for r in range(gr0, gr1) for c in range(g0, g1))
    ring = sorted(gray[r * width + c] for r in range(gr0, gr1) for c in range(g0, g1)
                  if not (r0 <= r < r1 and c0 <= c < c1))
    ground = ring[len(ring) // 2] if ring else (around[len(around) // 2] if around else 255)

    def is_ink(g):
        return abs(g - ground) >= INK_CONTRAST

    ink = ink_sum = n = 0
    for r in range(r0, r1):
        for c in range(c0, c1):
            n += 1
            g = gray[r * width + c]
            if is_ink(g):
                ink += 1
                ink_sum += g
    ink_density = ink / n if n else 0.0
    ink_luma = (ink_sum / ink) / 255.0 if ink else 1.0

    bg = [g for g in around if not is_ink(g)]
    bg_luma = bg[len(bg) // 2] / 255.0 if bg else 1.0

    if c0 == c1 or r0 == r1:
        rule_px_above = rule_px_below = 0.0
    else:
        reach = RULE_REACH_EM * em
        ncols = c1 - c0

        # A rule is unbroken: the row's longest run of ink, not its count,
        # must cover RULE_SPAN_SHARE of the line.
        def spans(r):
            longest = run = 0
            for c in range(c0, c1):
                if is_ink(gray[r * width + c]):
                    run += 1
                    longest = max(longest, run)
                else:
                    run = 0
            return longest / ncols >= RULE_SPAN_SHARE

        # A rule is thin: the run of spanning rows it sits in, on its side of
        # the line box, is at most RULE_MAX_ROWS deep.
        def thin(r, floor, ceil):
            if not spans(r):
                return False
            run, up, down = 1, r, r + 1
            while up > floor and run <= RULE_MAX_ROWS and spans(up - 1):
                up -= 1
                run += 1
            while down < ceil and run <= RULE_MAX_ROWS and spans(down):
                down += 1
                run += 1
            return run <= RULE_MAX_ROWS

        lo = min(r0, math.ceil(max(0.0, r0 - reach)))
        hi = max(r1, min(height, math.floor(r1 + reach - 1.0) + 1))
        rule_px_above = 1.0 if any(thin(r, 0, r0) for r in range(lo, r0)) else 0.0
        rule_px_below = 1.0 if any(thin(r, r1, height) for r in range(r1, hi)) else 0.0

    b0, b1, br0, br1 = _pixel_box([x0 - LEFT_BAND_EM * em, y0, x0 - LEFT_GAP_EM * em, y1],
                                 width, height, page_height_pt)
    band_ink = band_n = 0
    for r in range(br0, br1):
        for c in range(b0, b1):
            band_n += 1
            if is_ink(gray[r * width + c]):
                band_ink += 1
    left_ink = band_ink / band_n if band_n else 0.0
    return [ink_density, ink_luma, bg_luma, rule_px_above, rule_px_below, left_ink]


def _fill(gray, width, c0, c1, r0, r1, g):
    for r in range(r0, r1):
        for c in range(c0, c1):
            gray[r * width + c] = g


def parity_cases(n=100, seed=574):
    rng = random.Random(seed)
    cases = []
    for _ in range(n):
        width = rng.randint(60, 120)
        height = rng.randint(40, 70)
        ground = rng.choice([255, 255, 255, 235, 220, 200])
        gray = [ground] * (width * height)
        x0 = rng.uniform(-10, width - 30)
        y0 = rng.uniform(-5, height - 15)
        x1 = x0 + rng.uniform(20, 80)
        y1 = y0 + rng.uniform(6, 14)
        bbox = [x0, y0, x1, y1]
        size = rng.uniform(7, 14)
        em = max(size, 1.0)
        c0, c1, r0, r1 = _pixel_box(bbox, width, height, height)
        # Dark text-like blobs in the line box.
        for _ in range(rng.randint(1, 6)):
            if c1 - c0 < 2 or r1 - r0 < 1:
                break
            a = rng.randint(c0, c1 - 1)
            b = rng.randint(a + 1, min(c1, a + rng.randint(2, 20)))
            t = rng.randint(r0, r1 - 1)
            _fill(gray, width, a, b, t, min(r1, t + rng.randint(1, 4)),
                  rng.randint(0, 100))
        # A rule row above or below some lines: full-width or partial.
        if rng.random() < 0.4 and c1 > c0 and r1 > r0:
            reach = RULE_REACH_EM * em
            lo = min(r0, math.ceil(max(0.0, r0 - reach)))
            hi = max(r1, min(height, math.floor(r1 + reach - 1.0) + 1))
            rows = list(range(lo, r0)) + list(range(r1, hi))
            if rows:
                share = rng.choice([1.0, 1.0, rng.uniform(0.3, 0.9)])
                span = max(1, int((c1 - c0) * share))
                start = rng.randint(c0, c1 - span) if span < c1 - c0 else c0
                row = rng.choice(rows)
                _fill(gray, width, start, start + span, row,
                      min(height, row + rng.choice([1, 1, 2, 3, 5])),
                      rng.randint(0, 50))
        # A row of letters near some lines: ink broken by gaps, like type.
        if rng.random() < 0.3 and c1 > c0 and r1 > r0:
            row = rng.choice([max(0, r0 - 2), min(height - 1, r1 + 1)])
            c = c0
            while c < c1:
                w = rng.randint(3, 15)
                _fill(gray, width, c, min(c1, c + w), row, row + 1, rng.randint(0, 60))
                c += w + rng.randint(1, 4)
        # A blob in the left band for some lines.
        if rng.random() < 0.4:
            b0, b1, br0, br1 = _pixel_box([x0 - LEFT_BAND_EM * em, y0,
                                           x0 - LEFT_GAP_EM * em, y1],
                                          width, height, height)
            if b1 - b0 >= 2 and br1 - br0 >= 2:
                a = rng.randint(b0, b1 - 2)
                t = rng.randint(br0, br1 - 2)
                _fill(gray, width, a, min(b1, a + rng.randint(2, 8)),
                      t, min(br1, t + rng.randint(2, 8)), rng.randint(0, 80))
        # Clutter anywhere on the page, in or out of the bands.
        for _ in range(rng.randint(0, 3)):
            a = rng.randint(0, width - 1)
            t = rng.randint(0, height - 1)
            _fill(gray, width, a, min(width, a + rng.randint(1, 10)),
                  t, min(height, t + rng.randint(1, 4)), rng.randint(0, 255))
        cases.append({"width": width, "height": height,
                      "raster": base64.b64encode(bytes(gray)).decode(),
                      "bbox": bbox, "size": size,
                      "want": line_image(bbox, size, gray, width, height, height)})
    return cases


def self_test() -> int:
    width, height = 100, 40
    gray = [255] * (width * height)
    # The grown box of [30, 10, 70, 20] at em 10, shaded 220; a 20x10 word
    # of g=100 in the line; 60% rules over and under it; an 8x8 bullet.
    _fill(gray, width, 27, 73, 17, 33, 220)
    _fill(gray, width, 30, 50, 20, 30, 100)
    _fill(gray, width, 30, 54, 15, 16, 0)
    _fill(gray, width, 30, 54, 35, 36, 0)
    _fill(gray, width, 12, 20, 22, 30, 0)
    got = line_image([30, 10, 70, 20], 10, gray, width, height, 40.0)
    want = [0.5, 100 / 255, 220 / 255, 1.0, 1.0, 64 / 190]
    ok = [round(v, 9) for v in got] == [round(v, 9) for v in want]
    blank = line_image([30, 10, 70, 20], 10, [255] * (width * height),
                       width, height, 40.0)
    ok &= blank == [0.0, 1.0, 1.0, 0.0, 0.0, 0.0]
    # White type on a grey band reads as ink, lighter than its ground.
    band = [255] * (width * height)
    _fill(band, width, 20, 80, 10, 40, 180)
    _fill(band, width, 30, 50, 20, 30, 255)
    rev = line_image([30, 10, 70, 20], 10, band, width, height, 40.0)
    ok &= round(rev[0], 9) == 0.5 and rev[1] == 1.0 and round(rev[2], 9) == round(180 / 255, 9)
    print(f"raster: {'OK' if ok else 'FAIL'} ({got}; reversed {rev})")
    return 0 if ok else 1


if __name__ == "__main__":
    if "--self-test" in sys.argv:
        sys.exit(self_test())
    if "--write-parity" in sys.argv:
        path = sys.argv[sys.argv.index("--write-parity") + 1]
        with open(path, "w") as f:
            json.dump(parity_cases(), f)
        print(f"raster: wrote {path}")
        sys.exit(0)
    print(__doc__)
