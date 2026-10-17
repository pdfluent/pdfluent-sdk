"""What a line is set in, against what the page's body is set in (#574).

A heading on a financial report is often the body's size and not bold by the
font's own flag: `Note 3: Employee Benefits` in DIN-MediumAlternate over a
DIN-Regular body, `23 Contingent liabilities` in Rotis Regular and another
colour over a Rotis Light body. On T6's rig-like proxy 57 per cent of the
headings the engine missed carry a heavier face than the body's and 55 per
cent another colour; eight per cent are larger. So these read the line against
the page's own body, not against an absolute bold:

- `weight_vs_body`: the line's weight minus the body's, in hundreds, from the
  face's name (Light 300, Regular 400, Medium 500, Bold 700 ...), or the bold
  flag where the name says nothing;
- `colour_vs_body`: the share of the line's characters in a colour other than
  the body's;
- `face_vs_body`: the share in a face other than the body's;
- `family_vs_body`: the share in another family (the name before its weight);
- `style_share`: the share of the page's characters set in the line's own
  face, colour and size -- rare on the page is what a heading is.

The body is the face and colour with the most characters on the page. Every
value is computed the same way by `learned::style` in the engine.
"""
from __future__ import annotations

import collections
import re

from align_labels import lines_of

STYLE_FEATURES = ["weight_vs_body", "colour_vs_body", "face_vs_body", "family_vs_body",
                  "style_share"]

SUBSET = re.compile(r"^[A-Z]{6}\+")
# First match wins, so the compound names come before the words inside them.
WEIGHTS = [("extralight", 200), ("ultralight", 200), ("semibold", 600), ("demibold", 600),
           ("extrabold", 800), ("ultrabold", 800), ("hairline", 100), ("thin", 100),
           ("light", 300), ("medium", 500), ("demi", 600), ("heavy", 800), ("black", 900),
           ("bold", 700), ("book", 400), ("regular", 400), ("roman", 400), ("normal", 400)]


def face(word: dict) -> str:
    return SUBSET.sub("", word.get("font") or "")


def family(name: str) -> str:
    return re.split(r"[-,]", name, maxsplit=1)[0]


def weight(word: dict) -> int:
    name = face(word).lower()
    for token, value in WEIGHTS:
        if token in name:
            return value
    return 700 if word.get("bold") else 400


def colour(word: dict) -> tuple:
    return tuple(word.get("color") or ())


def page_style(record: dict) -> list[list[float]]:
    """One row of `STYLE_FEATURES` per line, blocks in the page's order and
    lines in `lines_of`'s -- the order of `train_heading_lines.page_lines`."""
    words = record.get("words") or []
    chars = collections.Counter()
    styles = collections.Counter()
    for w in words:
        n = len(w.get("text") or "")
        chars[(face(w), colour(w))] += n
        styles[(face(w), colour(w), round(w.get("size") or 0.0, 1))] += n
    total = sum(chars.values())
    if total:
        # Ties to the first in the page's order, so the engine picks the same.
        best = max(chars.values())
        body_face, body_colour = next(k for k in chars if chars[k] == best)
    else:
        body_face, body_colour = "", ()
    body_weight = next((weight(w) for w in words if face(w) == body_face), 400)
    out = []
    for block in record.get("blocks") or []:
        for line in lines_of(block, words):
            n = sum(len(w.get("text") or "") for w in line) or 1
            share = lambda pred: sum(len(w.get("text") or "") for w in line if pred(w)) / n
            mean_weight = sum(weight(w) * len(w.get("text") or "") for w in line) / n
            own = collections.Counter()
            for w in line:
                own[(face(w), colour(w), round(w.get("size") or 0.0, 1))] += len(w.get("text") or "")
            mine = max(own, key=own.get) if own else None
            out.append([
                (mean_weight - body_weight) / 100.0,
                share(lambda w: colour(w) != body_colour),
                share(lambda w: face(w) != body_face),
                share(lambda w: family(face(w)) != family(body_face)),
                styles[mine] / total if mine and total else 0.0,
            ])
    return out


def parity_cases(n=120, seed=574) -> list[dict]:
    """Random pages for the Rust twin to read: blocks of stacked lines, each
    word in one of a few faces (a subset prefix, a weight in the name, none),
    colours and sizes, the bold flag set at random. `want` is this file's
    reading of each page, one row per line."""
    import random
    rng = random.Random(seed)
    faces = ["ABCDEF+DIN-Regular", "DIN-MediumAlternate", "RotisSansSerif-Light",
             "RotisSansSerif", "Helvetica-Bold", "Times-Roman", "Arial,Bold", None]
    colours = [[0, 0, 0], [35, 31, 32], [0, 84, 159], None]
    cases = []
    for _ in range(n):
        words, blocks, y = [], [], 760.0
        for b in range(rng.randint(1, 5)):
            blocks.append({"i": b})
            for _line in range(rng.randint(1, 4)):
                size = rng.choice([8.0, 9.5, 10.0, 12.0, 14.25])
                x = 72.0
                for _w in range(rng.randint(1, 6)):
                    text = "w" * rng.randint(1, 9)
                    width = size * 0.5 * len(text)
                    words.append({"text": text, "bbox": [x, y - size, x + width, y],
                                  "block": b, "size": size, "bold": rng.random() < 0.3,
                                  "font": rng.choice(faces), "color": rng.choice(colours)})
                    x += width + size * 0.3
                y -= size * 1.4
            y -= 10.0
        record = {"words": words, "blocks": blocks}
        cases.append({"words": words, "n_blocks": len(blocks), "want": page_style(record)})
    return cases


def check_dump(path) -> int:
    """Hold this file to the Rust values a dump carries in `line_style`."""
    import gzip
    import json
    opener = gzip.open if str(path).endswith(".gz") else open
    records = lines = diffs = 0
    with opener(path, "rt", encoding="utf-8") as f:
        for text in f:
            r = json.loads(text)
            if "line_style" not in r:
                continue
            records += 1
            mine, theirs = page_style(r), r["line_style"]
            if len(mine) != len(theirs):
                diffs += 1
                continue
            for a, b in zip(mine, theirs):
                lines += 1
                diffs += any(abs(x - y) > 1e-9 for x, y in zip(a, b))
    print(f"style: {records} record(s), {lines} line(s), {diffs} difference(s)")
    return 1 if diffs or not records else 0


def self_test() -> int:
    """A heavier face in another colour over a lighter body reads as such."""
    body = {"text": "running", "bbox": [72, 700, 120, 710], "block": 0, "size": 10.0,
            "bold": False, "font": "ABCDEF+DIN-Regular", "color": [0, 0, 0]}
    rows = [dict(body, bbox=[72, 700 - 12 * k, 120, 710 - 12 * k]) for k in range(6)]
    head = {"text": "Note", "bbox": [72, 740, 100, 750], "block": 1, "size": 10.0,
            "bold": False, "font": "DIN-MediumAlternate", "color": [0, 84, 159]}
    got = page_style({"words": rows + [head], "blocks": [{"i": 0}, {"i": 1}]})
    ok = got[-1][:4] == [1.0, 1.0, 1.0, 0.0] and got[0][:4] == [0.0, 0.0, 0.0, 0.0]
    print(f"style: {'OK' if ok else 'FAIL'} ({got[-1]})")
    return 0 if ok else 1


if __name__ == "__main__":
    import json
    import sys
    if "--self-test" in sys.argv:
        sys.exit(self_test())
    if "--check-dump" in sys.argv:
        sys.exit(check_dump(sys.argv[sys.argv.index("--check-dump") + 1]))
    if "--write-parity" in sys.argv:
        path = sys.argv[sys.argv.index("--write-parity") + 1]
        with open(path, "w") as f:
            json.dump(parity_cases(), f)
        print(f"style: wrote {path}")
        sys.exit(0)
    print(__doc__)
