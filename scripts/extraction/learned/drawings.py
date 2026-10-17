#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""What a page draws around a line: the line network's vector inputs (#574).

    python3 scripts/extraction/learned/drawings.py --self-test
    python3 scripts/extraction/learned/drawings.py --write-parity <fixture.json>
    python3 scripts/extraction/learned/drawings.py --check-dump <records.jsonl[.gz]>

The Python twin of `pdf_engine::learned::drawings::line_drawings`, value for
value; `learned_drawings_match_python` holds the two together on the fixture
this writes. A record's `rulings` and `fills` are dump /10's.
"""
from __future__ import annotations

import json
import random
import sys

FEATURES = ["rule_above_em", "rule_below_em", "underlined", "framed", "filled"]
# The dump key `line_vector`'s row, T3's schema on #574.
VECTOR_FEATURES = FEATURES
RULE_FAR_EM = 5.0
RULE_OVERLAP = 0.5
UNDERLINE_EM = 0.4
UNDERLINE_OVERLAP = 0.6
FRAME_EM = 3.0


def _overlap(a0, a1, b0, b1):
    return max(min(a1, b1) - max(a0, b0), 0.0)


def line_drawings(bbox, size, rulings, fills):
    """bbox [x0, y0, x1, y1] (y up); rulings [[x0, y0, x1, y1], ...]; fills the same."""
    x0, y0, x1, y1 = bbox
    em = max(size, 1.0)
    width = max(x1 - x0, 1e-6)
    height = max(y1 - y0, 1e-6)
    horizontal = [r for r in rulings if abs(r[3] - r[1]) < 1e-6]
    vertical = [r for r in rulings if abs(r[2] - r[0]) < 1e-6]

    def spans(r, share):
        return _overlap(x0, x1, r[0], r[2]) >= share * width

    above = below = RULE_FAR_EM
    underlined = 0.0
    for r in horizontal:
        if not spans(r, RULE_OVERLAP):
            continue
        if r[1] >= y1:
            above = min(above, (r[1] - y1) / em)
        elif r[1] <= y0:
            below = min(below, (y0 - r[1]) / em)
            if y0 - r[1] <= UNDERLINE_EM * em and spans(r, UNDERLINE_OVERLAP):
                underlined = 1.0
    reach = FRAME_EM * em

    def covers_y(r):
        return _overlap(y0, y1, r[1], r[3]) >= 0.5 * height

    left = any(r[0] <= x0 and x0 - r[0] <= reach and covers_y(r) for r in vertical)
    right = any(r[0] >= x1 and r[0] - x1 <= reach and covers_y(r) for r in vertical)
    framed = 1.0 if left and right and above * em <= reach and below * em <= reach else 0.0
    filled = 0.0
    for f in fills:
        filled = max(filled, _overlap(x0, x1, f[0], f[2]) * _overlap(y0, y1, f[1], f[3]) / (width * height))
    return [above, below, underlined, framed, min(filled, 1.0)]


def page_vector(record):
    """Dump /10's `line_vector` for one record: per block in the page's order,
    per line in `lines_of` order, the drawings around the union of the line's
    word boxes at its middle word's size."""
    import pathlib
    sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
    from align_labels import lines_of
    words = record.get("words") or []
    rulings = record.get("rulings") or []
    fills = record.get("fills") or []
    rows = []
    for block in record.get("blocks") or []:
        for line in lines_of(block, words):
            if not line:
                continue
            bbox = [min(w["bbox"][0] for w in line), min(w["bbox"][1] for w in line),
                    max(w["bbox"][2] for w in line), max(w["bbox"][3] for w in line)]
            sizes = sorted(w["size"] for w in line)
            rows.append(line_drawings(bbox, sizes[len(sizes) // 2], rulings, fills))
    return rows


def check_dump(path) -> int:
    """Hold this file to the Rust values a dump carries: every line of every record."""
    import gzip
    opener = gzip.open if str(path).endswith(".gz") else open
    lines = records = diffs = 0
    with opener(path, "rt", encoding="utf-8") as f:
        for text in f:
            r = json.loads(text)
            if "line_vector" not in r:
                continue
            records += 1
            mine, theirs = page_vector(r), r["line_vector"]
            if len(mine) != len(theirs):
                diffs += 1
                print(f"  {r.get('page_id', '?')[:12]}: {len(mine)} lines here, {len(theirs)} in the dump")
                continue
            for a, b in zip(mine, theirs):
                lines += 1
                if any(abs(x - y) > 1e-6 for x, y in zip(a, b)):
                    diffs += 1
                    print(f"  {r.get('page_id', '?')[:12]}: {a} against {b}")
    print(f"drawings: {records} record(s), {lines} line(s), {diffs} difference(s)")
    return 1 if diffs or not records else 0


def parity_cases(n=200, seed=574):
    rng = random.Random(seed)
    cases = []
    for _ in range(n):
        x0 = rng.uniform(40, 400); y0 = rng.uniform(40, 700)
        bbox = [x0, y0, x0 + rng.uniform(20, 300), y0 + rng.uniform(6, 14)]
        size = rng.uniform(7, 14)
        rulings = []
        for _ in range(rng.randint(0, 6)):
            if rng.random() < 0.6:
                y = y0 + rng.uniform(-40, 40); a = rng.uniform(20, 500)
                rulings.append([a, y, a + rng.uniform(5, 400), y])
            else:
                x = x0 + rng.uniform(-60, 400); b = y0 + rng.uniform(-30, 10)
                rulings.append([x, b, x, b + rng.uniform(5, 60)])
        fills = [[bbox[0] + rng.uniform(-20, 20), bbox[1] + rng.uniform(-10, 5),
                  bbox[2] + rng.uniform(-40, 20), bbox[3] + rng.uniform(-5, 10)]
                 for _ in range(rng.randint(0, 2))]
        cases.append({"bbox": bbox, "size": size, "rulings": rulings, "fills": fills,
                      "want": line_drawings(bbox, size, rulings, fills)})
    return cases


def self_test() -> int:
    got = line_drawings([100, 500, 300, 510], 10,
                        [[90, 520, 310, 520], [100, 498, 290, 498], [95, 490, 95, 525], [305, 490, 305, 525]],
                        [[90, 495, 310, 515]])
    ok = [round(v, 9) for v in got] == [1.0, 0.2, 1.0, 1.0, 1.0]
    ok &= line_drawings([100, 500, 300, 510], 10, [], []) == [RULE_FAR_EM, RULE_FAR_EM, 0.0, 0.0, 0.0]
    print(f"drawings: {'OK' if ok else 'FAIL'} ({got})")
    return 0 if ok else 1


if __name__ == "__main__":
    if "--self-test" in sys.argv:
        sys.exit(self_test())
    if "--check-dump" in sys.argv:
        sys.exit(check_dump(sys.argv[sys.argv.index("--check-dump") + 1]))
    if "--write-parity" in sys.argv:
        path = sys.argv[sys.argv.index("--write-parity") + 1]
        with open(path, "w") as f:
            json.dump(parity_cases(), f)
        print(f"drawings: wrote {path}")
        sys.exit(0)
    print(__doc__)
