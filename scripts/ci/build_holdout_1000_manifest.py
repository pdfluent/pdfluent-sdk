#!/usr/bin/env python3
# Copyright (c) 2026 Innovation Trigger B.V.
#
# PDFluent is available under two licences, at your option: the GNU AGPLv3, or
# the PDFluent Commercial Licence. See the LICENSE file in this repository --
# that file travels with the copy you received, which a URL does not.
"""Materialise the thousand-document PDF/A holdout and pin it (#365).

    python3 scripts/ci/build_holdout_1000_manifest.py hash  --work /tmp/holdout
    python3 scripts/ci/build_holdout_1000_manifest.py write --work /tmp/holdout
    python3 scripts/ci/build_holdout_1000_manifest.py write --check

Two stages, and the split between them is the point. `hash` is the only one that
touches the network: it reads the central directory of every GovDocs1 zip the
list names, fetches each member and hashes it, and writes
`corpus/pdfa/pdfa_holdout1000_entries.tsv` -- the coordinates and digests that
turn a list of names into a set anybody can fetch back. `write` is a pure
function of files already in the repository, so the manifest can be redrawn on
any machine with no network at all, which is what
`the_holdout_1000_is_the_drawn_set.py` leans on when it redraws it on every push.

WHY THE SET IS NOT THE LIST

`benchmarks/pdfa/govdocs_holdout_1000.txt` was drawn before the corpus rule
existed and predates `benchmarks/HOLDOUTS.md`. It carries no digests, so nobody
could establish that the files measured were the files drawn; and it was drawn
against nothing, so it overlaps the ten thousand of #377 -- fifty-two of its
thousand names, thirty-two of them in the tuning half, which debugging may open
at any time. A holdout that shares documents with the set the rounds are tuned
on is not a holdout for those documents.

So the list is the POPULATION and not the set. The set is what survives the one
exclusion this file applies: every document that any tuning set, any test set,
or either half of the ten thousand already holds -- by name and by the bytes
behind that name -- is out. What survives is drawn in a seeded order and sealed.

WHY IT IS NOT TOPPED BACK UP TO A THOUSAND

Because there is nowhere to top it up from that would not cost more than the
round number is worth. The only pools with committed digests are the two halves
of the ten thousand, and taking documents out of the sealed half to pad this set
would spend that holdout to make this one look tidy. Drawing fresh documents
from GovDocs1 would mean a second population indexed on a different day, under a
rule the list itself never had. A short set is a smaller denominator, which is a
statement to make out loud rather than a hole to fill: `total` says what it drew
and `benchmarks/HOLDOUTS.md` says what it started from.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import pathlib
import sys
import threading

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import build_pdfa_corpus_manifest as corpus  # noqa: E402
import holdout_1000_rule as rule  # noqa: E402

REPO = pathlib.Path(__file__).resolve().parents[2]
PDFA_CORPUS = REPO / "corpus" / "pdfa"
POPULATION = REPO / "benchmarks" / "pdfa" / "govdocs_holdout_1000.txt"
ENTRIES = PDFA_CORPUS / "pdfa_holdout1000_entries.tsv"
MANIFEST = PDFA_CORPUS / "PDFA_HOLDOUT_1000.json"
THE_300 = REPO / "crates" / "pdf-manip" / "examples" / "pdfa_fidelity_inputs_300.json"

ENTRY_COLUMNS = ("key", "sha256", "bytes", "zip", "offset", "method", "csize")
SOURCE_LETTER = {"g": "govdocs1", "s": "safedocs"}
SCHEMA = "pdfluent.pdfa-holdout1000/1"

# What the draw is held out OF, in the order the exclusion applies it. Named in
# one place because the manifest publishes the list and the guard checks it: a
# set excluded in the code and absent from this tuple would be an exclusion the
# manifest does not admit to.
EXCLUDED_FROM = (
    "corpus/pdfa/PDFA_TUNING_1000.json",
    "corpus/pdfa/PDFA_TEST_300.json",
    "crates/pdf-manip/examples/pdfa_fidelity_inputs_300.json",
    "corpus/pdfa/PDFA_TUNING_MANIFEST.json",
    "corpus/pdfa/PDFA_HOLDOUT_MANIFEST.json",
)

DESCRIPTION = (
    "The thousand-document PDF/A holdout (#365): the documents of "
    "benchmarks/pdfa/govdocs_holdout_1000.txt that no tuning set, no test set "
    "and neither half of the ten thousand of #377 already holds -- excluded by "
    "name and by content. Measured once, on the four axes, against Ghostscript "
    "on the same files, and reported per document. Nothing is tuned on it; a "
    "change made because of something found here burns it, and "
    "benchmarks/HOLDOUTS.md says so and draws a fresh one.")

MATERIALISE = (
    "python3 scripts/ci/fetch_pdfa_corpus.py --set corpus/pdfa/"
    "PDFA_HOLDOUT_1000.json --out <dir>. Every entry carries archive "
    "coordinates, so the whole set is ranged requests and no GovDocs1 zip is "
    "downloaded whole; scripts/pdfa/fetch_the_set.sh holdout1000 does it.")


def population() -> list[str]:
    """The names of the list, in the order it holds them."""
    if not POPULATION.is_file():
        raise SystemExit(f"[holdout1000] FATAL: {POPULATION} is missing")
    names = [line.strip() for line in
             POPULATION.read_text(encoding="utf-8").splitlines() if line.strip()]
    if not names:
        raise SystemExit(f"[holdout1000] FATAL: {POPULATION} is empty")
    return names


# ----------------------------------------------------------------- stage: hash

def _hash_one(name: str) -> dict:
    """Fetch one GovDocs1 member from its zip and hash the bytes."""
    key = rule.key_of(name)
    shard = key.split("/", 1)[0]
    url = f"{corpus.GOVDOCS_ZIPS}/{shard}.zip"
    table = {entry_name: (method, csize, usize, lho)
             for entry_name, method, csize, usize, _crc, lho
             in corpus._central_directory(url)}
    if key not in table:
        raise RuntimeError(f"{key} is not in the central directory of {shard}.zip")
    method, csize, usize, offset = table[key]
    body = corpus._local_payload(url, offset, method, csize, usize)
    return {
        "key": key,
        "sha256": hashlib.sha256(body).hexdigest(),
        "bytes": usize,
        "zip": shard,
        "offset": offset,
        "method": method,
        "csize": csize,
    }


def hash_stage(work: pathlib.Path, workers: int) -> int:
    """Hash every document of the list, restartably, into an append-only log.

    One zip is read once and its whole central directory kept, because 632 of
    the thousand zips hold a single document of this list and re-reading a
    directory per document would triple a job that already moves half a
    gigabyte.
    """
    work.mkdir(parents=True, exist_ok=True)
    log = work / "holdout1000.jsonl"
    done: set[str] = set()
    if log.is_file():
        for line in log.read_text(encoding="utf-8").splitlines():
            if line:
                record = json.loads(line)
                if record.get("ok"):
                    done.add(record["key"])
    names = population()
    todo = [name for name in names if rule.key_of(name) not in done]
    print(f"[holdout1000] {len(names)} documents, {len(done)} already hashed, "
          f"{len(todo)} to go")

    directories: dict[str, dict] = {}
    dir_lock = threading.Lock()
    write_lock = threading.Lock()
    handle = log.open("a")
    failures = [0]

    def directory(shard: str) -> dict:
        with dir_lock:
            hit = directories.get(shard)
        if hit is not None:
            return hit
        url = f"{corpus.GOVDOCS_ZIPS}/{shard}.zip"
        table = {entry_name: (method, csize, usize, lho)
                 for entry_name, method, csize, usize, _crc, lho
                 in corpus._central_directory(url)}
        with dir_lock:
            directories[shard] = table
        return table

    def one(name: str) -> None:
        key = rule.key_of(name)
        shard = key.split("/", 1)[0]
        try:
            table = directory(shard)
            if key not in table:
                raise RuntimeError(
                    f"not in the central directory of {shard}.zip")
            method, csize, usize, offset = table[key]
            url = f"{corpus.GOVDOCS_ZIPS}/{shard}.zip"
            body = corpus._local_payload(url, offset, method, csize, usize)
        except Exception as e:  # noqa: BLE001 - recorded, the run continues
            record = {"key": key, "ok": False, "reason": str(e)[:200]}
        else:
            record = {"key": key, "ok": True,
                      "sha256": hashlib.sha256(body).hexdigest(),
                      "bytes": usize, "zip": shard, "offset": offset,
                      "method": method, "csize": csize}
        with write_lock:
            if not record["ok"]:
                failures[0] += 1
            handle.write(json.dumps(record, sort_keys=True) + "\n")
            handle.flush()

    with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
        list(pool.map(one, todo))
    handle.close()

    rows = read_log(log)
    print(f"[holdout1000] hashed {len(rows)} of {len(names)}; "
          f"{failures[0]} failure(s) this run")
    if len(rows) != len(names):
        print("[holdout1000] the log is short; run this stage again -- it "
              "resumes.", file=sys.stderr)
        return 1
    write_entries(rows)
    print(f"[holdout1000] wrote {ENTRIES.relative_to(REPO)}")
    return 0


def read_log(log: pathlib.Path) -> dict[str, dict]:
    """The successful records of the hash log, last write per key winning."""
    rows: dict[str, dict] = {}
    if not log.is_file():
        return rows
    for line in log.read_text(encoding="utf-8").splitlines():
        if not line:
            continue
        record = json.loads(line)
        if record.get("ok"):
            rows[record["key"]] = record
    return rows


def write_entries(rows: dict[str, dict]) -> None:
    """The coordinates table, sorted by key so a rerun is a no-op or a diff."""
    body = "\t".join(ENTRY_COLUMNS) + "\n"
    for key in sorted(rows):
        row = rows[key]
        body += "\t".join(str(row[column]) for column in ENTRY_COLUMNS) + "\n"
    ENTRIES.write_text(body, encoding="utf-8")


def read_entries() -> dict[str, dict]:
    """The committed coordinates table, as rows the draw can use."""
    if not ENTRIES.is_file():
        raise SystemExit(
            f"[holdout1000] FATAL: {ENTRIES.relative_to(REPO)} is missing. Run "
            "the `hash` stage; a list of names without digests is not a "
            "materialised set.")
    lines = ENTRIES.read_text(encoding="utf-8").splitlines()
    if not lines or lines[0].split("\t") != list(ENTRY_COLUMNS):
        raise SystemExit(
            f"[holdout1000] FATAL: {ENTRIES.name} does not begin with the "
            f"heading {'/'.join(ENTRY_COLUMNS)}")
    rows: dict[str, dict] = {}
    for number, line in enumerate(lines[1:], 2):
        if not line:
            continue
        cells = line.split("\t")
        if len(cells) != len(ENTRY_COLUMNS):
            raise SystemExit(
                f"[holdout1000] FATAL: {ENTRIES.name} line {number} has "
                f"{len(cells)} column(s) instead of {len(ENTRY_COLUMNS)}")
        row = dict(zip(ENTRY_COLUMNS, cells))
        for column in ("bytes", "offset", "method", "csize"):
            row[column] = int(row[column])
        row["source"] = "govdocs1"
        row["name"] = rule.name_of(row["key"])
        rows[row["key"]] = row
    return rows


# ---------------------------------------------------------------- stage: write

def spoken_for() -> tuple[set[str], set[str], dict[str, str]]:
    """Every document some other set already holds, by name and by content.

    The four sets are named rather than globbed, because a set that appears in
    the repository after this file was written must be a decision somebody made
    here and not a directory listing that silently shrinks the holdout.

    The third return value says, per name, which set claimed it: an exclusion
    that cannot say what excluded a document is an exclusion nobody can check.
    """
    names: set[str] = set()
    content: set[str] = set()
    claimed: dict[str, str] = {}

    def take(label: str, rows) -> None:
        for row in rows:
            names.add(row["name"])
            content.add(row["sha256"])
            claimed.setdefault(row["name"], label)
            claimed.setdefault(row["sha256"], label)

    for label, path in (
            ("tuning1000", PDFA_CORPUS / "PDFA_TUNING_1000.json"),
            ("test300", PDFA_CORPUS / "PDFA_TEST_300.json")):
        if not path.is_file():
            raise SystemExit(f"[holdout1000] FATAL: {path} is missing")
        take(label, json.loads(path.read_text(encoding="utf-8"))["entries"])

    take("pdfa-300", [
        {"name": entry["file"], "sha256": entry["sha256"]}
        for entry in json.loads(THE_300.read_text(encoding="utf-8"))])

    for half in ("tuning", "holdout"):
        rows = []
        for path in sorted(PDFA_CORPUS.glob(f"pdfa_{half}_entries.*.tsv")):
            for line in path.read_text(encoding="utf-8").splitlines():
                if not line:
                    continue
                cells = line.split("\t")
                source = SOURCE_LETTER[cells[0]]
                rows.append({
                    "name": cells[1].replace("/", "_") if source == "govdocs1"
                            else cells[1],
                    "sha256": cells[2]})
        if not rows:
            raise SystemExit(
                f"[holdout1000] FATAL: no entries for the {half} half of the "
                "ten thousand; the exclusion cannot be applied to a population "
                "that is not there")
        take(f"ten-thousand-{half}", rows)

    return names, content, claimed


def build() -> dict:
    """The manifest, as a pure function of files already in the repository."""
    listed = population()
    rows = read_entries()
    missing = [name for name in listed if rule.key_of(name) not in rows]
    if missing:
        raise SystemExit(
            f"[holdout1000] FATAL: {len(missing)} document(s) of "
            f"{POPULATION.name} have no digest in {ENTRIES.name} "
            f"({', '.join(missing[:5])}...). Run the `hash` stage; a set that "
            "silently dropped what it could not hash is a smaller denominator.")
        # (unreachable, kept as one branch: a short set is reported, never padded)
    extra = sorted(set(rows) - {rule.key_of(name) for name in listed})
    if extra:
        raise SystemExit(
            f"[holdout1000] FATAL: {ENTRIES.name} holds {len(extra)} document(s) "
            f"the list does not name ({', '.join(extra[:5])}). The list is the "
            "population; a digest table that grew past it is a set drawn "
            "somewhere other than here.")

    names, content, claimed = spoken_for()
    candidates = [dict(rows[rule.key_of(name)], source=rule.SOURCE)
                  for name in listed]
    kept, dropped = rule.draw(candidates, names, content)
    entries = [dict(sorted(row.items()))
               for row in sorted(kept, key=lambda r: r["name"])]
    excluded = [{"name": row["name"], "sha256": row["sha256"],
                 "why": row["why"],
                 "claimed_by": claimed.get(row["name"])
                               or claimed.get(row["sha256"]) or "the list"}
                for row in sorted(dropped, key=lambda r: r["name"])]

    return {
        "schema": SCHEMA,
        "capability": "pdfa",
        "role": "holdout",
        "set": "holdout1000",
        "draw": rule.DRAW,
        "description": DESCRIPTION,
        "generator": "scripts/ci/build_holdout_1000_manifest.py",
        "rule": {
            "module": "scripts/ci/holdout_1000_rule.py",
            "seed": rule.SEED,
            "drawn": rule.DRAWN,
            "version": rule.RULE_VERSION,
        },
        "populations": [POPULATION.relative_to(REPO).as_posix(),
                        ENTRIES.relative_to(REPO).as_posix()],
        "excluded_from": sorted(EXCLUDED_FROM),
        "population_total": len(listed),
        "excluded": excluded,
        "composition": {rule.SOURCE: len(entries)},
        "total": len(entries),
        "digest": rule.digest_of(entries),
        "materialise": MATERIALISE,
        "sources": sources_for(),
        "entries": entries,
    }


def sources_for() -> dict:
    """The provenance block, copied from the manifest the coordinates fit.

    Copied and not written here, for the reason `build_tuning_and_test_manifests`
    gives: `ratchet.licence_of` resolves a document's licence through this block,
    and a set whose entries cannot name a licence is a set that cannot be
    redistributed with the measurement (#215).
    """
    known = json.loads(
        (PDFA_CORPUS / "PDFA_TUNING_MANIFEST.json").read_text(
            encoding="utf-8"))["sources"]
    if rule.SOURCE not in known:
        raise SystemExit(
            f"[holdout1000] FATAL: PDFA_TUNING_MANIFEST.json has no provenance "
            f"for {rule.SOURCE}; a document that cannot name where it came from "
            "does not go in a manifest (#215)")
    return {rule.SOURCE: known[rule.SOURCE]}


def render(document: dict) -> str:
    return json.dumps(document, indent=2, sort_keys=True) + "\n"


def write_stage(check: bool) -> int:
    document = build()
    body = render(document)
    current = MANIFEST.read_text(encoding="utf-8") if MANIFEST.is_file() else None
    print(f"[holdout1000] {document['total']} of "
          f"{document['population_total']} documents "
          f"({len(document['excluded'])} excluded), digest "
          f"{document['digest'][:16]}\u2026")
    if check:
        if current != body:
            print(f"[holdout1000] FATAL: {MANIFEST.relative_to(REPO)} is not "
                  "what the rule draws", file=sys.stderr)
            return 1
        return 0
    MANIFEST.write_text(body, encoding="utf-8")
    print(f"[holdout1000] wrote {MANIFEST.relative_to(REPO)}")
    return 0


def main() -> int:
    p = argparse.ArgumentParser(
        description="Materialise and pin the thousand-document PDF/A holdout.")
    p.add_argument("stage", choices=("hash", "write"))
    p.add_argument("--work", type=pathlib.Path,
                   help="scratch directory for the hash log (hash stage only)")
    p.add_argument("--workers", type=int, default=12)
    p.add_argument("--check", action="store_true",
                   help="write stage: exit 1 if what is on disk is not what "
                        "the rule draws, and write nothing")
    args = p.parse_args()

    if args.stage == "hash":
        if not args.work:
            p.error("--work is required for the hash stage")
        return hash_stage(args.work, args.workers)
    return write_stage(args.check)


if __name__ == "__main__":
    raise SystemExit(main())
