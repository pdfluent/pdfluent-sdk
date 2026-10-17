#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Collect false-table hard negatives from DocLayNet train.

    python3 scripts/extraction/learned/collect_hard_negatives.py \
        --structured-markdown target/release/examples/structured_markdown \
        --collector target/release/collect_hard_negatives \
        --out-dir /path/to/out [--parts 72] [--first 0] [--count 72]

Runs where the train PDFs are read for the feature dumps, never on a
development machine: only the resulting JSONL leaves that machine. No PDF and
no page image is kept or copied.

WHAT IT RUNS. The `structured_markdown --json` build from the commit under
test on every sampled page: the blocks a caller gets, after the learned pass.
The produced `tableCell` blocks are grouped into tables where `(row, column)`
rises, and each table is labelled a host (covers two truth text regions,
touches no truth table) or a positive (overlaps a truth table), with the
table's overlap against its best-overlapping truth table. Labels only:
feature values for training come from the engine's own feature dump, joined
on page and box. That half lives in the `collect_hard_negatives` binary of
the `pdf-learned` crate, which this driver feeds and calls; the binary's
tests pin the rules on synthetic pages, no PDF.

Needs Python 3.11 or newer for `tomllib`; the collection VM runs Ubuntu 24.04,
whose python3 is 3.12.

WHICH PAGES. DocLayNet train, at most three per document, drawn with seed 574
in one pass (reservoir per document, shards in order). A page whose document
the rig names is refused at the source and counted; the run fails if one gets
through.
"""
from __future__ import annotations

import argparse
import collections
import concurrent.futures
import hashlib
import json
import pathlib
import random
import shutil
import subprocess
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO / "scripts" / "extraction" / "learned"))
sys.path.insert(0, str(REPO / "scripts" / "ci"))

SEED = 574
MAX_PAGES_PER_DOCUMENT = 3
HOST_CAP_PER_DOCUMENT = 5


def rig_documents() -> set[str]:
    from learned_training_uses_no_rig_document import rig_entries
    return {e["document"] for e in rig_entries(REPO) if e.get("document")}


def draw_pages(
    identities: dict[str, list[str]], limit: int, seed: int
) -> set[str]:
    """At most `limit` page ids per document, deterministically.

    Each document's pages are shuffled with the seed and the first `limit`
    kept. With a fixed seed the draw is stable; documents sort so the order
    the shards arrived in cannot move it.
    """
    rng = random.Random(seed)
    wanted: set[str] = set()
    for document in sorted(identities):
        page_ids = list(identities[document])
        rng.shuffle(page_ids)
        wanted.update(page_ids[:limit])
    return wanted


def run_structured_markdown(binary: pathlib.Path, pdf: pathlib.Path) -> dict:
    done = subprocess.run(
        [str(binary), "--json", str(pdf)],
        capture_output=True,
        text=True,
        timeout=120,
    )
    if done.returncode != 0:
        raise RuntimeError(f"{pdf.name}: structured_markdown exit {done.returncode}: {done.stderr[-300:]}")
    return json.loads(done.stdout)


def self_test() -> int:
    failures: list[str] = []

    def check(name: str, got, want) -> None:
        if got != want:
            failures.append(f"{name}: got {got!r}, want {want!r}")

    identities = {"a.pdf": [f"a{i}" for i in range(10)], "b.pdf": ["b0"]}
    first = draw_pages(identities, 3, SEED)
    second = draw_pages(identities, 3, SEED)
    check("the draw is stable", first, second)
    check("at most three per document", sum(1 for p in first if p.startswith("a")), 3)
    check("a small document keeps all its pages", "b0" in first, True)
    other = draw_pages(identities, 3, SEED + 1)
    check("the seed matters", first != other, True)

    for failure in failures:
        print(f"  FAIL  {failure}")
    print(f"[collect_hard_negatives] self-test: {len(failures)} failure(s)")
    return 1 if failures else 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--structured-markdown", required=False, default=None)
    ap.add_argument("--collector", required=False, default=None)
    ap.add_argument("--out-dir", required=False, default=None)
    ap.add_argument("--commit", default=None)
    ap.add_argument("--parts", type=int, default=None)
    ap.add_argument("--first", type=int, default=0)
    ap.add_argument("--count", type=int, default=None)
    ap.add_argument("--max-pages-per-document", type=int, default=MAX_PAGES_PER_DOCUMENT)
    ap.add_argument("--seed", type=int, default=SEED)
    ap.add_argument("--cap", type=int, default=HOST_CAP_PER_DOCUMENT)
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    if not args.structured_markdown or not args.collector or not args.out_dir:
        ap.error("--structured-markdown, --collector and --out-dir are required")

    if sys.version_info < (3, 11):
        raise SystemExit(
            f"collect_hard_negatives needs Python 3.11 or newer (tomllib); "
            f"this is {sys.version.split()[0]}"
        )
    import pyarrow.parquet as pq
    from huggingface_hub import hf_hub_download
    import tomllib
    import rig_data_stays_on_the_rig as rig_data
    import align_labels as al

    rig_data.refuse_split("doclaynet", "train")
    rig_data.refuse_here("collect_hard_negatives")

    datasets = tomllib.loads((REPO / "benchmarks/structured/comparison/DATASETS.toml").read_text())
    entry = datasets["datasets"]["doclaynet"]
    if not str(entry.get("training_use", "")).startswith("allowed"):
        raise SystemExit("DocLayNet has no training_use = \"allowed...\" verdict; not collecting it.")
    revision = entry.get("revision")
    if not revision:
        raise SystemExit("DocLayNet has no pinned revision in DATASETS.toml; not collecting a moving branch.")
    structured_markdown = pathlib.Path(args.structured_markdown)
    collector = pathlib.Path(args.collector)
    if not structured_markdown.is_file():
        raise SystemExit(f"{structured_markdown} is not built")
    if not collector.is_file():
        raise SystemExit(f"{collector} is not built: cargo build --release -p pdf-learned --bin collect_hard_negatives")
    if args.commit:
        commit = args.commit
    else:
        done = subprocess.run(
            ["git", "-C", str(REPO), "rev-parse", "HEAD"],
            capture_output=True, text=True)
        commit = done.stdout.strip() or "unknown"

    rig = rig_documents()
    out_dir = pathlib.Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    parts = args.parts or 72
    last = parts if args.count is None else min(parts, args.first + args.count)

    # TWO PASSES. The first lists every train page's identity per document
    # (no PDF bytes held); the draw keeps at most three per document. The
    # second fetches only the drawn pages and reads them. Peak disk is one
    # shard plus the pages being read.
    skipped_rig = 0
    identities: dict[str, list[str]] = collections.defaultdict(list)
    for part in range(args.first, last):
        name = f"data/train-{part:05d}-of-{parts:05d}.parquet"
        cache = pathlib.Path(tempfile.mkdtemp(prefix="hardneg-list-"))
        try:
            print(f"[collect] list {name} at {revision[:12]}", flush=True)
            shard = hf_hub_download(repo_id=entry["repo_id"], filename=name,
                                    repo_type="dataset", revision=revision,
                                    cache_dir=str(cache))
            for batch in pq.read_table(shard).to_batches(max_chunksize=256):
                for record in batch.to_pylist():
                    meta = record.get("metadata") or {}
                    document = meta.get("original_filename")
                    if not document:
                        continue
                    if document in rig:
                        skipped_rig += 1
                        continue
                    blob = record.get("pdf")
                    if not blob:
                        continue
                    page_id = meta.get("page_hash") or hashlib.sha256(blob).hexdigest()
                    identities[document].append(page_id)
        finally:
            shutil.rmtree(cache, ignore_errors=True)

    wanted = draw_pages(identities, args.max_pages_per_document, args.seed)
    print(f"[collect] {len(wanted)} page(s) drawn from {len(identities)} document(s)", flush=True)

    work = pathlib.Path(tempfile.mkdtemp(prefix="hardneg-pages-"))
    inputs_path = out_dir / "pages-input.jsonl"
    staged_inputs: list[dict] = []
    try:
        def dump_record(record: dict) -> dict | None:
            meta = record.get("metadata") or {}
            blob = record.get("pdf")
            if not blob:
                return None
            page_id = meta.get("page_hash") or hashlib.sha256(blob).hexdigest()
            if page_id not in wanted:
                return None
            document = meta.get("original_filename")
            pdf = work / f"{page_id}.pdf"
            pdf.write_bytes(blob)
            try:
                engine = run_structured_markdown(structured_markdown, pdf)
            except Exception as exc:
                print(f"[collect] {pdf.name}: {exc}", flush=True)
                return None
            finally:
                pdf.unlink(missing_ok=True)
            frame = {k: meta.get(k) for k in (
                "coco_width", "coco_height", "original_width", "original_height")}
            width = frame.get("original_width")
            height = frame.get("original_height")
            if not width or not height:
                print(f"[collect] {page_id}: no original page size; skipped", flush=True)
                return None
            page_for_frame = {"width": width, "height": height}
            regions = []
            labels = record.get("category_id") or []
            boxes = record.get("bboxes") or []
            for index, class_id in enumerate(labels):
                box = list(boxes[index]) if index < len(boxes) else None
                if box is None:
                    continue
                regions.append({
                    "category": al.label_of({"label": int(class_id)}),
                    "bbox": al.to_user_space(box, page_for_frame,
                                             (float(frame["coco_width"]), float(frame["coco_height"]))),
                })
            return {
                "page_id": page_id,
                "document": document,
                "page_no": meta.get("page_no"),
                "category": meta.get("doc_category"),
                "width": float(width),
                "height": float(height),
                "commit": commit,
                "blocks": engine.get("blocks") or [],
                "regions": regions,
            }

        for part in range(args.first, last):
            name = f"data/train-{part:05d}-of-{parts:05d}.parquet"
            cache = pathlib.Path(tempfile.mkdtemp(prefix="hardneg-shard-"))
            try:
                print(f"[collect] read {name}", flush=True)
                shard = hf_hub_download(repo_id=entry["repo_id"], filename=name,
                                        repo_type="dataset", revision=revision,
                                        cache_dir=str(cache))
                batch_records = []
                for batch in pq.read_table(shard).to_batches(max_chunksize=64):
                    batch_records.extend(batch.to_pylist())
                with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
                    for result in pool.map(dump_record, batch_records):
                        if result is not None:
                            staged_inputs.append(result)
            finally:
                shutil.rmtree(cache, ignore_errors=True)
        staged_inputs.sort(key=lambda p: (p["document"], p["page_id"]))
        with inputs_path.open("w", encoding="utf-8") as out:
            for page in staged_inputs:
                out.write(json.dumps(page, sort_keys=True) + "\n")
        print(f"[collect] {len(staged_inputs)} page(s) staged, {skipped_rig} refused as rig", flush=True)
    finally:
        shutil.rmtree(work, ignore_errors=True)

    rig_path = out_dir / "rig.json"
    rig_path.write_text(json.dumps(sorted(rig)) + "\n")
    tables_path = out_dir / "tables.jsonl"
    capped_path = out_dir / "hosts-capped.jsonl"
    summary_path = out_dir / "summary.json"
    done = subprocess.run(
        [str(collector), "--input", str(inputs_path), "--rig", str(rig_path),
         "--out", str(tables_path), "--capped", str(capped_path),
         "--summary", str(summary_path), "--cap", str(args.cap)],
        capture_output=True, text=True)
    print(done.stdout, end="")
    print(done.stderr, end="", file=sys.stderr)
    if done.returncode != 0:
        raise SystemExit(f"collector failed (exit {done.returncode})")
    # Only the rows leave the machine: the staged engine inputs name no PDF
    # bytes, and the shards and page PDFs are already deleted.
    inputs_path.unlink(missing_ok=True)
    summary = json.loads(summary_path.read_text())
    leaked = summary.get("refused_rig", 0) != skipped_rig
    if leaked:
        print(f"[collect] WARNING: staged refusals {skipped_rig} disagree with "
              f"collector refusals {summary.get('refused_rig')}", file=sys.stderr)
    print(f"[collect] {summary.get('n_hosts', 0)} host(s), "
          f"{summary.get('n_positives', 0)} positive(s) in {tables_path}", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
