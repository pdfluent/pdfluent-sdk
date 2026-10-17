#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Both table tracks draw from one rule, and no dev or rig document trains.

Track A dumps training data and track B measures zero-shot models on the
dev sets; the dev sets and the train sets are drawn by ONE module
(`scripts/extraction/tables_a/build_manifests.py`), which track B reads
instead of re-drawing. On synthetic annotation tarballs this draws the
three manifests through that module and holds, across both tracks:

- no table, page or DOCUMENT of either dev set (PT val tables, FTN
  dev-lane tables) appears in either training manifest;
- no document of `PUBTABLES1M.json`, `DOCLAYNET_TEST.json`, or an FTN
  eval company appears in either training manifest;
- `learned_training_uses_no_rig_document.py` passes on both;
- track B resolves the SAME drawn dev rows with truth inline.

A tampered copy with a dev row moved into train must fail the
disjointness below: a check that cannot fail proves nothing.

The colmerge held-out draws with the same module: a held-out document
or company appears in no training manifest, in tables or in the
guard-shaped documents; a tampered training manifest naming one fails
the real leakage guard, as does a never-train record; and a
`shards-eval/` beside the shards is never listed, so training over
the directory never reads the eval prefix. The header-band held-out
draws beside it: its companies appear in no training manifest either,
the removal reports per company, and a tampered manifest naming one
fails the guard naming the headerband set.
"""
from __future__ import annotations

import io
import json
import pathlib
import subprocess
import sys
import tarfile
import tempfile

HERE = pathlib.Path(__file__).resolve()
REPO = HERE.parents[2]
sys.path.insert(0, str(REPO / "scripts" / "extraction" / "tables_a"))
sys.path.insert(0, str(REPO / "scripts" / "extraction" / "tables_b"))
sys.path.insert(0, str(REPO / "scripts" / "extraction"))
import build_manifests as manifests  # noqa: E402
import dev_manifest as dev  # noqa: E402
import fintabnet_archive  # noqa: E402
import fintabnet_split  # noqa: E402
import graph  # noqa: E402

GUARD = REPO / "scripts" / "ci" / "learned_training_uses_no_rig_document.py"

RIG_PT = "PMC8000001"
RIG_DOCLAYNET = "doclaynet-rig-document.pdf"
REFUSED_PMC = "PMC8000002"

DATASETS = """\
[datasets.pubtables-1m]
name = "PT fixture"
licence = "fixture"
training_use = "allowed: fixture"
revision = "abc"
repo_id = "fixture/pt"
annotations_file = "pt.tar.gz"
documents_bucket = "https://example.invalid"
documents_licence_codes = ["CC0", "CC BY"]
[datasets.fintabnet]
name = "FTN fixture"
licence = "fixture"
training_use = "allowed: fixture"
revision = "def"
annotations_repo_id = "fixture/ftn"
annotations_revision = "def"
annotations_file = "ftn.tar.gz"
repo_id = "fixture/ftn"
documents_archive = "archive.zip"
documents_archive_bytes = 1
documents_pdfs = 1
"""


def cell(rows, cols, header):
    return {"row_nums": rows, "column_nums": cols,
            "is_column_header": header, "pdf_text_content": "w"}


def grid(spanning):
    """A header row plus two body rows: five cells spanning, six plain."""
    cells = []
    if spanning:
        cells.append(cell([0], [0, 1], True))
    else:
        cells += [cell([0], [0], True), cell([0], [1], True)]
    for row in (1, 2):
        cells += [cell([row], [0], False), cell([row], [1], False)]
    return cells


def pt_table(split, page, spanning):
    return {"split": split, "pdf_page_index": page,
            "pdf_table_bbox": [0.0, 0.0, 10.0, 10.0],
            "pdf_full_page_bbox": [0.0, 0.0, 100.0, 100.0],
            "exclude_for_structure": False, "cells": grid(spanning)}


def ftn_table(spanning):
    return {"pdf_page_index": 0, "exclude_for_structure": False,
            "pdf_table_bbox": [0.0, 0.0, 10.0, 10.0],
            "pdf_full_page_bbox": [0.0, 0.0, 100.0, 100.0],
            "cells": grid(spanning)}


def write_tarball(path: pathlib.Path, members: dict[str, list[dict]]) -> None:
    with tarfile.open(path, "w:gz") as tar:
        for name, annotation in members.items():
            data = json.dumps(annotation).encode("utf-8")
            info = tarfile.TarInfo(name)
            info.size = len(data)
            tar.addfile(info, io.BytesIO(data))


def read_tarball(path: pathlib.Path) -> dict[str, list[dict]]:
    out = {}
    with tarfile.open(path, "r|gz") as tar:
        for member in tar:
            handle = tar.extractfile(member)
            if handle is None:
                continue
            out[member.name] = json.load(handle)
    return out


def scratch_root(tmp: pathlib.Path) -> pathlib.Path:
    root = tmp / "root"
    structured = root / "corpus" / "structured"
    structured.mkdir(parents=True)
    (structured / "PUBTABLES1M.json").write_text(json.dumps({
        "set": "fixture", "pages": [
            {"source": "pubtables-1m/pmc-open-access",
             "name": f"{RIG_PT}-p0000.pdf", "document": RIG_PT,
             "page_no": 0, "sha256": "11" * 32, "split": "holdout"}]}))
    (structured / "DOCLAYNET_TEST.json").write_text(json.dumps({
        "set": "fixture", "pages": [
            {"source": "doclaynet-v1.2/test",
             "name": RIG_DOCLAYNET, "document": RIG_DOCLAYNET,
             "sha256": "22" * 32, "split": "test"}]}))
    comparison = root / "benchmarks" / "structured" / "comparison"
    comparison.mkdir(parents=True)
    (comparison / "DATASETS.toml").write_text(DATASETS)
    return root


def tickers(counts: dict[str, int]) -> dict[str, list[str]]:
    found: dict[str, list[str]] = {lane: [] for lane in counts}
    at = 0
    while any(len(found[lane]) < counts[lane] for lane in counts):
        ticker = f"C{at:03d}"
        at += 1
        lane = fintabnet_split.lane(ticker)
        if lane in found and len(found[lane]) < counts[lane]:
            found[lane].append(ticker)
    return found


def overlaps(train_rows: list[dict], dev_rows: list[dict]) -> dict[str, list]:
    """The dev rows train holds, at table, page and document grain."""
    train_tables = {(r["document"], r["page"], r["table"])
                    for r in train_rows}
    train_pages = {(r["document"], r["page"]) for r in train_rows}
    train_docs = {r["document"] for r in train_rows}
    return {
        "tables": sorted((r["document"], r["page"], r["table"])
                         for r in dev_rows
                         if (r["document"], r["page"], r["table"])
                         in train_tables),
        "pages": sorted({(r["document"], r["page"]) for r in dev_rows}
                        & train_pages),
        "documents": sorted({r["document"] for r in dev_rows} & train_docs),
    }


def run_guard(root: pathlib.Path) -> int:
    done = subprocess.run([sys.executable, str(GUARD), "--root", str(root)],
                          capture_output=True, text=True, timeout=120)
    return done.returncode


def main() -> int:
    failures: list[str] = []

    def check(what, cond, detail=""):
        if not cond:
            failures.append(f"{what} -- {detail}")

    check("both tracks read the one manifest module",
          dev.manifests is manifests,
          "track B importing anything else is a second rule")

    with tempfile.TemporaryDirectory() as raw:
        tmp = pathlib.Path(raw)
        root = scratch_root(tmp)
        pt_tar, ftn_tar = tmp / "pt.tar.gz", tmp / "ftn.tar.gz"
        write_tarball(pt_tar, {
            "PMC8000010_tables.json": [
                pt_table("train", 0, False), pt_table("val", 0, False),
                pt_table("train", 1, True), pt_table("train", 1, False)],
            "PMC8000020_tables.json": [
                pt_table("train", 0, False), pt_table("train", 1, True),
                pt_table("train", 2, False)],
            f"{RIG_PT}_tables.json": [pt_table("train", 0, False),
                                      pt_table("val", 0, False)],
            f"{REFUSED_PMC}_tables.json": [pt_table("train", 2, False)]})
        lanes = tickers({"train": 1, "dev": 1, "eval": 1})
        write_tarball(ftn_tar, {
            f"{lanes['train'][0]}_2019_page_0_tables.json":
                [ftn_table(True), ftn_table(False)],
            f"{lanes['dev'][0]}_2019_page_0_tables.json": [ftn_table(False)],
            f"{lanes['eval'][0]}_2019_page_0_tables.json":
                [ftn_table(False)]})

        def licence_of(pmcid: str):
            if pmcid == REFUSED_PMC:
                return False, "licence CC BY-NC (CC BY-NC)"
            return True, "CC BY"

        out = tmp / "out"
        argv = ["--pt-tar", str(pt_tar), "--ftn-tar", str(ftn_tar),
               "--out", str(out), "--root", str(root),
               "--pt-train", "2", "--ftn-train", "2", "--pt-dev", "1",
               "--ftn-dev", "1"]
        check("the shared draw exits 0",
              manifests.main(argv, licence_of=licence_of) == 0)
        pt_train = json.loads((out / "pt-train.json").read_text())
        ftn_train = json.loads((out / "ftn-train.json").read_text())
        dev_manifest = json.loads((out / "dev.json").read_text())
        pt_dev = [r for r in dev_manifest["tables"]
                  if r["register"] == "pubtables-1m"]
        ftn_dev = [r for r in dev_manifest["tables"]
                   if r["register"] == "fintabnet"]

        check("the PT dev rows are val tables",
              pt_dev and all(r["split"] == "val" for r in pt_dev),
              f"got {[r['split'] for r in pt_dev]}")
        check("the PT train rows are train tables",
              all(r["split"] == "train" for r in pt_train["tables"]))
        check("the FTN dev rows are all on the dev lane",
              ftn_dev and all(fintabnet_split.lane(r["company"]) == "dev"
                              for r in ftn_dev))
        check("the FTN train rows are all on the train lane",
              all(fintabnet_split.lane(r["company"]) == "train"
                  for r in ftn_train["tables"]))

        pt_overlap = overlaps(pt_train["tables"], pt_dev)
        ftn_overlap = overlaps(ftn_train["tables"], ftn_dev)
        for name, overlap in (("PT", pt_overlap), ("FTN", ftn_overlap)):
            for grain in ("tables", "pages", "documents"):
                check(f"no {name} dev {grain[:-1]} is in {name} train",
                      overlap[grain] == [], f"overlap: {overlap[grain]}")

        rig = {RIG_PT, RIG_DOCLAYNET}
        for name, manifest, dev_rows in (("PT", pt_train, pt_dev),
                                         ("FTN", ftn_train, ftn_dev)):
            trained = {d["document"] for d in manifest["documents"]}
            check(f"no rig document is in {name} train",
                  not (trained & rig), f"overlap: {sorted(trained & rig)}")
            check(f"no {name} dev document is in {name} train either",
                  not (trained & {r["document"] for r in dev_rows}),
                  "on the guard-shaped documents lists, not just the rows")
        check("the refused article is in no manifest",
              all(r["document"] != REFUSED_PMC
                  for r in pt_train["tables"] + dev_manifest["tables"]))
        check("the eval company is in no manifest",
              all(r.get("company") != lanes["eval"][0]
                  for r in ftn_train["tables"] + dev_manifest["tables"]))

        # The real leakage guard over both training manifests.
        for name, manifest in (("pt", pt_train), ("ftn", ftn_train)):
            models = root / "scripts" / "extraction" / "learned" \
                / "models" / name
            models.mkdir(parents=True)
            (models / "manifest.json").write_text(
                json.dumps({"documents": manifest["documents"]}))
        check("the leakage guard passes on both training manifests",
              run_guard(root) == 0)

        # Track B resolves the SAME drawn dev rows with truth inline.
        pt_annotations = {name.split("_")[0]: annotation
                          for name, annotation in
                          read_tarball(pt_tar).items()}
        fetch = {"pages": [
            {"name": f"{row['document']}-p{row['page']:04d}.pdf",
             "document": row["document"], "page_no": row["page"],
             "sha256": "33" * 32}
            for row in dev_manifest["tables"]
            if row["register"] == "pubtables-1m"]}
        resolved_pt = dev.materialise_pt(dev_manifest, fetch, pt_annotations)
        check("track B resolves every PT dev page",
              len(resolved_pt["pages"]) == len(
                  {(r["document"], r["page"]) for r in pt_dev})
              and resolved_pt["skipped_pages"] == [])
        check("with truth HTML inline",
              all(t["html"].startswith("<table>")
                  for p in resolved_pt["pages"] for t in p["tables"]))

        ftn_annotations = {}
        for name, annotation in read_tarball(ftn_tar).items():
            key = fintabnet_archive.page_key_of(name)
            if key is not None:
                ftn_annotations[key] = annotation
        resolved_ftn = dev.materialise_ftn(dev_manifest, ftn_annotations)
        check("track B resolves every FTN dev page",
              len(resolved_ftn["pages"]) == len(ftn_dev)
              and resolved_ftn["skipped_pages"] == [])

        # Teeth: a dev row moved into train must fail the disjointness.
        tampered = {"tables": pt_train["tables"] + pt_dev[:1],
                    "documents": manifests.documents_of(
                        pt_train["tables"] + pt_dev[:1])}
        found = overlaps(tampered["tables"], pt_dev)
        check("a dev row in train is flagged at every grain",
              all(found[grain] for grain in
                  ("tables", "pages", "documents")),
              "the passing overlap above then proves the draw, not the check")
        (root / "scripts" / "extraction" / "learned" / "models" / "pt"
         / "manifest.json").write_text(json.dumps(
             {"documents": tampered["documents"]}))
        check("the guard still passes on the tampered manifest",
              run_guard(root) == 0,
              "dev-in-train is this test's check to catch, rig-in-train "
              "the guard's -- the tampered row names no rig document")

    # --- the colmerge held-out, through the same module --------------------
    with tempfile.TemporaryDirectory() as raw:
        tmp = pathlib.Path(raw)
        root = scratch_root(tmp)
        pt_tar, ftn_tar = tmp / "pt.tar.gz", tmp / "ftn.tar.gz"
        write_tarball(pt_tar, {
            "PMC8000010_tables.json": [
                pt_table("train", 0, False), pt_table("train", 1, True)],
            "PMC8000030_tables.json": [
                pt_table("train", 0, False), pt_table("train", 1, True),
                pt_table("val", 0, False)],
            "PMC8000040_tables.json": [
                pt_table("train", 0, False), pt_table("train", 1, True)]})
        lanes = tickers({"train": 3, "dev": 1})
        held_co, clean_co, hb_co = lanes["train"]
        write_tarball(ftn_tar, {
            f"{held_co}_2019_page_0_tables.json":
                [ftn_table(False), ftn_table(True)],
            f"{clean_co}_2019_page_1_tables.json": [ftn_table(True)],
            f"{hb_co}_2019_page_2_tables.json": [ftn_table(True)],
            f"{lanes['dev'][0]}_2019_page_0_tables.json":
                [ftn_table(False)]})
        reference = root / "scripts" / "extraction" / "tables_a" \
            / "colmerge_heldout.json"
        reference.parent.mkdir(parents=True)
        manifests.write_heldout(reference, ["PMC8000010"], [held_co],
                                {"source": "test"})
        hb_reference = root / "scripts" / "extraction" / "tables_a" \
            / "heldout-headerband.json"
        manifests.write_heldout(hb_reference, [], [hb_co],
                                {"source": "test"})

        def licence_of(pmcid: str):
            return True, "CC BY"

        out = tmp / "out"
        argv = ["--pt-tar", str(pt_tar), "--ftn-tar", str(ftn_tar),
               "--out", str(out), "--root", str(root),
               "--pt-train", "2", "--ftn-train", "1", "--pt-dev", "1",
               "--ftn-dev", "1"]
        check("the held-out draw exits 0",
              manifests.main(argv, licence_of=licence_of) == 0)
        pt_train = json.loads((out / "pt-train.json").read_text())
        ftn_train = json.loads((out / "ftn-train.json").read_text())
        check("no held-out document is in any training manifest",
              all(r["document"] != "PMC8000010"
                  for r in pt_train["tables"] + ftn_train["tables"]))
        check("no held-out company is in any training manifest",
              all(r.get("company") != held_co
                  for r in pt_train["tables"] + ftn_train["tables"]))
        check("no headerband company is in any training manifest",
              all(r.get("company") != hb_co
                  for r in pt_train["tables"] + ftn_train["tables"]),
              f"got {[r.get('company') for r in ftn_train['tables']]}")
        check("nor in the guard-shaped documents",
              all(d["document"] != "PMC8000010"
                  and d.get("company") != held_co
                  and d.get("company") != hb_co
                  for d in pt_train["documents"]
                  + ftn_train["documents"]))
        check("no train row carries the eval split or the never flag",
              all(r["split"] != "eval-colmerge"
                  and r.get("train_use") != "never"
                  for r in pt_train["tables"] + ftn_train["tables"]))
        held = json.loads((out / "heldout-colmerge.json").read_text())
        check("the removal is reported per source",
              held["meta"]["removed_pt_tables"] == 2
              and held["meta"]["removed_ftn_tables"] == 2,
              f"got {held['meta']}")
        hb_held = json.loads(
            (out / "heldout-headerband.json").read_text())
        check("the headerband removal is reported per company",
              hb_held["meta"]["removed_ftn_tables"] == 1
              and hb_held["meta"]["removed_ftn_tables_per_company"]
              == {hb_co: 1},
              f"got {hb_held['meta']}")

        models = root / "scripts" / "extraction" / "learned" / "models"
        (models / "pt").mkdir(parents=True)
        (models / "ftn").mkdir(parents=True)
        (models / "pt" / "manifest.json").write_text(
            json.dumps({"documents": pt_train["documents"]}))
        (models / "ftn" / "manifest.json").write_text(
            json.dumps({"documents": ftn_train["documents"]}))
        check("the guard passes on the held-out training manifests",
              run_guard(root) == 0)

        def tampered_guard(documents, model):
            (models / model / "manifest.json").write_text(
                json.dumps({"documents": documents}))
            done = subprocess.run(
                [sys.executable, str(GUARD), "--root", str(root)],
                capture_output=True, text=True, timeout=120)
            return done.returncode, done.stdout + done.stderr

        # Each tamper is restored before the next, so every failure
        # below proves its own record, not the last one's.
        tampered_pt = pt_train["documents"] + [{
            "document": "PMC8000010",
            "source": "pubtables-1m/pmc-open-access", "split": "train"}]
        code, words = tampered_guard(tampered_pt, "pt")
        check("a training manifest with a held-out document fails",
              code == 1 and "colmerge held-out" in words,
              f"exit {code}: {words[-200:]}")
        (models / "pt" / "manifest.json").write_text(
            json.dumps({"documents": pt_train["documents"]}))
        tampered_ftn = ftn_train["documents"] + [{
            "document": f"{held_co}_2019",
            "source": "fintabnet/ibm-1.0.0", "split": "train",
            "company": held_co}]
        code, words = tampered_guard(tampered_ftn, "ftn")
        check("a training manifest with a held-out company fails",
              code == 1 and "colmerge held-out" in words,
              f"exit {code}: {words[-200:]}")
        (models / "ftn" / "manifest.json").write_text(
            json.dumps({"documents": ftn_train["documents"]}))
        tampered_hb = ftn_train["documents"] + [{
            "document": f"{hb_co}_2019",
            "source": "fintabnet/ibm-1.0.0", "split": "train",
            "company": hb_co}]
        code, words = tampered_guard(tampered_hb, "ftn")
        check("a training manifest with a headerband company fails",
              code == 1 and "headerband held-out" in words,
              f"exit {code}: {words[-200:]}")
        (models / "ftn" / "manifest.json").write_text(
            json.dumps({"documents": ftn_train["documents"]}))
        never = pt_train["documents"] + [{
            "document": "PMC8000099",
            "source": "pubtables-1m/pmc-open-access",
            "split": "eval-colmerge", "train_use": "never"}]
        code, words = tampered_guard(never, "pt")
        check("a training manifest with a never-train record fails",
              code == 1 and "train_use never" in words,
              f"exit {code}: {words[-200:]}")

        # Training over the shards never reads the eval prefix: the
        # listing stops at the directory it was given.
        shards = tmp / "shards"
        shards.mkdir()
        (shards / "shard-00000.jsonl").write_text(
            json.dumps({"split": "train"}) + "\n")
        inner = shards / "shards-eval"
        inner.mkdir()
        (inner / "shard-00000.jsonl").write_text(
            json.dumps({"split": "eval-colmerge"}) + "\n")
        check("the shard listing never names shards-eval/",
              [p.name for p in graph.list_shards(shards)]
              == ["shard-00000.jsonl"])
        check("and the rows read hold no eval row",
              [r.get("split") for r in graph.iter_shards(shards)]
              == ["train"])

    for failure in failures:
        print("FAIL:", failure)
    print("tables-one-manifest-rule: " + ("OK" if not failures else
                                          f"{len(failures)} failure(s)"))
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
