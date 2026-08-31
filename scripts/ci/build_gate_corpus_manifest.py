#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
#
# This software is proprietary. The PDFluent application is free to use,
# including for commercial purposes. Redistribution, or extraction or reuse
# of its components (including the embedded PDF engine), requires a licence.
# See https://pdfluent.com/license for terms.
"""Build the gate corpus manifest from public, licensed upstream suites.

Run by hand, not in CI. It talks to GitHub and to raw.githubusercontent, reads
the file list of each source at a pinned commit, picks a deterministic subset,
downloads every pick once and records its SHA-256.

Why a manifest of URLs instead of a directory of PDFs:

  * The corpus these gates used to read lived at /opt/xfa-corpus on one
    machine, and `corpus/CI_CORPUS_MANIFEST.json` still points every one of its
    500 entries at that path. The disk behind it stopped answering reads. The
    file names in that manifest (`0052.pdf`, `xfa-curated-20k__16387__...`) were
    assigned during curation from an index that lived on the same disk, so
    nothing in this repository can rebuild them. A corpus that only one drive
    can produce is not a gate, it is a dependency on a drive.

  * Test PDFs must never be committed here (#215). A manifest is text: URL,
    licence, provenance, checksum. It publishes safely and it pins exactly.

Determinism comes from the selection being a pure function of the pinned commit:
sort every candidate path, drop the ones over the size cap, then take an even
stride across what is left until the quota is met. Re-running against the same
commits produces the same list, and moving a commit is an edit somebody makes.

Usage:
    python3 scripts/ci/build_gate_corpus_manifest.py \
        --out corpus/GATE_CORPUS_MANIFEST.json
"""

from __future__ import annotations

import argparse
import concurrent.futures
import datetime
import hashlib
import json
import pathlib
import subprocess
import sys
import time
import urllib.parse
import urllib.request

# A gate has to finish while somebody is still looking at the pull request, and
# a cold fetch of the whole set has to stay in the low tens of megabytes. One
# 40 MB scanned brochure would buy less coverage than the sixty small files it
# displaces, so the cap is on the file, not only on the total.
MAX_FILE_BYTES = 3 * 1024 * 1024
MIN_FILE_BYTES = 64

SOURCES = {
    "pdfjs": {
        "repo": "mozilla/pdf.js",
        "commit": "a815e4f8f0f25ab305ee21752ccf832156f7ab58",
        "licence": "Apache-2.0",
        "provenance": (
            "Mozilla pdf.js test suite (test/pdfs/), redistributed by Mozilla "
            "under Apache-2.0. Fetched by URL at the pinned commit; never "
            "committed to this repository."
        ),
        "prefix": "test/pdfs/",
        "quota": 200,
    },
    "pdfbox": {
        "repo": "apache/pdfbox",
        "commit": "b9de8299ef80efc8bd7f3fddb5c8a3c63ee07a5c",
        "licence": "Apache-2.0",
        "provenance": (
            "Apache PDFBox test resources, redistributed by the Apache "
            "Software Foundation under Apache-2.0. Fetched by URL at the "
            "pinned commit; never committed to this repository."
        ),
        "prefix": "",
        "quota": 120,
    },
    "verapdf": {
        "repo": "veraPDF/veraPDF-corpus",
        "commit": "01e40281d48e2f3755006fdf596ca25caaea8634",
        "licence": "CC-BY-4.0",
        "provenance": (
            "veraPDF test corpus for PDF/A, PDF/UA, ISO 32000-1 and "
            "ISO 32000-2, published by the veraPDF consortium under "
            "CC BY 4.0. Fetched by URL at the pinned commit; never "
            "committed to this repository."
        ),
        "prefix": "",
        "quota": 180,
    },
}


def raw_url(repo: str, commit: str, path: str) -> str:
    quoted = urllib.parse.quote(path)
    return f"https://raw.githubusercontent.com/{repo}/{commit}/{quoted}"


def tree(repo: str, commit: str) -> list[dict]:
    out = subprocess.run(
        ["gh", "api", f"repos/{repo}/git/trees/{commit}?recursive=1", "--jq",
         '[.tree[] | select(.type=="blob") | {path, size}]'],
        capture_output=True, text=True, check=True, timeout=180,
    )
    return json.loads(out.stdout)


def kies(sid: str, spec: dict) -> list[dict]:
    """Deterministic subset: sort, filter by size, then take an even stride."""
    kandidaten = [
        b for b in tree(spec["repo"], spec["commit"])
        if b["path"].lower().endswith(".pdf")
        and b["path"].startswith(spec["prefix"])
        and MIN_FILE_BYTES <= b["size"] <= MAX_FILE_BYTES
    ]
    kandidaten.sort(key=lambda b: b["path"])
    quota = min(spec["quota"], len(kandidaten))
    if quota == 0:
        return []
    # Even stride rather than the first N: the first N of a sorted list is one
    # directory, and one directory of a test suite is one feature.
    stap = len(kandidaten) / quota
    gekozen = [kandidaten[int(i * stap)] for i in range(quota)]
    print(f"  {sid}: {len(kandidaten)} candidates -> {len(gekozen)} picked",
          file=sys.stderr)
    return gekozen


def naam(sid: str, path: str) -> str:
    """A flat, collision-free, shell-safe name for the fetched copy."""
    veilig = "".join(c if (c.isalnum() or c in "-._") else "_" for c in path)
    return f"{sid}__{veilig}"


def haal(url: str, pogingen: int = 5) -> bytes:
    # raw.githubusercontent answers 400, not 429, when too many connections
    # arrive at once. Four of the first five hundred came back that way and the
    # same URLs served fine a second later, so a bare failure here would record
    # upstream throttling as a missing file.
    laatste: Exception | None = None
    for poging in range(pogingen):
        req = urllib.request.Request(
            url, headers={"User-Agent": "pdfluent-gate-corpus"})
        try:
            with urllib.request.urlopen(req, timeout=120) as r:
                return r.read()
        except Exception as e:  # noqa: BLE001 - retried below
            laatste = e
            time.sleep(1.5 * (2 ** poging))
    raise RuntimeError(f"{pogingen} attempts failed: {laatste}")


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--out", required=True)
    args = p.parse_args()

    print("Reading upstream trees at their pinned commits:", file=sys.stderr)
    werk = []
    for sid, spec in SOURCES.items():
        for blob in kies(sid, spec):
            werk.append((sid, spec, blob))

    print(f"Downloading {len(werk)} files to checksum them:", file=sys.stderr)
    entries: list[dict] = []
    mislukt: list[str] = []

    def een(item):
        sid, spec, blob = item
        url = raw_url(spec["repo"], spec["commit"], blob["path"])
        body = haal(url)
        return {
            "name": naam(sid, blob["path"]),
            "source": sid,
            "path": blob["path"],
            "sha256": hashlib.sha256(body).hexdigest(),
            "bytes": len(body),
        }

    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
        for item, fut in [(i, pool.submit(een, i)) for i in werk]:
            try:
                entries.append(fut.result())
            except Exception as e:  # noqa: BLE001 - report and keep going
                mislukt.append(f"{item[0]}:{item[2]['path']}: {e}")

    if mislukt:
        print(f"{len(mislukt)} download(s) failed:", file=sys.stderr)
        for m in mislukt[:20]:
            print(f"  {m}", file=sys.stderr)
        return 1

    entries.sort(key=lambda e: e["name"])
    doc = {
        "description": (
            "Gate corpus: a small, deterministic, checksum-pinned set of PDFs "
            "fetched from public licensed suites. Runs on every pull request. "
            "No PDF from this set is committed to the repository (#215)."
        ),
        "generated": datetime.date.today().isoformat(),
        "generator": "scripts/ci/build_gate_corpus_manifest.py",
        "selection": {
            "max_file_bytes": MAX_FILE_BYTES,
            "min_file_bytes": MIN_FILE_BYTES,
            "rule": "sort candidate paths, drop those outside the size band, "
                    "then take an even stride to the source's quota",
        },
        "sources": {
            sid: {
                "repo": s["repo"],
                "commit": s["commit"],
                "licence": s["licence"],
                "provenance": s["provenance"],
                "raw_base": f"https://raw.githubusercontent.com/{s['repo']}/{s['commit']}/",
            }
            for sid, s in SOURCES.items()
        },
        "total": len(entries),
        "total_bytes": sum(e["bytes"] for e in entries),
        "entries": entries,
    }

    pathlib.Path(args.out).write_text(json.dumps(doc, indent=1) + "\n")
    print(f"Wrote {args.out}: {doc['total']} entries, "
          f"{doc['total_bytes'] / 1e6:.1f} MB", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
