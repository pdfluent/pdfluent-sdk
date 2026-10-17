#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Write the feature-parity fixture: Python's vectors for a few PDFs (#599).

    cargo build --release -p pdf-learned --bin feature_dump
    python3 scripts/extraction/learned/make_feature_parity.py

THE MODELS WERE TRAINED ON PYTHON'S VECTORS AND RUN ON RUST'S. The training
reads `features.py` and `list_lines.py` over the dump; the engine computes the
same vectors in `pdf_engine::learned`. Any place the two disagree -- a rounding,
a white-space rule, a missing neighbour routed another way -- is a model
reading input it never saw, and it looks exactly like a weak model. So this
writes what Python makes of a handful of real PDFs, and
`crates/pdf-engine/tests/learned_features_match_the_training.rs` requires the
engine to make the same, value for value, NaN for NaN.

Run it again whenever `features.py`, `list_lines.py` or `align_labels.lines_of`
change: the fixture is Python's side of the contract.
"""
from __future__ import annotations

import collections
import json
import math
import pathlib
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[2]
sys.path.insert(0, str(HERE))
from document_features import LOCAL_FEATURES, of_document  # noqa: E402
from features import block_vector  # noqa: E402
from list_lines import heads_on_page, line_vectors, prev_colon  # noqa: E402
import train_heading_lines as hl  # noqa: E402
import train_line_gnn as gnn  # noqa: E402

PDFS = [
    "fixtures/structured.pdf",
    "tests/corpus-mini/multi-page.pdf",
    "tests/corpus-mini/simple.pdf",
    "crates/visual-regression/fixtures/text-spacing.pdf",
    "corpus/fw7.pdf",
    "corpus/sf181.pdf",
    # Real pages as well (#574, parity audit): headings, lists, a patent's
    # two columns and a page with a picture, where the line models' rows
    # have something to say. DocLayNet validation pages, never the rig's.
    "crates/pdf-engine/tests/fixtures/doclaynet/nyse_ssl_2003_p59.pdf",
    "crates/pdf-engine/tests/fixtures/doclaynet/us_patent_7268176_p9.pdf",
    "crates/pdf-engine/tests/fixtures/doclaynet/asx_2003_p56_prose_under_a_table.pdf",
]
OUT = REPO / "crates/pdf-engine/tests/fixtures/learned_feature_parity.json"


def normalise(text: str) -> str:
    return " ".join(text.lower().split())


def clean(values: list[float]) -> list:
    return [None if isinstance(v, float) and math.isnan(v) else v for v in values]


def graph_of(record: dict, local: dict, at_page: int) -> dict:
    """The line graph `train_line_gnn.py` trains on: node rows, the neighbour
    in each direction (-1 for none) and the edge rows, before normalisation."""
    nodes, meta, edges = gnn.page_graph(record, local, at_page)
    neighbours = [[edges[d][i] for d in gnn.DIRECTIONS] for i in range(len(meta))]
    rows = [[f for d in gnn.DIRECTIONS for f in gnn.edge_features(meta, i, edges[d][i])]
            for i in range(len(meta))]
    # A node is its line, its block and its picture values, all above, then
    # its box on the page: only the box is new. Read from the meta the graph
    # was built on, not from the tail of the node row: the row has grown
    # fields past the box since (#574), and the tail is not the box anymore.
    width = record.get("width") or 1.0
    height = record.get("height") or 1.0
    boxes = [[b[0] / width, b[1] / height, b[2] / width, b[3] / height]
             for (_, _, b, _) in meta]
    return {"boxes": [clean(b) for b in boxes], "neighbours": neighbours,
            "edges": [clean(e) for e in rows]}


def main() -> int:
    binary = REPO / "target/release/feature_dump"
    if not binary.exists():
        print(f"build it first: cargo build --release -p pdf-learned --bin feature_dump ({binary})")
        return 2
    documents = []
    for pdf in PDFS:
        done = subprocess.run([str(binary), str(REPO / pdf)], capture_output=True, text=True, check=True)
        records = [json.loads(line) for line in done.stdout.split("\n") if line.strip()]
        # repeats_in_document as the dump driver fills it: the whole PDF is one
        # document, and a block's text counts once per page it is on.
        pages_with = collections.Counter()
        for r in records:
            pages_with.update({normalise(b["text"]) for b in r["blocks"] if normalise(b["text"])})
        pages = []
        # The six the heading lane appended are about the block within its PAGE
        # and its DOCUMENT, so they are computed over the whole PDF at once,
        # the way `of_document` does for training (#599).
        for r in records:
            for b in r["blocks"]:
                key = normalise(b["text"])
                b["repeats_in_document"] = pages_with[key] if key else 0
        local = of_document(records)
        for at_page, r in enumerate(records):
            heads = heads_on_page(r["blocks"], r["words"])
            lines = hl.page_lines(r, local, at_page)
            pages.append({
                "page": r["page"],
                "blocks": [clean(block_vector(b)
                                 + list(local[((r["page_id"], at_page), b["i"])][:len(LOCAL_FEATURES)]))
                           for b in r["blocks"]],
                "lines": [[clean(v) for v in line_vectors(b, r["words"], r.get("body_size") or 1.0, heads,
                                                          prev_colon(r["blocks"], at))]
                          for at, b in enumerate(r["blocks"])],
                # THE LINE MODELS' INPUTS AS WELL (#574, parity audit): the row
                # `heading_lines` and `line_classes` read, the four picture
                # values `caption_lines` adds to it, and the graph `line_gnn`
                # runs on. Each is assembled from the vectors above -- the
                # neighbours, the same-block flags, the NaN of a missing one --
                # and the assembly is where a port goes wrong.
                # Only what the assembly ADDS, to keep the fixture small: the
                # line and its block are already above, value for value, and
                # the test puts the row back together from them.
                "heading_context": [clean(hl.vector(lines, at)[23 + 30:]) for at in range(len(lines))],
                "picture": [clean(hl.picture_features(box, size, r.get("images") or []))
                            for box, size in hl.page_boxes(r)],
                "gnn": graph_of(r, local, at_page),
            })
        documents.append({"pdf": pdf, "pages": pages})
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps({"documents": documents}, sort_keys=True, separators=(",", ":")) + "\n")
    n_blocks = sum(len(p["blocks"]) for d in documents for p in d["pages"])
    n_lines = sum(len(ls) for d in documents for p in d["pages"] for ls in p["lines"])
    print(f"{OUT.relative_to(REPO)}: {len(documents)} PDFs, {n_blocks} blocks, {n_lines} lines")
    return 0


if __name__ == "__main__":
    sys.exit(main())
