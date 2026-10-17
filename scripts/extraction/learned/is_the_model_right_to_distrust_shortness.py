#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Is the class model WRONG about short blocks, or right in aggregate? (#602)

    is_the_model_right_to_distrust_shortness.py --shards 12 --stride 6

WHERE THIS COMES FROM. The largest cluster on the heading axis is a heading
already its own block, cut in the right place, called a paragraph: 276 blocks
worth 0.0661. Four attempts at more geometry did not move it, and neither did
words -- `does_a_heading_read_like_one.py` found that the STRONGEST separator
of the cluster from real paragraphs is `n_words` at AUC 0.820, a feature the
model has had in front of it the whole time.

A signal that is present and unused is not an input problem. T3's reading of
that, which this measures: in training, "short and not the body face" is
overwhelmingly NOT a heading -- captions, cells, footnotes, item fragments --
so the model learns to distrust shortness, and is right to in aggregate while
being wrong on exactly this population.

WHAT THE ANSWER DECIDES. If the share of `section_header` in that population is
LOW, the model is behaving correctly under its loss and the lever is
per-population weighting rather than another feature. If it is HIGH, the model
is leaving something on the table, which is a different problem with a
different fix. Either way it is one pass over rows that already exist, and it
is the question to ask before training anything.

AND IT PRINTS ITS DENOMINATORS, because every one of the five instruments that
misled this ticket was caught by a count looking wrong to a human rather than
by a test.
"""
from __future__ import annotations

import argparse
import collections
import gzip
import json
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve()
sys.path.insert(0, str(HERE.parent))
ROOT = pathlib.Path.home() / "Datasets" / "pdfluent-training" / "doclaynet"


def read_pages(shards: int, stride: int, split: str):
    available = sorted((ROOT / "features").glob(f"{split}-*.jsonl.gz"))
    chosen = available[::stride][:shards] if stride > 1 else available[:shards]
    for path in chosen:
        with gzip.open(path, "rt") as handle:
            for line in handle:
                yield json.loads(line)


def block_labels(page_ids: set, split: str) -> dict:
    out = {}
    with open(ROOT / "alignment" / f"{split}-rows.jsonl") as handle:
        for text in handle:
            row = json.loads(text)
            if row.get("row") != "block" or row["page_id"] not in page_ids:
                continue
            out[(row["page_id"], row["i"])] = row["label"]
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--shards", type=int, default=12)
    ap.add_argument("--stride", type=int, default=6)
    ap.add_argument("--split", default="train")
    ap.add_argument("--words", type=int, default=6,
                    help="a block is SHORT below this many words")
    args = ap.parse_args()

    pages = list(read_pages(args.shards, args.stride, args.split))
    print(f"pages: {len(pages)}")
    mix = collections.Counter(p.get("category") for p in pages)
    for genre, n in mix.most_common(5):
        print(f"  {genre:<24}{n:>7}  {100 * n / max(1, len(pages)):5.1f}%")
    ids = {p["page_id"] for p in pages}
    labels = block_labels(ids, args.split)
    print(f"block rows: {len(labels)}", flush=True)

    # THE BODY FACE IS THE DOCUMENT'S, counted by characters and not by blocks,
    # the way `document_features.of_document` counts it -- a document with a
    # hundred short captions in one face and twenty long paragraphs in another
    # is set in the second.
    faces: dict[str, collections.Counter] = collections.defaultdict(collections.Counter)
    for page in pages:
        doc = page.get("document") or page["page_id"]
        for block in page.get("blocks") or []:
            face = block.get("font")
            if face:
                faces[doc][face] += max(1, int(block.get("n_chars") or 0))
    body = {doc: counter.most_common(1)[0][0] for doc, counter in faces.items() if counter}

    def body_face(doc: str) -> str:
        """`""` and not `None` for a document no block of which names a font.

        `document_features.of_document` defaults `body_face` to `""`, so a
        fontless block there IS in the body face. Returning `None` here made
        `(font or "") != None` true for every block of such a document and put
        all of them in the off-the-body-face population, which is the one whose
        heading share this file is about (codex, #602).
        """
        return body.get(doc, "")

    # Four populations, so the share can be read against something.
    population = collections.Counter()
    heading = collections.Counter()
    what_they_are: dict[str, collections.Counter] = collections.defaultdict(collections.Counter)
    for page in pages:
        doc = page.get("document") or page["page_id"]
        face_of_body = body_face(doc)
        for block in page.get("blocks") or []:
            label = labels.get((page["page_id"], block.get("i")))
            if label is None:
                continue
            words = len((block.get("text") or "").split())
            short = words < args.words
            own_face = (block.get("font") or "") != face_of_body
            key = ("short" if short else "long") + \
                  (", off the body face" if own_face else ", in the body face")
            population[key] += 1
            if label == "section_header":
                heading[key] += 1
            else:
                what_they_are[key][label or "(none)"] += 1

    print(f"\n{'population':34s}{'blocks':>10s}{'section_header':>16s}{'share':>9s}")
    for key in ("short, off the body face", "short, in the body face",
                "long, off the body face", "long, in the body face"):
        n = population[key]
        if not n:
            continue
        print(f"{key:34s}{n:10d}{heading[key]:16d}{100 * heading[key] / n:8.1f}%")
    total = sum(population.values())
    got = sum(heading.values())
    print(f"{'every block':34s}{total:10d}{got:16d}{100 * got / max(1, total):8.1f}%")

    key = "short, off the body face"
    if population[key]:
        print(f"\nWHAT THE {population[key]} SHORT BLOCKS OFF THE BODY FACE ACTUALLY ARE:")
        for name, n in what_they_are[key].most_common(8):
            print(f"  {name:<26}{n:>8}  {100 * n / population[key]:5.1f}%")

    # AND WHAT IS LEFT ONCE THE PASS HAS REMOVED WHAT IT ALREADY KNOWS. The
    # confusors in that population are not mysteries: a table cell is what
    # `table_region` finds (#600), a running head is what the furniture pass
    # finds (#571), and a picture's data labels are the shape both were built
    # to refuse. So the heading question does not have to be asked on the raw
    # population -- and the share it is asked on is what its loss is made of.
    already = [("table cells", ["table"]),
               ("+ pictures", ["picture"]),
               ("+ formulas", ["formula"]),
               ("+ furniture", ["page_header", "page_footer"])]
    left = population[key]
    heads = heading[key]
    print(f"\n{'the heading question, asked on what is left':46s}"
          f"{'blocks':>9s}{'headings':>10s}{'share':>8s}")
    print(f"{'the raw population':46s}{left:9d}{heads:10d}"
          f"{100 * heads / max(1, left):7.1f}%")
    for name, labels_gone in already:
        for label in labels_gone:
            left -= what_they_are[key].get(label, 0)
        print(f"{'  without ' + name:46s}{left:9d}{heads:10d}"
              f"{100 * heads / max(1, left):7.1f}%")
    print(f"\nagainst {100 * got / max(1, total):.1f} per cent over every block. "
          "The confusors are\nwhat other passes already identify, so the share "
          "the class model is asked\nabout is a question of ORDER rather than "
          "of features.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
