#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Materialise one half of the PDF/A corpus from its manifest, byte for byte.

    python3 scripts/ci/fetch_pdfa_corpus.py --half tuning --out /srv/pdfa/tuning

Run by hand or by a measuring job, never by a pull-request gate: ten thousand
documents is thirty gigabytes and forty minutes. What runs on every pull request
is `a_manifest_pins_every_file.py`, which checks the manifest without fetching
anything.

Each entry carries the coordinates it needs -- which archive, at which offset,
compressed how -- so a document is one ranged request against a half-terabyte
zip rather than a download of the archive. The digest in the manifest is checked
against the bytes that arrive, and a mismatch is fatal: a corpus that silently
substitutes a document upstream has changed is a corpus whose numbers cannot be
compared with last month's.

The holdout is fetchable by the same command, and that is deliberate -- sealing
it is a protocol, not a lock. What the seal in `benchmarks/HOLDOUTS.md` gives is
the ability to say afterwards, and prove, which documents were in it before
anybody tuned anything.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import pathlib
import sys
import threading
import time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import build_pdfa_corpus_manifest as builder  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parents[2]
CORPUS = REPO / "corpus" / "pdfa"

SOURCE_LETTER = {"g": "govdocs1", "s": "safedocs"}


def zip_url(bases: dict[str, str], source: str, shard: str) -> str:
    """Where an entry's archive lives, from the base the MANIFEST states.

    Read out of the manifest rather than taken from the builder's constants, so
    that the URL in the pinned file is the URL that gets fetched. A manifest
    that names a source it does not fetch from is a manifest whose provenance
    line is decoration.
    """
    base = bases[source]
    if source == "govdocs1":
        return f"{base}/{shard}.zip"
    number = int(shard)
    group = (number // 1000) * 1000
    return f"{base}/{group:04d}-{group + 999:04d}/{number:04d}.zip"


def half_entries(directory: pathlib.Path, half: str) -> list[dict]:
    entries = []
    for path in sorted(directory.glob(f"pdfa_{half}_entries.*.tsv")):
        for line in path.read_text().splitlines():
            if not line:
                continue
            parts = line.split("\t")
            entries.append({
                "source": SOURCE_LETTER[parts[0]], "key": parts[1],
                "sha256": parts[2], "bytes": int(parts[3]), "shard": parts[4],
                "offset": int(parts[5]), "method": int(parts[6]),
                "csize": int(parts[7]),
            })
    return entries


def set_entries(manifest: pathlib.Path) -> tuple[list[dict], list[dict], dict]:
    """One set of #461: what can be fetched by offset, and what cannot.

    The 300 the PDF/A series is measured on predate the corpus rule and carry no
    archive coordinates, so they are not fetchable one ranged request at a time.
    They are named rather than skipped: a set that quietly materialised 700 of
    its 1000 documents is a smaller denominator on every axis, which is exactly
    the failure `a_published_number_names_its_run.py` exists for.
    """
    document = json.loads(manifest.read_text())
    ranged, whole = [], []
    for entry in document["entries"]:
        if entry.get("source") not in SOURCE_LETTER.values():
            continue
        if entry.get("offset") is None:
            whole.append(entry)
            continue
        ranged.append({
            "source": entry["source"], "key": entry["key"],
            "sha256": entry["sha256"], "bytes": entry["bytes"],
            "shard": entry["zip"], "offset": entry["offset"],
            "method": entry["method"], "csize": entry["csize"],
        })
    return ranged, whole, document


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--half", choices=["tuning", "holdout"],
                   help="a whole half of the ten thousand")
    p.add_argument("--set", dest="set_manifest",
                   help="a #461 set manifest instead of a half")
    p.add_argument("--out")
    p.add_argument("--corpus", default=str(CORPUS))
    p.add_argument("--workers", type=int, default=16)
    # Two questions a caller has to answer BEFORE it fetches anything, and the
    # only place that can answer them is the file that knows which entries carry
    # archive coordinates. A workflow step that derived either by hand would be
    # the mistake the 300 already made once: run 34194029831 asked for the zips
    # the documents "obviously" sit in, two of them did not, and the fetch
    # reported success having skipped them.
    p.add_argument("--print-list", action="store_true",
                   help="with --set: the document names, one per line, for "
                        "compare.mjs --list; fetches nothing")
    p.add_argument("--print-zips", action="store_true",
                   help="with --set: the GovDocs1 zips whose documents carry no "
                        "archive coordinates; fetches nothing")
    args = p.parse_args()

    if (args.print_list or args.print_zips):
        if not args.set_manifest:
            print("FATAL: --print-list and --print-zips need --set",
                  file=sys.stderr)
            return 1
        path = pathlib.Path(args.set_manifest)
        if not path.is_file():
            print(f"FATAL: {path} is missing", file=sys.stderr)
            return 1
        document = json.loads(path.read_text())
        if args.print_list:
            for entry in document["entries"]:
                print(entry["name"])
        else:
            _, whole, _ = set_entries(path)
            print(" ".join(sorted({e["zip"] for e in whole if e.get("zip")})))
        return 0

    if not args.out:
        print("FATAL: --out is required", file=sys.stderr)
        return 1
    if bool(args.half) == bool(args.set_manifest):
        print("FATAL: name exactly one of --half and --set", file=sys.stderr)
        return 1

    directory = pathlib.Path(args.corpus)
    out = pathlib.Path(args.out)
    out.mkdir(parents=True, exist_ok=True)

    what = args.half or pathlib.Path(args.set_manifest).name
    elsewhere: list[dict] = []
    if args.set_manifest:
        set_path = pathlib.Path(args.set_manifest)
        if not set_path.is_file():
            print(f"FATAL: {set_path} is missing", file=sys.stderr)
            return 1
        entries, elsewhere, header = set_entries(set_path)
        bases = {sid: source["archive_base"]
                 for sid, source in header["sources"].items()
                 if "archive_base" in source}
    else:
        manifest = directory / f"PDFA_{args.half.upper()}_MANIFEST.json"
        if not manifest.is_file():
            print(f"FATAL: {manifest} is missing", file=sys.stderr)
            return 1
        header = json.loads(manifest.read_text())
        bases = {sid: source["archive_base"]
                 for sid, source in header["sources"].items()}
        entries = half_entries(directory, args.half)

    if not entries and not elsewhere:
        print(f"FATAL: no entries for {what} in {directory}", file=sys.stderr)
        return 1

    # Before the first request, not on the 180th failure. A source with no
    # archive_base makes every one of its documents fail one at a time with a
    # KeyError for a message, which is how run 34292223960 reported "180
    # document(s) could not be produced" for one missing URL.
    unreachable = sorted({e["source"] for e in entries if e["source"] not in bases})
    if unreachable:
        print(f"FATAL: {', '.join(unreachable)} name(s) no `archive_base` in "
              "this manifest, so nothing from them can be fetched. The manifest "
              "is not wrong about the documents; it is missing the URL they "
              "come from.", file=sys.stderr)
        return 1

    problems: list[str] = []
    cached = [0]
    lock = threading.Lock()

    def one(entry: dict) -> None:
        name = builder._flat_name(entry["source"], entry["key"])
        target = out / name
        # A warm directory is the normal case on a reused machine. Verifying it
        # costs a read and removes the difference between cached and fetched.
        if target.is_file() and target.stat().st_size == entry["bytes"]:
            if hashlib.sha256(target.read_bytes()).hexdigest() == \
                    entry["sha256"]:
                with lock:
                    cached[0] += 1
                return
        try:
            body = builder._local_payload(
                zip_url(bases, entry["source"], entry["shard"]),
                entry["offset"],
                entry["method"], entry["csize"], entry["bytes"])
        except Exception as e:  # noqa: BLE001 - reported, never raised
            with lock:
                problems.append(f"{name}: {e}")
            return
        found = hashlib.sha256(body).hexdigest()
        if found != entry["sha256"]:
            with lock:
                problems.append(
                    f"{name}: SHA-256 mismatch -- the manifest pins "
                    f"{entry['sha256'][:16]}…, upstream now serves "
                    f"{found[:16]}…")
            return
        target.write_bytes(body)

    started = time.monotonic()
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as pool:
        list(pool.map(one, entries))

    fetched = len(entries) - cached[0] - len(problems)
    print(f"[pdfa-corpus] {what}: {len(entries)} documents, "
          f"{cached[0]} already present, {fetched} fetched, "
          f"{len(problems)} failed, {time.monotonic() - started:.0f}s")

    absent = [e for e in elsewhere if not (out / e["name"]).is_file()]
    if absent:
        zips = sorted({e["zip"] for e in absent if e.get("zip")})
        print(f"\n[pdfa-corpus] FATAL: {len(absent)} document(s) of this set "
              "carry no archive coordinates and are not in the output "
              "directory. They are the pre-#377 GovDocs1 sample; unpack them "
              "first with\n"
              f"    scripts/corpus-download-govdocs.sh --target {out} --zips "
              f"\"{' '.join(zips)}\"\n"
              "A set that materialised part of itself is a smaller denominator "
              "on every axis, not a shorter run.", file=sys.stderr)
        return 1

    if problems:
        print(f"\n[pdfa-corpus] FATAL: {len(problems)} document(s) could not be "
              "produced as the manifest describes them:", file=sys.stderr)
        for problem in problems[:25]:
            print(f"  {problem}", file=sys.stderr)
        if len(problems) > 25:
            print(f"  … and {len(problems) - 25} more", file=sys.stderr)
        print("\nA measurement set that cannot be reproduced byte for byte is "
              "not a baseline, and a run over a smaller set is not the same "
              "run. Either upstream moved -- which is a redraw, booked in "
              "benchmarks/HOLDOUTS.md -- or the network is refusing.",
              file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
