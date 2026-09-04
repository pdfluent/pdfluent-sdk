#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Materialise the gate corpus from its manifest, and prove it is the same set.

Every file is pinned twice: the URL carries the upstream commit, and the
manifest carries the SHA-256. A file that changed content under a moved tag, a
truncated download, a file quietly dropped from the manifest -- each of those
ends this script red rather than shrinking what the gate measures.

The two failure modes this exists to keep apart:

  * The corpus is wrong -> FATAL. Somebody changed what the gate looks at.
  * The corpus cannot be reached at all -> also FATAL here, on purpose. This
    is the gate corpus; it runs on every pull request and it is 35 MB. A
    "skip on network trouble" branch is how a gate stops running without
    anybody deciding that it should. The large holdout corpus is the one
    allowed to be absent, and it says `SKIPPED (not a pass)` when it is.

Usage:
    python3 scripts/ci/fetch_gate_corpus.py \
        --manifest corpus/GATE_CORPUS_MANIFEST.json \
        --out /tmp/gate-corpus
"""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import pathlib
import sys
import time
import urllib.parse
import urllib.request


# Five hundred entries times a five-step backoff is fifty minutes of grinding
# when nothing upstream answers, and the job would be killed before printing a
# word. The deadline turns a total outage into a result instead of a timeout:
# after it passes, the remaining entries fail immediately and the reason is the
# outage, in the log, where somebody can read it.
DEADLINE_S = 420


def sha256_bytes(body: bytes) -> str:
    return hashlib.sha256(body).hexdigest()


def sha256_file(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for blok in iter(lambda: f.read(1 << 16), b""):
            h.update(blok)
    return h.hexdigest()


def haal(url: str, pogingen: int = 5) -> bytes:
    # raw.githubusercontent answers 400 rather than 429 when several
    # connections arrive together, and the same URL serves fine a moment
    # later. Without this, upstream throttling reads as a corrupt corpus.
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
    p.add_argument("--manifest", required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--workers", type=int, default=8)
    args = p.parse_args()

    pad = pathlib.Path(args.manifest)
    if not pad.is_file():
        print(f"FATAL: manifest not found: {pad}", file=sys.stderr)
        return 1
    doc = json.loads(pad.read_text())
    bronnen = doc["sources"]
    entries = doc["entries"]

    uit = pathlib.Path(args.out)
    uit.mkdir(parents=True, exist_ok=True)

    fout: list[str] = []
    hergebruikt = 0
    verloopt = time.monotonic() + DEADLINE_S

    def een(entry: dict) -> str | None:
        doel = uit / entry["name"]
        # A warm cache is the normal case on a reused instance; verifying it
        # costs a read and removes the difference between cached and fetched.
        if doel.is_file() and sha256_file(doel) == entry["sha256"]:
            return "cached"
        bron = bronnen.get(entry["source"])
        if bron is None:
            return (f"{entry['name']}: source `{entry['source']}` is not "
                    "declared in the manifest")
        url = bron["raw_base"] + urllib.parse.quote(entry["path"])
        if time.monotonic() > verloopt:
            return (f"{entry['name']}: not attempted — the fetch passed its "
                    f"{DEADLINE_S}s deadline")
        try:
            body = haal(url)
        except Exception as e:  # noqa: BLE001 - reported, not raised
            return f"{entry['name']}: {e}"
        gevonden = sha256_bytes(body)
        if gevonden != entry["sha256"]:
            return (f"{entry['name']}: SHA-256 mismatch — manifest says "
                    f"{entry['sha256'][:16]}…, upstream now serves "
                    f"{gevonden[:16]}… ({len(body)} bytes)")
        doel.write_bytes(body)
        return None

    with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as pool:
        for r in pool.map(een, entries):
            if r == "cached":
                hergebruikt += 1
            elif r is not None:
                fout.append(r)

    aanwezig = sorted(x.name for x in uit.iterdir() if x.is_file())
    verwacht = sorted(e["name"] for e in entries)

    if time.monotonic() > verloopt:
        print(f"[gate-corpus] the {DEADLINE_S}s deadline passed; entries after "
              "that point were not attempted.", file=sys.stderr)

    print(f"[gate-corpus] {len(entries)} entries, {hergebruikt} already present, "
          f"{len(entries) - hergebruikt - len(fout)} fetched, "
          f"{len(fout)} failed")

    if fout:
        print(f"\n[gate-corpus] FATAL: {len(fout)} entr(ies) could not be "
              "produced as the manifest describes them:", file=sys.stderr)
        for f in fout[:25]:
            print(f"  {f}", file=sys.stderr)
        if len(fout) > 25:
            print(f"  … and {len(fout) - 25} more", file=sys.stderr)
        print(
            "\nA gate corpus that cannot be reproduced byte for byte is not a "
            "baseline. Either upstream moved (repin the commit in "
            "scripts/ci/build_gate_corpus_manifest.py and rebuild the "
            "manifest, in a commit somebody reviews) or the network is "
            "refusing — and neither is a reason to measure a smaller set. "
            "(#276)",
            file=sys.stderr,
        )
        return 1

    if aanwezig != verwacht:
        extra = set(aanwezig) - set(verwacht)
        print(f"[gate-corpus] FATAL: the output directory holds {len(extra)} "
              "file(s) the manifest does not list; the gate would measure "
              "something the manifest does not describe.", file=sys.stderr)
        for x in sorted(extra)[:10]:
            print(f"  unexpected: {x}", file=sys.stderr)
        return 1

    print(f"[gate-corpus] OK: {len(entries)} files in {uit}, every one matching "
          "its pinned checksum.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
