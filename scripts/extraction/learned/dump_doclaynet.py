#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Stream DocLayNet through the feature dump, one shard at a time (#598).

    python3 scripts/extraction/learned/dump_doclaynet.py --split train \
        --out /path/to/doclaynet-train.jsonl [--parts 72] [--first 0] [--count 72] \
        [--binary target/release/feature_dump] [--max-pages N]

ONE PROCESS PER SHARD. feature_dump runs once per shard over all its pages,
in parallel (--jobs); one process per page made extraction the slow step.

WHY STREAMING. The train split is 72 shards of about 0.52 GB, 37 GB in all, and
this Mac's push gate refuses to run below 35 GB free -- a full local copy
stops every lane on the machine, not only this one (T3, #599). So: fetch one
shard, dump its pages, append the JSONL, delete the shard, next. Peak disk is
one shard plus the pages being dumped; the JSONL is what survives.

WHAT A RECORD CARRIES. The dump's `pdfluent.features/1` record, plus what only
the dataset knows: `document` (DocLayNet's original_filename), `page_no`,
`category`, and `labels` -- DocLayNet's own boxes and classes, UNCONVERTED
(`bbox` in DocLayNet's coordinates, `coco` [x, y, w, h] from the top-left of
the page image). Aligning them onto our blocks is the lane's job (#599), and a
conversion hidden in here would be an error nobody could tell from a bad model.

AND WHAT ONLY THE WHOLE DOCUMENT KNOWS. After every shard is dumped,
`repeats_in_document` is filled in each block: on how many pages of the same
document the same normalised text recurs (the furniture signal, T3).

NEVER A RIG DOCUMENT. A page whose document the rig names
(corpus/structured/*.json, tuning half and sealed fifths) is skipped at the
source, and counted; the leakage guard checks the trained manifest again.

Runs on the Mac or on a rented CPU VM. On a VM the exported tree must be committed (git init + commit) before the build, or the engine stamp reads "unknown". The train and validation splits only: the test split is the rig's and is refused off the round machine, as is any machine that holds rig pages.
"""
from __future__ import annotations

import argparse
import collections
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO / "scripts" / "ci" / "loop"))
sys.path.insert(0, str(REPO / "scripts" / "ci"))


def rig_documents() -> set[str]:
    from learned_training_uses_no_rig_document import rig_entries
    return {e["document"] for e in rig_entries(REPO) if e.get("document")}


def normalise(text: str) -> str:
    return " ".join(text.lower().split())


def fill_repeats(path: pathlib.Path) -> None:
    """Rewrite the JSONL with repeats_in_document filled, per document."""
    pages_with: dict[str, collections.Counter] = collections.defaultdict(collections.Counter)
    with path.open(encoding="utf-8") as f:
        for line in f:
            r = json.loads(line)
            seen = {normalise(b["text"]) for b in r["blocks"] if normalise(b["text"])}
            pages_with[r.get("document") or r["pdf_sha256"]].update(seen)
    tmp = path.with_suffix(".repeats.tmp")
    with path.open(encoding="utf-8") as f, tmp.open("w", encoding="utf-8") as out:
        for line in f:
            r = json.loads(line)
            counts = pages_with[r.get("document") or r["pdf_sha256"]]
            for b in r["blocks"]:
                key = normalise(b["text"])
                b["repeats_in_document"] = counts[key] if key else 0
            out.write(json.dumps(r, sort_keys=True) + "\n")
    tmp.replace(path)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--split", default="train", choices=["train", "validation", "test"])
    ap.add_argument("--parts", type=int, default=None,
                    help="shards in the split (72 train, 7 validation, 6 test)")
    ap.add_argument("--first", type=int, default=0)
    ap.add_argument("--count", type=int, default=None)
    ap.add_argument("--out", required=True, type=pathlib.Path)
    ap.add_argument("--binary", type=pathlib.Path, default=REPO / "target" / "release" / "feature_dump")
    ap.add_argument("--max-pages", type=int, default=None)
    ap.add_argument("--jobs", type=int, default=os.cpu_count() or 4,
                    help="threads feature_dump uses inside one shard")
    # THE PAGES THEMSELVES, for a measurement that has to run the engine again
    # with something switched on (`worth_of_the_switches.py`, #602). The dump
    # is one run of the engine; a switch table is four, and re-downloading a
    # shard for each of them is an hour of nothing.
    ap.add_argument("--keep-pages", type=pathlib.Path, default=None,
                    help="keep each page PDF here, with pages.list and "
                         "labels.jsonl beside them")
    # SPREAD OVER THE SPLIT RATHER THAN TAKEN FROM THE FRONT. 400 consecutive
    # pages of one shard were 13 documents, and a difference between two
    # columns measured over 13 documents is a difference between 13 layouts.
    ap.add_argument("--stride", type=int, default=1,
                    help="take one page in every N, so a small sample is not "
                         "one document repeated")
    args = ap.parse_args()

    import pyarrow.parquet as pq
    from huggingface_hub import hf_hub_download
    import tomllib
    import rig_data_stays_on_the_rig as rig_data
    rig_data.refuse_split("doclaynet", args.split)
    rig_data.refuse_here("dump_doclaynet")

    parts = args.parts or {"train": 72, "validation": 7, "test": 6}[args.split]
    datasets = tomllib.loads((REPO / "benchmarks/structured/comparison/DATASETS.toml").read_text())
    entry = datasets["datasets"]["doclaynet"]
    if not str(entry.get("training_use", "")).startswith("allowed"):
        raise SystemExit("DocLayNet has no training_use = \"allowed...\" verdict; not dumping it.")
    revision = entry.get("revision")
    if not revision:
        raise SystemExit("DocLayNet has no pinned revision in DATASETS.toml; not dumping a moving branch.")
    if not args.binary.is_file():
        raise SystemExit(f"{args.binary} is not built: cargo build --release -p pdf-learned --bin feature_dump")

    rig = rig_documents()
    keep = args.keep_pages
    if keep:
        keep.mkdir(parents=True, exist_ok=True)
        (keep / "pages.list").write_text("")
        (keep / "labels.jsonl").write_text("")
    args.out.parent.mkdir(parents=True, exist_ok=True)
    written = skipped_rig = failed = 0
    last = parts if args.count is None else min(parts, args.first + args.count)
    with args.out.open("a", encoding="utf-8") as out:
        for part in range(args.first, last):
            name = f"data/{args.split}-{part:05d}-of-{parts:05d}.parquet"
            cache = pathlib.Path(tempfile.mkdtemp(prefix="doclaynet-shard-"))
            try:
                print(f"[dump] {name} at {revision[:12]}", flush=True)
                shard = hf_hub_download(repo_id=entry["repo_id"], filename=name, repo_type="dataset",
                                        revision=revision, cache_dir=str(cache))
                # ONE PROCESS PER SHARD (#598): every page PDF of the shard on
                # disk, one feature_dump call over all of them in parallel,
                # records joined back to their rows by page id.
                pages = keep or (cache / "pages")
                pages.mkdir(parents=True, exist_ok=True)
                listing, extra, at = [], {}, -1
                for batch in pq.read_table(shard).to_batches(max_chunksize=64):
                    for record in batch.to_pylist():
                        at += 1
                        if args.stride > 1 and at % args.stride:
                            continue
                        meta = record.get("metadata") or {}
                        document = meta.get("original_filename")
                        if document in rig:
                            skipped_rig += 1
                            continue
                        blob = record.get("pdf")
                        if not blob:
                            continue
                        page_id = meta.get("page_hash") or hashlib.sha256(blob).hexdigest()
                        pdf = pages / f"{page_id}.pdf"
                        pdf.write_bytes(blob)
                        listing.append(f"{pdf}\t{page_id}")
                        bboxes = record.get("bboxes") or []
                        extra[page_id] = {
                            "source": f"doclaynet-v1.2/{args.split}", "document": document,
                            "page_no": meta.get("page_no"), "category": meta.get("doc_category"),
                            # The frame the label boxes are in (T5, #599): the
                            # coco image and the original page, so a lane can
                            # convert them to PDF points without guessing.
                            "label_frame": {k: meta.get(k) for k in (
                                "coco_width", "coco_height", "original_width", "original_height")},
                            "labels": [{"category_id": int(c),
                                        "bbox": list(bboxes[i]) if i < len(bboxes) else None}
                                       for i, c in enumerate(record.get("category_id") or [])],
                        }
                        if args.max_pages and len(listing) + written >= args.max_pages:
                            break
                    if args.max_pages and len(listing) + written >= args.max_pages:
                        break
                (cache / "list.txt").write_text("\n".join(listing) + "\n")
                if keep:
                    with (keep / "pages.list").open("a") as out_list, \
                         (keep / "labels.jsonl").open("a") as out_labels:
                        out_list.write("\n".join(listing) + "\n")
                        for page_id, said in extra.items():
                            out_labels.write(json.dumps(
                                dict(said, page_id=page_id), sort_keys=True) + "\n")
                chunk = cache / "chunk.jsonl"
                done = subprocess.run([str(args.binary), "--jobs", str(args.jobs), "--list",
                                       str(cache / "list.txt"), "--out", str(chunk)],
                                      capture_output=True, text=True)
                failed += sum(1 for line in done.stderr.split("\n") if line.strip())
                # A chunk is taken only whole (codex, #1961): exit 1 is "some
                # PDFs unreadable, each named on stderr", anything else -- a
                # full disk, a crash -- leaves a chunk that may stop anywhere.
                if done.returncode not in (0, 1):
                    raise SystemExit(f"[dump] feature_dump failed on {name} (exit {done.returncode}): "
                                     f"{done.stderr[-400:]}")
                seen: set[str] = set()
                if chunk.exists():
                    # split("\n"), never splitlines(): that also breaks on
                    # U+2028/U+2029/U+0085, which JSON may carry raw (T3, 23-09).
                    for line in chunk.read_text(encoding="utf-8").split("\n"):
                        if not line.strip():
                            continue
                        r = json.loads(line)
                        if r["page_id"] not in extra:
                            raise SystemExit(f"[dump] {name}: a record for a page not asked for: {r['page_id']}")
                        if r["page_id"] in seen:
                            raise SystemExit(f"[dump] {name}: page {r['page_id']} came back twice")
                        seen.add(r["page_id"])
                        r.update(extra[r["page_id"]])
                        out.write(json.dumps(r, sort_keys=True) + "\n")
                        written += 1
                # Every page asked for is either a record or named on stderr
                # (codex, #1961): a chunk one short with exit 0 is a truncation.
                lost = [p for p in extra if p not in seen and p not in done.stderr]
                if lost:
                    raise SystemExit(f"[dump] {name}: {len(lost)} page(s) neither dumped nor reported, "
                                     f"e.g. {lost[0]}")
            finally:
                shutil.rmtree(cache, ignore_errors=True)
            if args.max_pages and written >= args.max_pages:
                break
    fill_repeats(args.out)
    print(f"[dump] {written} page(s) written to {args.out}; {skipped_rig} skipped as rig documents; "
          f"{failed} the dump could not read.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
