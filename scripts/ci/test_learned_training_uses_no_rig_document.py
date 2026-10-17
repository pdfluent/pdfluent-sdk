#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""The leakage guard of #598 refuses what it exists to refuse.

A scratch tree with one rig manifest and one training manifest, varied case by
case: a clean manifest passes; a rig page by hash, a rig page by name, and a
DIFFERENT page of a rig document all fail -- the last is the split-by-document
rule; a dataset without a training verdict fails; no rig at all is exit 2, not
a pass. And the real tree passes.

AND PARSEBENCH, WITH THE REAL FILES (#622): its committed manifest and its real
DATASETS.toml row. One of its pages in a training manifest fails by hash, and
any ParseBench page fails by its source alone -- the row refuses training, so a
page that is not even on the rig may not be trained on either.

AND THE COLMERGE HELD-OUT (track A): a held-out PT document or FTN
company in a track-A training manifest fails; the same names pass with
no reference beside the manifests, and under any other source, because
the check is scoped to track A; a record marked never-train fails
either way.

AND THE HEADERBAND HELD-OUT (track A): a held-out company in a
track-A training manifest fails naming the headerband set, and passes
with no headerband reference beside it.
"""
from __future__ import annotations

import json
import pathlib
import subprocess
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[2]
GUARD = REPO / "scripts" / "ci" / "learned_training_uses_no_rig_document.py"

RIG = {"set": "fixture", "pages": [
    {"source": "doclaynet-v1.2/test", "name": "aaa.pdf", "document": "REPORT_2002.pdf",
     "page_no": 3, "sha256": "11" * 32, "split": "holdout"},
]}
DATASETS = '[datasets.doclaynet]\nname = "x"\nlicence = "y"\ntraining_use = "allowed: fixture"\n' \
           '[datasets.omnidocbench]\nname = "x"\nlicence = "y"\ntraining_use = "refused: fixture"\n'
TRACK_DATASETS = (DATASETS
                  + '[datasets.pubtables-1m]\nname = "x"\nlicence = "y"\n'
                  + 'training_use = "allowed: fixture"\n'
                  + '[datasets.fintabnet]\nname = "x"\nlicence = "y"\n'
                  + 'training_use = "allowed: fixture"\n')
HELDOUT = {"meta": {}, "documents": ["PMC9000001"], "companies": ["HELD"]}
HEADERBAND = {"meta": {}, "documents": [], "companies": ["HBCO"]}


def run(documents, rig: dict | None = RIG,
        datasets: str = DATASETS,
        heldout: dict | None = None,
        headerband: dict | None = None) -> tuple[int, str]:
    with tempfile.TemporaryDirectory() as d:
        root = pathlib.Path(d)
        (root / "corpus" / "structured").mkdir(parents=True)
        if rig is not None:
            (root / "corpus" / "structured" / "RIG.json").write_text(json.dumps(rig))
        models = root / "scripts" / "extraction" / "learned" / "models" / "m"
        models.mkdir(parents=True)
        (models / "manifest.json").write_text(json.dumps({"documents": documents}))
        comparison = root / "benchmarks" / "structured" / "comparison"
        comparison.mkdir(parents=True)
        (comparison / "DATASETS.toml").write_text(datasets)
        tables_a = root / "scripts" / "extraction" / "tables_a"
        if heldout is not None:
            tables_a.mkdir(parents=True, exist_ok=True)
            (tables_a / "colmerge_heldout.json").write_text(
                json.dumps(heldout))
        if headerband is not None:
            tables_a.mkdir(parents=True, exist_ok=True)
            (tables_a / "heldout-headerband.json").write_text(
                json.dumps(headerband))
        p = subprocess.run([sys.executable, str(GUARD), "--root", str(root)],
                           capture_output=True, text=True)
        return p.returncode, p.stdout + p.stderr


def doc(**kw) -> dict:
    base = {"source": "doclaynet-v1.2/train", "document": "OTHER.pdf",
            "page_id": "bbb", "sha256": "22" * 32}
    base.update(kw)
    return base


def main() -> int:
    fails = []
    cases = [
        ("a clean manifest passes", [doc()], 0),
        ("a rig page by hash fails", [doc(sha256="11" * 32)], 1),
        ("a rig page by name fails", [doc(page_id="aaa")], 1),
        ("another page of a rig document fails (split by document)",
         [doc(document="REPORT_2002.pdf", page_id="ccc", sha256="33" * 32)], 1),
        ("a dataset that refuses training fails", [doc(source="omnidocbench/x")], 1),
        ("a dataset with no verdict fails", [doc(source="somewhere-else/x")], 1),
        ("a page from the local benchmark copy fails by its path",
         [doc(path="share/pdfluent/pb-tuning/data/p.pdf")], 1),
        ("the repository's fixtures need no verdict", [doc(source="fixtures/*.pdf")], 0),
    ]
    for what, docs, want in cases:
        rc, out = run(docs)
        if rc != want:
            fails.append(f"{what}: exit {rc}, wanted {want}\n{out[-300:]}")

    track = [
        ("a held-out PT document in a track-A manifest fails",
         [doc(source="pubtables-1m/pmc-open-access",
              document="PMC9000001")], 1, True, "colmerge held-out"),
        ("a held-out FTN company in a track-A manifest fails",
         [doc(source="fintabnet/ibm-1.0.0", document="HELD_2019",
              company="HELD")], 1, True, "colmerge held-out"),
        ("clean track-A documents pass",
         [doc(source="pubtables-1m/pmc-open-access",
              document="PMC9000002"),
          doc(source="fintabnet/ibm-1.0.0", document="OTHER_2019",
              company="OTHER")], 0, True, ""),
        ("a held-out document passes with no reference beside it",
         [doc(source="pubtables-1m/pmc-open-access",
              document="PMC9000001")], 0, False, ""),
        ("a held-out document under another source passes",
         [doc(source="doclaynet-v1.2/train",
              document="PMC9000001")], 0, True, ""),
        ("a never-train record fails with a reference",
         [doc(source="pubtables-1m/pmc-open-access",
              document="PMC9000002", train_use="never")], 1, True,
         "train_use never"),
        ("a never-train record fails without one",
         [doc(source="pubtables-1m/pmc-open-access",
              document="PMC9000002", train_use="never")], 1, False,
         "train_use never"),
    ]
    for what, docs, want, with_ref, words in track:
        rc, out = run(docs, datasets=TRACK_DATASETS,
                      heldout=HELDOUT if with_ref else None)
        if rc != want:
            fails.append(f"{what}: exit {rc}, wanted {want}\n{out[-300:]}")
        elif words and words not in out:
            fails.append(f"{what}: exit {rc} but says nothing of it\n"
                         f"{out[-300:]}")

    band = [
        ("a headerband company in a track-A manifest fails",
         [doc(source="fintabnet/ibm-1.0.0", document="HBCO_2019",
              company="HBCO")], 1, HEADERBAND, "headerband held-out"),
        ("a headerband company passes with no headerband reference",
         [doc(source="fintabnet/ibm-1.0.0", document="HBCO_2019",
              company="HBCO")], 0, None, ""),
        ("a colmerge company passes under the headerband reference alone",
         [doc(source="fintabnet/ibm-1.0.0", document="HELD_2019",
              company="HELD")], 0, HEADERBAND, ""),
    ]
    for what, docs, want, ref, words in band:
        rc, out = run(docs, datasets=TRACK_DATASETS, headerband=ref)
        if rc != want:
            fails.append(f"{what}: exit {rc}, wanted {want}\n{out[-300:]}")
        elif words and words not in out:
            fails.append(f"{what}: exit {rc} but says nothing of it\n"
                         f"{out[-300:]}")

    # A MANIFEST THAT NAMES NO DOCUMENTS MUST BE A VERDICT, NOT A TRACEBACK,
    # and the exit code alone cannot tell them apart: the crash this replaces
    # also exited 1. So these check the sentence. Two table models wrote
    # `documents` as a count per split and this guard died on them instead of
    # reading them, which is why it had never checked either one (#600).
    for what, shape in (("a count per split", {"train": 3, "test": 1}),
                        ("a list of bare names", ["REPORT_2002.pdf"])):
        rc, out = run(shape)
        if rc != 1 or "not a list of records" not in out:
            fails.append(f"{what} must be refused in words, not crash: exit {rc}\n{out[-300:]}")
        if "Traceback" in out:
            fails.append(f"{what} crashed the guard:\n{out[-300:]}")
    parsebench = json.loads((REPO / "corpus" / "structured" / "PARSEBENCH.json").read_text())
    real_rows = (REPO / "benchmarks" / "structured" / "comparison" / "DATASETS.toml").read_text()
    page = parsebench["pages"][0]
    rc, out = run([doc(source="doclaynet-v1.2/train", sha256=page["sha256"])],
                  rig=parsebench, datasets=real_rows)
    if rc != 1:
        fails.append(f"a ParseBench page in a training manifest (by hash) passed: exit {rc}")
    rc, out = run([doc(source="parsebench/docs/table/x.pdf", document="NOT_ON_THE_RIG.pdf",
                       page_id="zzz", sha256="44" * 32)], rig=parsebench, datasets=real_rows)
    if rc != 1 or "parsebench" not in out:
        fails.append(f"a ParseBench page off the rig was allowed for training: exit {rc}")
    rc, out = run([doc()], rig=None)
    if rc != 2 or "SKIPPED (not a pass)" not in out:
        fails.append(f"no rig manifest must be exit 2 and say so, got {rc}")
    real = subprocess.run([sys.executable, str(GUARD)], capture_output=True, text=True)
    if real.returncode != 0:
        fails.append(f"the real tree fails the guard:\n{real.stderr[-400:]}")
    if fails:
        for f in fails:
            print(f"FAIL: {f}", file=sys.stderr)
        return 1
    total = len(cases) + len(track) + len(band) + 6
    print(f"learned-leakage guard: {total} cases, all as expected.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
