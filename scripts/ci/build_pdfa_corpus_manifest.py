#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Draw the PDF/A measurement corpus -- 10,000 tuning, 10,000 holdout -- and pin it.

Run by hand, not by a job: it moves seventeen gigabytes over the network and
takes hours. What CI checks is its output, in `a_manifest_pins_every_file.py`.

    python3 scripts/ci/build_pdfa_corpus_manifest.py index  --work /srv/pdfa-corpus
    python3 scripts/ci/build_pdfa_corpus_manifest.py probe  --work /srv/pdfa-corpus
    python3 scripts/ci/build_pdfa_corpus_manifest.py select --work /srv/pdfa-corpus \
        --repo .

Three stages, in that order, each restartable, each writing plain text into the
work directory:

  index   the universe. Every PDF upstream holds, with the coordinates needed to
          fetch it back: which zip, at which offset, compressed how. For
          GovDocs1 that means reading the central directory of a thousand zips
          -- eighty-five megabytes of range requests against half a terabyte of
          archives, because a zip central directory is at the end of the file
          and S3 serves ranges. For SAFEDOCS it means the two published metadata
          tables, which carry the SHA-256 and the length of all 7.9 million
          files, so nothing has to be downloaded to know what is there.

  probe   the pool. Walk the seeded order, fetch each document's bytes, hash
          them, and read the strata off the bytes themselves. Restartable: the
          probe log is append-only and a rerun skips what it already holds.

  select  the corpus. A pure function of the probe log -- no network, no clock,
          no order-of-arrival. Given the same log it draws the same twenty
          thousand documents, which is the property the whole exercise rests on.

WHY THIS IS NOT scripts/build_corpus_manifest.py OR build_gate_corpus_manifest.py

Both of those exist and neither answers this. `build_corpus_manifest.py` takes a
directory of documents somebody already has and canonicalises it -- dedupe by
digest, primary path, oracle status. `build_gate_corpus_manifest.py` pins 500
files from four upstream git repositories, by commit, for the pull-request gate.
What is new here is drawing a sample that nobody has yet: an ordering over two
hundred thousand documents that a stranger can recompute, a split into halves
that cannot be edited afterwards, and strata read from bytes that have to be
fetched before they can be read.

The reason the strata are read from the bytes and not from the published
metadata, even where the metadata exists: SAFEDOCS ships `pdfinfo` output for
every file, and it would have been free. But `pdfinfo` does not say whether the
fonts are embedded, whether there is transparency, or whether the colour is
CMYK -- and those are the three things a PDF/A converter actually struggles
with. A corpus stratified on what was cheap to know is a corpus stratified away
from the problem.
"""

from __future__ import annotations

import argparse
import collections
import concurrent.futures
import csv
import gzip
import hashlib
import io
import json
import pathlib
import struct
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
import zlib

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import pdfa_corpus_rule as rule  # noqa: E402

S3 = "https://digitalcorpora.s3.amazonaws.com"
GOVDOCS_ZIPS = f"{S3}/corpora/files/govdocs1/zipfiles"
SAFEDOCS = f"{S3}/corpora/files/CC-MAIN-2021-31-PDF-UNTRUNCATED"

SOURCES = {
    "govdocs1": {
        "name": "GovDocs1",
        "home": "https://digitalcorpora.org/corpora/file-corpora/files/",
        "archive_base": GOVDOCS_ZIPS,
        "archive_layout": "<archive_base>/<zip>.zip, where <zip> is the entry's "
                          "three-digit zip column; the document is the zip "
                          "member at the entry's offset.",
        "licence": "Freely redistributable; the corpus was assembled from US "
                   "government web servers and is published for unrestricted "
                   "research use by digitalcorpora.org.",
        "provenance": "GovDocs1, ~986,000 files harvested from .gov web "
                      "servers by the Naval Postgraduate School and published "
                      "by digitalcorpora.org as 1,000 zip archives; the PDF "
                      "subset is ~200,000 files. Read 07-09-2026.",
    },
    "safedocs": {
        "name": "CC-MAIN-2021-31-PDF-UNTRUNCATED (DARPA SafeDocs)",
        "home": f"{SAFEDOCS}/README.md",
        "archive_base": f"{SAFEDOCS}/zipfiles",
        "archive_layout": "<archive_base>/<group>/<zip>.zip, where <zip> is the "
                          "entry's zip column padded to four digits and <group> "
                          "is the thousand it falls in, as `0000-0999`; the "
                          "document is the zip member at the entry's offset.",
        "licence": "Common Crawl terms of use "
                   "(https://commoncrawl.org/terms-of-use/full/); the corpus "
                   "is published for research by digitalcorpora.org under the "
                   "DARPA SafeDocs programme.",
        "provenance": "7.9 million unique PDFs refetched untruncated from the "
                      "Common Crawl CC-MAIN-2021-31 crawl (July/August 2021) "
                      "and published by digitalcorpora.org in 7,933 zip "
                      "archives with per-file SHA-256 in "
                      "cc-provenance-20230303.csv.gz. Read 07-09-2026.",
    },
}

# One in ten zip shards. SAFEDOCS numbers its files by the SHA-256 of their
# content and packs them a thousand to a zip, so the shard a document lands in
# is independent of anything about the document. Taking a tenth of the shards
# therefore costs nothing in representativeness and saves seven thousand central
# directories -- roughly a gigabyte of range requests every time somebody
# reproduces this.
SAFEDOCS_SHARD_SHARE = 10
SAFEDOCS_ZIP_COUNT = 7933

# How deep to walk before selecting. The pool has to survive the half-split
# (roughly half of it lands in each half), the documents that turn out not to be
# PDFs at all, and the strata floors, which need headroom above the plain
# six-and-four-thousand.
POOL = {"govdocs1": 16_000, "safedocs": 11_000}

TIMEOUT = 120


def _flat_name(source: str, key: str) -> str:
    """The name a document carries on disk once it is out of its zip.

    GovDocs1 keeps the shape the 300-document set already uses
    (`000/000012.pdf` -> `000_000012.pdf`), so the older lists and this one name
    the same file. SAFEDOCS names are already flat and unique.
    """
    return key.replace("/", "_") if source == "govdocs1" else key


def _fetch(url: str, start: int | None = None, length: int | None = None,
           attempts: int = 5) -> bytes:
    headers = {"User-Agent": "pdfluent-pdfa-corpus"}
    if start is not None:
        end = "" if length is None else str(start + length - 1)
        headers["Range"] = f"bytes={start}-{end}"
    last: Exception | None = None
    for attempt in range(attempts):
        try:
            req = urllib.request.Request(url, headers=headers)
            with urllib.request.urlopen(req, timeout=TIMEOUT) as r:
                return r.read()
        except Exception as e:  # noqa: BLE001 - retried, then reported
            last = e
            time.sleep(1.5 * (2 ** attempt))
    raise RuntimeError(f"{attempts} attempts failed for {url}: {last}")


def _content_length(url: str) -> int:
    req = urllib.request.Request(url, method="HEAD",
                                 headers={"User-Agent": "pdfluent-pdfa-corpus"})
    with urllib.request.urlopen(req, timeout=TIMEOUT) as r:
        return int(r.headers["Content-Length"])


# ---------------------------------------------------------------- stage: index

_CD_ENTRY = struct.Struct("<HHHHHHIIIHHHHHII")


def _central_directory(url: str) -> list[tuple[str, int, int, int, int, int]]:
    """(name, method, compressed, uncompressed, crc32, local header offset)."""
    size = _content_length(url)
    tail = _fetch(url, max(0, size - 70_000), min(70_000, size))
    i = tail.rfind(b"PK\x05\x06")
    if i < 0:
        raise RuntimeError(f"no end-of-central-directory in {url}")
    count, cd_size, cd_off = struct.unpack("<HII", tail[i + 10:i + 20])
    if cd_off == 0xFFFFFFFF:
        raise RuntimeError(f"{url} is zip64; this builder does not read it")
    cd = _fetch(url, cd_off, cd_size)
    out = []
    p = 0
    while p + 46 <= len(cd) and cd[p:p + 4] == b"PK\x01\x02":
        (_v, _vn, _fl, method, _mt, _md, crc, csize, usize,
         nlen, elen, clen, _dk, _ia, _ea, lho) = _CD_ENTRY.unpack(
            cd[p + 4:p + 46])
        name = cd[p + 46:p + 46 + nlen].decode("utf-8", "replace")
        if name.lower().endswith(".pdf") and usize > 0:
            out.append((name, method, csize, usize, crc, lho))
        p += 46 + nlen + elen + clen
    if not out:
        raise RuntimeError(f"{url}: {count} entries, no PDF among them")
    return out


def _index_govdocs(work: pathlib.Path, workers: int,
                   shards: int = 1000) -> pathlib.Path:
    out = work / "index_govdocs1.tsv"
    done = set()
    if out.is_file():
        done = {line.split("\t", 1)[0].split("/", 1)[0]
                for line in out.read_text().splitlines() if line}
    lock = threading.Lock()
    handle = out.open("a")

    def one(n: int) -> str | None:
        shard = f"{n:03d}"
        if shard in done:
            return None
        url = f"{GOVDOCS_ZIPS}/{shard}.zip"
        try:
            entries = _central_directory(url)
        except Exception as e:  # noqa: BLE001 - reported, the shard is skipped
            return f"{shard}: {e}"
        rows = "".join(
            f"{name}\t{shard}\t{method}\t{csize}\t{usize}\t{crc}\t{lho}\n"
            for name, method, csize, usize, crc, lho in entries)
        with lock:
            handle.write(rows)
            handle.flush()
        return None

    with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
        for problem in pool.map(one, range(shards)):
            if problem:
                print(f"[index] govdocs1 {problem}", file=sys.stderr)
    handle.close()
    return out


def _safedocs_shards() -> list[int]:
    """The seeded tenth of the zip shards this corpus may draw from."""
    keep = []
    for n in range(SAFEDOCS_ZIP_COUNT):
        digest = hashlib.sha256(
            f"{rule.SEED}|safedocs-shard|{n:04d}".encode()).hexdigest()
        if int(digest[:8], 16) % SAFEDOCS_SHARD_SHARE == 0:
            keep.append(n)
    return keep


def _safedocs_zip_url(shard: int) -> str:
    group = (shard // 1000) * 1000
    return (f"{SAFEDOCS}/zipfiles/{group:04d}-{group + 999:04d}/"
            f"{shard:04d}.zip")


def _stream_csv_gz(url: str):
    req = urllib.request.Request(url,
                                 headers={"User-Agent": "pdfluent-pdfa-corpus"})
    with urllib.request.urlopen(req, timeout=TIMEOUT) as r:
        with gzip.GzipFile(fileobj=r) as gz:
            yield from csv.DictReader(io.TextIOWrapper(gz, "utf-8", "replace"))


def _index_safedocs(work: pathlib.Path) -> pathlib.Path:
    """The published provenance table, narrowed to the shards we may draw from.

    One pass over 8.4 million rows. The table is URL-keyed, so a file that was
    crawled from five addresses appears five times; the first row wins, which is
    deterministic because the table ships in a fixed order and we pin the date
    in its name.
    """
    out = work / "index_safedocs.tsv"
    if out.is_file() and out.stat().st_size > 0:
        return out
    shards = set(_safedocs_shards())
    seen: set[str] = set()
    tmp = out.with_suffix(".partial")
    rows = 0
    with tmp.open("w") as f:
        for row in _stream_csv_gz(
                f"{SAFEDOCS}/metadata/cc-provenance-20230303.csv.gz"):
            name = (row.get("file_name") or "").strip()
            digest = (row.get("fetched_digest") or "").strip().lower()
            length = (row.get("fetched_length") or "").strip()
            if not name.endswith(".pdf") or len(digest) != 64:
                continue
            if name in seen:
                continue
            try:
                number = int(name[:-4])
                nbytes = int(length)
            except ValueError:
                continue
            if nbytes <= 0 or number // 1000 not in shards:
                continue
            seen.add(name)
            f.write(f"{name}\t{number // 1000}\t{digest}\t{nbytes}\t"
                    f"{row.get('url_id', '')}\n")
            rows += 1
    tmp.rename(out)
    print(f"[index] safedocs: {rows} files in "
          f"{len(shards)} of {SAFEDOCS_ZIP_COUNT} shards")
    return out


def _read_index(work: pathlib.Path, source: str) -> list[dict]:
    path = work / f"index_{source}.tsv"
    entries = []
    for line in path.read_text().splitlines():
        if not line:
            continue
        parts = line.split("\t")
        if source == "govdocs1":
            name, shard, method, csize, usize, crc, lho = parts
            entries.append({"key": name, "shard": shard,
                            "method": int(method), "csize": int(csize),
                            "bytes": int(usize), "crc32": int(crc),
                            "offset": int(lho)})
        else:
            name, shard, digest, nbytes, url_id = parts
            entries.append({"key": name, "shard": int(shard),
                            "sha256": digest, "bytes": int(nbytes),
                            "url_id": url_id})
    entries.sort(key=lambda e: rule.rank(source, e["key"]))
    return entries


# ---------------------------------------------------------------- stage: probe

def _local_payload(url: str, offset: int, method: int, csize: int,
                   usize: int) -> bytes:
    """Fetch and inflate one zip member from its local header outward."""
    raw = _fetch(url, offset, 30 + csize + 4096)
    if raw[:4] != b"PK\x03\x04":
        raise RuntimeError(f"no local file header at {offset}")
    nlen, elen = struct.unpack("<HH", raw[26:30])
    start = 30 + nlen + elen
    data = raw[start:start + csize]
    if len(data) < csize:
        data += _fetch(url, offset + start + len(data), csize - len(data))
    body = zlib.decompress(data, -15) if method == 8 else data
    if len(body) != usize:
        raise RuntimeError(f"inflated {len(body)} bytes, header says {usize}")
    return body


class _ZipDirectories:
    """Central directories, read once per zip and kept for the run."""

    def __init__(self) -> None:
        self._cache: dict[int, dict[str, tuple[int, int, int, int]]] = {}
        self._lock = threading.Lock()

    def offsets(self, shard: int) -> dict[str, tuple[int, int, int, int]]:
        with self._lock:
            hit = self._cache.get(shard)
        if hit is not None:
            return hit
        table = {name: (method, csize, usize, lho)
                 for name, method, csize, usize, _crc, lho
                 in _central_directory(_safedocs_zip_url(shard))}
        with self._lock:
            self._cache[shard] = table
        return table


def _probe(work: pathlib.Path, source: str, workers: int, limit: int,
           keep: pathlib.Path | None = None, first: int = 0,
           tag: str = "") -> None:
    already = set()
    for other in sorted(work.glob(f"probe_{source}*.jsonl")):
        for line in other.read_text().splitlines():
            if line:
                already.add(json.loads(line)["key"])
    entries = _read_index(work, source)[first:limit]
    todo = [e for e in entries if e["key"] not in already]
    print(f"[probe] {source}: pool {len(entries)}, "
          f"{len(already)} already probed, {len(todo)} to go")
    _probe_entries(work, source, todo, workers, keep, tag)


def _probe_entries(work: pathlib.Path, source: str, todo: list[dict],
                   workers: int, keep: pathlib.Path | None,
                   tag: str = "") -> None:
    """Fetch, hash and classify one list of index entries.

    Split out from `_probe` because the targeted top-up feeds it a list nobody
    walked to -- the candidates upstream metadata says might fill a thin cell --
    and the fetching, hashing and recording is the same work either way.
    """
    log = work / f"probe_{source}{tag}.jsonl"

    dirs = _ZipDirectories()
    lock = threading.Lock()
    handle = log.open("a")
    counted = [0]

    def one(entry: dict) -> None:
        key = entry["key"]
        try:
            if source == "govdocs1":
                url = f"{GOVDOCS_ZIPS}/{entry['shard']}.zip"
                body = _local_payload(url, entry["offset"], entry["method"],
                                      entry["csize"], entry["bytes"])
                offset, method, csize = (entry["offset"], entry["method"],
                                         entry["csize"])
            else:
                shard = entry["shard"]
                url = _safedocs_zip_url(shard)
                table = dirs.offsets(shard)
                if key not in table:
                    raise RuntimeError("not in its zip's central directory")
                method, csize, usize, offset = table[key]
                body = _local_payload(url, offset, method, csize, usize)
        except Exception as e:  # noqa: BLE001 - recorded, never fatal
            record = {"key": key, "ok": False, "reason": str(e)[:200]}
        else:
            digest = hashlib.sha256(body).hexdigest()
            if source == "safedocs" and digest != entry["sha256"]:
                record = {"key": key, "ok": False,
                          "reason": "digest differs from the published "
                                    "provenance table"}
            else:
                seen = rule.classify(body)
                if seen is None:
                    record = {"key": key, "ok": False,
                              "reason": "no %PDF header in the first kilobyte"}
                else:
                    record = {"key": key, "ok": True, "sha256": digest,
                              "bytes": len(body), "offset": offset,
                              "method": method, "csize": csize,
                              "shard": entry["shard"], **seen}
                    if source == "safedocs":
                        record["url_id"] = entry["url_id"]
                    # Probing already paid for these bytes. Writing them out
                    # here means the epic's first measuring run does not fetch
                    # the same seventeen gigabytes a second time; the corpus is
                    # still reproducible from the manifest alone.
                    if keep is not None:
                        # Flat, and the same names fetch_pdfa_corpus.py writes,
                        # so pointing that script at this directory verifies
                        # what is already here instead of fetching it again.
                        (keep / _flat_name(source, key)).write_bytes(body)
        with lock:
            handle.write(json.dumps(record, sort_keys=True) + "\n")
            counted[0] += 1
            if counted[0] % 250 == 0:
                handle.flush()
                print(f"[probe] {source}: {counted[0]}/{len(todo)}", flush=True)

    with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
        list(pool.map(one, todo))
    handle.close()


# -------------------------------------------------------------- stage: targets

# How far past a cell's shortfall to probe before giving up on it. Upstream
# metadata predicts a cell; our own classifier decides it, and the two disagree
# often enough that a one-for-one walk would leave every cell a little short.
# Four times is generous without turning a top-up into a second full probe.
TARGET_OVERSHOOT = 4

# A ceiling on the whole top-up, so a cell that upstream simply does not hold --
# PDF 1.0 in a corpus crawled in 2021 -- costs a bounded amount of network and
# then gets written down as thin.
TARGET_BUDGET = 12_000


def _safedocs_metadata(work: pathlib.Path) -> dict[str, tuple[str, str]]:
    """file_name -> (pdf_version, producer), from the published pdfinfo table.

    This is used to AIM, never to classify. The stratum a document ends up in is
    always the one our own classifier reads off its bytes; poppler's answer only
    decides which documents are worth fetching when a cell is thin. That
    distinction is what keeps the corpus honest about what it holds while still
    letting a rare producer be found in eight hundred thousand files without
    downloading all of them.
    """
    cache = work / "meta_safedocs.tsv"
    wanted = {row["key"] for row in _read_index(work, "safedocs")}
    if not cache.is_file():
        tmp = cache.with_suffix(".partial")
        rows = 0
        with tmp.open("w") as f:
            for row in _stream_csv_gz(
                    f"{SAFEDOCS}/metadata/pdfinfo-20230315.csv.gz"):
                name = (row.get("file_name") or "").strip()
                if name not in wanted:
                    continue
                version = (row.get("pdf_version") or "").strip()
                producer = (row.get("producer") or
                            row.get("creator") or "").strip()
                producer = producer.replace("\t", " ").replace("\n", " ")[:200]
                f.write(f"{name}\t{version}\t{producer}\n")
                rows += 1
        tmp.rename(cache)
        print(f"[targets] pdfinfo: {rows} rows kept for the indexed shards")
    out = {}
    for line in cache.read_text().splitlines():
        parts = line.split("\t")
        if len(parts) == 3:
            out[parts[0]] = (parts[1], parts[2])
    return out


def _shortfall(work: pathlib.Path) -> dict[tuple[str, str], int]:
    """Which cells are still under the floor, and by how much, per half."""
    census: dict[str, collections.Counter[str]] = {
        "tuning": collections.Counter(), "holdout": collections.Counter()}
    for source in rule.SOURCE_SHARE:
        for row in _pool(work, source):
            census[rule.half(source, row["key"])].update(rule.strata_of(row))
    short: dict[tuple[str, str], int] = {}
    for where in ("tuning", "holdout"):
        for cell in rule.floored_cells():
            if cell.startswith("feature:"):
                continue  # no published table predicts a feature; see _targets
            missing = rule.STRATUM_FLOOR - census[where][cell]
            if missing > 0:
                short[(where, cell)] = missing
    return short


def _targets(work: pathlib.Path, workers: int,
             keep: pathlib.Path | None) -> None:
    """Fetch more documents, aimed at the cells the plain walk left thin.

    Only producer, version and size are targetable, and only those. A feature
    class -- transparency, embedded fonts, CMYK -- is not in any published
    table, so a thin one can only be filled by probing further into the same
    order, which the plain walk already does. That asymmetry is written into
    the rule rather than papered over.
    """
    short = _shortfall(work)
    if not short:
        print("[targets] every cell is at its floor; nothing to aim at")
        return
    print(f"[targets] {len(short)} cell(s) under the floor")

    meta = _safedocs_metadata(work)
    indexes = {source: _read_index(work, source) for source in rule.SOURCE_SHARE}
    probed = set()
    for source in rule.SOURCE_SHARE:
        for log in sorted(work.glob(f"probe_{source}*.jsonl")):
            for line in log.read_text().splitlines():
                if line:
                    probed.add((source, json.loads(line)["key"]))

    budget = TARGET_BUDGET
    for (where, cell), missing in sorted(short.items()):
        kind, value = cell.split(":", 1)
        picked: dict[str, list[dict]] = {s: [] for s in rule.SOURCE_SHARE}
        want = missing * TARGET_OVERSHOOT
        for source, entries in indexes.items():
            if kind in ("producer", "version") and source != "safedocs":
                continue  # GovDocs1 publishes no per-file metadata to aim with
            for entry in entries:
                if len(picked[source]) >= want or budget <= 0:
                    break
                key = entry["key"]
                if (source, key) in probed or rule.half(source, key) != where:
                    continue
                if kind == "size":
                    if rule.size_band(entry["bytes"]) != value:
                        continue
                elif source == "safedocs":
                    found = meta.get(key)
                    if found is None:
                        continue
                    version, producer = found
                    if kind == "version" and version != value:
                        continue
                    if (kind == "producer"
                            and rule.producer_family(producer) != value):
                        continue
                picked[source].append(entry)
                probed.add((source, key))
                budget -= 1
        for source, entries in picked.items():
            if not entries:
                continue
            print(f"[targets] {where} {cell}: {len(entries)} candidate(s) "
                  f"from {source}")
            _probe_entries(work, source, entries, workers, keep,
                           tag=".targets")
    print(f"[targets] {TARGET_BUDGET - budget} extra document(s) probed")


# --------------------------------------------------------------- stage: select

def _pool(work: pathlib.Path, source: str) -> list[dict]:
    """Every document that probed clean, from every slice's log.

    Slices exist because classifying a document is CPU work and one process
    fills one core; they are disjoint ranges of the same seeded order, so the
    union is the same pool whether it was probed by one machine or four.
    """
    kept = {}
    for log in sorted(work.glob(f"probe_{source}*.jsonl")):
        for line in log.read_text().splitlines():
            if not line:
                continue
            row = json.loads(line)
            if row.get("ok"):
                kept[row["key"]] = row
    out = []
    for key, row in kept.items():
        row = dict(row)
        row["source"] = source
        row["producer_family"] = rule.producer_family(row.get("producer"))
        row["size_band"] = rule.size_band(row["bytes"])
        out.append(row)
    out.sort(key=lambda r: rule.rank(source, r["key"]))
    return out


def _draw(pool: list[dict], quota: int) -> list[dict]:
    """Fill the strata floors first, then top up in seeded order.

    Two passes over the same order. The first admits a document only if it is
    still needed by some cell that is under its floor -- that is what stops a
    corpus of ten thousand from being ten thousand Word files with four scanned
    ones. The second fills the rest in order, so what is not reserved stays a
    plain seeded sample rather than a shaped one.
    """
    census: collections.Counter[str] = collections.Counter()
    chosen: dict[str, dict] = {}

    for row in pool:
        if len(chosen) >= quota:
            break
        cells = rule.strata_of(row)
        floored = [c for c in cells
                   if c.split(":", 1)[0] != "feature"
                   or c.split(":", 1)[1] in rule.FLOORED_FEATURES]
        if any(census[c] < rule.STRATUM_FLOOR for c in floored):
            chosen[row["key"]] = row
            census.update(cells)

    for row in pool:
        if len(chosen) >= quota:
            break
        if row["key"] not in chosen:
            chosen[row["key"]] = row
            census.update(rule.strata_of(row))

    return sorted(chosen.values(), key=lambda r: rule.rank(r["source"],
                                                           r["key"]))


def _entry_line(row: dict) -> str:
    return "\t".join([
        row["source"][0],
        row["key"],
        row["sha256"],
        str(row["bytes"]),
        str(row["shard"]),
        str(row["offset"]),
        str(row["method"]),
        str(row["csize"]),
        row["producer_family"],
        row["version"],
        row["size_band"],
        ",".join(row["features"]) or "-",
    ])


SHARDS = 4


def _write_set(repo: pathlib.Path, half: str,
               rows: list[dict]) -> dict:
    directory = repo / "corpus" / "pdfa"
    directory.mkdir(parents=True, exist_ok=True)
    buckets: dict[int, list[str]] = {i: [] for i in range(SHARDS)}
    for row in rows:
        buckets[rule.shard(row["source"], row["key"], SHARDS)].append(
            _entry_line(row))
    files = {}
    for i, lines in buckets.items():
        name = f"pdfa_{half}_entries.{i:02d}.tsv"
        body = "".join(line + "\n" for line in sorted(lines))
        (directory / name).write_text(body)
        files[name] = {
            "lines": len(lines),
            "sha256": hashlib.sha256(body.encode()).hexdigest(),
        }
    return files


def _select(work: pathlib.Path, repo: pathlib.Path) -> int:
    halves: dict[str, list[dict]] = {"tuning": [], "holdout": []}
    per_source_census: dict[str, dict] = {}
    # One content-uniqueness pass over both sources before either half is
    # filled. Doing it per source, or per half, would leave exactly the pairs
    # that matter: the same bytes under two names, one name in each half.
    pools = {source: _pool(work, source) for source in rule.SOURCE_SHARE}
    kept, duplicates = rule.content_unique(
        [row for rows in pools.values() for row in rows])
    print(f"[select] {duplicates} document(s) dropped as a copy of an "
          f"earlier-ranked one; {len(kept)} distinct documents in the pool")
    unique: dict[str, list[dict]] = {source: [] for source in pools}
    for row in kept:
        unique[row["source"]].append(row)

    for source, quota in rule.SOURCE_SHARE.items():
        pool = unique[source]
        split: dict[str, list[dict]] = {"tuning": [], "holdout": []}
        for row in pool:
            split[rule.half(source, row["key"])].append(row)
        for half, candidates in split.items():
            drawn = _draw(candidates, quota)
            halves[half].extend(drawn)
            per_source_census.setdefault(half, {})[source] = {
                "pool": len(candidates), "drawn": len(drawn), "quota": quota,
            }

    written = {}
    for half, rows in halves.items():
        rows.sort(key=lambda r: (r["source"], r["key"]))
        census: collections.Counter[str] = collections.Counter()
        for row in rows:
            census.update(rule.strata_of(row))
        files = _write_set(repo, half, rows)
        short = sorted(f"{cell} ({census[cell]})"
                       for cell in rule.floored_cells()
                       if census[cell] < rule.STRATUM_FLOOR)
        header = {
            "schema": "pdfluent.pdfa-corpus/1",
            "half": half,
            "rule": {
                "module": "scripts/ci/pdfa_corpus_rule.py",
                "version": rule.RULE_VERSION,
                "seed": rule.SEED,
                "per_half": rule.PER_HALF,
                "source_share": rule.SOURCE_SHARE,
                "stratum_floor": rule.STRATUM_FLOOR,
                # The parameters that produced this draw, recorded so the
                # manifest says what it cost as well as what it is.
                "pool": POOL,
                "safedocs_shard_share": SAFEDOCS_SHARD_SHARE,
                "target_overshoot": TARGET_OVERSHOOT,
                "target_budget": TARGET_BUDGET,
            },
            "sources": {
                key: {**meta, "id": key} for key, meta in SOURCES.items()},
            "total": len(rows),
            "total_bytes": sum(r["bytes"] for r in rows),
            # A pool-level fact, recorded in both halves because both were
            # drawn from the same de-duplicated pool.
            "content_duplicates_dropped": duplicates,
            "drawn_from": per_source_census[half],
            "entry_files": files,
            "columns": ["source", "key", "sha256", "bytes", "zip", "offset",
                        "method", "csize", "producer_family", "version",
                        "size_band", "features"],
            "strata": dict(sorted(census.items())),
            "strata_below_floor": short,
        }
        path = repo / "corpus" / "pdfa" / f"PDFA_{half.upper()}_MANIFEST.json"
        path.write_text(json.dumps(header, indent=2, sort_keys=True) + "\n")
        written[half] = (len(rows), short)
        print(f"[select] {half}: {len(rows)} documents, "
              f"{len(short)} cell(s) below the floor")
        if len(rows) < rule.PER_HALF:
            print(f"[select] {half}: SHORT -- {rule.PER_HALF - len(rows)} "
                  "document(s) missing. The probe pool did not carry enough "
                  "clean documents; probe further into the seeded order "
                  "(--first/--limit) and run select again.", file=sys.stderr)
        for cell in short:
            print(f"[select]   short: {cell}")

    return 0


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("stage",
                   choices=["index", "probe", "targets", "select"])
    p.add_argument("--work", required=True)
    p.add_argument("--repo", default=".")
    p.add_argument("--workers", type=int, default=16)
    p.add_argument("--source", choices=["govdocs1", "safedocs"])
    p.add_argument("--limit", type=int)
    p.add_argument("--first", type=int, default=0,
                   help="start this far into the seeded order, so "
                        "several machines can probe disjoint slices")
    p.add_argument("--tag", default="",
                   help="suffix for this slice's probe log")
    p.add_argument("--keep",
                   help="write every probed document here, so the "
                        "first measuring run need not refetch them")
    p.add_argument("--shards", type=int,
                   help="index fewer GovDocs1 zips; smoke tests only")
    args = p.parse_args()

    work = pathlib.Path(args.work)
    work.mkdir(parents=True, exist_ok=True)
    sources = [args.source] if args.source else list(rule.SOURCE_SHARE)

    if args.stage == "index":
        for source in sources:
            if source == "govdocs1":
                _index_govdocs(work, args.workers,
                               args.shards or 1000)
            else:
                _index_safedocs(work)
            rows = len((work / f"index_{source}.tsv").read_text().splitlines())
            print(f"[index] {source}: {rows} PDFs in the universe")
        return 0

    if args.keep:
        pathlib.Path(args.keep).mkdir(parents=True, exist_ok=True)

    if args.stage == "targets":
        _targets(work, args.workers,
                 pathlib.Path(args.keep) if args.keep else None)
        return 0

    if args.stage == "probe":
        for source in sources:
            _probe(work, source, args.workers,
                   args.limit or POOL[source],
                   pathlib.Path(args.keep) if args.keep else None,
                   args.first, args.tag)
        return 0

    return _select(work, pathlib.Path(args.repo))


if __name__ == "__main__":
    raise SystemExit(main())
