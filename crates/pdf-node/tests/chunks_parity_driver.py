#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""This binding's chunks as JSON, for the Node parity suite.

``tests/chunks_parity.test.js`` runs this with fixture paths and compares what
it prints against its own ``iterChunks``: the same number of chunks and the
same texts, in both modes and with ``hierarchical`` on and off, plus the
per-page ``toc_items`` and ``tables`` in markdown mode. Plain ``import`` on
purpose: the suite probes first that the interpreter's ``pdfluent`` is this
tree's (``chunks.py`` byte-identical, same version; see
``chunks_parity_guard.js``), so a stale install reads as a skip rather than as
parity.
"""

import json
import sys

from pdfluent.chunks import iter_chunks


def main(paths):
    cases = []
    for fixture in paths:
        for mode in ("blocks", "markdown"):
            for hierarchical in (False, True):
                chunks = list(
                    iter_chunks(
                        fixture, mode=mode, hierarchical=hierarchical
                    )
                )
                case = {
                    "fixture": fixture,
                    "mode": mode,
                    "hierarchical": hierarchical,
                    "texts": [c.text for c in chunks],
                }
                if mode == "markdown":
                    case["toc_items"] = [
                        c.metadata["toc_items"] for c in chunks
                    ]
                    case["tables"] = [c.metadata["tables"] for c in chunks]
                cases.append(case)
    json.dump({"cases": cases}, sys.stdout)


main(sys.argv[1:])
